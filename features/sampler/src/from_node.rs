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
