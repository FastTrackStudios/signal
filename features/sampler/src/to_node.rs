//! Lifting the rig's audio tree into the domain — the other direction from
//! [`from_node`](crate::from_node).
//!
//! # Why this is the piece that mattered
//!
//! Both rigs build [`Container`] trees in code: `KeysProfile::build_tree_with`
//! assembles lanes from patches and macro settings, `signal_synth::engine`
//! assembles a synth program from modules. Those builders are where the
//! knowledge lives, and rewriting them to emit [`Node`]s would have meant
//! rewriting the rigs.
//!
//! Lifting means they do not have to be. A builder keeps building a
//! `Container`; this puts it in a [`NodeLibrary`], where it gains stable
//! identity, variants at every level, overrides, persistence and gapless
//! recall — none of which `Container` has — and
//! [`from_node::to_container`](crate::from_node::to_container) renders it
//! back for the audio thread.
//!
//! # Parameters are the whole difficulty
//!
//! A `RigBlock` parameter is a real value in its backend's units; a domain
//! [`BlockParameter`](signal_proto::BlockParameter) is a 0..=1 position plus
//! the range that gives it meaning. The range comes from
//! [`native::range_of`](crate::native::range_of), which reads it off the DSP.
//!
//! Where no range is available — a hosted plugin's parameter, a NAM
//! backend's, a native's parameter that type does not declare — the value is
//! **not** guessed into the unit interval, which would clamp 2.5 Hz to 1.0.
//! It is carried verbatim as a node setting named `param:<name>` and listed
//! in [`LiftReport::unranged`]. Nothing is lost, the round trip is exact, and
//! the report says what to declare to do better.

use signal_proto::block_kind::{BlockKind, HostedPluginRef, IrRef, NamRef, SampleRef};
use signal_proto::model::Block;
use signal_proto::node::{Content, Node, NodeId, NodeLibrary, Variant};
use signal_proto::node_routing::Setting;
use signal_proto::{BlockParameter, ParameterRange};

use crate::rig::RigBlock;
use crate::rig_node::{Container, RigNode};

/// Prefix for a parameter carried as a setting because its range is unknown.
///
/// A prefix rather than a separate field: a lifted block's raw values are
/// still *settings of that node*, and giving them their own channel in the
/// domain would be a second parameter system to keep in step. Defined in
/// `signal-proto` because the resolver has to honour it — a value carried
/// this way is still overridable.
pub use signal_proto::node_routing::RAW_PARAM;

/// The setting a block's explicit module grouping is carried as.
///
/// In the node model grouping is the parent container, so this has no
/// structural equivalent — but the time-bypass reads it, so dropping it
/// would change behaviour.
pub const MODULE_SETTING: &str = "module";

/// What a lift produced.
pub struct Lift {
    /// Every node, flat — the container tree and its leaves.
    pub library: NodeLibrary,
    /// The node the tree hangs from.
    pub root: NodeId,
    pub report: LiftReport,
}

/// What the lift could not express as domain parameters.
///
/// Empty means every value crossed as a ranged parameter. Non-empty is not a
/// failure — those values are carried verbatim and render identically — it is
/// a list of ranges worth declaring.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LiftReport {
    /// `(block name, parameter name)` pairs carried as raw settings.
    pub unranged: Vec<(String, String)>,
}

impl LiftReport {
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.unranged.is_empty()
    }
}

/// Lift a container tree into a node library.
///
/// Ids are preserved: a `Container` or `RigBlock` that already has one keeps
/// it, so lifting the same tree twice names the same nodes and any override
/// or selection addressing them still lands. A node with no id yet is minted
/// one.
#[must_use]
pub fn lift(container: &Container) -> Lift {
    let mut library = NodeLibrary::new();
    let mut report = LiftReport::default();
    let root = lift_container(container, &mut library, &mut report);
    // Routes were authored by name; now that every node is in the library
    // with an id, they can point at ids instead. This is the same pass
    // `Container::canonicalize` runs, on the other side of the boundary.
    library.resolve_refs();
    Lift {
        library,
        root,
        report,
    }
}

fn lift_container(
    container: &Container,
    library: &mut NodeLibrary,
    report: &mut LiftReport,
) -> NodeId {
    let children: Vec<NodeId> = container
        .children
        .iter()
        .map(|child| match child {
            RigNode::Container { container } => lift_container(container, library, report),
            RigNode::Block { block } => lift_block(block, library, report),
        })
        .collect();

    let modulators: Vec<NodeId> = container
        .modulators
        .iter()
        .map(|block| lift_block(block, library, report))
        .collect();

    let default = Variant::new("Default");
    let node = Node {
        id: id_of(&container.id),
        name: container.name.clone(),
        role: container.role,
        combine: container.combine,
        content: Content::Children { nodes: children },
        default_variant: default.id.clone(),
        variants: vec![default],
        // A container tree has no engine type — `Role::Engine` is as much as
        // it ever said. Naming which kind of playable thing an Engine is
        // belongs to whoever authors the library, not to a lift that cannot
        // know.
        engine_type: signal_proto::EngineType::default(),
        input_db: container.input_db,
        output_db: container.output_db,
        modulators,
        sends: container.sends.clone(),
        mod_routes: container.mod_routes.clone(),
        settings: container.params.clone(),
        zone: container.zone,
        bypassed: container.bypassed,
    };
    library.insert(node)
}

/// A [`RigPreset`](crate::rig_library::RigPreset) as a node with one variant
/// per scene — the third preset concept, folded into the first.
///
/// A rig library's preset is a name with several scenes under it, each a
/// whole chain: "Marshall JCM800" holding Clean, Drive and Lead. A `Node`
/// with variants is the same idea, so this is the same collapse the five
/// levels went through — `RigPreset`/`RigScene` were a parallel preset
/// system, with their own default, their own naming and no overrides.
///
/// The shape needs one indirection. A scene is not a *diff* of another scene,
/// it is an independent chain, and a variant may not rewrite its node's child
/// list. So the preset node holds a single slot, each scene is its own chain
/// node, and each variant swaps the slot with a `ReplaceRef`. Recalling a
/// scene is then the ordinary variant switch every other level already has —
/// including gapless, which a `RigScene` never was.
///
/// Returns the library, the preset node, and each scene's variant by name.
#[must_use]
pub fn lift_preset(preset: &crate::rig_library::RigPreset) -> ScenePreset {
    use signal_proto::node::{Combine, Role};
    use signal_proto::overrides::{NodeOverrideOp, NodePath, NodePathSegment, Override};

    let mut library = NodeLibrary::new();
    let mut report = LiftReport::default();

    // Each scene as its own chain node. A scene's trims are the node's input
    // and output level — the same unification a block's trims got.
    let mut scenes = Vec::new();
    for scene in &preset.scenes {
        let mut node = Node::container(&scene.name, Role::Module, Combine::Serial);
        node.input_db = scene.input_trim_db;
        node.output_db = scene.output_trim_db;
        if let Content::Children { nodes } = &mut node.content {
            for block in &scene.chain {
                nodes.push(lift_block(block, &mut library, &mut report));
            }
        }
        scenes.push((scene.name.clone(), library.insert(node)));
    }

    // The preset: one slot, holding its default scene.
    let default_index = preset.default_scene.min(scenes.len().saturating_sub(1));
    let mut node = Node::container(&preset.name, Role::Preset, Combine::Serial);
    let slot = scenes.get(default_index).map(|(_, id)| id.clone());
    if let (Content::Children { nodes }, Some(slot)) = (&mut node.content, slot.as_ref()) {
        nodes.push(slot.clone());
    }

    // One variant per scene. The default scene's variant is the node's own
    // default and needs no override; every other swaps the slot.
    let mut variants = Vec::new();
    for (index, (name, id)) in scenes.iter().enumerate() {
        if index == default_index {
            if let Some(default) = node.variants.first_mut() {
                default.name.clone_from(name);
                variants.push((name.clone(), default.id.clone()));
            }
            continue;
        }
        let mut variant = Variant::new(name);
        if let Some(slot) = slot.as_ref() {
            variant.overrides.push(Override {
                path: NodePath::new(vec![NodePathSegment::Block {
                    id: slot.as_str().to_string(),
                }]),
                op: NodeOverrideOp::ReplaceRef {
                    id: id.as_str().to_string(),
                },
            });
        }
        variants.push((name.clone(), variant.id.clone()));
        node.variants.push(variant);
    }

    let root = library.insert(node);
    ScenePreset {
        library,
        root,
        scenes: variants,
        report,
    }
}

/// A [`RigPreset`](crate::rig_library::RigPreset) in the domain.
pub struct ScenePreset {
    pub library: NodeLibrary,
    /// The preset node. Resolve it with one of [`scenes`](Self::scenes).
    pub root: NodeId,
    /// Scene name → the variant that recalls it.
    pub scenes: Vec<(String, signal_proto::node::VariantId)>,
    pub report: LiftReport,
}

impl ScenePreset {
    /// The variant for a scene by name.
    #[must_use]
    pub fn scene(&self, name: &str) -> Option<&signal_proto::node::VariantId> {
        self.scenes
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v)
    }
}

/// One block as a leaf node, with whatever the lift could not range.
///
/// Public because a rig that composes its own library needs the same
/// per-block conversion without a container to wrap it: the guitar rig's
/// chain is a flat `Vec<RigBlock>` whose slots are then pointed at shared
/// pedal and amp nodes, which only it knows how to do.
#[must_use]
pub fn lift_one_block(block: &RigBlock) -> (Node, LiftReport) {
    let mut library = NodeLibrary::new();
    let mut report = LiftReport::default();
    let id = lift_block(block, &mut library, &mut report);
    let node = library.get(&id).cloned().unwrap_or_else(|| {
        Node::leaf(
            block.display_name(),
            block.block_type,
            Block::new(0.0, 0.0, 0.0),
        )
    });
    (node, report)
}

fn lift_block(block: &RigBlock, library: &mut NodeLibrary, report: &mut LiftReport) -> NodeId {
    let mut parameters = Vec::new();
    let mut settings = Vec::new();

    for param in &block.params {
        let Ok(real) = param.value.trim().parse::<f32>() else {
            // Not a number at all — an IR path, a file name. Those are
            // settings by nature, not knob positions.
            settings.push(Setting {
                name: format!("{RAW_PARAM}{}", param.name),
                value: param.value.clone(),
            });
            continue;
        };
        if let Some(range) = range_for(block, &param.name) {
            parameters.push(BlockParameter::ranged(
                &param.name,
                &param.name,
                real,
                range,
            ));
        } else {
            report
                .unranged
                .push((block.display_name(), param.name.clone()));
            settings.push(Setting {
                name: format!("{RAW_PARAM}{}", param.name),
                value: param.value.clone(),
            });
        }
    }

    if !block.module.is_empty() {
        settings.push(Setting {
            name: MODULE_SETTING.to_string(),
            value: block.module.clone(),
        });
    }

    let mut domain = Block::with_exact_parameters(parameters);
    domain.kind = realization(block);

    let default = Variant::new("Default");
    let node = Node {
        id: id_of(&block.id),
        name: block.display_name(),
        role: signal_proto::node::Role::Module,
        combine: signal_proto::node::Combine::Serial,
        content: Content::Leaf {
            block_type: block.block_type,
            block: domain,
        },
        default_variant: default.id.clone(),
        variants: vec![default],
        engine_type: signal_proto::EngineType::default(),
        // A NAM block's trims are exactly a leaf's input and output level.
        // The five-level model needed a separate pair of fields for them
        // because a block was not a node; here it is one.
        input_db: block.input_trim_db,
        output_db: block.output_trim_db,
        modulators: Vec::new(),
        sends: Vec::new(),
        mod_routes: Vec::new(),
        settings,
        zone: signal_proto::node_routing::Zone::full(),
        bypassed: block.bypassed,
    };
    library.insert(node)
}

/// The range for one of a block's parameters, if anything knows it.
///
/// Only a natively-realized block has declarable ranges: they come from its
/// own DSP. A hosted plugin's parameters are the plugin's, readable only once
/// it is loaded from disk, and a lift does not load plugins.
fn range_for(block: &RigBlock, param: &str) -> Option<ParameterRange> {
    if !block.is_native() {
        return None;
    }
    crate::native::range_of(block.block_type, param)
}

/// How this block is realized, in the domain's terms.
///
/// Order matters where a block carries more than one asset path: a NAM amp
/// with a cabinet IR is realized by its model, and the IR rides along as the
/// cabinet's own block in the chain.
fn realization(block: &RigBlock) -> BlockKind {
    if !block.nam.is_empty() {
        return BlockKind::Nam {
            model: NamRef {
                model_path: block.nam.clone(),
                model_id: None,
            },
        };
    }
    if !block.ir.is_empty() {
        return BlockKind::ImpulseResponse {
            ir: IrRef {
                path: block.ir.clone(),
            },
        };
    }
    if !block.plugin.is_empty() {
        return BlockKind::HostedPlugin {
            plugin: HostedPluginRef {
                // The loader detects the format from the path; a lift has no
                // better answer and inventing "Clap" would be a claim.
                format: String::new(),
                path: block.plugin.clone(),
                state_b64: block.state_b64.clone(),
            },
        };
    }
    if !block.sample.is_empty() {
        return BlockKind::Sample {
            sample: SampleRef {
                spec_path: block.sample.clone(),
                samples_root: block.samples_root.clone(),
                section: block.sample_section.clone(),
                mic: block.sample_mic.clone(),
            },
        };
    }
    BlockKind::Native
}

/// The node id for an audio-side id string, minting one if it is empty.
fn id_of(id: &str) -> NodeId {
    if id.is_empty() {
        NodeId::new()
    } else {
        NodeId::from(id.to_string())
    }
}
