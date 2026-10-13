//! Multi Tool — zones over the selection, each a different transform.
//!
//! Modelled on juliansader's `js_Mouse editing - Multi Tool.lua`. The
//! idea it gets right: instead of remembering a dozen modifier
//! combinations, the tool **shows you the zones**. Where the drag
//! starts picks the transform, and the zones are drawn over the
//! selection's bounding box so the choice is spatial rather than
//! memorised.
//!
//! Two design points carried over deliberately:
//!
//! - Transforms run from a **captured snapshot**, never from the
//!   current state. Re-deriving from the live curve each frame
//!   accumulates rounding error, and the Lua original calls this out as
//!   the reason it keeps float positions between steps.
//! - The curve shape and its steepness are adjustable **mid-gesture**,
//!   with a detent at neutral so returning to linear is easy. That is
//!   most of why the tool feels powerful rather than fiddly.

use crate::shape::smoothstep;

/// Which transform a zone performs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Zone {
    /// Squeeze values toward the top edge, by a curve across time.
    CompressTop,
    CompressBottom,
    /// Scale values about the opposite edge.
    ScaleTop,
    ScaleBottom,
    /// Redistribute positions in time, bunching toward one end.
    Warp,
    /// Scale positions from the left/right edge.
    StretchLeft,
    StretchRight,
    /// Ramp values from one side, hinged at the other.
    TiltLeft,
    TiltRight,
    /// Offset both axes.
    Move,
    Undo,
    Redo,
}

impl Zone {
    pub const ALL: [Zone; 12] = [
        Zone::CompressTop,
        Zone::CompressBottom,
        Zone::ScaleTop,
        Zone::ScaleBottom,
        Zone::Warp,
        Zone::StretchLeft,
        Zone::StretchRight,
        Zone::TiltLeft,
        Zone::TiltRight,
        Zone::Move,
        Zone::Undo,
        Zone::Redo,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Zone::CompressTop => "Compress top",
            Zone::CompressBottom => "Compress bottom",
            Zone::ScaleTop => "Scale top",
            Zone::ScaleBottom => "Scale bottom",
            Zone::Warp => "Warp",
            Zone::StretchLeft => "Stretch left",
            Zone::StretchRight => "Stretch right",
            Zone::TiltLeft => "Tilt left",
            Zone::TiltRight => "Tilt right",
            Zone::Move => "Move",
            Zone::Undo => "Undo",
            Zone::Redo => "Redo",
        }
    }

    /// The mousewheel alternative, per the original's zone table.
    pub fn wheel_label(&self) -> Option<&'static str> {
        Some(match self {
            Zone::CompressTop | Zone::CompressBottom | Zone::Move => "Flip absolute",
            Zone::ScaleTop | Zone::ScaleBottom => "Flip relative",
            Zone::Warp => "Even out",
            Zone::StretchLeft | Zone::StretchRight => "Reverse",
            Zone::TiltLeft | Zone::TiltRight => "Snap to chased",
            Zone::Undo | Zone::Redo => return None,
        })
    }

    /// Zones that move events in time rather than changing values.
    ///
    /// These are the only ones available when several lanes are being
    /// edited together, because a value transform needs a single dimension's
    /// range to be meaningful.
    pub fn is_positional(&self) -> bool {
        matches!(
            self,
            Zone::Warp | Zone::StretchLeft | Zone::StretchRight | Zone::Move
        )
    }
}

/// Curve family used by the value transforms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bend {
    /// Symmetric ease — the original's "sine".
    Sine,
    /// Exponential-ish — the original's "power".
    Power,
}

impl Bend {
    pub fn toggled(self) -> Self {
        match self {
            Bend::Sine => Bend::Power,
            Bend::Power => Bend::Sine,
        }
    }
}

/// Curve steepness, adjustable mid-gesture.
///
/// Zero is linear. The sign selects which end the curve favours, so a
/// single wheel sweep can cross from one extreme to the other through a
/// well-defined neutral.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steepness(pub f64);

impl Default for Steepness {
    fn default() -> Self {
        Steepness(0.0)
    }
}

impl Steepness {
    pub const MAX: f64 = 4.0;

    /// Apply a wheel delta, with a detent at neutral.
    ///
    /// Sweeping across zero *pauses* there rather than sliding through,
    /// so linear is easy to return to — without this the control has no
    /// findable home and every gesture ends slightly bent.
    pub fn nudge(self, delta: f64) -> Self {
        const DETENT: f64 = 0.18;
        let next = self.0 + delta;
        let crossed = self.0.signum() != next.signum() && self.0 != 0.0;
        if crossed && next.abs() < DETENT {
            return Steepness(0.0);
        }
        if self.0 == 0.0 && next.abs() < DETENT {
            return Steepness(0.0);
        }
        Steepness(next.clamp(-Self::MAX, Self::MAX))
    }

    pub fn is_neutral(&self) -> bool {
        self.0.abs() < 1e-9
    }

    /// Map `x` in 0..1 through the curve.
    pub fn curve(&self, x: f64, bend: Bend) -> f64 {
        let x = x.clamp(0.0, 1.0);
        if self.is_neutral() {
            return x;
        }
        let k = self.0.abs();
        let shaped = match bend {
            Bend::Sine => {
                let s = smoothstep(x);
                // Blend from linear toward full smoothstep as k rises,
                // then past it into a harder S.
                let t = (k / Self::MAX).clamp(0.0, 1.0);
                x + (s - x) * t
            }
            Bend::Power => x.powf(1.0 + k),
        };
        if self.0 >= 0.0 {
            shaped
        } else {
            // Negative steepness mirrors the curve, favouring the other
            // end rather than being a separate shape.
            1.0 - match bend {
                Bend::Sine => {
                    let s = smoothstep(1.0 - x);
                    let t = (k / Self::MAX).clamp(0.0, 1.0);
                    (1.0 - x) + (s - (1.0 - x)) * t
                }
                Bend::Power => (1.0 - x).powf(1.0 + k),
            }
        }
    }
}

/// A point being transformed: time and value, both floats so repeated
/// steps do not accumulate quantisation error.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub t: f64,
    pub value: f64,
}

/// The captured state a gesture transforms.
///
/// Holding the original means every frame recomputes from the same
/// input rather than compounding — the reason the Lua original keeps
/// float positions between steps.
#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
    pub points: Vec<Pt>,
    pub t0: f64,
    pub t1: f64,
    pub v_lo: f64,
    pub v_hi: f64,
}

impl Capture {
    pub fn new(points: Vec<Pt>) -> Option<Self> {
        if points.is_empty() {
            return None;
        }
        let mut t0 = f64::MAX;
        let mut t1 = f64::MIN;
        let mut v_lo = f64::MAX;
        let mut v_hi = f64::MIN;
        for p in &points {
            t0 = t0.min(p.t);
            t1 = t1.max(p.t);
            v_lo = v_lo.min(p.value);
            v_hi = v_hi.max(p.value);
        }
        Some(Self {
            points,
            t0,
            t1,
            v_lo,
            v_hi,
        })
    }

    fn tx(&self, t: f64) -> f64 {
        let span = self.t1 - self.t0;
        if span.abs() < 1e-12 {
            0.0
        } else {
            ((t - self.t0) / span).clamp(0.0, 1.0)
        }
    }

    fn v_span(&self) -> f64 {
        (self.v_hi - self.v_lo).max(1e-9)
    }
}

/// How far a gesture has been dragged, normalized.
///
/// `-1..1` for most zones; the caller maps pixels to this so the core
/// never learns about screens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub amount: f64,
    /// Two-sided mode: the transform mirrors about the centre.
    pub symmetric: bool,
}

/// Run a zone's transform against a capture.
///
/// Returns the transformed points in the capture's own order, so the
/// caller can splice them straight back.
pub fn apply(zone: Zone, cap: &Capture, drag: Drag, bend: Bend, steep: Steepness) -> Vec<Pt> {
    let mut out = cap.points.clone();
    let a = drag.amount;

    match zone {
        Zone::CompressTop | Zone::CompressBottom => {
            let from_top = zone == Zone::CompressTop;
            for p in out.iter_mut() {
                let x = cap.tx(p.t);
                // The envelope is strongest where the curve says so,
                // which is what makes a compress taper across the
                // selection instead of squashing it uniformly.
                let w = envelope(x, drag.symmetric, bend, steep);
                let edge = if from_top { cap.v_hi } else { cap.v_lo };
                let target = edge;
                p.value += (target - p.value) * (a.clamp(-1.0, 1.0) * w);
            }
        }
        Zone::ScaleTop | Zone::ScaleBottom => {
            // Scale about the OPPOSITE edge: grabbing the top and
            // pulling down should shrink the range toward the bottom,
            // not slide the whole thing.
            let pivot = if zone == Zone::ScaleTop {
                cap.v_lo
            } else {
                cap.v_hi
            };
            let factor = 1.0 + a;
            for p in out.iter_mut() {
                let x = cap.tx(p.t);
                let w = envelope(x, drag.symmetric, bend, steep);
                let f = 1.0 + (factor - 1.0) * w;
                p.value = pivot + (p.value - pivot) * f;
            }
        }
        Zone::TiltLeft | Zone::TiltRight => {
            // Hinged at the far end: a tilt is a ramp, so one side must
            // stay put or it is just a move.
            let from_left = zone == Zone::TiltLeft;
            for p in out.iter_mut() {
                let x = cap.tx(p.t);
                let along = if from_left { 1.0 - x } else { x };
                let w = steep.curve(along, bend);
                p.value += a * cap.v_span() * w;
            }
        }
        Zone::Warp => {
            // Redistribute positions: same points, new spacing.
            let span = cap.t1 - cap.t0;
            for p in out.iter_mut() {
                let x = cap.tx(p.t);
                let warped = warp_x(x, a, bend, steep);
                p.t = cap.t0 + warped * span;
            }
        }
        Zone::StretchLeft | Zone::StretchRight => {
            let anchor = if zone == Zone::StretchLeft {
                cap.t1
            } else {
                cap.t0
            };
            let factor = (1.0 + a).max(0.01);
            for p in out.iter_mut() {
                p.t = anchor + (p.t - anchor) * factor;
            }
        }
        Zone::Move => {
            let dt = a * (cap.t1 - cap.t0);
            for p in out.iter_mut() {
                p.t += dt;
            }
        }
        Zone::Undo | Zone::Redo => {}
    }
    out
}

/// The wheel alternative for a zone.
pub fn apply_wheel(zone: Zone, cap: &Capture) -> Vec<Pt> {
    let mut out = cap.points.clone();
    match zone {
        Zone::CompressTop | Zone::CompressBottom | Zone::Move => {
            // Flip absolute: mirror about the middle of the FULL range,
            // so a quiet passage becomes a loud one.
            let mid = (cap.v_lo + cap.v_hi) * 0.5;
            for p in out.iter_mut() {
                p.value = 2.0 * mid - p.value;
            }
        }
        Zone::ScaleTop | Zone::ScaleBottom => {
            // Flip relative: mirror about each point's own deviation
            // from the mean, preserving the overall level.
            let mean = out.iter().map(|p| p.value).sum::<f64>() / out.len().max(1) as f64;
            for p in out.iter_mut() {
                p.value = 2.0 * mean - p.value;
            }
        }
        Zone::Warp => {
            // Even out: same values, evenly spaced.
            let n = out.len();
            if n > 1 {
                let span = cap.t1 - cap.t0;
                for (i, p) in out.iter_mut().enumerate() {
                    p.t = cap.t0 + span * (i as f64 / (n - 1) as f64);
                }
            }
        }
        Zone::StretchLeft | Zone::StretchRight => {
            // Reverse positions, mirroring each point's whole offset.
            for p in out.iter_mut() {
                p.t = cap.t0 + (cap.t1 - p.t);
            }
            out.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(core::cmp::Ordering::Equal));
        }
        Zone::TiltLeft | Zone::TiltRight => {
            // Snap to chased: hold each point at the previous value,
            // producing a staircase from the side being tilted.
            let from_left = zone == Zone::TiltLeft;
            if from_left {
                for i in 1..out.len() {
                    out[i].value = out[i - 1].value;
                }
            } else {
                for i in (0..out.len().saturating_sub(1)).rev() {
                    out[i].value = out[i + 1].value;
                }
            }
        }
        Zone::Undo | Zone::Redo => {}
    }
    out
}

/// Weight across the selection for a value transform.
fn envelope(x: f64, symmetric: bool, bend: Bend, steep: Steepness) -> f64 {
    if symmetric {
        // Mirror about the centre so both ends move together.
        let d = (x - 0.5).abs() * 2.0;
        1.0 - steep.curve(d, bend)
    } else {
        steep.curve(x, bend)
    }
}

/// Position redistribution for [`Zone::Warp`].
fn warp_x(x: f64, amount: f64, bend: Bend, steep: Steepness) -> f64 {
    let shaped = steep.curve(x, bend);
    // `amount` blends between the identity and the shaped curve, and
    // negative amounts blend toward its inverse — so one axis of drag
    // covers "bunch left" through "even" to "bunch right".
    let target = if amount >= 0.0 {
        shaped
    } else {
        1.0 - steep.curve(1.0 - x, bend)
    };
    let blend = amount.abs().clamp(0.0, 1.0);
    (x + (target - x) * blend).clamp(0.0, 1.0)
}

/// Where each zone sits over the selection's bounding box.
///
/// Fractions of the box, so the caller multiplies by pixels. Edge bands
/// are deliberately generous — these are grab targets under a moving
/// pointer, not precise controls.
pub fn layout(zone: Zone) -> (f64, f64, f64, f64) {
    const E: f64 = 0.16;
    match zone {
        Zone::CompressTop => (E, 0.0, 1.0 - 2.0 * E, E),
        Zone::CompressBottom => (E, 1.0 - E, 1.0 - 2.0 * E, E),
        Zone::ScaleTop => (E, E, 1.0 - 2.0 * E, E),
        Zone::ScaleBottom => (E, 1.0 - 2.0 * E, 1.0 - 2.0 * E, E),
        Zone::StretchLeft => (0.0, E, E, 1.0 - 2.0 * E),
        Zone::StretchRight => (1.0 - E, E, E, 1.0 - 2.0 * E),
        Zone::TiltLeft => (0.0, 0.0, E, E),
        Zone::TiltRight => (1.0 - E, 0.0, E, E),
        Zone::Undo => (0.0, 1.0 - E, E, E),
        Zone::Redo => (1.0 - E, 1.0 - E, E, E),
        // The middle is the most-used transform, so it gets the space
        // that needs no aiming.
        Zone::Warp => (2.0 * E, 2.0 * E, 1.0 - 4.0 * E, 1.0 - 4.0 * E),
        Zone::Move => (E, 2.0 * E, E, 1.0 - 4.0 * E),
    }
}

/// The zone at a normalized point in the box, if any.
///
/// Searched in [`Zone::ALL`] order with corners first, so a small
/// deliberate target is never shadowed by the large one behind it.
pub fn zone_at(x: f64, y: f64) -> Option<Zone> {
    let mut best: Option<(f64, Zone)> = None;
    for z in Zone::ALL {
        let (zx, zy, zw, zh) = layout(z);
        if x >= zx && x <= zx + zw && y >= zy && y <= zy + zh {
            let area = zw * zh;
            if best.as_ref().is_none_or(|(a, _)| area < *a) {
                best = Some((area, z));
            }
        }
    }
    best.map(|(_, z)| z)
}
