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

use signal_proto::{Block, resolve::ResolvedGraph};

use super::daw_bridge::DawStateChunk;
use super::morph::{DawParamValue, DawParameterSnapshot};

/// A single DAW parameter as seen by the bridge: just a name and a value.
/// Callers provide a slice of these; the actual type (`FxParameter`, etc.)
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
#[must_use]
pub fn param_name_matches(signal_id: &str, daw_name: &str) -> bool {
    let norm_daw: String = daw_name
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect();
    let norm_sig: String = signal_id
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect();
    norm_daw.contains(&norm_sig)
}

/// Build a [`DawParameterSnapshot`] by mapping a [`Block`]'s parameters onto
/// a slice of live DAW parameters.
///
/// Only parameters that match by name are included. Returns the snapshot and
/// the count of parameters that were matched and applied.
#[must_use]
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
                value: f64::from(sp.value().get()),
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
#[must_use]
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
                                value: f64::from(sp.value().get()),
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
/// data from any `ResolvedBlock` that carries `state_data`. Each chunk is
/// tagged with the provided `fx_id` and the block's label.
///
/// Returns an empty `Vec` when no blocks carry state data (the normal case
/// for rig-based resolution). For `BlockSnapshot` targets pointing at catalog
/// presets with `.bin` files, this returns one chunk per block.
#[must_use]
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

/// Extract [`DawStateChunk`]s from a **resolved node tree** — the node-side
/// counterpart of [`graph_state_chunks`].
///
/// Walks the tree's leaves and collects every block realized by
/// [`BlockKind::HostChain`]: a chain the host itself saved, which is what an
/// imported rig's patch is. Each chunk carries the document as bytes, tagged
/// with `fx_id` and the chain's label.
///
/// `host` filters to the host that can actually realize the document —
/// `"reaper"` for the REAPER applier. A chain saved by another host is
/// skipped rather than handed over, because a foreign document is not a
/// chain that host can load and pretending otherwise is how you get a patch
/// that reports success and produces silence.
///
/// This is the whole of what a DAW applier needed from the domain. The rest
/// of `reaper_applier::apply_graph` — the preload fast path, tail tracks,
/// delayed mutes, track creation, chunk splicing — never looked at the
/// resolved model at all.
#[must_use]
pub fn node_state_chunks(
    resolved: &signal_proto::node_resolve::Resolved,
    fx_id: &str,
    host: &str,
) -> Vec<DawStateChunk> {
    use signal_proto::block_kind::BlockKind;
    use signal_proto::node_resolve::ResolvedContent;

    let mut chunks = Vec::new();
    for leaf in resolved.leaves() {
        let ResolvedContent::Leaf { block_type, block } = &leaf.content else {
            continue;
        };
        let BlockKind::HostChain { chain } = &block.kind else {
            continue;
        };
        if !chain.host.eq_ignore_ascii_case(host) {
            tracing::warn!(
                node.name = %leaf.name,
                chain.host = %chain.host,
                applier.host = %host,
                "signal: host chain skipped — saved by a different host"
            );
            continue;
        }
        let Some(document) = chain.document() else {
            tracing::warn!(
                node.name = %leaf.name,
                "signal: host chain skipped — document did not decode"
            );
            continue;
        };
        chunks.push(DawStateChunk {
            fx_id: fx_id.to_string(),
            plugin_name: if chain.label.is_empty() {
                leaf.name.clone()
            } else {
                chain.label.clone()
            },
            block_type: *block_type,
            chunk_data: document.into_bytes(),
        });
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
#[must_use]
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
#[must_use]
pub fn find_param_index(live: &[LiveParam], name: &str) -> Option<u32> {
    live.iter()
        .find(|p| param_name_matches(name, &p.name))
        .map(|p| p.index)
}
