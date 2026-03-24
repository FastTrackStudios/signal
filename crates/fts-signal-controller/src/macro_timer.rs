//! Timer-based macro mapping using the sync DAW API.
//!
//! Runs at ~30Hz via `daw::register_timer`. Uses `daw::main_thread_daw()`
//! for zero-overhead direct REAPER calls — no async, no RPC, no channels.

use std::sync::Mutex;
use tracing::{info, warn};

use crate::plugin::NUM_MACROS;

const MAPPING_CONFIG_KEY: &str = "FTS_MACROS";
const MAPPING_CONFIG_SUBKEY: &str = "mapping_config";

// ── Mapping types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize)]
struct MappingConfig {
    #[allow(dead_code)]
    version: String,
    mappings: Vec<Mapping>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Mapping {
    source_param: u8,
    #[allow(dead_code)]
    target_track: TrackRef,
    target_fx: FxRef,
    target_param_index: u32,
    mode: MapMode,
}

#[derive(Debug, Clone, serde::Deserialize)]
enum TrackRef { ByIndex(u32) }

#[derive(Debug, Clone, serde::Deserialize)]
enum FxRef { ByIndex(u32) }

#[derive(Debug, Clone, serde::Deserialize)]
enum MapMode {
    ScaleRange { min: f32, max: f32 },
    MultiPoint { points: Vec<CurvePoint> },
    PassThrough,
    Toggle,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct CurvePoint { macro_value: f32, param_value: f32 }

impl MapMode {
    fn apply(&self, source: f32) -> f32 {
        let v = source.clamp(0.0, 1.0);
        match self {
            MapMode::ScaleRange { min, max } => min + v * (max - min),
            MapMode::PassThrough => v,
            MapMode::Toggle => if v >= 0.5 { 1.0 } else { 0.0 },
            MapMode::MultiPoint { points } => {
                if points.is_empty() { return v; }
                if points.len() == 1 { return points[0].param_value; }
                if v <= points[0].macro_value { return points[0].param_value; }
                let last = &points[points.len() - 1];
                if v >= last.macro_value { return last.param_value; }
                for window in points.windows(2) {
                    let (a, b) = (&window[0], &window[1]);
                    if v >= a.macro_value && v <= b.macro_value {
                        let range = b.macro_value - a.macro_value;
                        if range < 1e-6 { return a.param_value; }
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
    track_guid: String,
    controller_fx_index: Option<u32>,
    mappings: Vec<Mapping>,
    config_json: String,
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

/// Called at ~30Hz on REAPER's main thread.
pub fn poll() {
    let Ok(mut state) = STATE.try_lock() else { return };

    state.tick_count += 1;

    if !state.initialized {
        scan_tracks(&mut state);
        state.initialized = true;
        return;
    }

    if state.tick_count % 150 == 0 {
        scan_tracks(&mut state);
    }

    if state.tick_count % 30 == 0 {
        refresh_configs(&mut state);
    }

    apply_macros(&mut state);
    flush_console_log();
}

pub fn invalidate() {
    if let Ok(mut state) = STATE.lock() {
        state.initialized = false;
        state.tracks.clear();
    }
}

fn scan_tracks(state: &mut MacroState) {
    state.tracks.clear();

    let Some(daw) = daw::main_thread_daw() else { return };
    let tracks = daw.track_list();

    for track_info in &tracks {
        let config_json = match daw.track_get_ext_state(&track_info.guid, MAPPING_CONFIG_KEY, MAPPING_CONFIG_SUBKEY) {
            Some(json) if !json.is_empty() => json,
            _ => continue,
        };

        let mappings = parse_mappings(&config_json);
        if mappings.is_empty() { continue; }

        // Find the signal controller FX on this track
        let controller_fx_index = daw.fx_list(&track_info.guid)
            .iter()
            .find(|fx| fx.name.contains("Signal Controller"))
            .map(|fx| fx.index);

        info!(
            "[macro-timer] Track '{}': {} mappings, controller FX={:?}",
            track_info.name, mappings.len(), controller_fx_index
        );

        state.tracks.push(MacroTrackState {
            track_guid: track_info.guid.clone(),
            controller_fx_index,
            mappings,
            config_json,
            prev_macros: [f32::MIN; NUM_MACROS],
        });
    }

    info!("[macro-timer] Scan complete: {} track(s) with mappings", state.tracks.len());
}

fn refresh_configs(state: &mut MacroState) {
    let Some(daw) = daw::main_thread_daw() else { return };

    for ts in &mut state.tracks {
        let new_json = daw.track_get_ext_state(&ts.track_guid, MAPPING_CONFIG_KEY, MAPPING_CONFIG_SUBKEY)
            .unwrap_or_default();

        if new_json != ts.config_json {
            ts.mappings = parse_mappings(&new_json);
            info!("[macro-timer] Refreshed '{}': {} mappings", ts.track_guid, ts.mappings.len());
            ts.config_json = new_json;
        }
    }

    // Pick up NEW tracks
    let all_tracks = daw.track_list();
    for track_info in &all_tracks {
        if state.tracks.iter().any(|t| t.track_guid == track_info.guid) { continue; }

        let config_json = match daw.track_get_ext_state(&track_info.guid, MAPPING_CONFIG_KEY, MAPPING_CONFIG_SUBKEY) {
            Some(json) if !json.is_empty() => json,
            _ => continue,
        };
        let mappings = parse_mappings(&config_json);
        if mappings.is_empty() { continue; }

        info!("[macro-timer] New track '{}': {} mappings", track_info.name, mappings.len());
        state.tracks.push(MacroTrackState {
            track_guid: track_info.guid.clone(),
            controller_fx_index: None,
            mappings,
            config_json,
            prev_macros: [f32::MIN; NUM_MACROS],
        });
    }
}

fn apply_macros(state: &mut MacroState) {
    let Some(daw) = daw::main_thread_daw() else { return };

    for ts in &mut state.tracks {
        if ts.mappings.is_empty() { continue; }

        // Read macro values from the signal controller's FX params
        let macros = if let Some(fx_idx) = ts.controller_fx_index {
            let mut values = [0.0f32; NUM_MACROS];
            for i in 0..NUM_MACROS {
                if let Some(v) = daw.fx_param_get(&ts.track_guid, fx_idx, i as u32) {
                    values[i] = v as f32;
                }
            }
            values
        } else {
            [0.0f32; NUM_MACROS]
        };

        // Check for changes
        let mut changed = false;
        for i in 0..NUM_MACROS {
            if (macros[i] - ts.prev_macros[i]).abs() > 1e-5 {
                changed = true;
                break;
            }
        }
        if !changed { continue; }

        // Track which macro changed the most
        if ts.controller_fx_index.is_some() {
            let mut best_idx = 0usize;
            let mut best_delta = 0.0f32;
            for i in 0..NUM_MACROS {
                let delta = (macros[i] - ts.prev_macros[i]).abs();
                if delta > best_delta {
                    best_delta = delta;
                    best_idx = i;
                }
            }
            if best_delta > 1e-3 {
                daw.ext_state_set("FTS_SIGNAL", "last_macro_index", &best_idx.to_string(), false);
            }
        }

        ts.prev_macros = macros;

        // Check last_touched_fx — don't override params the user is actively adjusting
        let last_touched = daw.last_touched_fx();

        // Apply each mapping
        for mapping in &ts.mappings {
            let source_idx = mapping.source_param as usize;
            if source_idx >= NUM_MACROS { continue; }

            let target_fx_idx = match &mapping.target_fx { FxRef::ByIndex(idx) => *idx };
            let param_idx = mapping.target_param_index;

            if let Some(ref lt) = last_touched {
                if lt.fx_index == target_fx_idx && lt.param_index == param_idx {
                    continue;
                }
            }

            let source_val = macros[source_idx];
            let target_val = mapping.mode.apply(source_val);

            daw.fx_param_set(&ts.track_guid, target_fx_idx, param_idx, target_val as f64);
        }
    }
}

static LAST_CONSOLE_MSG: Mutex<String> = Mutex::new(String::new());

fn flush_console_log() {
    let Some(daw) = daw::main_thread_daw() else { return };

    let Some(msg) = daw.ext_state_get("FTS_SIGNAL", "console_log") else { return };
    if msg.is_empty() { return; }

    let mut last = LAST_CONSOLE_MSG.lock().unwrap();
    if msg == last.as_str() { return; }
    *last = msg.clone();

    daw.show_console_msg(&format!("[Signal] {msg}\n"));
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
        assert!((mode.apply(1.0) - 0.1).abs() < 1e-4);
        let mid = mode.apply(0.165);
        assert!((mid - 0.75).abs() < 0.01, "expected ~0.75, got {mid}");
    }

    #[test]
    fn parse_mapping_json() {
        let json = r#"{"version":"0.1","mappings":[{"source_param":0,"target_track":{"ByIndex":0},"target_fx":{"ByIndex":1},"target_param_index":3,"mode":{"ScaleRange":{"min":0.2,"max":0.8}}}]}"#;
        let mappings = parse_mappings(json);
        assert_eq!(mappings.len(), 1);
        assert!((mappings[0].mode.apply(0.5) - 0.5).abs() < 1e-4);
    }
}
