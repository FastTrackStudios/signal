//! The routing axis of the node model: where audio goes besides down, what
//! drives a parameter, and which notes reach a subtree.
//!
//! Distinct from [`crate::routing`], which is about the rig's *edges* — where
//! signal enters from and leaves to. This module is about movement *inside*
//! a [`NodeLibrary`](crate::node::NodeLibrary).
//!
//! Containment says what holds what — a Layer inside an Engine. Routing is
//! independent of it: a lane's output also feeds a shared rotary, an envelope
//! drives a filter three levels away. The tree groups and owns; these route.
//!
//! These types lived in `signal-sampler` and are domain, not audio. A Layer's
//! key split and its fader are things a *player* sets, so they belong beside
//! [`Node`](crate::node::Node) in the wire contract rather than in the crate
//! that renders it.
//!
//! # Names author, ids store
//!
//! A route's target is a [`NodeRef`]: an id, or a name. Both exist because
//! they are for different moments.
//!
//! Writing a rig, a name is the only thing available — a lane sends "To
//! Rotary" before the Rotary exists, so a builder cannot hold its id. Once
//! the tree is complete the names are resolved to ids in one pass
//! ([`NodeRef::Name`] → [`NodeRef::Id`]), and from then on renaming the
//! Rotary cannot break the send. That was the hazard: in the sampler's
//! originals a route target was a name *permanently*, resolved afresh at
//! render time, so a rename silently stopped it applying — not at edit time,
//! not at switch time, on stage.
//!
//! A name that resolves to nothing stays a name. It is still wrong, but it
//! is wrong in a way a person can read.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::node::NodeId;

/// How a route names the node it points at.
///
/// See the module docs: a name is what authoring has, an id is what storage
/// keeps. Struct variants because facet-styx cannot read back a newtype
/// tuple variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum NodeRef {
    /// Resolved. Survives any rename.
    Id { id: NodeId },
    /// Authored, or unresolvable. Matched case-insensitively by display name.
    Name { name: String },
}

impl NodeRef {
    /// A reference to a node whose id is known.
    #[must_use]
    pub const fn id(id: NodeId) -> Self {
        Self::Id { id }
    }

    /// A reference by display name, for authoring.
    #[must_use]
    pub fn name(name: impl Into<String>) -> Self {
        Self::Name { name: name.into() }
    }

    /// The id, if this reference has been resolved.
    #[must_use]
    pub const fn as_id(&self) -> Option<&NodeId> {
        match self {
            Self::Id { id } => Some(id),
            Self::Name { .. } => None,
        }
    }

    /// The lower-cased string this matches on — an id, or a name.
    ///
    /// Ids are already unique and case-exact; lower-casing one is harmless
    /// and lets both kinds share a lookup table.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Self::Id { id } => id.as_str().to_lowercase(),
            Self::Name { name } => name.to_lowercase(),
        }
    }

    /// Whether this still points by name.
    #[must_use]
    pub const fn is_unresolved(&self) -> bool {
        matches!(self, Self::Name { .. })
    }

    /// Resolve a name against `lookup`, which answers with the id for a
    /// lower-cased name. An id is left alone; an unknown name is left alone.
    pub fn resolve(&mut self, lookup: &impl Fn(&str) -> Option<NodeId>) {
        if let Self::Name { name } = self
            && let Some(id) = lookup(&name.to_lowercase())
        {
            *self = Self::Id { id };
        }
    }
}

impl std::fmt::Display for NodeRef {
    /// The name, or the id. A resolved reference has no name of its own —
    /// only the node it points at does — so anything wanting a readable
    /// label should look the node up rather than format the reference.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Id { id } => f.write_str(id.as_str()),
            Self::Name { name } => f.write_str(name),
        }
    }
}

/// A cross-tree audio send: this node's output also flows to `target`.
///
/// Named `AudioSend` rather than `Send` because a bare `Send` shadows the
/// marker trait in every module that imports it, and the first thing it
/// broke was a `Send + Sync` bound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct AudioSend {
    /// The destination node.
    pub target: NodeRef,
    /// Human label for the route, e.g. "To Rotary".
    pub label: String,
}

impl AudioSend {
    pub fn new(target: NodeRef, label: impl Into<String>) -> Self {
        Self {
            target,
            label: label.into(),
        }
    }
}

/// Where a modulation route gets its signal.
///
/// Either a modulator node — an envelope, an LFO — or a gesture the player
/// makes. The second kind has no node to point at, which is why this is an
/// enum rather than a bare [`NodeRef`].
///
/// The gesture's name is **not** a vocabulary this crate owns. "Wheel",
/// "CC74", "MPE Timbre", "Key Track" are resolved by whatever renders the
/// route, and that list grows with the renderer, not with the domain — the
/// same reasoning as [`Setting`]. What the domain knows is that this route's
/// signal comes from the player rather than from a node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum ModSource {
    /// A modulator node — an envelope, an LFO, an arpeggiator.
    Node { node: NodeRef },
    /// A performance gesture, named in the renderer's vocabulary.
    Performance { name: String },
}

impl ModSource {
    /// A gesture by name.
    #[must_use]
    pub fn performance(name: impl Into<String>) -> Self {
        Self::Performance { name: name.into() }
    }

    /// A modulator, by whatever the author had — an id or a name.
    #[must_use]
    pub const fn node(node: NodeRef) -> Self {
        Self::Node { node }
    }

    /// The string a renderer looks this up by: a node's id or name, or the
    /// gesture's name. Lower-cased, like [`NodeRef::key`].
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Self::Node { node } => node.key(),
            Self::Performance { name } => name.to_lowercase(),
        }
    }
}

impl std::fmt::Display for ModSource {
    /// The node's id or name, or the gesture's name — see
    /// [`NodeRef`]'s `Display` for why a resolved reference has no better
    /// label to offer.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Node { node } => node.fmt(f),
            Self::Performance { name } => f.write_str(name),
        }
    }
}

/// One row of the modulation matrix: a source drives one parameter of one
/// block, scaled by `depth`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ModRoute {
    pub source: ModSource,
    /// The block whose parameter moves.
    pub target: NodeRef,
    /// The parameter's id on that block.
    pub parameter: String,
    /// −1..=1 scale of the source into the parameter's normalized range,
    /// added to its base value each block.
    pub depth: f32,
}

/// A node-level setting that is not a block — a Layer's `voice_mode` or
/// `octave`, an Engine's menu options.
///
/// Stringly-typed on purpose: these are open-ended and per-node-kind, and an
/// unknown one must survive a round trip rather than be dropped for not
/// matching an enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct Setting {
    pub name: String,
    pub value: String,
}

/// The keyboard window a node occupies — the MIDI router's per-node rule.
///
/// A note must fall in both the key and velocity windows to reach this
/// subtree; crossfade edges blend it in and out. The combined gain scales the
/// note's velocity into the subtree, so a note in a crossfade region plays
/// adjacent layers at partial level — a real blend rather than a hard pick.
/// Nested zones multiply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct Zone {
    /// Lowest playable key (MIDI note).
    pub key_lo: u8,
    /// Highest playable key.
    pub key_hi: u8,
    /// Crossfade width in semitones at each key edge. 0 = hard split.
    pub key_xfade: u8,
    /// Lowest velocity that sounds.
    pub vel_lo: u8,
    /// Highest velocity that sounds.
    pub vel_hi: u8,
    /// Crossfade width in velocity units at each edge. 0 = hard.
    pub vel_xfade: u8,
}

impl Default for Zone {
    fn default() -> Self {
        Self::full()
    }
}

impl Zone {
    /// The everything-passes zone — full range, no crossfade.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            key_lo: 0,
            key_hi: 127,
            key_xfade: 0,
            vel_lo: 1,
            vel_hi: 127,
            vel_xfade: 0,
        }
    }

    /// A key split with hard edges.
    #[must_use]
    pub const fn keys(lo: u8, hi: u8) -> Self {
        Self {
            key_lo: lo,
            key_hi: hi,
            ..Self::full()
        }
    }

    #[must_use]
    pub fn is_full(&self) -> bool {
        *self == Self::full()
    }

    /// Key-axis gain for `key`.
    #[must_use]
    pub fn key_gain(&self, key: u8) -> f32 {
        ramp(key, self.key_lo, self.key_hi, self.key_xfade)
    }

    /// Velocity-axis gain for `vel`.
    #[must_use]
    pub fn vel_gain(&self, vel: u8) -> f32 {
        ramp(vel, self.vel_lo, self.vel_hi, self.vel_xfade)
    }

    /// Combined routing gain for a note, in `0..=1`.
    #[must_use]
    pub fn note_gain(&self, key: u8, vel: u8) -> f32 {
        self.key_gain(key) * self.vel_gain(vel)
    }
}

/// Trapezoidal window gain: 0 outside `[lo, hi]`, ramped across `xfade` at
/// each edge, 1 in the middle. `xfade == 0` is a hard window.
fn ramp(x: u8, lo: u8, hi: u8, xfade: u8) -> f32 {
    let (x, lo, hi, xf) = (f32::from(x), f32::from(lo), f32::from(hi), f32::from(xfade));

    let rising = if xf == 0.0 {
        f32::from(u8::from(x >= lo))
    } else {
        (x - lo) / xf
    };
    let falling = if xf == 0.0 {
        f32::from(u8::from(x <= hi))
    } else {
        (hi - x) / xf
    };
    rising.min(falling).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_zone_passes_everything() {
        let z = Zone::full();
        assert!(z.is_full());
        for key in [0u8, 60, 127] {
            assert!((z.note_gain(key, 100) - 1.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn a_hard_split_silences_the_other_side() {
        // The worship keys rig's Rhodes lane: below middle C only.
        let under = Zone::keys(0, 59);
        assert!(under.note_gain(48, 100) > 0.99);
        assert!(
            under.note_gain(72, 100) < f32::EPSILON,
            "silent above the split"
        );
    }

    /// The point of a crossfade: a note in the overlap plays both neighbours
    /// at partial level, rather than one of them at full.
    #[test]
    fn a_crossfade_blends_rather_than_picks() {
        let z = Zone {
            key_lo: 60,
            key_hi: 72,
            key_xfade: 12,
            ..Zone::full()
        };
        let edge = z.key_gain(66);
        assert!(
            edge > 0.0 && edge < 1.0,
            "a note inside the fade is partial, got {edge}"
        );
        assert!(z.key_gain(48) < f32::EPSILON, "and outside is silent");
    }

    /// Once resolved, a route holds an id, so renaming the target cannot
    /// break it — there is no name left to miss.
    #[test]
    fn a_resolved_route_survives_a_rename() {
        let filter = NodeId::new();
        let mut target = NodeRef::name("Filter");
        assert!(target.is_unresolved());

        let id = filter.clone();
        target.resolve(&|name| (name == "filter").then(|| id.clone()));
        assert_eq!(target.as_id(), Some(&filter));

        // The node is renamed. The reference does not care.
        target.resolve(&|_| None);
        assert_eq!(target.as_id(), Some(&filter));
    }

    /// A name that resolves to nothing stays a name: still wrong, but
    /// readable. Silently becoming a dangling id would be worse.
    #[test]
    fn an_unresolvable_name_is_kept() {
        let mut target = NodeRef::name("Rotary");
        target.resolve(&|_| None);
        assert_eq!(target, NodeRef::name("Rotary"));
        assert!(target.is_unresolved());
    }
}
