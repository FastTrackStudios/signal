//! Macro learn action handlers.
//!
//! Stateless set_min / set_max workflow:
//!
//! 1. User moves a macro knob on the signal controller → signal-controller's
//!    macro_timer writes `last_macro_index` to global ExtState
//! 2. User touches an FX parameter (e.g. turns ReaComp Threshold)
//! 3. User sets the param to their desired minimum, runs **Set Min**
//! 4. User sets the param to their desired maximum, runs **Set Max**
//!
//! Each action reads `last_macro_index` + `last_touched_fx`, then immediately
//! persists the binding to the track's `P_EXT:FTS_MACROS:mapping_config`.
//! The signal-controller's macro_timer picks it up and starts driving the param.

use daw::Daw;
use eyre::{Result, WrapErr};
use tracing::info;

const EXT_SECTION: &str = "FTS_MACROS";

/// Log to REAPER console via global ExtState → signal-controller ShowConsoleMsg.
async fn console_log(daw: &Daw, msg: &str) {
    info!("[macro-learn] {msg}");
    let _ = daw
        .ext_state()
        .set("FTS_SIGNAL", "console_log", msg, false)
        .await;
}

/// Read the last-moved macro index from global ExtState (written by macro_timer).
async fn last_macro_index(daw: &Daw) -> Result<u8> {
    let val = daw
        .ext_state()
        .get("FTS_SIGNAL", "last_macro_index")
        .await?;
    match val {
        Some(s) => s
            .parse::<u8>()
            .map_err(|_| eyre::eyre!("Invalid last_macro_index: {s}")),
        None => Ok(0), // default to macro 0
    }
}

/// Poll last touched FX param and return all the info we need.
struct TouchedParam {
    #[allow(dead_code)]
    track_guid: String,
    fx_index: u32,
    param_index: u32,
    fx_name: String,
    param_name: String,
    param_value: f64,
    track: daw::TrackHandle,
}

async fn poll_last_touched(daw: &Daw) -> Result<TouchedParam> {
    let lt = daw
        .last_touched_fx()
        .await?
        .ok_or_else(|| eyre::eyre!("No FX parameter has been touched — move a knob first"))?;

    let project = daw.current_project().await.wrap_err("no current project")?;
    let track = project
        .tracks()
        .by_guid(&lt.track_guid)
        .await?
        .ok_or_else(|| eyre::eyre!("Track not found: {}", lt.track_guid))?;

    let chain = if lt.is_input_fx {
        track.input_fx_chain()
    } else {
        track.fx_chain()
    };
    let fx = chain
        .by_index(lt.fx_index)
        .await?
        .ok_or_else(|| eyre::eyre!("FX not found at index {}", lt.fx_index))?;

    let fx_info = fx.info().await?;
    let param_info = fx.param(lt.param_index).info().await?;

    Ok(TouchedParam {
        track_guid: lt.track_guid,
        fx_index: lt.fx_index,
        param_index: lt.param_index,
        fx_name: fx_info.name,
        param_name: param_info.name,
        param_value: param_info.value,
        track,
    })
}

/// Update the mapping_config on a track, setting a specific point for a binding.
///
/// Finds or creates a mapping for (source_param, target_fx, target_param),
/// then sets the given curve point (macro_value, param_value).
async fn update_mapping(
    track: &daw::TrackHandle,
    source_param: u8,
    fx_index: u32,
    param_index: u32,
    macro_value: f64,
    param_value: f64,
) -> Result<()> {
    // Read existing config
    let existing = track
        .get_ext_state(EXT_SECTION, "mapping_config")
        .await?;

    let mut config: serde_json::Value = match &existing {
        Some(json) if !json.is_empty() => {
            serde_json::from_str(json).unwrap_or_else(|_| {
                serde_json::json!({"version": "0.1", "mappings": []})
            })
        }
        _ => serde_json::json!({"version": "0.1", "mappings": []}),
    };

    let mappings = config["mappings"]
        .as_array_mut()
        .expect("mappings should be array");

    // Find existing mapping for this (source_param, fx_index, param_index)
    let existing_idx = mappings.iter().position(|m| {
        m["source_param"].as_u64() == Some(source_param as u64)
            && m["target_fx"]["ByIndex"].as_u64() == Some(fx_index as u64)
            && m["target_param_index"].as_u64() == Some(param_index as u64)
    });

    if let Some(idx) = existing_idx {
        // Update existing mapping's curve points
        let mapping = &mut mappings[idx];
        let mode = &mapping["mode"];

        // Extract existing points
        let mut points: Vec<(f64, f64)> = if let Some(sr) = mode.get("ScaleRange") {
            vec![
                (0.0, sr["min"].as_f64().unwrap_or(0.0)),
                (1.0, sr["max"].as_f64().unwrap_or(1.0)),
            ]
        } else if let Some(mp) = mode.get("MultiPoint") {
            mp["points"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .map(|p| {
                            (
                                p["macro_value"].as_f64().unwrap_or(0.0),
                                p["param_value"].as_f64().unwrap_or(0.0),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        // Replace or add the point at this macro_value
        points.retain(|&(mv, _)| (mv - macro_value).abs() > 1e-6);
        points.push((macro_value, param_value));
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        // Write back as appropriate mode
        let new_mode = if points.len() == 2
            && (points[0].0 - 0.0).abs() < 1e-6
            && (points[1].0 - 1.0).abs() < 1e-6
        {
            serde_json::json!({"ScaleRange": {"min": points[0].1, "max": points[1].1}})
        } else {
            let pts: Vec<serde_json::Value> = points
                .iter()
                .map(|&(mv, pv)| serde_json::json!({"macro_value": mv, "param_value": pv}))
                .collect();
            serde_json::json!({"MultiPoint": {"points": pts}})
        };
        mappings[idx]["mode"] = new_mode;
    } else {
        // Create new mapping with single point — will become ScaleRange once both min+max set
        let mode = if (macro_value - 0.0).abs() < 1e-6 {
            // Just min set so far — use ScaleRange with max=param_value (will be updated)
            serde_json::json!({"ScaleRange": {"min": param_value, "max": param_value}})
        } else if (macro_value - 1.0).abs() < 1e-6 {
            serde_json::json!({"ScaleRange": {"min": param_value, "max": param_value}})
        } else {
            let pts = vec![serde_json::json!({"macro_value": macro_value, "param_value": param_value})];
            serde_json::json!({"MultiPoint": {"points": pts}})
        };

        mappings.push(serde_json::json!({
            "source_param": source_param,
            "target_track": {"ByIndex": 0},
            "target_fx": {"ByIndex": fx_index},
            "target_param_index": param_index,
            "mode": mode,
        }));
    }

    let config_json = config.to_string();
    track
        .set_ext_state(EXT_SECTION, "mapping_config", &config_json)
        .await?;

    Ok(())
}

// ── Action Handlers ──────────────────────────────────────────────────

/// Set the minimum (macro=0) value for the last-touched parameter.
pub async fn handle_macro_set_min(daw: &Daw) -> Result<()> {
    let macro_idx = last_macro_index(daw).await?;
    let touched = poll_last_touched(daw).await?;

    update_mapping(
        &touched.track,
        macro_idx,
        touched.fx_index,
        touched.param_index,
        0.0,
        touched.param_value,
    )
    .await?;

    let msg = format!(
        "Macro {} Set MIN: {} / {} = {:.4}",
        macro_idx, touched.fx_name, touched.param_name, touched.param_value,
    );
    console_log(daw, &msg).await;
    Ok(())
}

/// Set the maximum (macro=1) value for the last-touched parameter.
pub async fn handle_macro_set_max(daw: &Daw) -> Result<()> {
    let macro_idx = last_macro_index(daw).await?;
    let touched = poll_last_touched(daw).await?;

    update_mapping(
        &touched.track,
        macro_idx,
        touched.fx_index,
        touched.param_index,
        1.0,
        touched.param_value,
    )
    .await?;

    let msg = format!(
        "Macro {} Set MAX: {} / {} = {:.4}",
        macro_idx, touched.fx_name, touched.param_name, touched.param_value,
    );
    console_log(daw, &msg).await;
    Ok(())
}

/// Set a curve point for the last touched parameter at a specific macro position.
pub async fn handle_macro_set_point(daw: &Daw) -> Result<()> {
    let macro_idx = last_macro_index(daw).await?;
    let touched = poll_last_touched(daw).await?;

    // For set_point, use the param value as both the macro position and target value.
    // TODO: Read the actual macro knob position from P_EXT.
    let macro_value = touched.param_value;
    let param_value = touched.param_value;

    update_mapping(
        &touched.track,
        macro_idx,
        touched.fx_index,
        touched.param_index,
        macro_value,
        param_value,
    )
    .await?;

    let msg = format!(
        "Macro {} Set Point: {} / {} = {:.4} at macro={:.4}",
        macro_idx, touched.fx_name, touched.param_name, param_value, macro_value,
    );
    console_log(daw, &msg).await;
    Ok(())
}

/// Clear all mappings for the last-moved macro on the last-touched track.
pub async fn handle_macro_clear(daw: &Daw) -> Result<()> {
    let macro_idx = last_macro_index(daw).await?;
    let touched = poll_last_touched(daw).await?;

    // Read existing config and remove mappings for this macro
    let existing = touched
        .track
        .get_ext_state(EXT_SECTION, "mapping_config")
        .await?;

    if let Some(json) = existing {
        if let Ok(mut config) = serde_json::from_str::<serde_json::Value>(&json) {
            if let Some(mappings) = config["mappings"].as_array_mut() {
                let before = mappings.len();
                mappings.retain(|m| m["source_param"].as_u64() != Some(macro_idx as u64));
                let removed = before - mappings.len();

                let config_json = config.to_string();
                touched
                    .track
                    .set_ext_state(EXT_SECTION, "mapping_config", &config_json)
                    .await?;

                let msg = format!("Cleared {} mapping(s) for Macro {}", removed, macro_idx);
                console_log(daw, &msg).await;
                return Ok(());
            }
        }
    }

    console_log(daw, &format!("No mappings to clear for Macro {}", macro_idx)).await;
    Ok(())
}

// Keep arm/disarm as no-ops for backward compat (actions still registered)

pub async fn handle_macro_arm(daw: &Daw) -> Result<()> {
    console_log(
        daw,
        "Macro learn is automatic — move a macro slider, touch an FX param, then use Set Min / Set Max",
    )
    .await;
    Ok(())
}

pub async fn handle_macro_disarm(daw: &Daw) -> Result<()> {
    console_log(daw, "Macro learn is automatic — no disarm needed").await;
    Ok(())
}

pub async fn handle_macro_remove_last_point(daw: &Daw) -> Result<()> {
    // TODO: implement point removal for MultiPoint curves
    console_log(daw, "Remove last point: not yet implemented").await;
    Ok(())
}
