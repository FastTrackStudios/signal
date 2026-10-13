//! What the profile cannot say — stored beside it, keyed by node id.
//!
//! # Why an overlay and not the whole library
//!
//! The node library is *derived*: [`to_nodes`](crate::nodes::to_nodes) builds
//! it from `ProfileDef` on every edit. That is deliberate — the profile is
//! the authoring format, flat and hand-editable, and a person editing
//! `profile.styx` on a Sunday morning should see their change in the rig.
//!
//! But the domain can express things the profile has no vocabulary for: a
//! preset on a *module*, a preset someone saved from a tweak, a selection on
//! a node that is not a drive slot. Those cannot be written back to the
//! profile, and re-deriving would drop them.
//!
//! So they live here, in `nodes.styx`, and are applied on top of the derived
//! library. Two stores, but not two models of one thing: the profile says
//! what the rig *is*, this says what has been saved about its nodes.
//!
//! # This only works because ids are stable
//!
//! Every entry is keyed by a node id, and `to_nodes` seeds those
//! deterministically from what each node is. Before that, an id was minted
//! per build and an overlay would have dangled on the next rebuild — which
//! is why this file could not have been written first.

use facet::Facet;
use signal_proto::node::{NodeId, Variant, VariantId};

use crate::nodes::RigNodes;

/// The file's name inside the rig directory.
pub const NODE_STORE_FILE: &str = "nodes.styx";

/// Everything saved about nodes that the profile cannot hold.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct NodeStore {
    /// Presets a player saved, each belonging to one node.
    #[facet(default)]
    pub presets: Vec<SavedPreset>,
    /// Which preset a node is currently recalled as, where the profile has
    /// no field for that choice.
    ///
    /// A drive slot's capture is *not* here: the profile holds that in
    /// `DriveSlotDef::option`, and one fact stored twice is a fact that can
    /// disagree with itself.
    #[facet(default)]
    pub selections: Vec<SavedSelection>,
}

/// One saved preset: a variant, and the node it belongs to.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct SavedPreset {
    /// The node this is a preset *of*.
    pub node: String,
    /// The variant's id — what a selection names.
    pub id: String,
    pub name: String,
    /// What the preset changes, relative to the node's default: the same
    /// override list every variant carries.
    #[facet(default)]
    pub overrides: Vec<signal_proto::overrides::Override>,
    /// Which variant each child uses, for a preset of a container.
    #[facet(default)]
    pub selections: Vec<ChildSelection>,
}

/// A container preset's choice of one child's variant.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct ChildSelection {
    pub node: String,
    pub variant: String,
}

/// A node, and the preset it is currently recalled as.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct SavedSelection {
    pub node: String,
    pub variant: String,
}

/// A node id from stored text, or `None` when the text is not one.
///
/// `NodeId::from` **panics** on anything that is not a UUID, and this file is
/// hand-editable config: a typo in `nodes.styx` would otherwise take the rig
/// down on load rather than costing one stale preset. The conversion lives in
/// the `utils` crate, outside this repo, so validating here is the fix
/// available — and the right one anyway, since bad input is this file's
/// problem to survive.
fn node_id(raw: &str) -> Option<NodeId> {
    uuid::Uuid::parse_str(raw).ok()?;
    Some(NodeId::from(raw.to_string()))
}

/// The same, for a variant.
fn variant_id(raw: &str) -> Option<VariantId> {
    uuid::Uuid::parse_str(raw).ok()?;
    Some(VariantId::from(raw.to_string()))
}

impl NodeStore {
    /// Merge everything saved into a freshly derived library.
    ///
    /// Presets become variants on their node; selections become the node's
    /// current default. An entry naming a node the profile no longer builds
    /// is skipped and reported — a preset for a pedal that was removed is
    /// stale, not a reason to fail the load.
    pub fn apply(&self, rig: &mut RigNodes) {
        for preset in &self.presets {
            let (Some(id), Some(preset_id)) = (node_id(&preset.node), variant_id(&preset.id))
            else {
                tracing::warn!(
                    node.id = %preset.node,
                    preset.name = %preset.name,
                    "guitar: saved preset has an id that is not a node id — skipped"
                );
                continue;
            };
            let Some(node) = rig.library.get_mut(&id) else {
                tracing::warn!(
                    node.id = %preset.node,
                    preset.name = %preset.name,
                    "guitar: saved preset names a node this profile no longer builds"
                );
                continue;
            };
            if node.variants.iter().any(|v| v.id == preset_id) {
                continue;
            }
            let mut variant = Variant::new(&preset.name);
            variant.id = preset_id;
            variant.overrides.clone_from(&preset.overrides);
            variant.selections = preset
                .selections
                .iter()
                .filter_map(|s| {
                    Some(signal_proto::node::Selection {
                        node: node_id(&s.node)?,
                        variant: variant_id(&s.variant)?,
                    })
                })
                .collect();
            node.variants.push(variant);
        }

        for selection in &self.selections {
            let (Some(id), Some(wanted)) =
                (node_id(&selection.node), variant_id(&selection.variant))
            else {
                tracing::warn!(
                    node.id = %selection.node,
                    "guitar: saved selection has an id that is not a node id — skipped"
                );
                continue;
            };
            let Some(node) = rig.library.get_mut(&id) else {
                continue;
            };
            if node.variants.iter().any(|v| v.id == wanted) {
                node.default_variant = wanted;
            } else {
                tracing::warn!(
                    node.id = %selection.node,
                    "guitar: saved selection names a preset this node no longer has"
                );
            }
        }
    }

    /// Record which preset a node is recalled as, replacing any previous
    /// choice for it.
    pub fn select(&mut self, node: &str, variant: &str) {
        match self.selections.iter_mut().find(|s| s.node == node) {
            Some(existing) => existing.variant = variant.to_string(),
            None => self.selections.push(SavedSelection {
                node: node.to_string(),
                variant: variant.to_string(),
            }),
        }
    }

    /// Whether this node has a saved preset by that name already.
    #[must_use]
    pub fn has_preset(&self, node: &str, name: &str) -> bool {
        self.presets
            .iter()
            .any(|p| p.node == node && p.name.eq_ignore_ascii_case(name))
    }
}

/// Capture a node's current state as a preset of it.
///
/// A preset is a **diff**, not a copy: what this records is every parameter
/// that differs from what the node resolves to on its own. Recall it and
/// those parameters move; everything else stays as the node has it, so
/// editing the node later still reaches every preset of it. Copying the whole
/// state instead would freeze each preset against the day it was saved.
///
/// `current` is the node as it sounds right now — the resolved tree the rig
/// is playing, patch overrides and all. The baseline is the same node
/// resolved by itself.
///
/// Paths are relative to the node, naming the block and the parameter, which
/// is what makes the preset portable: it applies wherever that node sits.
#[must_use]
pub fn capture(
    rig: &RigNodes,
    current: &signal_proto::node_resolve::Resolved,
    node: &NodeId,
    name: &str,
) -> Option<SavedPreset> {
    use signal_proto::node_resolve::{ResolvedContent, resolve};
    use signal_proto::overrides::{NodeOverrideOp, NodePath, NodePathSegment, Override};

    let live = current.find(node)?;
    let (base, _) = resolve(&rig.library, node, None).ok()?;

    let mut overrides = Vec::new();
    for live_leaf in live.leaves() {
        let ResolvedContent::Leaf {
            block: live_block, ..
        } = &live_leaf.content
        else {
            continue;
        };
        // The same leaf in the baseline, by id — not by position, since a
        // preset may have swapped which node sits in a slot.
        let Some(base_leaf) = base.find(&live_leaf.id) else {
            continue;
        };
        let ResolvedContent::Leaf {
            block: base_block, ..
        } = &base_leaf.content
        else {
            continue;
        };

        for param in live_block.parameters() {
            let unchanged = base_block
                .parameters()
                .iter()
                .find(|p| p.id() == param.id())
                .is_some_and(|p| (p.value().get() - param.value().get()).abs() < f32::EPSILON);
            if unchanged {
                continue;
            }
            overrides.push(Override {
                path: NodePath::new(vec![
                    NodePathSegment::Block {
                        id: live_leaf.id.as_str().to_string(),
                    },
                    NodePathSegment::Parameter {
                        id: param.id().to_string(),
                    },
                ]),
                op: NodeOverrideOp::Set {
                    value: param.value(),
                },
            });
        }
    }

    Some(SavedPreset {
        node: node.as_str().to_string(),
        // Seeded from the node and the name, so saving the same preset twice
        // is the same preset rather than two with one name.
        id: signal_proto::seed_id(&format!("{}@{name}", node.as_str())).to_string(),
        name: name.to_string(),
        overrides,
        selections: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::to_nodes;
    use signal_proto::overrides::{NodeOverrideOp, NodePath, NodePathSegment, Override};

    fn shipped() -> (
        crate::profiles::ProfileDef,
        Vec<crate::profiles::DrivePresetDef>,
    ) {
        #[derive(facet::Facet)]
        struct Presets {
            presets: Vec<crate::profiles::DrivePresetDef>,
        }
        let def = facet_styx::from_str(crate::library::DEFAULT_PROFILE).expect("profile");
        let drives = facet_styx::from_str::<Presets>(crate::library::DEFAULT_DRIVE_PRESETS)
            .expect("drives")
            .presets;
        (def, drives)
    }

    /// A saved preset becomes a variant of its node — which is what makes a
    /// module preset possible at all, since the profile has no field for one.
    #[test]
    fn a_saved_preset_becomes_a_variant() {
        let (def, drives) = shipped();
        let mut rig = to_nodes(&def, &drives);

        // The Time module: delays and reverbs, grouped. The profile cannot
        // say anything about it beyond the blocks inside.
        let time = rig
            .library
            .nodes
            .iter()
            .find(|n| n.name == "Time" && !n.is_leaf())
            .map(|n| n.id.as_str().to_string())
            .expect("the chain has a Time module");
        let before = rig
            .library
            .get(&NodeId::from(time.clone()))
            .map_or(0, |n| n.variants.len());

        let store = NodeStore {
            presets: vec![SavedPreset {
                node: time.clone(),
                id: signal_proto::seed_id("test/ambient-wash").to_string(),
                name: "Ambient wash".into(),
                overrides: vec![Override {
                    path: NodePath::new(vec![
                        NodePathSegment::Block {
                            id: "VERB 1".into(),
                        },
                        NodePathSegment::Parameter { id: "mix".into() },
                    ]),
                    op: NodeOverrideOp::Set {
                        value: signal_proto::ParameterValue::new(0.6),
                    },
                }],
                selections: Vec::new(),
            }],
            selections: Vec::new(),
        };
        store.apply(&mut rig);

        let time_node = rig.library.get(&NodeId::from(time)).expect("still there");
        assert_eq!(
            time_node.variants.len(),
            before + 1,
            "the preset is a variant"
        );
        assert!(
            time_node.variants.iter().any(|v| v.name == "Ambient wash"),
            "by name"
        );
    }

    /// And selecting it makes it what the node resolves as.
    #[test]
    fn a_saved_selection_becomes_the_nodes_default() {
        let (def, drives) = shipped();
        let mut rig = to_nodes(&def, &drives);

        let pedal = rig
            .library
            .nodes
            .iter()
            .find(|n| n.name == "King of Tone")
            .expect("the pedal");
        let node = pedal.id.as_str().to_string();
        let wanted = pedal
            .variants
            .last()
            .map(|v| v.id.as_str().to_string())
            .expect("a second capture");
        assert_ne!(
            pedal.default_variant.as_str(),
            wanted,
            "not already the default, or this proves nothing"
        );

        let mut store = NodeStore::default();
        store.select(&node, &wanted);
        store.apply(&mut rig);

        assert_eq!(
            rig.library
                .get(&NodeId::from(node))
                .map(|n| n.default_variant.as_str().to_string()),
            Some(wanted)
        );
    }

    /// A selection is recorded once per node: choosing twice replaces rather
    /// than accumulating, or the file grows without bound and the last write
    /// is whichever entry happens to be read last.
    #[test]
    fn choosing_twice_replaces_the_choice() {
        let mut store = NodeStore::default();
        store.select("node", "first");
        store.select("node", "second");
        assert_eq!(store.selections.len(), 1);
        assert_eq!(store.selections[0].variant, "second");
    }

    /// An entry for a node the profile no longer builds is skipped, not
    /// fatal. A pedal can be removed from a rig; its saved presets should not
    /// stop the rig loading.
    #[test]
    fn a_stale_entry_is_skipped() {
        let (def, drives) = shipped();
        let mut rig = to_nodes(&def, &drives);
        let before = rig.library.nodes.len();

        let store = NodeStore {
            presets: vec![SavedPreset {
                node: signal_proto::seed_id("a-pedal-that-was-deleted").to_string(),
                id: signal_proto::seed_id("x").to_string(),
                name: "Gone".into(),
                overrides: Vec::new(),
                selections: Vec::new(),
            }],
            selections: vec![SavedSelection {
                node: signal_proto::seed_id("also-gone").to_string(),
                variant: signal_proto::seed_id("y").to_string(),
            }],
        };
        store.apply(&mut rig);
        assert_eq!(
            rig.library.nodes.len(),
            before,
            "nothing added, nothing lost"
        );
    }

    /// The store round-trips through styx — it is the format the rig's
    /// config is written in, and a preset that cannot be re-read is not
    /// saved.
    #[test]
    fn the_store_round_trips_through_styx() {
        let store = NodeStore {
            presets: vec![SavedPreset {
                node: signal_proto::seed_id("node-1").to_string(),
                id: signal_proto::seed_id("variant-1").to_string(),
                name: "Bridge".into(),
                overrides: vec![Override {
                    path: NodePath::new(vec![NodePathSegment::Parameter { id: "mix".into() }]),
                    op: NodeOverrideOp::Set {
                        value: signal_proto::ParameterValue::new(0.25),
                    },
                }],
                selections: vec![ChildSelection {
                    node: signal_proto::seed_id("child").to_string(),
                    variant: signal_proto::seed_id("child-variant").to_string(),
                }],
            }],
            selections: vec![SavedSelection {
                node: signal_proto::seed_id("node-1").to_string(),
                variant: signal_proto::seed_id("variant-1").to_string(),
            }],
        };

        let text = facet_styx::to_string(&store).expect("write");
        let back: NodeStore = facet_styx::from_str(&text)
            .unwrap_or_else(|e| panic!("a saved preset must read back: {e}\n{text}"));
        assert_eq!(back, store);
    }

    /// A hand-edited file with a mistyped id must cost one stale preset, not
    /// the rig.
    ///
    /// `NodeId::from` panics on anything that is not a UUID, and this file is
    /// config a person can open. Applying it has to survive that.
    #[test]
    fn a_mistyped_id_does_not_take_the_rig_down() {
        let (def, drives) = shipped();
        let mut rig = to_nodes(&def, &drives);
        let before = rig.library.nodes.len();

        let store = NodeStore {
            presets: vec![SavedPreset {
                node: "Time".into(), // a name, not an id — the obvious slip
                id: "my-preset".into(),
                name: "Ambient".into(),
                overrides: Vec::new(),
                selections: Vec::new(),
            }],
            selections: vec![SavedSelection {
                node: "oops".into(),
                variant: "also-oops".into(),
            }],
        };
        store.apply(&mut rig);

        assert_eq!(rig.library.nodes.len(), before);
    }

    /// Saving a tweak as a preset, and recalling it — the loop the UI could
    /// not close: a knob move was recorded as a patch override, permanently
    /// and anonymously, and there was no way to keep it as a named thing.
    #[test]
    fn a_tweak_is_captured_and_recalled() {
        use signal_proto::node_resolve::{ResolvedContent, resolve};

        let (def, drives) = shipped();
        let mut rig = to_nodes(&def, &drives);

        // The Time module, and the reverb inside it.
        let time = rig
            .library
            .nodes
            .iter()
            .find(|n| n.name == "Time" && !n.is_leaf())
            .map(|n| n.id.clone())
            .expect("a Time module");

        // As it sounds now, with the reverb mix pushed up — what a player
        // would have just dialled in.
        let (mut current, _) = resolve(&rig.library, &time, None).expect("resolves");
        let verb = current
            .leaves()
            .iter()
            .find(|l| l.name == "VERB 1")
            .map(|l| l.id.clone())
            .expect("the module holds VERB 1");
        {
            let leaf = current.find_mut(&verb).expect("still there");
            let ResolvedContent::Leaf { block, .. } = &mut leaf.content else {
                panic!("VERB 1 is a leaf");
            };
            let index = block
                .parameters()
                .iter()
                .position(|p| p.id() == "mix")
                .expect("a reverb has a mix");
            block.set_parameter_value(index, 0.62);
        }

        let preset = capture(&rig, &current, &time, "Ambient wash").expect("captures");
        assert_eq!(preset.name, "Ambient wash");
        assert_eq!(
            preset.overrides.len(),
            1,
            "only what changed: one parameter, not the whole module"
        );

        // Store it, rebuild the library from scratch, and recall it.
        let store = NodeStore {
            presets: vec![preset.clone()],
            selections: Vec::new(),
        };
        let mut rebuilt = to_nodes(&def, &drives);
        store.apply(&mut rebuilt);

        let variant = rebuilt
            .library
            .get(&time)
            .and_then(|n| n.variants.iter().find(|v| v.name == "Ambient wash"))
            .map(|v| v.id.clone())
            .expect("the preset survived a rebuild");

        let (recalled, report) = resolve(&rebuilt.library, &time, Some(&variant)).expect("recalls");
        assert!(report.is_clean(), "{report:?}");

        let mix = recalled
            .leaves()
            .iter()
            .find(|l| l.name == "VERB 1")
            .and_then(|l| match &l.content {
                ResolvedContent::Leaf { block, .. } => block
                    .parameters()
                    .iter()
                    .find(|p| p.id() == "mix")
                    .map(|p| p.value().get()),
                ResolvedContent::Children(_) => None,
            })
            .expect("the reverb is there");
        assert!(
            (mix - 0.62).abs() < 0.001,
            "the saved mix came back, got {mix}"
        );
    }

    /// Saving the same name twice is the same preset, not two with one name.
    #[test]
    fn saving_the_same_name_twice_is_one_preset() {
        use signal_proto::node_resolve::resolve;

        let (def, drives) = shipped();
        let rig = to_nodes(&def, &drives);
        let time = rig
            .library
            .nodes
            .iter()
            .find(|n| n.name == "Time" && !n.is_leaf())
            .map(|n| n.id.clone())
            .expect("a Time module");
        let (current, _) = resolve(&rig.library, &time, None).expect("resolves");

        let first = capture(&rig, &current, &time, "Wash").expect("captures");
        let second = capture(&rig, &current, &time, "Wash").expect("captures");
        assert_eq!(first.id, second.id);
    }
}
