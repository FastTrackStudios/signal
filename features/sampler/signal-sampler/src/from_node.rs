//! The domain's resolved tree, rendered as the rig's audio chain.
//!
//! This is the bridge from [`signal_proto::node_resolve::Resolved`] to
//! [`RigBlock`] — the boundary where a decided tree becomes something the
//! sampler plays.
//!
//! # Why this exists when `RigProfile::from_proto` already did
//!
//! It didn't, quite. `from_proto` walked a `PatchResolver` into
//! `ResolvedRigBlock`, which carried a `BlockKind` and an optional cabinet IR
//! path — and nothing else. Block names, parameters, bypass state and the
//! block's own type were all dropped, and a natively-realized block with no
//! IR was skipped outright with a log line:
//!
//! ```text
//! from_proto: skipping block — not supported in the standalone rig yet
//! ```
//!
//! So the worship rig's Amp EQ, with its five bands, could not survive the
//! trip; nor could a bypassed delay, nor a compressor's threshold. That is
//! why the guitar rig never used it and built its chains directly instead.
//!
//! This conversion is lossless by construction: everything a `RigBlock` can
//! hold is read off the resolved node, and nothing is skipped.
//!
//! # Units cross correctly
//!
//! A `RigBlock` parameter is a raw string in the DSP's own units — the chain
//! builder writes `80` for 80 Hz and `-40` for −40 dB. The domain stores a
//! *normalized* position, because that is what automation, modulation and
//! MIDI learn all want.
//!
//! Each parameter carries a `ParameterRange` saying what its position means,
//! so this boundary denormalizes: `BlockParameter::real()` is the value the
//! DSP is handed. Before ranges existed the two halves disagreed silently —
//! a band frequency of 5500 clamped to 1.0 on the way in, and nothing said
//! so.

use signal_proto::block_kind::BlockKind;
use signal_proto::node_resolve::{Resolved, ResolvedContent};

use crate::rig::RigBlock;

/// Flatten a resolved tree into the ordered chain the rig renders.
///
/// Leaves in depth-first order, which for a serial tree is signal order.
///
/// Parallel containers are flattened along with everything else for now — the
/// `RigBlock` chain is a list, and expressing a sum needs the graph the keys
/// rig uses. A guitar chain is serial throughout, so this is exact for it and
/// approximate for anything that sums.
#[must_use]
pub fn to_chain(resolved: &Resolved) -> Vec<RigBlock> {
    resolved.leaves().into_iter().map(to_block).collect()
}

/// One resolved leaf as a `RigBlock`, carrying everything it holds.
#[must_use]
pub fn to_block(leaf: &Resolved) -> RigBlock {
    let ResolvedContent::Leaf { block_type, block } = &leaf.content else {
        // `leaves()` only yields leaves; a container here would be a bug in
        // the traversal, not in the data.
        return RigBlock::of_type(signal_proto::block::BlockType::default());
    };

    let mut rb = RigBlock::of_type(*block_type).named(&leaf.name);
    // The node's identity travels with it, so an override or a selection that
    // addressed this block upstream still addresses it here.
    rb.id = leaf.id.as_str().to_string();

    // How the block is realized. `Native` needs nothing: its parameters are
    // its realization, and they are copied below.
    match &block.kind {
        BlockKind::Nam(nam) => rb = rb.with_nam(nam.model_path.clone()),
        BlockKind::HostedPlugin(plugin) => {
            rb = RigBlock::plugin_with_state(plugin.path.clone(), plugin.state_b64.clone())
                .named(&leaf.name);
            rb.id = leaf.id.as_str().to_string();
            rb.block_type = *block_type;
        }
        BlockKind::Native | BlockKind::Custom(_) => {}
    }

    // Every parameter, by the id the DSP knows it by, denormalized through
    // its range. This is the half the old bridge dropped entirely.
    for param in block.parameters() {
        rb = rb.with_param(param.id(), param.real().to_string());
    }

    // Bypass is resolved state, not authored state: it may have been set by
    // an override from any level above this leaf.
    rb.bypassed = leaf.bypassed;
    rb
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_proto::{BlockParameter, ParameterRange, Unit};
    use signal_proto::block::BlockType;
    use signal_proto::block_kind::NamRef;
    use signal_proto::model::Block;
    use signal_proto::node::{Combine, Node, NodeLibrary, Role, Variant};
    use signal_proto::node_resolve::resolve;
    use signal_proto::overrides::{NodePath, NodePathSegment, Override};

    /// The worship Amp EQ's magic frequencies, in Hz, on a logarithmic range
    /// — which is how a frequency control has to behave to be playable.
    fn eq_block() -> Block {
        let audio = || ParameterRange::logarithmic(20.0, 20_000.0, Unit::Hz);
        Block::from_parameters(vec![
            BlockParameter::ranged("b1_freq", "Low cut", 80.0, audio()),
            BlockParameter::ranged("b2_freq", "Body", 212.0, audio()),
            BlockParameter::ranged("b3_freq", "Character", 560.0, audio()),
            BlockParameter::ranged("b4_freq", "Honk", 1400.0, audio()),
            BlockParameter::ranged("b5_freq", "Presence", 5500.0, audio()),
        ])
    }

    fn approx(a: Option<f32>, b: f32) -> bool {
        a.is_some_and(|v| (v - b).abs() < b * 0.001)
    }

    /// The case the old bridge could not survive: a natively-realized block
    /// with parameters and no IR path. It was skipped outright.
    #[test]
    fn a_native_block_keeps_its_name_type_and_every_parameter() {
        let mut lib = NodeLibrary::new();
        let eq = Node::leaf("Amp EQ", BlockType::Eq, eq_block());
        let chain = Node::container("P", Role::Preset, Combine::Serial).with_child(&eq);
        let root = chain.id.clone();
        lib.insert(eq);
        lib.insert(chain);

        let (resolved, _) = resolve(&lib, &root, None).expect("resolves");
        let chain = to_chain(&resolved);

        assert_eq!(chain.len(), 1, "a native block is not skipped");
        let block = &chain[0];
        assert_eq!(block.name, "Amp EQ");
        assert_eq!(block.block_type, BlockType::Eq);
        // Real Hz reach the DSP, not a normalized 0..1 position. Before
        // ranges, 5500 clamped to 1.0 on the way in and nothing said so.
        assert!(
            approx(block.param_f32("b5_freq"), 5500.0),
            "presence band should arrive as 5500 Hz, got {:?}",
            block.param_f32("b5_freq")
        );
        assert!(approx(block.param_f32("b1_freq"), 80.0));
        assert_eq!(block.params.len(), 5, "all five bands, not none");
        assert!(!block.id.is_empty(), "identity travels with the block");
    }

    #[test]
    fn a_nam_capture_keeps_its_model_path() {
        let mut lib = NodeLibrary::new();
        let mut block = Block::from_parameters(Vec::new());
        block.kind = BlockKind::Nam(NamRef {
            model_path: "models/AC30.nam".into(),
            model_id: None,
        });
        let amp = Node::leaf("AC30", BlockType::Amp, block);
        let chain = Node::container("P", Role::Preset, Combine::Serial).with_child(&amp);
        let root = chain.id.clone();
        lib.insert(amp);
        lib.insert(chain);

        let (resolved, _) = resolve(&lib, &root, None).expect("resolves");
        let chain = to_chain(&resolved);
        assert_eq!(chain[0].nam, "models/AC30.nam");
        assert_eq!(chain[0].block_type, BlockType::Amp);
        assert!(chain[0].is_nam());
    }

    /// Bypass is resolved state — set from above, and it has to arrive.
    #[test]
    fn a_bypass_from_an_override_reaches_the_audio_block() {
        let mut lib = NodeLibrary::new();
        let gate = Node::leaf("Gate", BlockType::Gate, Block::from_parameters(Vec::new()));
        let mut edge = Variant::new("Edge");
        // "Crunch Edge" turns the gate off so every rattle rings.
        edge.overrides.push(Override::bypass(
            NodePath::new(vec![NodePathSegment::Block { id: "Gate".into() }]),
            true,
        ));
        let edge_id = edge.id.clone();
        let chain = Node::container("P", Role::Preset, Combine::Serial)
            .with_child(&gate)
            .with_variant(edge);
        let root = chain.id.clone();
        lib.insert(gate);
        lib.insert(chain);

        let (plain, _) = resolve(&lib, &root, None).expect("resolves");
        assert!(!to_chain(&plain)[0].bypassed);

        let (edged, report) = resolve(&lib, &root, Some(&edge_id)).expect("resolves");
        assert!(report.is_clean());
        assert!(
            to_chain(&edged)[0].bypassed,
            "an override from the patch reaches the rendered block"
        );
    }

    /// Order is signal order.
    #[test]
    fn the_chain_comes_out_in_the_order_it_is_played() {
        let mut lib = NodeLibrary::new();
        let comp = Node::leaf("Comp", BlockType::Compressor, Block::from_parameters(vec![]));
        let amp = Node::leaf("Amp", BlockType::Amp, Block::from_parameters(vec![]));
        let verb = Node::leaf("Verb", BlockType::Reverb, Block::from_parameters(vec![]));
        // A nested module, to prove depth-first is signal order.
        let time = Node::container("Time", Role::Module, Combine::Serial).with_child(&verb);
        let chain = Node::container("P", Role::Preset, Combine::Serial)
            .with_child(&comp)
            .with_child(&amp)
            .with_child(&time);
        let root = chain.id.clone();
        for n in [comp, amp, verb] {
            lib.insert(n);
        }
        lib.insert(time);
        lib.insert(chain);

        let (resolved, _) = resolve(&lib, &root, None).expect("resolves");
        let names: Vec<_> = to_chain(&resolved)
            .into_iter()
            .map(|b| b.name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["Comp", "Amp", "Verb"]);
    }
}
