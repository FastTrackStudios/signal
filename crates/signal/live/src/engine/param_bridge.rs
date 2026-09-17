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

    /// The piece a DAW applier actually needed from the domain, and the
    /// reason #11 read as untestable: it is a pure function.
    ///
    /// An imported patch is one leaf realized by the host's own saved chain,
    /// and this hands the applier that document.
    #[test]
    fn a_host_chain_becomes_the_applier_chunk() {
        use signal_proto::block::BlockType;
        use signal_proto::block_kind::{BlockKind, HostChainRef};
        use signal_proto::model::Block;
        use signal_proto::node::{Node, NodeLibrary};
        use signal_proto::node_resolve::resolve;

        let document = "<FXCHAIN\n  WNDRECT 0 0 0 0\n>\n";
        let mut block = Block::with_exact_parameters(Vec::new());
        block.kind = BlockKind::HostChain {
            chain: HostChainRef::new("reaper", document).labelled("Clean"),
        };

        let mut library = NodeLibrary::new();
        let leaf = Node::leaf("Imported chain", BlockType::Amp, block);
        let root = leaf.id.clone();
        library.insert(leaf);

        let (resolved, _) = resolve(&library, &root, None).expect("resolves");
        let chunks = node_state_chunks(&resolved, "fx-1", "reaper");

        assert_eq!(chunks.len(), 1);
        let chunk = &chunks[0];
        assert_eq!(chunk.fx_id, "fx-1");
        assert_eq!(chunk.plugin_name, "Clean", "the chain's label names the FX");
        assert_eq!(chunk.block_type, BlockType::Amp);
        assert_eq!(
            String::from_utf8(chunk.chunk_data.clone()).as_deref(),
            Ok(document),
            "the document reaches the applier byte for byte"
        );
    }

    /// A chain another host saved is skipped, not handed over. Loading a
    /// foreign document is how a patch reports success and produces silence.
    #[test]
    fn a_chain_from_another_host_is_skipped() {
        use signal_proto::block::BlockType;
        use signal_proto::block_kind::{BlockKind, HostChainRef};
        use signal_proto::model::Block;
        use signal_proto::node::{Node, NodeLibrary};
        use signal_proto::node_resolve::resolve;

        let mut block = Block::with_exact_parameters(Vec::new());
        block.kind = BlockKind::HostChain {
            chain: HostChainRef::new("ableton", "<something else>"),
        };
        let mut library = NodeLibrary::new();
        let leaf = Node::leaf("Foreign", BlockType::Amp, block);
        let root = leaf.id.clone();
        library.insert(leaf);

        let (resolved, _) = resolve(&library, &root, None).expect("resolves");
        assert!(node_state_chunks(&resolved, "fx-1", "reaper").is_empty());
    }

    /// A rig built rather than imported carries no host chain, so there is
    /// nothing to splice — the applier's cold path says so instead of
    /// loading an empty one.
    #[test]
    fn a_native_rig_yields_no_chunks() {
        use signal_proto::block::BlockType;
        use signal_proto::model::Block;
        use signal_proto::node::{Node, NodeLibrary};
        use signal_proto::node_resolve::resolve;

        let mut library = NodeLibrary::new();
        let leaf = Node::leaf("Amp", BlockType::Amp, Block::new(0.5, 0.5, 0.5));
        let root = leaf.id.clone();
        library.insert(leaf);

        let (resolved, _) = resolve(&library, &root, None).expect("resolves");
        assert!(node_state_chunks(&resolved, "fx-1", "reaper").is_empty());
    }

    /// Chunks come out in chain order, because the applier takes the first
    /// and a rig with two imported chains must not get whichever the walk
    /// happened to reach.
    #[test]
    fn chunks_follow_chain_order() {
        use signal_proto::block::BlockType;
        use signal_proto::block_kind::{BlockKind, HostChainRef};
        use signal_proto::model::Block;
        use signal_proto::node::{Combine, Node, NodeLibrary, Role};
        use signal_proto::node_resolve::resolve;

        let chain_leaf = |name: &str| {
            let mut block = Block::with_exact_parameters(Vec::new());
            block.kind = BlockKind::HostChain {
                chain: HostChainRef::new("reaper", "<FXCHAIN>").labelled(name),
            };
            Node::leaf(name, BlockType::Amp, block)
        };

        let first = chain_leaf("First");
        let second = chain_leaf("Second");
        let root_node = Node::container("Rig", Role::Preset, Combine::Serial)
            .with_child(&first)
            .with_child(&second);
        let root = root_node.id.clone();

        let mut library = NodeLibrary::new();
        for node in [first, second, root_node] {
            library.insert(node);
        }

        let (resolved, _) = resolve(&library, &root, None).expect("resolves");
        let names: Vec<String> = node_state_chunks(&resolved, "fx-1", "reaper")
            .into_iter()
            .map(|c| c.plugin_name)
            .collect();
        assert_eq!(names, vec!["First".to_string(), "Second".to_string()]);
    }
}
