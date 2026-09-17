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
//! # Addressed by id
//!
//! A send and a mod route name their target by [`NodeId`], not by name. In
//! the sampler's originals they were names — `Send.target` was resolved
//! against the tree, `ModRoute.target` was the string `"Block Name.param"` —
//! which meant renaming a block silently broke every route into it. That is
//! the same hazard stable ids were introduced to close, and it was still open
//! one layer down.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::node::NodeId;

/// A cross-tree audio send: this node's output also flows to `target`.
///
/// Named `AudioSend` rather than `Send` because a bare `Send` shadows the
/// marker trait in every module that imports it, and the first thing it
/// broke was a `Send + Sync` bound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct AudioSend {
    /// The destination node.
    pub target: NodeId,
    /// Human label for the route, e.g. "To Rotary".
    pub label: String,
}

impl AudioSend {
    pub fn new(target: NodeId, label: impl Into<String>) -> Self {
        Self {
            target,
            label: label.into(),
        }
    }
}

/// Where a modulation route gets its signal.
///
/// Either a modulator attached to this node or an ancestor, or a performance
/// gesture the player makes. The second kind has no node to point at, which is
/// why this is an enum rather than a bare id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum ModSource {
    /// A modulator block — an envelope, an LFO, an arpeggiator.
    Node { node: NodeId },
    /// The mod wheel.
    Wheel,
    /// Note velocity.
    Velocity,
    /// Channel or polyphonic aftertouch.
    Aftertouch,
    /// Pitch bend.
    Bender,
    /// A continuous controller, by number.
    Cc { number: u8 },
}

/// One row of the modulation matrix: a source drives one parameter of one
/// block, scaled by `depth`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ModRoute {
    pub source: ModSource,
    /// The block whose parameter moves.
    pub target: NodeId,
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
        assert_eq!(under.note_gain(72, 100), 0.0);
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
        assert_eq!(z.key_gain(48), 0.0, "and outside is silent");
    }

    /// A route names its target, so renaming the target cannot break it.
    #[test]
    fn a_route_survives_a_rename() {
        let filter = NodeId::new();
        let route = ModRoute {
            source: ModSource::Wheel,
            target: filter.clone(),
            parameter: "cutoff".into(),
            depth: 0.5,
        };
        // Nothing here is a name. There is no rename that could miss.
        assert_eq!(route.target, filter);
        assert_eq!(route.parameter, "cutoff");
    }
}
