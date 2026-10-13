//! The node tree as a UI needs to draw and address it.
//!
//! The wire-facing view of [`Node`](crate::node::Node): what a surface needs
//! to render the rig and change it, without the resolver's machinery. Both
//! rigs resolve the same domain, so both describe it with the same words —
//! this is in `signal-proto` rather than in either rig's contract for that
//! reason.

use facet::Facet;

use crate::block::BlockType;

/// One preset a node can be recalled as — a [`Variant`] of it, on the wire.
///
/// "Preset" is the player's word and `Variant` is the domain's; they are the
/// same thing seen from two sides. A pedal's presets are its captures, a
/// module's are the combinations of its blocks, an amp's are the models it
/// can load.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct LivePreset {
    /// The variant's id — what a rig's `select_preset`
    /// takes. An id rather than an index into the list beside it, because a
    /// list's order changes when a capture is imported and an index does not
    /// survive that.
    pub id: String,
    pub name: String,
}

/// One node of the live rig, as the UI needs to draw and address it.
///
/// A flat list in tree order with an explicit `depth`, rather than a nested
/// structure: the wire contract is `Facet`-encoded and a recursive type is
/// awkward to encode, while every surface that renders this — a chain strip,
/// an indented list, a routing graph — walks it in order anyway.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct LiveNode {
    /// Stable node id. What every edit addresses.
    pub id: String,
    pub name: String,
    /// What it means to a player: `preset` / `engine` / `layer` / `module`.
    /// A UI groups and indents by this; it carries no behaviour.
    pub role: String,
    /// How deep in the tree, for indenting a flat list.
    pub depth: u32,
    /// True for a leaf — a block, the only thing that processes audio.
    pub is_block: bool,
    /// The block's type, when this is one. Empty for a container.
    pub block_type: Option<BlockType>,
    /// Whether this node (or its whole subtree) is bypassed.
    pub bypassed: bool,
    /// The presets this node can be recalled as. Empty means it has only its
    /// default — most blocks, until someone saves a second setting.
    pub presets: Vec<LivePreset>,
    /// Which of them is loaded.
    pub preset_id: String,
    /// What else could sit in this slot — a *different* node, not a different
    /// setting of this one.
    ///
    /// The distinction the domain draws and a player also draws: presets are
    /// "this pedal, its other settings"; alternatives are "a different pedal
    /// in this slot". One is a variant, the other is a `ReplaceRef`, and
    /// conflating them is how you end up unable to say either.
    ///
    /// Empty where nothing else fits, which is most of a fixed chain.
    #[facet(default)]
    pub alternatives: Vec<LivePreset>,
}
