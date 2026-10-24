//! Bridge between Signal's macro system and the fts-macros CLAP plugin.
//!
//! When a module with a `MacroBank` is loaded onto a REAPER track, this module:
//! 1. Adds an fts-macros CLAP plugin instance to the track (after all block FX)
//! 2. Converts resolved `LiveMacroBinding`s into the fts-macros JSON mapping format
//! 3. Stores the config in track `P_EXT` (`P_EXT:FTS_MACROS:mapping_config`)
//!
//! The fts-macros plugin reads mapping config from its own track's `P_EXT` on each
//! timer tick — no global `ExtState` IPC or acknowledgment polling needed.

use daw::rpc::{FxHandle, TrackHandle};

use crate::daw_block_ops::LoadBlockResult;

/// `P_EXT` section used by fts-macros for track-scoped config storage.
const EXT_STATE_SECTION: &str = "FTS_MACROS";

/// CLAP plugin identifier for fts-macros.
const FTS_MACROS_CLAP: &str = "CLAP: FTS Macros";

/// Fallback display name if CLAP ID doesn't match.
const FTS_MACROS_NAME: &str = "FTS Macros";

/// Build the mapping bank JSON from loaded FX results.
///
/// Iterates all `LoadBlockResult`s, collects macro bindings, and converts each
/// `LiveMacroBinding` into the fts-macros JSON mapping format. Returns `None`
/// if no macros are present.
///
/// This is a pure function — no DAW calls needed.
#[must_use]
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
/// 4. Stores the mapping config JSON in the track's `P_EXT`
///
/// The fts-macros plugin reads `P_EXT:FTS_MACROS:mapping_config` from its own
/// track on each timer tick, so no acknowledgment polling is needed.
///
/// This function is **non-fatal**: failures are logged but do not block module loading.
///
/// # Errors
///
/// Returns an error if the macro configuration cannot be set up or applied.
pub async fn bridge_macros(
    track: &TrackHandle,
    loaded_fx: &[LoadBlockResult],
) -> Result<Option<FxHandle>, String> {
    // Quick check: any macros at all?
    let has_macros = loaded_fx.iter().any(|r| {
        r.macro_setup
            .as_ref()
            .is_some_and(|s| !s.bindings.is_empty())
    });
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
    let Some(mapping_json) = build_mapping_bank_json(track_index, &fx_indices, loaded_fx) else {
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

    // TODO(daw-track-pext): daw main removed `TrackHandle::set_ext_state` and
    // exposes no per-track P_EXT *write* (only global `Project::ext_state()`,
    // which is REAPER-wide and breaks the per-track contract the fts-macros
    // plugin relies on). The low-level read helper still exists
    // (`daw-reaper sync_api::track_get_ext_state`). Until daw re-exposes a
    // per-track P_EXT write, the mapping config is NOT delivered to the plugin,
    // so live macro mapping is a no-op at runtime.
    let _ = (track, EXT_STATE_SECTION);
    eprintln!(
        "[signal] macro bridge: WARNING — track P_EXT write unavailable on daw \
         main; mapping config ({} chars) NOT stored. Live macro mapping disabled \
         until daw restores per-track P_EXT write (see TODO daw-track-pext).",
        mapping_json.len()
    );

    Ok(Some(macros_fx))
}
