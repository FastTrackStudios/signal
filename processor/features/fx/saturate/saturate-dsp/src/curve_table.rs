//! A closed-form curve, tabulated at construction for cheap per-sample use.
//!
//! The circuit models' curves cost tens of transcendental calls a sample in
//! closed form. This samples a curve (value and slope) on three uniform
//! grids — a fine one over its knee, coarse ones either side — and reads it
//! back by cubic Hermite interpolation. Left of the table the curve is on its
//! asymptote (a constant, or the identity); right of it the closed form is
//! called directly, so the table is a cache, never a clamp.
//!
//! Built from the equations themselves (no measured data), off the audio
//! thread; `eval` neither allocates nor branches beyond the segment choice.

use alloc::vec::Vec;

/// What the curve does left of its table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Left {
    /// A constant (the curve's limit).
    Const(f64),
    /// `f(x) = x`.
    Identity,
}

/// One uniform segment: nodes `lo + i·h`, stored as `[y, h·y']`.
#[derive(Clone, Copy, Debug)]
struct Segment {
    lo: f64,
    hi: f64,
    inv_h: f64,
    /// Index of the segment's first node in `nodes`.
    first: usize,
    /// Its last valid interval index.
    last: usize,
}

/// A three-segment Hermite table of `f`, with `f` itself past the right end.
#[derive(Clone, Debug)]
pub struct CurveTable {
    left: Left,
    below: Segment,
    fine: Segment,
    above: Segment,
    nodes: Vec<[f64; 2]>,
    direct: fn(f64) -> f64,
}

/// Slope by the five-point stencil.
fn slope(curve: fn(f64) -> f64, at: f64) -> f64 {
    let d = 1e-3;
    (curve(at - 2.0 * d) - 8.0 * curve(at - d) + 8.0 * curve(at + d) - curve(at + 2.0 * d)) / (12.0 * d)
}

#[expect(clippy::as_conversions, clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a non-negative, bounded grid count")]
fn count(lo: f64, hi: f64, step: f64) -> usize {
    libm::round((hi - lo) / step).max(1.0) as usize
}

impl CurveTable {
    /// Tabulate `curve` at `edges = [lo, knee_lo, knee_hi, hi]`: the knee at
    /// step `steps[1]`, either side of it at `steps[0]` and `steps[2]`.
    #[must_use]
    pub fn new(curve: fn(f64) -> f64, left: Left, edges: [f64; 4], steps: [f64; 3]) -> Self {
        let [lo, knee_lo, knee_hi, hi] = edges;
        let [step_below, step_fine, step_above] = steps;
        let mut nodes = Vec::new();
        let mut segment = |from: f64, to: f64, step: f64| {
            let intervals = count(from, to, step);
            let step = (to - from) / dsp_core::num::count_to_f64(intervals);
            let first = nodes.len();
            for node in 0..=intervals {
                let at = from + step * dsp_core::num::count_to_f64(node);
                nodes.push([curve(at), step * slope(curve, at)]);
            }
            Segment { lo: from, hi: to, inv_h: 1.0 / step, first, last: intervals.saturating_sub(1) }
        };
        let below = segment(lo, knee_lo, step_below);
        let fine = segment(knee_lo, knee_hi, step_fine);
        let above = segment(knee_hi, hi, step_above);
        Self { left, below, fine, above, nodes, direct: curve }
    }

    #[inline]
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "the grid position is non-negative and below the segment's length (a few thousand)"
    )]
    fn hermite(&self, seg: &Segment, at: f64) -> f64 {
        let pos = (at - seg.lo) * seg.inv_h;
        let cell = (pos as usize).min(seg.last);
        let frac = pos - cell as f64;
        let node = seg.first.saturating_add(cell);
        let (Some(&[y0, m0]), Some(&[y1, m1])) = (self.nodes.get(node), self.nodes.get(node.saturating_add(1))) else {
            return (self.direct)(at);
        };
        let rise = y1 - y0;
        ((((m0 + m1) - 2.0 * rise) * frac + (3.0 * rise - 2.0 * m0 - m1)) * frac + m0) * frac + y0
    }

    /// `f(x)`.
    #[inline]
    #[must_use]
    pub fn eval(&self, x: f64) -> f64 {
        if x < self.fine.lo {
            if x >= self.below.lo {
                self.hermite(&self.below, x)
            } else {
                match self.left {
                    Left::Const(c) => c,
                    Left::Identity => x,
                }
            }
        } else if x < self.fine.hi {
            self.hermite(&self.fine, x)
        } else if x < self.above.hi {
            self.hermite(&self.above, x)
        } else {
            (self.direct)(x)
        }
    }

    /// Largest `|table − f|` over a dense sweep of `[from, to]` (for tests).
    #[must_use]
    pub fn max_error(&self, from: f64, to: f64, steps: usize) -> f64 {
        let n = dsp_core::num::count_to_f64(steps.max(1));
        (0..=steps)
            .map(|i| {
                let x = from + (to - from) * dsp_core::num::count_to_f64(i) / n;
                (self.eval(x) - (self.direct)(x)).abs()
            })
            .fold(0.0, f64::max)
    }

    /// Bytes held by the table.
    #[must_use]
    pub const fn bytes(&self) -> usize {
        self.nodes.len().saturating_mul(16)
    }
}
