//! Bridges the signal domain parameter model to DAW parameter snapshots.
//!
//! Provides two pure functions that convert between:
//! - [`Block`] / [`ResolvedGraph`] — the signal domain's declarative parameter model
//! - [`DawParameterSnapshot`] / [`DawParamValue`] — the live DAW capture format
//!
//! The matching heuristic — signal param ID is a case-insensitive substring of
//! the DAW param name — is intentionally centralised here so every caller uses
//! the same rule and it can be tested in isolation.
//!
//! # Usage
//!
//! ```ignore
//! // Apply a single Block to a snapshot
//! let (snapshot, count) = block_to_snapshot(&block, &live_params, "jm-amp");
//!
//! // Apply an entire resolved graph to a snapshot
//! let (snapshot, count) = graph_to_snapshot(&graph, &live_params, "jm-amp");
//!
//! // Map live DAW values back onto a domain Block
//! let updated = live_params_into_block(block, &live_params);
//! ```

use signal_proto::{resolve::ResolvedGraph, Block};

use super::daw_bridge::DawStateChunk;
use super::morph::{DawParamValue, DawParameterSnapshot};

/// A single DAW parameter as seen by the bridge: just a name and a value.
/// Callers provide a slice of these; the actual type (FxParameter, etc.)
/// stays in the DAW-specific crate.
pub struct LiveParam {
    /// Parameter index within the FX plugin (used as the `param_index` in the snapshot).
    pub index: u32,
    /// Human-readable parameter name exposed by the plugin.
    pub name: String,
    /// Current normalized value (0.0–1.0).
    pub value: f64,
}

/// Match a signal domain parameter ID against a live DAW parameter name.
///
/// Returns `true` if `signal_id` is a case-insensitive substring of `daw_name`,
/// comparing after stripping separators (spaces, hyphens, underscores).
/// This handles camelCase fingerprint names like `"dumbleGain"` matching
/// DAW-exposed names like `"Dumble Gain"`.
pub fn param_name_matches(signal_id: &str, daw_name: &str) -> bool {
    let norm_daw: String = daw_name
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_'))
        .flat_map(|c| c.to_lowercase())
        .collect();
    let norm_sig: String = signal_id
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_'))
        .flat_map(|c| c.to_lowercase())
        .collect();
    norm_daw.contains(&norm_sig)
}

/// Build a [`DawParameterSnapshot`] by mapping a [`Block`]'s parameters onto
/// a slice of live DAW parameters.
///
/// Only parameters that match by name are included. Returns the snapshot and
/// the count of parameters that were matched and applied.
pub fn block_to_snapshot(
    block: &Block,
    live: &[LiveParam],
    fx_id: &str,
) -> (DawParameterSnapshot, usize) {
    let mut values = Vec::new();
    for sp in block.parameters() {
        if let Some(lp) = live.iter().find(|p| param_name_matches(sp.id(), &p.name)) {
            values.push(DawParamValue {
                fx_id: fx_id.to_string(),
                param_index: lp.index,
                param_name: lp.name.clone(),
                value: sp.value().get() as f64,
            });
        }
    }
    let count = values.len();
    (DawParameterSnapshot::new(values), count)
}

/// Build a [`DawParameterSnapshot`] from a fully resolved graph by walking
/// all engines → layers → modules → blocks and collecting every matched param.
///
/// Returns the combined snapshot and the total count of matched parameters.
pub fn graph_to_snapshot(
    graph: &ResolvedGraph,
    live: &[LiveParam],
    fx_id: &str,
) -> (DawParameterSnapshot, usize) {
    let mut values = Vec::new();
    for engine in &graph.engines {
        for layer in &engine.layers {
            for module in &layer.modules {
                for rb in &module.blocks {
                    for sp in rb.block.parameters() {
                        if let Some(lp) = live.iter().find(|p| param_name_matches(sp.id(), &p.name))
                        {
                            values.push(DawParamValue {
                                fx_id: fx_id.to_string(),
                                param_index: lp.index,
                                param_name: lp.name.clone(),
                                value: sp.value().get() as f64,
                            });
                        }
                    }
                }
            }
        }
    }
    let count = values.len();
    (DawParameterSnapshot::new(values), count)
}

/// Extract [`DawStateChunk`]s from a resolved graph.
///
/// Walks all engines → layers → modules → blocks and collects binary state
/// data from any [`ResolvedBlock`] that carries `state_data`. Each chunk is
/// tagged with the provided `fx_id` and the block's label.
///
/// Returns an empty `Vec` when no blocks carry state data (the normal case
/// for rig-based resolution). For `BlockSnapshot` targets pointing at catalog
/// presets with `.bin` files, this returns one chunk per block.
pub fn graph_state_chunks(graph: &ResolvedGraph, fx_id: &str) -> Vec<DawStateChunk> {
    let mut chunks = Vec::new();
    for engine in &graph.engines {
        for layer in &engine.layers {
            for module in &layer.modules {
                for rb in &module.blocks {
                    if let Some(data) = &rb.state_data {
                        chunks.push(DawStateChunk {
                            fx_id: fx_id.to_string(),
                            plugin_name: rb.label.clone(),
                            block_type: rb.block_type,
                            chunk_data: data.clone(),
                        });
                    }
                }
            }
        }
    }
    chunks
}

/// Map live DAW parameter values back onto a domain [`Block`].
///
/// Matches each block parameter by name against `live`, overwriting the
/// block's stored value with the live DAW value. Parameters with no live
/// match are left unchanged.
///
/// Returns the updated block.
pub fn live_params_into_block(mut block: Block, live: &[LiveParam]) -> Block {
    let updates: Vec<(usize, f32)> = block
        .parameters()
        .iter()
        .enumerate()
        .filter_map(|(i, sp)| {
            live.iter()
                .find(|p| param_name_matches(sp.id(), &p.name))
                .map(|lp| (i, lp.value as f32))
        })
        .collect();
    for (i, v) in updates {
        block.set_parameter_value(i, v);
    }
    block
}

/// Find a parameter in a live snapshot by name (case-insensitive substring match).
///
/// Returns the first matching `param_index`, or `None` if not found.
pub fn find_param_index(live: &[LiveParam], name: &str) -> Option<u32> {
    live.iter()
        .find(|p| param_name_matches(name, &p.name))
        .map(|p| p.index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_proto::{Block, BlockParameter};

    fn live(index: u32, name: &str, value: f64) -> LiveParam {
        LiveParam {
            index,
            name: name.to_string(),
            value,
        }
    }

    fn block_with_params(params: &[(&str, &str, f32)]) -> Block {
        Block::from_parameters(
            params
                .iter()
                .map(|(id, label, val)| BlockParameter::new(*id, *label, *val))
                .collect(),
        )
    }

    #[test]
    fn param_name_matches_substring() {
        assert!(param_name_matches("gain", "Amp Gain"));
        assert!(param_name_matches("gain", "Gain"));
        assert!(param_name_matches("gain", "GAIN"));
        assert!(!param_name_matches("gain", "Volume"));
    }

    #[test]
    fn block_to_snapshot_maps_matching_params() {
        let block = block_with_params(&[("gain", "Gain", 0.75), ("bass", "Bass", 0.50)]);
        let live_params = vec![
            live(0, "Amp Gain", 0.0),
            live(1, "Bass EQ", 0.0),
            live(2, "Treble", 0.0),
        ];
        let (snap, count) = block_to_snapshot(&block, &live_params, "jm-amp");
        assert_eq!(count, 2);
        assert_eq!(snap.params.len(), 2);
        // gain → index 0, value = 0.75
        let gain = snap.params.iter().find(|p| p.param_index == 0).unwrap();
        assert!((gain.value - 0.75).abs() < 1e-6);
        // bass → index 1, value = 0.50
        let bass = snap.params.iter().find(|p| p.param_index == 1).unwrap();
        assert!((bass.value - 0.50).abs() < 1e-6);
    }

    #[test]
    fn block_to_snapshot_skips_unmatched() {
        let block = block_with_params(&[("gain", "Gain", 0.75), ("reverb", "Reverb", 0.30)]);
        let live_params = vec![live(0, "Amp Gain", 0.0)]; // no reverb param
        let (snap, count) = block_to_snapshot(&block, &live_params, "fx1");
        assert_eq!(count, 1);
        assert_eq!(snap.params.len(), 1);
    }

    #[test]
    fn live_params_into_block_overwrites_values() {
        let block = block_with_params(&[("gain", "Gain", 0.25), ("bass", "Bass", 0.50)]);
        let live_params = vec![live(0, "Amp Gain", 0.75), live(1, "Bass EQ", 0.60)];
        let updated = live_params_into_block(block, &live_params);
        let params = updated.parameters();
        assert!((params[0].value().get() - 0.75).abs() < 1e-4);
        assert!((params[1].value().get() - 0.60).abs() < 1e-4);
    }

    #[test]
    fn live_params_into_block_leaves_unmatched_unchanged() {
        let block = block_with_params(&[("gain", "Gain", 0.25), ("reverb", "Reverb", 0.80)]);
        let live_params = vec![live(0, "Amp Gain", 0.75)]; // no reverb
        let updated = live_params_into_block(block, &live_params);
        let params = updated.parameters();
        assert!((params[0].value().get() - 0.75).abs() < 1e-4);
        assert!((params[1].value().get() - 0.80).abs() < 1e-4); // unchanged
    }

    #[test]
    fn find_param_index_by_name() {
        let live_params = vec![live(0, "Input Gain", 0.5), live(1, "Bass EQ", 0.3)];
        assert_eq!(find_param_index(&live_params, "gain"), Some(0));
        assert_eq!(find_param_index(&live_params, "bass"), Some(1));
        assert_eq!(find_param_index(&live_params, "treble"), None);
    }
}
