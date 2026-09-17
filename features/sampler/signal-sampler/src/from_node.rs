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
use crate::rig_node::{Container, RigNode};

/// Flatten a resolved tree into the ordered chain the rig renders.
///
/// Leaves in depth-first order, which for a serial tree is signal order.
///
/// Parallel containers are flattened along with everything else, because a
/// `Vec<RigBlock>` is a list and a list cannot express a sum. That is exact
/// for a guitar chain, which is serial throughout, and wrong for anything
/// that sums — use [`to_container`] there, which keeps the tree.
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
        BlockKind::Nam { model } => rb = rb.with_nam(model.model_path.clone()),
        BlockKind::HostedPlugin { plugin } => {
            rb = RigBlock::plugin_with_state(plugin.path.clone(), plugin.state_b64.clone())
                .named(&leaf.name);
            rb.id = leaf.id.as_str().to_string();
            rb.block_type = *block_type;
        }
        BlockKind::ImpulseResponse { ir } => rb.ir = ir.path.clone(),
        BlockKind::HostChain { chain } => {
            // A chain saved by a DAW, in that DAW's format. This engine has
            // no way to realize one — the plugins in it are that host's, and
            // so is the document — so the block renders as a pass-through.
            //
            // Pass-through rather than silence on purpose: an imported patch
            // reaching the standalone rig should let the guitar through
            // unprocessed, not mute it. The warning is how you find out why
            // the tone is missing rather than wondering.
            tracing::warn!(
                block.name = %leaf.name,
                chain.host = %chain.host,
                "signal: a host's saved chain cannot be realized here — passing audio through"
            );
        }
        BlockKind::Sample { sample } => {
            rb.sample = sample.spec_path.clone();
            rb.samples_root = sample.samples_root.clone();
            rb.sample_section = sample.section.clone();
            rb.sample_mic = sample.mic.clone();
        }
        BlockKind::Native | BlockKind::Custom { .. } => {}
    }

    // Every parameter, by the id the DSP knows it by, denormalized through
    // its range. This is the half the old bridge dropped entirely.
    for param in block.parameters() {
        rb = rb.with_param(param.id(), param.real().to_string());
    }

    // Values a lift could not range are carried as settings rather than
    // clamped into the unit interval — see `to_node`. They go back exactly
    // as they came, which is what makes that trade lossless.
    for setting in &leaf.settings {
        if let Some(name) = setting.name.strip_prefix(crate::to_node::RAW_PARAM) {
            rb = rb.with_param(name, setting.value.clone());
        } else if setting.name == crate::to_node::MODULE_SETTING {
            rb.module = setting.value.clone();
        }
    }

    // A leaf's input and output level ARE a block's trims. The five-level
    // model needed a separate pair of fields because a block was not a node.
    rb.input_trim_db = leaf.input_db;
    rb.output_trim_db = leaf.output_db;

    // Bypass is resolved state, not authored state: it may have been set by
    // an override from any level above this leaf.
    rb.bypassed = leaf.bypassed;
    rb
}

/// A resolved tree as the rig's container tree — the whole node, not a list
/// of its leaves.
///
/// [`to_chain`] loses three things a keys rig cannot do without: the shape
/// (a parallel container flattens into its serial siblings and the lanes
/// stop summing), the per-node state (faders, zones, settings), and the
/// routing axis. This keeps all of it, so a `Node` can say everything a
/// hand-built `Container` says.
///
/// Sends and mod routes cross verbatim — both sides hold
/// [`signal_proto::node_routing`] types now, addressed the same way. They
/// used to be translated here, from the domain's ids to the renderer's
/// display names, which is exactly the translation that made a rename able
/// to break a route.
#[must_use]
pub fn to_container(resolved: &Resolved) -> Container {
    let children = match &resolved.content {
        ResolvedContent::Leaf { .. } => vec![RigNode::Block {
            block: to_block(resolved),
        }],
        ResolvedContent::Children(children) => children
            .iter()
            .map(|child| match &child.content {
                ResolvedContent::Leaf { .. } => RigNode::Block {
                    block: to_block(child),
                },
                ResolvedContent::Children(_) => RigNode::Container {
                    container: to_container(child),
                },
            })
            .collect(),
    };

    Container {
        id: resolved.id.as_str().to_string(),
        role: resolved.role,
        name: resolved.name.clone(),
        combine: resolved.combine,
        input_db: resolved.input_db,
        output_db: resolved.output_db,
        children,
        modulators: resolved.modulators.iter().map(to_block).collect(),
        sends: resolved.sends.clone(),
        mod_routes: resolved.mod_routes.clone(),
        params: resolved.settings.clone(),
        zone: resolved.zone,
        bypassed: resolved.bypassed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_proto::block::BlockType;
    use signal_proto::block_kind::NamRef;
    use signal_proto::model::Block;
    use signal_proto::node::{Combine, Node, NodeLibrary, Role, Variant};
    use signal_proto::node_resolve::resolve;
    use signal_proto::node_routing::{AudioSend, ModRoute, ModSource, NodeRef, Setting, Zone};
    use signal_proto::overrides::{NodePath, NodePathSegment, Override};
    use signal_proto::{BlockParameter, ParameterRange, Unit};

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
        block.kind = BlockKind::Nam {
            model: NamRef {
                model_path: "models/AC30.nam".into(),
                model_id: None,
            },
        };
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
        let comp = Node::leaf(
            "Comp",
            BlockType::Compressor,
            Block::from_parameters(vec![]),
        );
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

    /// The shape `to_chain` cannot express: a parallel container's lanes sum,
    /// and flattening them into a list turns a stack of sounds into a
    /// chain of them.
    #[test]
    fn a_parallel_container_keeps_its_shape() {
        let mut lib = NodeLibrary::new();
        let rhodes = Node::leaf("Rhodes", BlockType::Sampler, Block::new(0.0, 0.0, 0.0));
        let pad = Node::leaf("Pad", BlockType::Sampler, Block::new(0.0, 0.0, 0.0));
        let voices = Node::container("Voices", Role::Module, Combine::Parallel)
            .with_child(&rhodes)
            .with_child(&pad);
        let engine = Node::container("Keys", Role::Engine, Combine::Serial).with_child(&voices);

        let root = engine.id.clone();
        for n in [rhodes, pad, voices, engine] {
            lib.insert(n);
        }

        let (resolved, report) = resolve(&lib, &root, None).expect("resolves");
        assert!(report.is_clean());

        let container = to_container(&resolved);
        assert_eq!(container.combine, Combine::Serial, "the Engine chains");
        let [RigNode::Container { container: voices }] = &container.children[..] else {
            panic!("the Engine holds one container, not a flattened list");
        };
        assert_eq!(voices.combine, Combine::Parallel, "the lanes sum");
        assert_eq!(voices.children.len(), 2);

        // What the list-shaped conversion does with the same tree.
        assert_eq!(
            to_chain(&resolved).len(),
            2,
            "to_chain yields the leaves, with nothing left saying they sum"
        );
    }

    /// The fields that made a keys rig inexpressible as nodes: the fader, the
    /// key split, the layer settings.
    #[test]
    fn the_fader_zone_and_settings_survive() {
        let mut lib = NodeLibrary::new();
        let mut lane = Node::container("Lower", Role::Layer, Combine::Serial)
            .in_zone(Zone::keys(0, 59))
            .at_db(-3.0);
        lane.settings.push(Setting {
            name: "voice_mode".into(),
            value: "poly".into(),
        });
        let root = lane.id.clone();
        lib.insert(lane);

        let (resolved, _) = resolve(&lib, &root, None).expect("resolves");
        let container = to_container(&resolved);

        assert_eq!(container.output_db, -3.0);
        assert_eq!(container.zone.key_hi, 59, "the split is a hard C4 boundary");
        assert_eq!(container.zone.note_gain(72, 100), 0.0);
        assert_eq!(
            container.params.first().map(|p| p.value.as_str()),
            Some("poly")
        );
    }

    /// A route reaches the renderer still holding the id it was authored
    /// with. Nothing translates it back to a name on the way.
    #[test]
    fn routes_keep_their_ids_across_the_boundary() {
        let mut lib = NodeLibrary::new();
        let filter = Node::leaf("Filter", BlockType::Filter, Block::new(0.0, 0.0, 0.0));
        let rotary = Node::container("Rotary", Role::Module, Combine::Serial);
        let mut lane = Node::container("Organ", Role::Layer, Combine::Serial)
            .with_child(&filter)
            .with_child(&rotary);
        let rotary_id = rotary.id.clone();
        let filter_id = filter.id.clone();
        lane.sends
            .push(AudioSend::new(NodeRef::id(rotary_id.clone()), "To Rotary"));
        lane.mod_routes.push(ModRoute {
            source: ModSource::performance("Wheel"),
            target: NodeRef::id(filter_id.clone()),
            parameter: "cutoff".into(),
            depth: 0.5,
        });

        let root = lane.id.clone();
        for n in [filter, rotary, lane] {
            lib.insert(n);
        }

        let (resolved, _) = resolve(&lib, &root, None).expect("resolves");
        let container = to_container(&resolved);

        assert_eq!(container.sends[0].target.as_id(), Some(&rotary_id));
        assert_eq!(
            container.mod_routes[0].source,
            ModSource::performance("Wheel")
        );
        assert_eq!(container.mod_routes[0].target.as_id(), Some(&filter_id));
        assert_eq!(container.mod_routes[0].parameter, "cutoff");
    }

    /// The realization the domain was missing. A keys lane is a sampler, and
    /// before `BlockKind::Sample` a `Node` had no way to say which library,
    /// section and mic — so a keys rig could not be expressed as nodes at
    /// all, however many other fields were added.
    #[test]
    fn a_sample_realization_renders_as_a_sampler_block() {
        use signal_proto::block_kind::{BlockKind, SampleRef};

        let mut block = Block::new(0.0, 0.0, 0.0);
        block.kind = BlockKind::Sample {
            sample: SampleRef {
                spec_path: "keyscape/library.styx".into(),
                samples_root: "/packs/keyscape".into(),
                section: "1v".into(),
                mic: "Mix".into(),
            },
        };

        let mut lib = NodeLibrary::new();
        let lane = Node::leaf("Rhodes", BlockType::Sampler, block);
        let root = lane.id.clone();
        lib.insert(lane);

        let (resolved, _) = resolve(&lib, &root, None).expect("resolves");
        let rb = to_block(&resolved);

        assert_eq!(rb.sample, "keyscape/library.styx");
        assert_eq!(rb.samples_root, "/packs/keyscape");
        assert_eq!(rb.sample_section, "1v", "which section of the library");
        assert_eq!(rb.sample_mic, "Mix", "and which mic position");
    }

    /// Pins the one thing that stops a hand-built `Container` from being
    /// lifted into a `NodeLibrary` wholesale.
    ///
    /// A `RigBlock` parameter is a real value in the DSP's units — the rig's
    /// own param path takes dB, Hz and ms and normalizes against the
    /// backend's declared min/max. A domain `BlockParameter` is a 0..=1 knob
    /// position plus the range that gives it meaning, and that invariant is
    /// right: it is what automation, modulation and MIDI learn all want.
    ///
    /// So lifting `("rate", "2.5")` — 2.5 Hz — into a parameter whose range
    /// nobody declared clamps it to 1.0, silently. The range is not missing
    /// by oversight: it lives in the backend, readable only once the block
    /// is instantiated. A lift is therefore not a pure data transform, and
    /// writing one that guesses would reintroduce exactly the bug ranges
    /// were added to fix.
    #[test]
    fn an_unranged_parameter_cannot_hold_a_real_value() {
        let unranged = BlockParameter::new("rate", "Rate", 2.5);
        assert_eq!(
            unranged.real(),
            1.0,
            "2.5 Hz clamped to the unit interval — the lift's blocker"
        );

        // With the range declared, the same value survives.
        let ranged = BlockParameter::ranged(
            "rate",
            "Rate",
            2.5,
            ParameterRange::logarithmic(0.01, 40.0, Unit::Hz),
        );
        assert!((ranged.real() - 2.5).abs() < 0.01, "got {}", ranged.real());
    }
}
