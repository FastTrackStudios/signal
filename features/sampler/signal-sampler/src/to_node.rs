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
/// domain would be a second parameter system to keep in step.
pub const RAW_PARAM: &str = "param:";

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
                default.name = name.clone();
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
        match range_for(block, &param.name) {
            Some(range) => {
                parameters.push(BlockParameter::ranged(
                    &param.name,
                    &param.name,
                    real,
                    range,
                ));
            }
            None => {
                report
                    .unranged
                    .push((block.display_name().to_string(), param.name.clone()));
                settings.push(Setting {
                    name: format!("{RAW_PARAM}{}", param.name),
                    value: param.value.clone(),
                });
            }
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
        name: block.display_name().to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use signal_proto::block::BlockType;
    use signal_proto::node_resolve::resolve;

    use crate::from_node::{to_chain, to_container};

    /// Lift, resolve, render — and the tree that comes out is the tree that
    /// went in. `dump()` is the comparison because it shows everything the
    /// renderer reads: shape, roles, combines, zones, settings, modulators
    /// and sends, at every level.
    #[test]
    fn a_tree_survives_the_round_trip_through_the_domain() {
        let mut tree = Container::preset("Worship")
            .add(
                Container::module("Drive board")
                    .add(Container::module("Comp").block(BlockType::Compressor, "Compressor"))
                    .block(BlockType::Drive, "Drive"),
            )
            .add(Container::module("Amp").block(BlockType::Amp, "AC30"))
            .add(Container::module("Rotary"))
            .send("Rotary", "To Rotary")
            .volume(-3.0);
        tree.canonicalize();
        let before = tree.dump();

        let lifted = lift(&tree);
        let (resolved, report) = resolve(&lifted.library, &lifted.root, None).expect("resolves");
        assert!(report.is_clean(), "{report:?}");

        let after = to_container(&resolved).dump();
        assert_eq!(before, after, "the tree changed crossing the domain");
    }

    /// The point of the range source: a real value goes in and the same real
    /// value comes out, rather than the 1.0 an unranged parameter clamps to.
    #[test]
    fn a_real_parameter_value_survives_the_round_trip() {
        let mut tree = Container::layer("L").extend(vec![RigNode::Block {
            block: RigBlock::of_type(BlockType::Compressor)
                .named("Comp")
                .with_param("threshold", "-18")
                .with_param("attack", "10")
                .with_param("ratio", "4"),
        }]);
        tree.canonicalize();

        let lifted = lift(&tree);
        assert!(
            lifted.report.is_clean(),
            "the compressor's parameters are all declared: {:?}",
            lifted.report
        );

        let (resolved, _) = resolve(&lifted.library, &lifted.root, None).expect("resolves");
        let back = to_container(&resolved);
        let RigNode::Block { block } = &back.children[0] else {
            panic!("the leaf came back as a container");
        };

        for (name, want) in [("threshold", -18.0), ("attack", 10.0), ("ratio", 4.0)] {
            let got = block
                .param_f32(name)
                .unwrap_or_else(|| panic!("{name} kept"));
            assert!(
                (got - want).abs() < want.abs() * 0.01 + 0.01,
                "{name}: {want} came back as {got}"
            );
        }
    }

    /// A value whose range nobody can know is carried verbatim and reported,
    /// not clamped into the unit interval. This is the honest half of the
    /// lift — and the round trip still has to be exact.
    #[test]
    fn an_unrangeable_value_is_carried_and_reported() {
        let mut tree = Container::layer("L").extend(vec![RigNode::Block {
            block: RigBlock::plugin("/plugins/Amp.clap")
                .named("Hosted Amp")
                // 2.5 of something. Only the plugin knows of what.
                .with_param("rate", "2.5"),
        }]);
        tree.canonicalize();

        let lifted = lift(&tree);
        assert_eq!(
            lifted.report.unranged,
            vec![("Hosted Amp".to_string(), "rate".to_string())],
            "the lift says which value it could not range"
        );

        let (resolved, _) = resolve(&lifted.library, &lifted.root, None).expect("resolves");
        let back = to_container(&resolved);
        let RigNode::Block { block } = &back.children[0] else {
            panic!("the leaf came back as a container");
        };
        assert_eq!(
            block.param_f32("rate"),
            Some(2.5),
            "carried verbatim rather than clamped to 1.0"
        );
    }

    /// Every realization crosses, including the two the domain gained for
    /// this: a cabinet IR and a sample library.
    #[test]
    fn every_realization_crosses() {
        let mut tree = Container::layer("Lanes").extend(vec![
            RigNode::Block {
                block: RigBlock::nam("amps/ac30.nam").named("AC30"),
            },
            RigNode::Block {
                block: RigBlock::cab_ir("cabs/greenback.wav").named("Cab"),
            },
            RigNode::Block {
                block: RigBlock::sample_lib("keyscape/library.styx").named("Rhodes"),
            },
            RigNode::Block {
                block: RigBlock::plugin("/plugins/Amp.clap").named("Hosted"),
            },
            RigNode::Block {
                block: RigBlock::of_type(BlockType::Reverb).named("Verb"),
            },
        ]);
        tree.canonicalize();

        let lifted = lift(&tree);
        let kinds: Vec<&'static str> = lifted
            .library
            .nodes
            .iter()
            .filter_map(|n| match &n.content {
                Content::Leaf { block, .. } => Some(block.kind.tag()),
                Content::Children { .. } => None,
            })
            .collect();
        for want in ["nam", "ir", "sample", "plugin", "native"] {
            assert!(kinds.contains(&want), "{want} realization lost: {kinds:?}");
        }

        let (resolved, _) = resolve(&lifted.library, &lifted.root, None).expect("resolves");
        let back = to_container(&resolved);
        assert_eq!(back.dump(), tree.dump());
    }

    /// Ids are preserved, because they are what everything else points at:
    /// lifting a tree twice must name the same nodes or every override and
    /// selection addressing them breaks.
    #[test]
    fn lifting_twice_names_the_same_nodes() {
        let mut tree =
            Container::preset("P").add(Container::module("M").block(BlockType::Amp, "A"));
        tree.canonicalize();

        let first = lift(&tree);
        let second = lift(&tree);
        assert_eq!(first.root, second.root);

        let mut a: Vec<String> = first
            .library
            .nodes
            .iter()
            .map(|n| n.id.as_str().to_string())
            .collect();
        let mut b: Vec<String> = second
            .library
            .nodes
            .iter()
            .map(|n| n.id.as_str().to_string())
            .collect();
        a.sort();
        b.sort();
        assert_eq!(a, b, "the same tree lifted twice is the same nodes");
    }

    /// What the lift buys: once a tree is in the library it has variants,
    /// which a `Container` has never had. This is a Snapshot of a lifted
    /// rig — the same mechanism the guitar rig's twelve patches use.
    #[test]
    fn a_lifted_tree_can_have_variants() {
        let mut tree = Container::preset("Keys").add(
            Container::engine("Keys")
                .add(Container::parallel("Voices").block(BlockType::Sampler, "Rhodes")),
        );
        tree.canonicalize();

        let mut lifted = lift(&tree);
        let engine = lifted
            .library
            .nodes
            .iter()
            .find(|n| {
                n.name == "Keys" && !n.is_leaf() && n.role == signal_proto::node::Role::Engine
            })
            .map(|n| n.id.clone())
            .expect("the Engine is in the library");

        let bridge = Variant::new("Bridge");
        let bridge_id = bridge.id.clone();
        lifted
            .library
            .get_mut(&engine)
            .expect("present")
            .variants
            .push(bridge);

        let (resolved, _) =
            resolve(&lifted.library, &engine, Some(&bridge_id)).expect("the variant resolves");
        assert_eq!(resolved.name, "Keys");
    }

    /// The Nord Stage reference program — the densest tree the sampler
    /// builds by hand: three engines, per-lane modulators, a shared rotary
    /// fed by cross-tree sends, and mod routes into named blocks. If this
    /// crosses unchanged, the lift is not getting by on simple cases.
    #[test]
    fn the_nord_reference_program_survives_the_domain() {
        let mut tree = crate::nord::nord_stage_preset();
        tree.canonicalize();
        let before = tree.dump();

        let lifted = lift(&tree);
        let (resolved, report) = resolve(&lifted.library, &lifted.root, None).expect("resolves");
        assert!(report.is_clean(), "{report:?}");

        assert_eq!(
            to_container(&resolved).dump(),
            before,
            "the Nord program changed crossing the domain"
        );
    }

    /// And its parameters cross as ranged parameters rather than raw
    /// settings — the measure of how much of the rig the domain understands.
    #[test]
    fn the_nord_programs_parameters_are_all_ranged() {
        let mut tree = crate::nord::nord_stage_preset();
        tree.canonicalize();
        let lifted = lift(&tree);
        assert!(
            lifted.report.is_clean(),
            "parameters the domain could not range: {:?}",
            lifted.report.unranged
        );
    }

    /// A cross-tree send keeps pointing at the right node through the lift:
    /// the layers' "To Rotary" sends are resolved to the Rotary's id on the
    /// way in, so the rotary bus still forms on the way out.
    #[test]
    fn cross_tree_sends_survive_the_lift() {
        let mut tree = crate::nord::nord_stage_preset();
        tree.canonicalize();

        let lifted = lift(&tree);
        let sends: Vec<_> = lifted
            .library
            .nodes
            .iter()
            .flat_map(|n| n.sends.iter())
            .collect();
        assert!(!sends.is_empty(), "the program has sends");
        for send in sends {
            assert!(
                send.target.as_id().is_some(),
                "{} still points by name after the lift",
                send.label
            );
        }
    }

    /// A rig library preset's scenes become variants of one node, which is
    /// the third preset system folded into the first.
    #[test]
    fn a_presets_scenes_become_its_variants() {
        use crate::rig_library::{RigPreset, RigScene};

        let preset = RigPreset::new("Marshall JCM800")
            .with_scene(RigScene::single(
                "Clean",
                RigBlock::nam("amps/jcm800-clean.nam").named("Amp"),
            ))
            .with_scene(RigScene::single(
                "Lead",
                RigBlock::nam("amps/jcm800-lead.nam").named("Amp"),
            ));

        let lifted = lift_preset(&preset);
        assert_eq!(lifted.scenes.len(), 2, "one variant per scene");

        // The default scene is what resolves when nothing names one.
        let (default, report) = resolve(&lifted.library, &lifted.root, None).expect("resolves");
        assert!(report.is_clean(), "{report:?}");
        assert_eq!(
            to_chain(&default)
                .first()
                .map(|b| b.nam.clone())
                .unwrap_or_default(),
            "amps/jcm800-clean.nam"
        );

        // And each scene resolves to its own chain — an ordinary variant
        // switch, which is what a scene recall never was.
        let lead = lifted.scene("Lead").expect("Lead is a variant");
        let (resolved, _) = resolve(&lifted.library, &lifted.root, Some(lead)).expect("resolves");
        assert_eq!(
            to_chain(&resolved)
                .first()
                .map(|b| b.nam.clone())
                .unwrap_or_default(),
            "amps/jcm800-lead.nam"
        );

        // The scene chains are separate nodes, shared by nothing — because a
        // scene is an independent chain, not a diff of another.
        let chains = lifted
            .library
            .nodes
            .iter()
            .filter(|n| !n.is_leaf() && n.role == signal_proto::node::Role::Module)
            .count();
        assert_eq!(chains, 2);
    }

    /// A scene's trims are the node's input and output level — the same
    /// unification a block's trims got, one level up.
    #[test]
    fn a_scenes_trims_become_the_nodes_levels() {
        use crate::rig_library::{RigPreset, RigScene};

        let mut scene = RigScene::single("Hot", RigBlock::nam("a.nam").named("Amp"));
        scene.input_trim_db = -6.0;
        scene.output_trim_db = 3.0;
        let preset = RigPreset::new("P").with_scene(scene);

        let lifted = lift_preset(&preset);
        let (resolved, _) = resolve(&lifted.library, &lifted.root, None).expect("resolves");
        let container = to_container(&resolved);
        let hot = container.find("Hot").expect("the scene is a node");
        assert_eq!(hot.input_db, -6.0);
        assert_eq!(hot.output_db, 3.0);
    }

    /// Two identical modules, each with its own envelope driving its own
    /// filter — the case that makes name resolution scoped rather than
    /// global.
    ///
    /// Resolved globally, both modules' routes point at the *first* module's
    /// envelope and filter: one envelope drives both filters and the other
    /// drives nothing. Nothing reports it. You hear it months later as "the
    /// second layer sounds wrong".
    #[test]
    fn identical_siblings_keep_their_own_routes() {
        use signal_proto::block::BlockType;

        let module = |name: &str| {
            Container::module(name)
                .block(BlockType::Filter, "Filter 1")
                .modulator(BlockType::Envelope, "Filter Env")
                .route("Filter Env", "Filter 1.cutoff", 0.5)
        };
        let mut tree = Container::layer("L").add(module("L A")).add(module("L B"));
        tree.canonicalize();

        let lifted = lift(&tree);

        // Each module's route points inside that module, not at its
        // sibling's namesakes.
        let modules: Vec<_> = lifted
            .library
            .nodes
            .iter()
            .filter(|n| n.name == "L A" || n.name == "L B")
            .collect();
        assert_eq!(modules.len(), 2);

        for module in modules {
            let route = module.mod_routes.first().expect("its route");
            let target = route.target.as_id().expect("resolved to an id");
            let source = match &route.source {
                signal_proto::node_routing::ModSource::Node { node } => {
                    node.as_id().expect("resolved to an id")
                }
                other => panic!("the source should be a node, got {other:?}"),
            };
            assert!(
                module.children().contains(target),
                "{}'s route targets its own filter",
                module.name
            );
            assert!(
                module.modulators.contains(source),
                "{}'s route is driven by its own envelope",
                module.name
            );
        }
    }
}
