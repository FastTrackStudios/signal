//! Timer-based macro mapping using the DAW API.
//!
//! Runs at ~30Hz via `daw::register_timer`. Scans tracks for mapping
//! config in P_EXT, reads macro values, and drives target FX params.
//! Also tracks which macro knob was last touched for set_min/set_max.
//!
//! Uses only `daw::get()` / `daw::block_on()` — no direct reaper-rs.

use std::sync::Mutex;
use tracing::{info, warn};

use crate::plugin::NUM_MACROS;

/// P_EXT key for mapping config.
const MAPPING_CONFIG_KEY: &str = "FTS_MACROS";
const MAPPING_CONFIG_SUBKEY: &str = "mapping_config";

/// P_EXT key for macro knob values (JSON array of f32, length 8).
const MACRO_VALUES_SUBKEY: &str = "macro_values";

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

/// Called at ~30Hz via `daw::register_timer`.
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
    track_last_macro_touch();
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

    let Some(daw) = daw::get() else { return };
    let Some(tracks) = daw::block_on(async {
        let project = daw.current_project().await.ok()?;
        let all = project.tracks().all().await.ok()?;
        Some(all)
    }) else { return };
    let Some(tracks) = tracks else { return };

    for track_info in &tracks {
        let config_json = daw::block_on(async {
            let daw = daw::get()?;
            let project = daw.current_project().await.ok()?;
            let track = project.tracks().by_guid(&track_info.guid).await.ok()??;
            track.get_ext_state(MAPPING_CONFIG_KEY, MAPPING_CONFIG_SUBKEY).await.ok()?
        });
        let Some(Some(config_json)) = config_json else { continue };
        if config_json.is_empty() { continue; }

        let mappings = parse_mappings(&config_json);
        if mappings.is_empty() { continue; }

        info!("[macro-timer] Track '{}': {} mappings", track_info.name, mappings.len());

        state.tracks.push(MacroTrackState {
            track_guid: track_info.guid.clone(),
            mappings,
            config_json,
            prev_macros: [f32::MIN; NUM_MACROS],
        });
    }

    info!("[macro-timer] Scan complete: {} track(s) with mappings", state.tracks.len());
}

fn refresh_configs(state: &mut MacroState) {
    let Some(daw) = daw::get() else { return };

    for ts in &mut state.tracks {
        let new_json = daw::block_on(async {
            let project = daw.current_project().await.ok()?;
            let track = project.tracks().by_guid(&ts.track_guid).await.ok()??;
            track.get_ext_state(MAPPING_CONFIG_KEY, MAPPING_CONFIG_SUBKEY).await.ok()?
        });
        let new_json = new_json.flatten().unwrap_or_default();

        if new_json != ts.config_json {
            ts.mappings = parse_mappings(&new_json);
            info!("[macro-timer] Refreshed '{}': {} mappings", ts.track_guid, ts.mappings.len());
            ts.config_json = new_json;
        }
    }

    // Also pick up NEW tracks
    let Some(daw) = daw::get() else { return };
    let all_tracks = daw::block_on(async {
        let project = daw.current_project().await.ok()?;
        Some(project.tracks().all().await.ok()?)
    });
    let Some(Some(all_tracks)) = all_tracks else { return };

    for track_info in &all_tracks {
        if state.tracks.iter().any(|t| t.track_guid == track_info.guid) {
            continue;
        }
        let config_json = daw::block_on(async {
            let project = daw.current_project().await.ok()?;
            let track = project.tracks().by_guid(&track_info.guid).await.ok()??;
            track.get_ext_state(MAPPING_CONFIG_KEY, MAPPING_CONFIG_SUBKEY).await.ok()?
        });
        let Some(Some(config_json)) = config_json else { continue };
        if config_json.is_empty() { continue; }
        let mappings = parse_mappings(&config_json);
        if mappings.is_empty() { continue; }
        info!("[macro-timer] New track '{}': {} mappings", track_info.name, mappings.len());
        state.tracks.push(MacroTrackState {
            track_guid: track_info.guid.clone(),
            mappings,
            config_json,
            prev_macros: [f32::MIN; NUM_MACROS],
        });
    }
}

fn apply_macros(state: &mut MacroState) {
    let Some(daw) = daw::get() else { return };

    for ts in &mut state.tracks {
        if ts.mappings.is_empty() { continue; }

        // Read macro values from P_EXT
        let macros = daw::block_on(async {
            let project = daw.current_project().await.ok()?;
            let track = project.tracks().by_guid(&ts.track_guid).await.ok()??;
            let json = track.get_ext_state(MAPPING_CONFIG_KEY, MACRO_VALUES_SUBKEY).await.ok()??;
            let arr: Vec<f32> = serde_json::from_str(&json).ok()?;
            let mut values = [0.0f32; NUM_MACROS];
            for (i, &v) in arr.iter().enumerate().take(NUM_MACROS) {
                values[i] = v;
            }
            Some(values)
        });
        let Some(Some(macros)) = macros else { continue };

        // Check for changes
        let mut changed = false;
        for i in 0..NUM_MACROS {
            if (macros[i] - ts.prev_macros[i]).abs() > 1e-5 {
                changed = true;
                break;
            }
        }
        if !changed { continue; }
        ts.prev_macros = macros;

        info!("[macro-timer] Applying {} mappings (macro[0]={:.4})", ts.mappings.len(), macros[0]);

        // Apply each mapping via Daw API
        for mapping in &ts.mappings {
            let source_idx = mapping.source_param as usize;
            if source_idx >= NUM_MACROS { continue; }
            let source_val = macros[source_idx];
            let target_val = mapping.mode.apply(source_val);

            let target_fx_idx = match &mapping.target_fx { FxRef::ByIndex(idx) => *idx };
            let param_idx = mapping.target_param_index;
            let track_guid = ts.track_guid.clone();

            let _ = daw::block_on(async {
                let project = daw.current_project().await.ok()?;
                // ByIndex(0) means same track
                let track = match &mapping.target_track {
                    TrackRef::ByIndex(0) => project.tracks().by_guid(&track_guid).await.ok()?,
                    TrackRef::ByIndex(idx) => {
                        let all = project.tracks().all().await.ok()?;
                        let t = all.get(*idx as usize)?;
                        project.tracks().by_guid(&t.guid).await.ok()?
                    }
                }?;
                let fx = track.fx_chain().by_index(target_fx_idx).await.ok()??;
                fx.param(param_idx).set(target_val as f64).await.ok()
            });
        }
    }
}

/// Track which macro knob was last touched on any signal controller.
fn track_last_macro_touch() {
    let Some(daw) = daw::get() else { return };

    let result = daw::block_on(async {
        let lt = daw.last_touched_fx().await.ok()??;
        if lt.param_index >= NUM_MACROS as u32 { return None; }

        let project = daw.current_project().await.ok()?;
        let track = project.tracks().by_guid(&lt.track_guid).await.ok()??;
        let chain = if lt.is_input_fx { track.input_fx_chain() } else { track.fx_chain() };
        let fx = chain.by_index(lt.fx_index).await.ok()??;
        let fx_info = fx.info().await.ok()?;

        if !fx_info.name.contains("Signal Controller") { return None; }
        Some(lt.param_index)
    });

    let Some(Some(param_index)) = result else { return };

    static LAST_INDEX: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);
    let prev = LAST_INDEX.load(std::sync::atomic::Ordering::Relaxed);
    if prev == param_index { return; }
    LAST_INDEX.store(param_index, std::sync::atomic::Ordering::Relaxed);

    let _ = daw::block_on(async {
        daw.ext_state().set("FTS_SIGNAL", "last_macro_index", &param_index.to_string(), false).await
    });

    info!("[macro-timer] Last touched macro: {} (Macro {})", param_index, param_index + 1);
}

/// Last console message displayed (for dedup).
static LAST_CONSOLE_MSG: Mutex<String> = Mutex::new(String::new());

/// Read console_log from global ext_state and display via Daw API.
fn flush_console_log() {
    let Some(daw) = daw::get() else { return };

    let msg = daw::block_on(async {
        daw.ext_state().get("FTS_SIGNAL", "console_log").await.ok()?
    });
    let Some(Some(msg)) = msg else { return };
    if msg.is_empty() { return; }

    let mut last = LAST_CONSOLE_MSG.lock().unwrap();
    if msg == last.as_str() { return; }
    *last = msg.clone();

    let display = format!("[Signal] {msg}\n");
    let _ = daw::block_on(async {
        daw.show_console_msg(&display).await
    });
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
