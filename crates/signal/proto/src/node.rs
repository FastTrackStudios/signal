//! The one recursive primitive the whole domain is built from.
//!
//! Signal has five levels — Block, Module, Layer, Engine, Preset — and for a
//! long time it had five near-identical pairs of types to express them, plus
//! a macro (`impl_collection!`) that existed precisely *because* they were
//! identical. This module is the collapse of all of it into one type.
//!
//! # Only a Block does anything
//!
//! A [`Node`] is either a **leaf** carrying a [`Block`] — the only thing that
//! runs DSP — or a **container** holding other nodes. Module, Layer, Engine
//! and Preset are all containers. They differ in what they *mean*, not in
//! what they *are*, so the difference is a label:
//!
//! - [`Role`] says what the node means to a player, and how a UI draws it.
//! - [`Combine`] says what the audio does: `Serial` chains the children,
//!   `Parallel` sums them.
//!
//! Nothing that renders audio branches on `Role`. That separation is
//! load-bearing: a keys Engine is `Serial` so its FX sit after the sum, while
//! the Layers inside it hang off a `Parallel` node so they *stack*. Same
//! role, different combine, completely different instrument.
//!
//! # Children are references, not owned subtrees
//!
//! [`Content::Children`] holds [`NodeId`]s resolved through a [`NodeLibrary`],
//! not nested `Node` values. That is what makes a preset a *preset*: one AC30
//! node is referenced by every patch that uses it, and editing it once
//! changes all of them. An owned tree would make each use a private copy, and
//! "edit the preset" would mean "edit twelve copies".
//!
//! # Every level has presets with variations
//!
//! Because there is one type, there is one variant mechanism, and it applies
//! at every level for free. A node's [`variants`](Node::variants) are its
//! named alternatives; a [`Variant`] records **which variant each child
//! uses** ([`Selection`]) plus any [`Override`]s. So a variant is a diff
//! against a base, and bases nest.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::block::BlockType;
use crate::model::{Block, EngineType};
use crate::node_routing::{AudioSend, ModRoute, ModSource, Setting, Zone};
use crate::overrides::Override;

crate::typed_uuid_id!(
    /// Identifies a node. UUIDv7 — a rig is distributable, so two people
    /// building a "Pad" on two machines must never mint the same id.
    NodeId
);
crate::typed_uuid_id!(
    /// Identifies one variant of a node.
    VariantId
);

/// What a node means to a player — a label, never a behaviour.
///
/// See the module docs: audio comes from [`Combine`]. Roles drive display and
/// say where shared-versus-per-child processing is understood to sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum Role {
    /// The whole program — every Engine sounding together.
    Preset,
    /// A *type* of playable thing: Keys, Organ, Pad, Bass. Holds Layers and
    /// no sound of its own.
    Engine,
    /// A playable Engine sound. Several stack inside one Engine.
    Layer,
    /// Nodes grouped for a purpose — the Time module, the drive board.
    /// Nests freely: a Module of Modules is still a Module.
    ///
    /// The default, because a node whose meaning is unstated is a grouping —
    /// the one role that carries no promise about what it holds.
    #[default]
    Module,
}

impl Role {
    /// The word a UI shows for this role.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Preset => "Preset",
            Self::Engine => "Engine",
            Self::Layer => "Layer",
            Self::Module => "Module",
        }
    }
}

/// How a container combines its children into its output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum Combine {
    /// Children chained in order: `child[0] → child[1] → … → out`.
    Serial,
    /// Children fed the same input; their outputs summed.
    Parallel,
}

impl Combine {
    /// The word this serializes and displays as.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Serial => "serial",
            Self::Parallel => "parallel",
        }
    }
}

/// What a node holds: DSP, or other nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum Content {
    /// A leaf. The only node that processes audio.
    ///
    /// Carries its own [`BlockType`] because `Block` does not: in the older
    /// model the type lived on the block-level `Preset` wrapping it, which a
    /// single node type has no room for. A leaf has to know whether it is an
    /// amp or a delay — the renderer asks, and so does every UI.
    Leaf { block_type: BlockType, block: Block },
    /// References to other nodes, in order. Resolved through a
    /// [`NodeLibrary`] — see the module docs on why these are ids.
    Children { nodes: Vec<NodeId> },
}

/// Which variant of a child this variant wants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct Selection {
    pub node: NodeId,
    pub variant: VariantId,
}

/// One named alternative of a node.
///
/// Not a copy of it: a variant records only what *differs* — which variant
/// each child uses, and which parameters are changed. That is what lets one
/// capture be loaded in twenty songs, each bending a different parameter of
/// it, with one thing on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Variant {
    pub id: VariantId,
    pub name: String,
    /// Which variant each child uses. A child not named here uses its own
    /// default.
    #[serde(default)]
    pub selections: Vec<Selection>,
    /// Parameter changes reaching anywhere below this node — see
    /// [`crate::overrides`] for the path model and
    /// [`crate::override_policy`] for which operations a given level may
    /// perform.
    #[serde(default)]
    pub overrides: Vec<Override>,
}

impl Variant {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: VariantId::new(),
            name: name.into(),
            selections: Vec::new(),
            overrides: Vec::new(),
        }
    }

    /// Point a child at one of its variants.
    #[must_use]
    pub fn selecting(mut self, node: NodeId, variant: VariantId) -> Self {
        self.selections.push(Selection { node, variant });
        self
    }

    /// The variant this one wants for `node`, if it names one.
    #[must_use]
    pub fn selection_for(&self, node: &NodeId) -> Option<&VariantId> {
        self.selections
            .iter()
            .find(|s| &s.node == node)
            .map(|s| &s.variant)
    }
}

/// A node in the domain: a leaf Block, or a container of other nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    /// What it means. A label — see [`Role`].
    pub role: Role,
    /// What the audio does. The behaviour — see [`Combine`].
    pub combine: Combine,
    pub content: Content,
    /// This node's named alternatives. Always at least one: the default.
    pub variants: Vec<Variant>,
    pub default_variant: VariantId,

    // ─── The rest of what a node is ──────────────────────────────
    //
    // Role, combine and content say what the node *is*. These say how it
    // sounds and what reaches it — the fields the sampler's `Container`
    // carried and this type did not, which is why the keys rig could not be
    // expressed as nodes at all.
    /// Which kind of playable thing, when [`role`](Self::role) is
    /// [`Role::Engine`].
    ///
    /// `Role::Engine` says it *is* an Engine; this says it is the Organ.
    /// Meaningless on any other role, where it keeps its default and nothing
    /// reads it — an `Option` would say that better, but a styx document
    /// cannot read back an optional unit-tagged enum, and a field that
    /// cannot be persisted is worse than one that is occasionally ignored.
    #[serde(default)]
    #[facet(default)]
    pub engine_type: EngineType,
    /// Input trim (dB), applied before the children.
    #[serde(default)]
    #[facet(default)]
    pub input_db: f32,
    /// Output volume (dB) — the fader. A Layer's and an Engine's native
    /// volume; a Module's output trim.
    #[serde(default)]
    #[facet(default)]
    pub output_db: f32,
    /// Control-rate modulators attached here — envelopes, LFOs. Leaf nodes
    /// like any other, but off the audio path: they reach parameters through
    /// [`mod_routes`](Self::mod_routes), never through the chain.
    #[serde(default)]
    #[facet(default)]
    pub modulators: Vec<NodeId>,
    /// Cross-tree audio sends from this node's output.
    #[serde(default)]
    #[facet(default)]
    pub sends: Vec<AudioSend>,
    /// Modulation matrix rows scoped to this subtree.
    #[serde(default)]
    #[facet(default)]
    pub mod_routes: Vec<ModRoute>,
    /// Node-level settings that are not blocks — a Layer's `voice_mode`,
    /// `unison`, `octave`.
    #[serde(default)]
    #[facet(default)]
    pub settings: Vec<Setting>,
    /// The keyboard window that reaches this subtree. Default passes
    /// everything; a narrower one is a key split.
    #[serde(default)]
    #[facet(default)]
    pub zone: Zone,
    /// Whether this whole subtree is bypassed.
    #[serde(default)]
    #[facet(default)]
    pub bypassed: bool,
}

impl Node {
    /// A container with one empty default variant.
    pub fn container(name: impl Into<String>, role: Role, combine: Combine) -> Self {
        let default = Variant::new("Default");
        Self {
            id: NodeId::new(),
            name: name.into(),
            role,
            combine,
            content: Content::Children { nodes: Vec::new() },
            default_variant: default.id.clone(),
            variants: vec![default],
            engine_type: EngineType::default(),
            input_db: 0.0,
            output_db: 0.0,
            modulators: Vec::new(),
            sends: Vec::new(),
            mod_routes: Vec::new(),
            settings: Vec::new(),
            zone: Zone::full(),
            bypassed: false,
        }
    }

    /// A leaf holding one block.
    pub fn leaf(name: impl Into<String>, block_type: BlockType, block: Block) -> Self {
        let default = Variant::new("Default");
        Self {
            id: NodeId::new(),
            name: name.into(),
            role: Role::Module,
            combine: Combine::Serial,
            content: Content::Leaf { block_type, block },
            default_variant: default.id.clone(),
            variants: vec![default],
            engine_type: EngineType::default(),
            input_db: 0.0,
            output_db: 0.0,
            modulators: Vec::new(),
            sends: Vec::new(),
            mod_routes: Vec::new(),
            settings: Vec::new(),
            zone: Zone::full(),
            bypassed: false,
        }
    }

    /// Name which kind of playable thing this Engine is.
    #[must_use]
    pub const fn of_type(mut self, engine_type: EngineType) -> Self {
        self.engine_type = engine_type;
        self
    }

    /// Restrict which notes reach this subtree.
    #[must_use]
    pub const fn in_zone(mut self, zone: Zone) -> Self {
        self.zone = zone;
        self
    }

    /// Set the fader.
    #[must_use]
    pub const fn at_db(mut self, output_db: f32) -> Self {
        self.output_db = output_db;
        self
    }

    /// Append a child reference. No-op on a leaf.
    #[must_use]
    pub fn with_child(mut self, child: &Self) -> Self {
        if let Content::Children { nodes } = &mut self.content {
            nodes.push(child.id.clone());
        }
        self
    }

    /// Add a named variant.
    #[must_use]
    pub fn with_variant(mut self, variant: Variant) -> Self {
        self.variants.push(variant);
        self
    }

    /// The ids this node references, empty for a leaf.
    #[must_use]
    pub fn children(&self) -> &[NodeId] {
        match &self.content {
            Content::Children { nodes } => nodes,
            Content::Leaf { .. } => &[],
        }
    }

    #[must_use]
    pub const fn is_leaf(&self) -> bool {
        matches!(self.content, Content::Leaf { .. })
    }

    /// A variant by id.
    #[must_use]
    pub fn variant(&self, id: &VariantId) -> Option<&Variant> {
        self.variants.iter().find(|v| &v.id == id)
    }

    /// The variant used when nothing names one.
    ///
    /// Falls back to the first variant if the default id has gone missing —
    /// a node with variants must always resolve to one of them, and failing
    /// to render is worse than rendering the wrong voicing.
    #[must_use]
    pub fn default_variant(&self) -> Option<&Variant> {
        self.variant(&self.default_variant)
            .or_else(|| self.variants.first())
    }
}

/// Resolve one reference, reporting whether it changed.
fn resolve_one(
    reference: &mut crate::node_routing::NodeRef,
    lookup: &impl Fn(&str) -> Option<NodeId>,
) -> bool {
    if !reference.is_unresolved() {
        return false;
    }
    reference.resolve(lookup);
    !reference.is_unresolved()
}

/// Every node, by id — what [`Content::Children`] references resolve against.
///
/// Flat rather than nested on purpose: a node referenced by six parents is
/// stored once, so editing it changes all six.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct NodeLibrary {
    pub nodes: Vec<Node>,
}

impl NodeLibrary {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, node: Node) -> NodeId {
        let id = node.id.clone();
        match self.nodes.iter_mut().find(|n| n.id == id) {
            Some(existing) => *existing = node,
            None => self.nodes.push(node),
        }
        id
    }

    #[must_use]
    pub fn get(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| &n.id == id)
    }

    pub fn get_mut(&mut self, id: &NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| &n.id == id)
    }

    /// Turn every route that still points by name into one that points by
    /// id, wherever the name matches a node in this library.
    ///
    /// Authoring produces names — a lane sends "To Rotary" before the Rotary
    /// exists — and this is the pass that makes them permanent. Run it once
    /// the library is complete. Returns how many references it resolved.
    ///
    /// A name matching nothing is left alone: see
    /// [`NodeRef`](crate::node_routing::NodeRef). Two nodes sharing a name
    /// resolve to the first, which is the cost of ever having allowed a name
    /// — and the reason this pass exists rather than resolving at render
    /// time, every time.
    pub fn resolve_refs(&mut self) -> usize {
        let by_name: Vec<(String, NodeId)> = self
            .nodes
            .iter()
            .map(|n| (n.name.to_lowercase(), n.id.clone()))
            .collect();
        let lookup = |name: &str| {
            by_name
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, id)| id.clone())
        };

        let mut resolved = 0;
        for node in &mut self.nodes {
            for send in &mut node.sends {
                resolved += usize::from(resolve_one(&mut send.target, &lookup));
            }
            for route in &mut node.mod_routes {
                resolved += usize::from(resolve_one(&mut route.target, &lookup));
                if let ModSource::Node { node: source } = &mut route.source {
                    resolved += usize::from(resolve_one(source, &lookup));
                }
            }
        }
        resolved
    }

    /// How many nodes reference `id` — what makes deleting a shared preset a
    /// question rather than an action.
    #[must_use]
    pub fn users_of(&self, id: &NodeId) -> usize {
        self.nodes
            .iter()
            .filter(|n| n.children().contains(id))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block() -> Block {
        Block::new(0.0, 0.0, 0.0)
    }

    /// The guitar rig's shape: one tone at a time, so it is Modules holding
    /// leaves, serial the whole way down.
    #[test]
    fn a_serial_chain_is_modules_and_leaves() {
        let mut lib = NodeLibrary::new();
        let comp = Node::leaf("Compressor", BlockType::Compressor, block());
        let amp = Node::leaf("AC30", BlockType::Amp, block());
        let board = Node::container("Drive board", Role::Module, Combine::Serial);

        let chain = Node::container("Worship", Role::Preset, Combine::Serial)
            .with_child(&comp)
            .with_child(&board)
            .with_child(&amp);

        for n in [comp, amp, board, chain.clone()] {
            lib.insert(n);
        }

        assert_eq!(chain.children().len(), 3);
        assert!(!chain.is_leaf());
        assert_eq!(lib.get(&chain.id).map(|n| n.role), Some(Role::Preset));
    }

    /// The keys rig's shape, and the distinction that forces Role and
    /// Combine to be separate axes: the Engine is Serial so its FX sit after
    /// the sum, but its Layers hang off a Parallel node so they stack. A
    /// serial Engine would let the last lane overwrite the ones before it.
    #[test]
    fn an_engine_is_serial_but_its_layers_are_parallel() {
        let keys1 = Node::container("Keys 1", Role::Layer, Combine::Serial);
        let keys2 = Node::container("Keys 2", Role::Layer, Combine::Serial);
        let voices = Node::container("Keys Voices", Role::Module, Combine::Parallel)
            .with_child(&keys1)
            .with_child(&keys2);
        let engine = Node::container("Keys", Role::Engine, Combine::Serial).with_child(&voices);

        assert_eq!(engine.combine, Combine::Serial);
        assert_eq!(voices.combine, Combine::Parallel);
        assert_eq!(voices.children().len(), 2);
    }

    /// The reuse that makes a preset a preset — and the reason children are
    /// ids rather than owned subtrees.
    #[test]
    fn one_node_is_shared_by_every_parent_that_references_it() {
        let mut lib = NodeLibrary::new();
        let ac30 = Node::leaf("AC30", BlockType::Amp, block());
        let clean = Node::container("Clean", Role::Preset, Combine::Serial).with_child(&ac30);
        let ambient = Node::container("Ambient", Role::Preset, Combine::Serial).with_child(&ac30);

        let ac30_id = ac30.id.clone();
        lib.insert(ac30);
        lib.insert(clean);
        lib.insert(ambient);

        assert_eq!(lib.users_of(&ac30_id), 2, "both patches point at one AC30");
        assert_eq!(lib.nodes.len(), 3, "it is stored once, not copied");

        // Editing it once changes both — the whole point.
        lib.get_mut(&ac30_id).expect("present").name = "AC30 Top Boost".to_string();
        assert_eq!(
            lib.get(&ac30_id).map(|n| n.name.as_str()),
            Some("AC30 Top Boost")
        );
    }

    /// A variant selects which variant each child uses. This is "recall the
    /// Engine, but with a different Layer preset loaded".
    #[test]
    fn a_variant_selects_its_childrens_variants() {
        let shimmer = Node::container("Shimmer", Role::Layer, Combine::Serial);
        let choir = Variant::new("Choir");
        let shimmer = shimmer.with_variant(choir.clone());

        let bridge = Variant::new("Bridge").selecting(shimmer.id.clone(), choir.id.clone());

        assert_eq!(bridge.selection_for(&shimmer.id), Some(&choir.id));
        // A child the variant does not name falls back to its own default.
        assert_eq!(bridge.selection_for(&NodeId::new()), None);
        assert_eq!(
            shimmer.default_variant().map(|v| v.name.as_str()),
            Some("Default")
        );
    }

    /// A node must always resolve to some variant. Failing to render is
    /// worse than rendering a different voicing.
    #[test]
    fn a_lost_default_falls_back_rather_than_failing() {
        let mut node = Node::container("Pad", Role::Layer, Combine::Serial);
        node.default_variant = VariantId::new();
        assert!(node.variant(&node.default_variant).is_none());
        assert!(
            node.default_variant().is_some(),
            "a node with variants always resolves to one"
        );
    }
}
