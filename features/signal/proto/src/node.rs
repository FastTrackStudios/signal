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
    /// id.
    ///
    /// Authoring produces names — a lane sends "To Rotary" before the Rotary
    /// exists — and this is the pass that makes them permanent. Run it once
    /// the library is complete. Returns how many references it resolved.
    ///
    /// # A name is resolved in the scope it was written in
    ///
    /// **A mod route's target** resolves against the subtree of the node
    /// carrying the route, and **its source** against that node's own
    /// modulators and its ancestors'. Not globally, and this is the whole
    /// subtlety: a keys layer holds two identical modules, each with its own
    /// "Filter Env" driving its own "Filter 1". Resolved globally, both
    /// modules' routes point at the *first* module's envelope and filter —
    /// so one module's envelope drives both filters and the other's drives
    /// nothing. Silent, and the kind of thing you hear as "the second layer
    /// sounds wrong" months later.
    ///
    /// **A send's target** does resolve globally, because a send is
    /// cross-tree by definition — routing to a shared rotary that is
    /// deliberately somewhere else is the reason it exists.
    ///
    /// A name matching nothing in its scope is left alone: see
    /// [`NodeRef`](crate::node_routing::NodeRef). Two nodes sharing a name
    /// *within one scope* resolve to the first, which is the cost of ever
    /// having allowed a name — and the reason this pass exists rather than
    /// resolving at render time, every time.
    pub fn resolve_refs(&mut self) -> usize {
        let names: Vec<(NodeId, String)> = self
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n.name.to_lowercase()))
            .collect();
        let lookup_in = |scope: &[NodeId], name: &str| {
            scope.iter().find_map(|id| {
                names
                    .iter()
                    .find(|(candidate, candidate_name)| candidate == id && candidate_name == name)
                    .map(|(id, _)| id.clone())
            })
        };
        let global = |name: &str| {
            names
                .iter()
                .find(|(_, candidate)| candidate == name)
                .map(|(id, _)| id.clone())
        };

        let mut resolved = 0;
        let scopes: Vec<(NodeId, Vec<NodeId>, Vec<NodeId>)> = self
            .nodes
            .iter()
            .map(|n| {
                (
                    n.id.clone(),
                    self.subtree_of(&n.id),
                    self.modulator_scope_of(&n.id),
                )
            })
            .collect();

        for (id, subtree, modulators) in scopes {
            let Some(index) = self.nodes.iter().position(|n| n.id == id) else {
                continue;
            };
            let node = &mut self.nodes[index];
            for send in &mut node.sends {
                resolved += usize::from(resolve_one(&mut send.target, &global));
            }
            for route in &mut node.mod_routes {
                resolved += usize::from(resolve_one(&mut route.target, &|name| {
                    lookup_in(&subtree, name)
                }));
                if let ModSource::Node { node: source } = &mut route.source {
                    resolved +=
                        usize::from(resolve_one(source, &|name| lookup_in(&modulators, name)));
                }
            }
        }
        resolved
    }

    /// Every node at or below `root`, including the modulators hanging off
    /// each of them — a mod route's target is a block in this subtree.
    #[must_use]
    pub fn subtree_of(&self, root: &NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(id) = stack.pop() {
            if out.contains(&id) {
                continue;
            }
            out.push(id.clone());
            if let Some(node) = self.get(&id) {
                stack.extend(node.children().iter().cloned());
                stack.extend(node.modulators.iter().cloned());
            }
        }
        out
    }

    /// The modulators a node may draw on: its own, then its ancestors' —
    /// nearest first, so an inner "Filter Env" wins over an outer one.
    #[must_use]
    pub fn modulator_scope_of(&self, node: &NodeId) -> Vec<NodeId> {
        let mut out = self
            .get(node)
            .map(|n| n.modulators.clone())
            .unwrap_or_default();
        let mut current = node.clone();
        // Walk up by looking for whoever references this node. A node with
        // two parents takes the first, which is the same ambiguity sharing a
        // node always carries.
        while let Some(parent) = self
            .nodes
            .iter()
            .find(|n| n.children().contains(&current))
            .map(|n| n.id.clone())
        {
            if parent == current {
                break;
            }
            if let Some(node) = self.get(&parent) {
                out.extend(node.modulators.iter().cloned());
            }
            current = parent;
        }
        out
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
