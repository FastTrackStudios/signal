//! Motion presets: the animations hardware parts have, one call each.
//!
//! Each returns a [`Motion`] with a sensible target; retarget it with
//! [`Motion::on`], rescale with [`Motion::range`], and attach it with
//! [`add`] (or push onto `Node::motion` before inserting the node):
//!
//! ```ignore
//! use surface_kit::motion;
//! motion::add(b, needle, motion::spring_follow("wah/position").range(-20.0, 20.0));
//! motion::add(b, coil, motion::ring_on("spring/in", 5.0, "0.3 + 3*norm(spring/dwell)"));
//! motion::add(b, reel, motion::spin("0.3 + 1.2*norm(tape/speed)"));
//! motion::add(b, filament, motion::flicker("0.25*norm(tube/drive)"));
//! ```
//!
//! Sources are motion expressions (see `motion_proto::expr`): `norm(addr)`,
//! `param(addr)`, `level(feed)`, `scalar(feed)`, `time`, …

use document_proto::NodeId;
pub use document_proto::motion::{Dynamics, Easing, Motion, Num, Pivot, Target, Wave};
use surface_widgets::Builder;

/// Attach `m` to node `id` (after any motions it already has).
pub fn add(b: &mut Builder<'_>, id: NodeId, m: Motion) {
    if let Some(n) = b.scene.node_mut(id) {
        n.motion.push(m);
    }
}

/// A part that follows a param through a lively spring (a slight
/// overshoot, settled in about half a second): rotates 0..1 by default —
/// set the travel with `.range(from, to)`.
#[must_use]
pub fn spring_follow(addr: &str) -> Motion {
    Motion::new(Target::Rotate)
        .input(format!("norm({addr})"))
        .dynamics(Dynamics::spring(260.0, 20.0))
}

/// Physics on a part a control already moves (a needle, a treadle, a
/// lamp's glow): the spring's lag behind `input`, added to what the
/// control shows — so the part swings past and settles. `range` is the
/// control's travel for a position of 1 (a 300° knob: `(0, 300)`).
#[must_use]
pub fn spring_lag(input: &str, target: Target, travel: f64) -> Motion {
    Motion::new(target)
        .input(input)
        .dynamics(Dynamics::spring(220.0, 14.0))
        .range(0.0, travel)
        .lag()
}

/// A struck resonator: at rest until the level feed rises, then it rings
/// at `freq` Hz and dies away over `decay` seconds (to −60 dB) — a
/// number or an expression like `"param(room/decay)"` or
/// `"0.3 + 3*norm(spring/dwell)"`. Moves the node down/up by default, ±1
/// px per unit of rise; scale with `.range(0, px)`.
#[must_use]
pub fn ring_on(level_feed: &str, freq: impl Into<Num>, decay: impl Into<Num>) -> Motion {
    Motion::new(Target::Y).dynamics(Dynamics::Oscillator {
        freq: freq.into(),
        decay: Some(decay.into()),
        damping: None,
        excite: format!("level({level_feed})"),
        gain: Num::Value(1.0),
    })
}

/// Turn continuously at `turns_per_second` (an expression: a reel on
/// `"0.3 + 1.2*norm(tape/speed)"`), about the node's centre.
#[must_use]
pub fn spin(turns_per_second: &str) -> Motion {
    Motion::new(Target::Rotate)
        .input(turns_per_second)
        .dynamics(Dynamics::Spin)
        .range(0.0, 360.0)
}

/// A filament's flicker: the node's light (its emission) wavers by
/// `amount` (0.1 = ±10 %) about its level, eight times a second.
#[must_use]
pub fn flicker(amount: impl Into<Num>) -> Motion {
    Motion::new(Target::Emission).dynamics(Dynamics::Noise {
        rate: Num::Value(8.0),
        amount: amount.into(),
    })
}

/// A lamp pulsing with an LFO at `rate` Hz (a number or `"param(x/rate)"`),
/// its opacity between `low` and 1.
#[must_use]
pub fn pulse(rate: impl Into<Num>, low: f64) -> Motion {
    Motion::new(Target::Opacity)
        .dynamics(Dynamics::Lfo {
            rate: rate.into(),
            shape: Wave::Sine,
            phase: 0.0,
        })
        .range(low, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_valid_motions() {
        for m in [
            spring_follow("a/b").range(-20.0, 20.0),
            spring_lag("norm(a/b)", Target::Rotate, 300.0),
            ring_on("a/in", 5.0, "0.3 + 3*norm(a/dwell)"),
            spin("0.5 + norm(a/speed)"),
            flicker("0.2*norm(a/drive)"),
            pulse("param(a/rate)", 0.2),
        ] {
            assert!(m.problems().is_empty(), "{m:?}: {:?}", m.problems());
        }
        assert_eq!(spin("1").to, 360.0);
    }
}
