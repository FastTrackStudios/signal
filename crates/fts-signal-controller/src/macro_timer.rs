//! Timer-based macro mapping on REAPER's main thread.
//!
//! Runs at ~30Hz alongside the scene timer. Scans for tracks with
//! `P_EXT:FTS_MACROS:mapping_config`, reads macro knob values from
//! `P_EXT:FTS_MACROS:macro_values`, and drives target FX parameters
//! via `TrackFX_SetParamNormalized`.
//!
//! Macro values are written to P_EXT by the signal controller's
//! `process()` callback (audio thread → P_EXT on main thread).
//! The test harness can also write values directly.

use daw::reaper::bootstrap::{HighReaper, LowReaper, MediaTrack};
use std::ffi::CString;
use std::sync::Mutex;
use tracing::{info, warn};

use crate::plugin::NUM_MACROS;

/// P_EXT key for mapping config.
const MAPPING_CONFIG_KEY: &str = "P_EXT:FTS_MACROS:mapping_config";

/// P_EXT key for macro knob values (JSON array of f32, length 8).
const MACRO_VALUES_KEY: &str = "P_EXT:FTS_MACROS:macro_values";

// ── Mapping types (deserialized from JSON) ────────────────────────────

#[derive(Debug, Clone, serde::Deserialize)]
struct MappingConfig {
    #[allow(dead_code)]
    version: String,
    mappings: Vec<Mapping>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Mapping {
    source_param: u8,
    target_track: TrackRef,
    target_fx: FxRef,
    target_param_index: u32,
    mode: MapMode,
}

#[derive(Debug, Clone, serde::Deserialize)]
enum TrackRef {
    ByIndex(u32),
}

#[derive(Debug, Clone, serde::Deserialize)]
enum FxRef {
    ByIndex(u32),
}

#[derive(Debug, Clone, serde::Deserialize)]
enum MapMode {
    ScaleRange { min: f32, max: f32 },
    MultiPoint { points: Vec<CurvePoint> },
    PassThrough,
    Toggle,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct CurvePoint {
    macro_value: f32,
    param_value: f32,
}

impl MapMode {
    fn apply(&self, source: f32) -> f32 {
        let v = source.clamp(0.0, 1.0);
        match self {
            MapMode::ScaleRange { min, max } => min + v * (max - min),
            MapMode::PassThrough => v,
            MapMode::Toggle => {
                if v >= 0.5 { 1.0 } else { 0.0 }
            }
            MapMode::MultiPoint { points } => {
                if points.is_empty() {
                    return v;
                }
                if points.len() == 1 {
                    return points[0].param_value;
                }
                if v <= points[0].macro_value {
                    return points[0].param_value;
                }
                let last = &points[points.len() - 1];
                if v >= last.macro_value {
                    return last.param_value;
                }
                for window in points.windows(2) {
                    let a = &window[0];
                    let b = &window[1];
                    if v >= a.macro_value && v <= b.macro_value {
                        let range = b.macro_value - a.macro_value;
                        if range < 1e-6 {
                            return a.param_value;
                        }
                        let t = (v - a.macro_value) / range;
                        return a.param_value + (b.param_value - a.param_value) * t;
                    }
                }
                last.param_value
            }
        }
    }
}

// ── Cached state ──────────────────────────────────────────────────────

struct MacroTrackState {
    /// REAPER track index.
    track_idx: u32,
    /// Parsed mapping config.
    mappings: Vec<Mapping>,
    /// Raw config JSON (for change detection).
    config_json: String,
    /// Previous macro values (for change detection).
    prev_macros: [f32; NUM_MACROS],
}

struct MacroState {
    tracks: Vec<MacroTrackState>,
    initialized: bool,
    tick_count: u32,
}

static STATE: Mutex<MacroState> = Mutex::new(MacroState {
    tracks: Vec::new(),
    initialized: false,
    tick_count: 0,
});

/// Called from the timer callback at ~30Hz on REAPER's main thread.
pub fn poll() {
    let Ok(mut state) = STATE.try_lock() else {
        return;
    };

    state.tick_count += 1;

    if !state.initialized {
        scan_tracks(&mut state);
        state.initialized = true;
        return;
    }

    // Re-scan every ~5 seconds (picks up new tracks)
    if state.tick_count % 150 == 0 {
        scan_tracks(&mut state);
    }

    // Re-read mapping config every ~1 second
    if state.tick_count % 30 == 0 {
        refresh_configs(&mut state);
    }

    apply_macros(&mut state);

    // Check for console messages from the signal-extension
    flush_console_log();
}

/// Invalidate cached state.
pub fn invalidate() {
    if let Ok(mut state) = STATE.lock() {
        state.initialized = false;
        state.tracks.clear();
    }
}

/// Scan all tracks for mapping_config in P_EXT.
fn scan_tracks(state: &mut MacroState) {
    state.tracks.clear();

    let reaper = HighReaper::get();
    let project = reaper.current_project();
    let track_count = project.track_count();
    let low = reaper.medium_reaper().low();

    for track_idx in 0..track_count {
        let Some(track) = project.track_by_index(track_idx) else {
            continue;
        };
        let raw = match track.raw() {
            Ok(r) => r,
            Err(_) => continue,
        };

        // Check if this track has mapping config
        let config_json = match read_p_ext(low, raw.as_ptr(), MAPPING_CONFIG_KEY) {
            Some(json) if !json.is_empty() => json,
            _ => continue,
        };

        let mappings = parse_mappings(&config_json);
        if mappings.is_empty() {
            continue;
        }

        let track_name = track.name().map(|n| n.to_string()).unwrap_or_default();
        info!(
            "[macro-timer] Track {} '{}': {} mappings",
            track_idx, track_name, mappings.len()
        );

        state.tracks.push(MacroTrackState {
            track_idx,
            mappings,
            config_json,
            prev_macros: [f32::MIN; NUM_MACROS], // sentinel: forces first apply
        });
    }

    info!(
        "[macro-timer] Scan complete: {} track(s) with mappings",
        state.tracks.len()
    );
}

/// Re-read mapping configs for existing tracks.
fn refresh_configs(state: &mut MacroState) {
    let reaper = HighReaper::get();
    let project = reaper.current_project();
    let low = reaper.medium_reaper().low();

    for ts in &mut state.tracks {
        let Some(track) = project.track_by_index(ts.track_idx) else {
            continue;
        };
        let raw = match track.raw() {
            Ok(r) => r,
            Err(_) => continue,
        };

        let new_json = read_p_ext(low, raw.as_ptr(), MAPPING_CONFIG_KEY)
            .unwrap_or_default();

        if new_json != ts.config_json {
            ts.mappings = parse_mappings(&new_json);
            info!(
                "[macro-timer] Refreshed track {}: {} mappings",
                ts.track_idx, ts.mappings.len()
            );
            ts.config_json = new_json;
        }
    }

    // Also pick up NEW tracks that got mapping config since last scan
    let track_count = reaper.current_project().track_count();
    for track_idx in 0..track_count {
        if state.tracks.iter().any(|t| t.track_idx == track_idx) {
            continue; // Already tracked
        }
        let Some(track) = project.track_by_index(track_idx) else {
            continue;
        };
        let raw = match track.raw() {
            Ok(r) => r,
            Err(_) => continue,
        };
        let config_json = match read_p_ext(low, raw.as_ptr(), MAPPING_CONFIG_KEY) {
            Some(json) if !json.is_empty() => json,
            _ => continue,
        };
        let mappings = parse_mappings(&config_json);
        if mappings.is_empty() {
            continue;
        }
        let track_name = track.name().map(|n| n.to_string()).unwrap_or_default();
        info!(
            "[macro-timer] New track {} '{}': {} mappings",
            track_idx, track_name, mappings.len()
        );
        state.tracks.push(MacroTrackState {
            track_idx,
            mappings,
            config_json,
            prev_macros: [f32::MIN; NUM_MACROS], // sentinel: forces first apply
        });
    }
}

/// Track which macro knob was last changed (written to global ExtState
/// so the signal-extension's set_min/set_max can read it).
fn update_last_macro_index(low: &LowReaper, macros: &[f32; NUM_MACROS], prev: &[f32; NUM_MACROS]) {
    // Find the macro with the largest change
    let mut best_idx = 0usize;
    let mut best_delta = 0.0f32;
    for i in 0..NUM_MACROS {
        let delta = (macros[i] - prev[i]).abs();
        if delta > best_delta {
            best_delta = delta;
            best_idx = i;
        }
    }
    if best_delta < 1e-5 {
        return;
    }

    let section = CString::new("FTS_SIGNAL").unwrap();
    let key = CString::new("last_macro_index").unwrap();
    let val = CString::new(best_idx.to_string()).unwrap();
    unsafe {
        low.SetExtState(section.as_ptr(), key.as_ptr(), val.as_ptr(), false);
    }
}

/// Read macro values from P_EXT, apply mappings, and track last-changed macro.
fn apply_macros(state: &mut MacroState) {
    let reaper = HighReaper::get();
    let project = reaper.current_project();
    let low = reaper.medium_reaper().low();

    // Also scan ALL tracks for macro_values changes (not just those with mappings)
    // so we can track last_macro_index even before mappings exist.
    let track_count = project.track_count();
    for track_idx in 0..track_count {
        let Some(track) = project.track_by_index(track_idx) else { continue };
        let raw = match track.raw() { Ok(r) => r, Err(_) => continue };
        let macros = read_macro_values(low, raw.as_ptr());

        // Check if any values are non-default (i.e. someone wrote macro_values)
        let has_values = macros.iter().any(|&v| v.abs() > 1e-6 || v > 0.0);
        if !has_values {
            continue;
        }

        // Find existing tracked state or create temp for comparison
        if let Some(ts) = state.tracks.iter_mut().find(|t| t.track_idx == track_idx) {
            let mut changed = false;
            for i in 0..NUM_MACROS {
                if (macros[i] - ts.prev_macros[i]).abs() > 1e-5 {
                    changed = true;
                    break;
                }
            }
            if changed {
                update_last_macro_index(low, &macros, &ts.prev_macros);
            }
        }
    }

    for ts in &mut state.tracks {
        let Some(track) = project.track_by_index(ts.track_idx) else { continue };
        let raw = match track.raw() { Ok(r) => r, Err(_) => continue };

        // Read macro values from P_EXT
        let macros = read_macro_values(low, raw.as_ptr());

        // Check for changes
        let mut changed = false;
        for i in 0..NUM_MACROS {
            if (macros[i] - ts.prev_macros[i]).abs() > 1e-5 {
                changed = true;
                break;
            }
        }
        if !changed {
            continue;
        }

        update_last_macro_index(low, &macros, &ts.prev_macros);
        ts.prev_macros = macros;

        if ts.mappings.is_empty() {
            continue;
        }

        info!(
            "[macro-timer] Track {}: applying {} mappings (macro[0]={:.4})",
            ts.track_idx, ts.mappings.len(), macros[0]
        );

        // Apply each mapping
        for mapping in &ts.mappings {
            let source_idx = mapping.source_param as usize;
            if source_idx >= NUM_MACROS {
                continue;
            }
            let source_val = macros[source_idx];
            let target_val = mapping.mode.apply(source_val);

            // Resolve target track
            let target_track_ptr = match &mapping.target_track {
                TrackRef::ByIndex(0) => raw.as_ptr(),
                TrackRef::ByIndex(idx) => {
                    match project.track_by_index(*idx) {
                        Some(t) => match t.raw() {
                            Ok(r) => r.as_ptr(),
                            Err(_) => continue,
                        },
                        None => continue,
                    }
                }
            };

            let target_fx_idx = match &mapping.target_fx {
                FxRef::ByIndex(idx) => *idx as i32,
            };

            unsafe {
                low.TrackFX_SetParamNormalized(
                    target_track_ptr,
                    target_fx_idx,
                    mapping.target_param_index as i32,
                    target_val as f64,
                );
            }
        }
    }
}

/// Read macro values from P_EXT:FTS_MACROS:macro_values.
/// Expects a JSON array like [0.5, 0.0, 0.0, ...].
fn read_macro_values(low: &LowReaper, track_ptr: *mut MediaTrack) -> [f32; NUM_MACROS] {
    let mut values = [0.0f32; NUM_MACROS];
    let Some(json) = read_p_ext(low, track_ptr, MACRO_VALUES_KEY) else {
        return values;
    };
    if let Ok(arr) = serde_json::from_str::<Vec<f32>>(&json) {
        for (i, &v) in arr.iter().enumerate().take(NUM_MACROS) {
            values[i] = v;
        }
    }
    values
}

/// Read a P_EXT string from a raw track pointer.
fn read_p_ext(low: &LowReaper, track_ptr: *mut MediaTrack, key: &str) -> Option<String> {
    let attr = CString::new(key).ok()?;
    let mut buf = vec![0u8; 4096];
    let ok = unsafe {
        low.GetSetMediaTrackInfo_String(
            track_ptr,
            attr.as_ptr(),
            buf.as_mut_ptr() as *mut i8,
            false,
        )
    };
    if !ok {
        return None;
    }
    let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let s = std::str::from_utf8(&buf[..nul]).ok()?.to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Last console message we displayed (for dedup).
static LAST_CONSOLE_MSG: Mutex<String> = Mutex::new(String::new());

/// Read console_log from global ext_state and display via ShowConsoleMsg.
fn flush_console_log() {
    let reaper = HighReaper::get();
    let low = reaper.medium_reaper().low();

    // Read from global ExtState (not per-track P_EXT)
    let section = CString::new("FTS_SIGNAL").unwrap();
    let key = CString::new("console_log").unwrap();
    let ptr = unsafe { low.GetExtState(section.as_ptr(), key.as_ptr()) };
    if ptr.is_null() {
        return;
    }
    let msg = match unsafe { std::ffi::CStr::from_ptr(ptr) }.to_str() {
        Ok(s) if !s.is_empty() => s,
        _ => return,
    };

    // Dedup: only show if different from last message
    let mut last = LAST_CONSOLE_MSG.lock().unwrap();
    if msg == last.as_str() {
        return;
    }
    *last = msg.to_string();

    // Display in REAPER console
    let display = format!("[Signal] {msg}\n");
    if let Ok(c_msg) = CString::new(display) {
        unsafe {
            low.ShowConsoleMsg(c_msg.as_ptr());
        }
    }
}

fn parse_mappings(json: &str) -> Vec<Mapping> {
    match serde_json::from_str::<MappingConfig>(json) {
        Ok(config) => config.mappings,
        Err(e) => {
            warn!("[macro-timer] Failed to parse mapping config: {e}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_range_apply() {
        let mode = MapMode::ScaleRange { min: 0.2, max: 0.8 };
        assert!((mode.apply(0.0) - 0.2).abs() < 1e-4);
        assert!((mode.apply(0.5) - 0.5).abs() < 1e-4);
        assert!((mode.apply(1.0) - 0.8).abs() < 1e-4);
    }

    #[test]
    fn multi_point_four_stages() {
        let mode = MapMode::MultiPoint {
            points: vec![
                CurvePoint { macro_value: 0.0, param_value: 0.9 },
                CurvePoint { macro_value: 0.33, param_value: 0.6 },
                CurvePoint { macro_value: 0.66, param_value: 0.3 },
                CurvePoint { macro_value: 1.0, param_value: 0.1 },
            ],
        };
        assert!((mode.apply(0.0) - 0.9).abs() < 1e-4);
        assert!((mode.apply(0.33) - 0.6).abs() < 1e-4);
        assert!((mode.apply(0.66) - 0.3).abs() < 1e-4);
        assert!((mode.apply(1.0) - 0.1).abs() < 1e-4);
        let mid = mode.apply(0.165);
        assert!((mid - 0.75).abs() < 0.01, "expected ~0.75, got {mid}");
    }

    #[test]
    fn parse_mapping_json() {
        let json = r#"{
            "version": "0.1",
            "mappings": [
                {
                    "source_param": 0,
                    "target_track": {"ByIndex": 0},
                    "target_fx": {"ByIndex": 1},
                    "target_param_index": 3,
                    "mode": {"ScaleRange": {"min": 0.2, "max": 0.8}}
                },
                {
                    "source_param": 0,
                    "target_track": {"ByIndex": 0},
                    "target_fx": {"ByIndex": 1},
                    "target_param_index": 1,
                    "mode": {"MultiPoint": {"points": [
                        {"macro_value": 0.0, "param_value": 0.1},
                        {"macro_value": 0.5, "param_value": 0.9},
                        {"macro_value": 1.0, "param_value": 0.5}
                    ]}}
                }
            ]
        }"#;
        let mappings = parse_mappings(json);
        assert_eq!(mappings.len(), 2);
        assert!((mappings[0].mode.apply(0.5) - 0.5).abs() < 1e-4);
        assert!((mappings[1].mode.apply(0.5) - 0.9).abs() < 1e-4);
    }
}
