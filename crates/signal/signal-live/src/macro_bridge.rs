//! Bridge between Signal's macro system and the fts-macros CLAP plugin.
//!
//! When a module with a `MacroBank` is loaded onto a REAPER track, this module:
//! 1. Adds an fts-macros CLAP plugin instance to the track (after all block FX)
//! 2. Converts resolved `LiveMacroBinding`s into the fts-macros JSON mapping format
//! 3. Injects the config via REAPER ExtState IPC
//! 4. Polls for acknowledgment from the plugin's timer callback
//!
//! The fts-macros plugin autonomously drives target FX parameters via its timer —
//! no ongoing communication is needed after the initial config injection.

use std::time::{Duration, Instant};

use daw::{ExtState, FxHandle, TrackHandle};

use crate::daw_block_ops::LoadBlockResult;

/// ExtState section used by fts-macros for IPC.
const EXT_STATE_SECTION: &str = "FTS_MACROS";

/// CLAP plugin identifier for fts-macros.
const FTS_MACROS_CLAP: &str = "CLAP: FTS Macros";

/// Fallback display name if CLAP ID doesn't match.
const FTS_MACROS_NAME: &str = "FTS Macros";

/// Timeout for polling the mapping config acknowledgment.
const POLL_TIMEOUT: Duration = Duration::from_secs(3);

/// Interval between polling attempts.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Build the mapping bank JSON from loaded FX results.
///
/// Iterates all `LoadBlockResult`s, collects macro bindings, and converts each
/// `LiveMacroBinding` into the fts-macros JSON mapping format. Returns `None`
/// if no macros are present.
///
/// This is a pure function — no DAW calls needed.
pub fn build_mapping_bank_json(
    track_index: u32,
    fx_indices: &[(usize, u32)], // (loaded_fx index, actual FX index in chain)
    loaded_fx: &[LoadBlockResult],
) -> Option<String> {
    let mut mappings = Vec::new();

    for &(result_idx, fx_index) in fx_indices {
        let result = &loaded_fx[result_idx];
        if let Some(ref macro_setup) = result.macro_setup {
            for binding in &macro_setup.bindings {
                mappings.push(serde_json::json!({
                    "source_param": binding.knob_index,
                    "target_track": {"ByIndex": track_index},
                    "target_fx": {"ByIndex": fx_index},
                    "target_param_index": binding.param_index,
                    "mode": {"ScaleRange": {"min": binding.min, "max": binding.max}}
                }));
            }
        }
    }

    if mappings.is_empty() {
        return None;
    }

    let bank = serde_json::json!({
        "version": "0.1",
        "mappings": mappings
    });

    Some(bank.to_string())
}

/// Bridge macros from loaded module FX to an fts-macros CLAP plugin instance.
///
/// After a module is loaded onto a track, this function:
/// 1. Checks if any loaded FX have macro bindings — returns `Ok(None)` if not
/// 2. Resolves the actual FX index for each block via its GUID
/// 3. Adds an fts-macros CLAP instance to the track
/// 4. Injects the mapping config JSON via ExtState
/// 5. Polls for acknowledgment from the plugin's timer
///
/// This function is **non-fatal**: failures are logged but do not block module loading.
pub async fn bridge_macros(
    track: &TrackHandle,
    ext_state: &ExtState,
    loaded_fx: &[LoadBlockResult],
) -> Result<Option<FxHandle>, String> {
    // Quick check: any macros at all?
    let has_macros = loaded_fx
        .iter()
        .any(|r| r.macro_setup.as_ref().is_some_and(|s| !s.bindings.is_empty()));
    if !has_macros {
        return Ok(None);
    }

    // Get track index for the mapping config.
    let track_info = track
        .info()
        .await
        .map_err(|e| format!("Failed to get track info: {e}"))?;
    let track_index = track_info.index;

    // Resolve actual FX indices by GUID for each loaded block with macros.
    let mut fx_indices: Vec<(usize, u32)> = Vec::new();
    for (i, result) in loaded_fx.iter().enumerate() {
        if result
            .macro_setup
            .as_ref()
            .is_some_and(|s| !s.bindings.is_empty())
        {
            let fx = track
                .fx_chain()
                .by_guid(&result.fx_guid)
                .await
                .map_err(|e| format!("Failed to find FX by GUID {}: {e}", result.fx_guid))?
                .ok_or_else(|| format!("FX not found by GUID: {}", result.fx_guid))?;
            let fx_info = fx
                .info()
                .await
                .map_err(|e| format!("Failed to get FX info: {e}"))?;
            fx_indices.push((i, fx_info.index));
        }
    }

    // Build mapping JSON.
    let Some(mapping_json) =
        build_mapping_bank_json(track_index, &fx_indices, loaded_fx)
    else {
        return Ok(None);
    };

    // Add fts-macros CLAP plugin to the track.
    let macros_fx = match track.fx_chain().add(FTS_MACROS_CLAP).await {
        Ok(fx) => fx,
        Err(_) => track
            .fx_chain()
            .add(FTS_MACROS_NAME)
            .await
            .map_err(|e| format!("Could not add FTS Macros plugin: {e}"))?,
    };

    // Inject mapping config via ExtState.
    ext_state
        .set(EXT_STATE_SECTION, "mapping_config", &mapping_json, false)
        .await
        .map_err(|e| format!("Failed to set mapping config: {e}"))?;

    // Poll for acknowledgment (non-blocking deterministic polling).
    let start = Instant::now();
    loop {
        if let Ok(Some(val)) = ext_state
            .get(EXT_STATE_SECTION, "mapping_config_ack")
            .await
        {
            if !val.is_empty() {
                eprintln!(
                    "[signal] macro bridge: fts-macros acknowledged config ({} mappings)",
                    val
                );
                break;
            }
        }
        if start.elapsed() > POLL_TIMEOUT {
            eprintln!(
                "[signal] macro bridge: timed out waiting for mapping_config_ack ({}s) — \
                 plugin may not be running or timer not active",
                POLL_TIMEOUT.as_secs()
            );
            break;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    Ok(Some(macros_fx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daw_block_ops::LoadBlockResult;
    use crate::macro_setup::{LiveMacroBinding, MacroSetupResult};

    #[test]
    fn build_mapping_bank_json_no_macros() {
        let loaded_fx = vec![LoadBlockResult {
            fx_guid: "guid-1".into(),
            display_name: "EQ Block".into(),
            macro_setup: None,
        }];
        let result = build_mapping_bank_json(0, &[], &loaded_fx);
        assert!(result.is_none());
    }

    #[test]
    fn build_mapping_bank_json_with_bindings() {
        let loaded_fx = vec![
            LoadBlockResult {
                fx_guid: "guid-1".into(),
                display_name: "Compressor".into(),
                macro_setup: Some(MacroSetupResult {
                    track_guid: "track-1".into(),
                    target_fx_guid: "guid-1".into(),
                    bindings: vec![
                        LiveMacroBinding {
                            knob_index: 0,
                            knob_id: "drive".into(),
                            param_index: 5,
                            min: 0.0,
                            max: 1.0,
                        },
                        LiveMacroBinding {
                            knob_index: 1,
                            knob_id: "tone".into(),
                            param_index: 3,
                            min: 0.2,
                            max: 0.8,
                        },
                    ],
                }),
            },
            LoadBlockResult {
                fx_guid: "guid-2".into(),
                display_name: "EQ Block".into(),
                macro_setup: None,
            },
        ];

        let fx_indices = vec![(0, 2)]; // result index 0, FX index 2 in chain
        let result = build_mapping_bank_json(0, &fx_indices, &loaded_fx);
        assert!(result.is_some());

        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(json["version"], "0.1");

        let mappings = json["mappings"].as_array().unwrap();
        assert_eq!(mappings.len(), 2);

        // First mapping: drive → param 5
        assert_eq!(mappings[0]["source_param"], 0);
        assert_eq!(mappings[0]["target_track"]["ByIndex"], 0);
        assert_eq!(mappings[0]["target_fx"]["ByIndex"], 2);
        assert_eq!(mappings[0]["target_param_index"], 5);
        assert_eq!(mappings[0]["mode"]["ScaleRange"]["min"], 0.0);
        assert_eq!(mappings[0]["mode"]["ScaleRange"]["max"], 1.0);

        // Second mapping: tone → param 3
        assert_eq!(mappings[1]["source_param"], 1);
        assert_eq!(mappings[1]["target_param_index"], 3);
        // f32 → JSON f64 introduces precision artifacts, so use approximate checks
        let min = mappings[1]["mode"]["ScaleRange"]["min"].as_f64().unwrap();
        let max = mappings[1]["mode"]["ScaleRange"]["max"].as_f64().unwrap();
        assert!((min - 0.2).abs() < 1e-6, "min should be ~0.2, got {min}");
        assert!((max - 0.8).abs() < 1e-6, "max should be ~0.8, got {max}");
    }

    #[test]
    fn build_mapping_bank_json_multiple_fx_with_macros() {
        let loaded_fx = vec![
            LoadBlockResult {
                fx_guid: "guid-1".into(),
                display_name: "Comp".into(),
                macro_setup: Some(MacroSetupResult {
                    track_guid: "track-1".into(),
                    target_fx_guid: "guid-1".into(),
                    bindings: vec![LiveMacroBinding {
                        knob_index: 0,
                        knob_id: "threshold".into(),
                        param_index: 1,
                        min: 0.8,
                        max: 0.1,
                    }],
                }),
            },
            LoadBlockResult {
                fx_guid: "guid-2".into(),
                display_name: "EQ".into(),
                macro_setup: Some(MacroSetupResult {
                    track_guid: "track-1".into(),
                    target_fx_guid: "guid-2".into(),
                    bindings: vec![LiveMacroBinding {
                        knob_index: 0,
                        knob_id: "presence".into(),
                        param_index: 7,
                        min: 0.3,
                        max: 0.9,
                    }],
                }),
            },
        ];

        let fx_indices = vec![(0, 0), (1, 1)];
        let result = build_mapping_bank_json(3, &fx_indices, &loaded_fx);
        assert!(result.is_some());

        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        let mappings = json["mappings"].as_array().unwrap();
        assert_eq!(mappings.len(), 2);

        // Both target track index 3
        assert_eq!(mappings[0]["target_track"]["ByIndex"], 3);
        assert_eq!(mappings[1]["target_track"]["ByIndex"], 3);

        // Different FX indices
        assert_eq!(mappings[0]["target_fx"]["ByIndex"], 0);
        assert_eq!(mappings[1]["target_fx"]["ByIndex"], 1);
    }
}
