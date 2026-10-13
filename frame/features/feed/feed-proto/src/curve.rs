//! The one response-curve type.
//!
//! Every "shape this 0..1 value" in a surface — a parameter's taper, a macro
//! binding's response, a MIDI CC's feel — is a [`Curve`]. It replaces three
//! parallel systems (CC curves, easing/power response curves, multi-point
//! macro curves) with one serializable, testable function `[0,1] → [0,1]`.

use facet::Facet;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
pub struct CurvePoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Curve {
    /// `y = x`.
    #[default]
    Linear,
    /// `y = x^exponent`: 1 = linear, 2 = slow start, 0.5 = fast start.
    Power { exponent: f64 },
    /// Slow start, fast end (finer control at the low end).
    Exponential,
    /// Fast start, slow end (finer control at the high end).
    Logarithmic,
    /// S-curve: fine control near both ends.
    SmoothStep,
    /// Off below `threshold`, on at or above it.
    Toggle { threshold: f64 },
    /// Snap to `steps` evenly spaced values (steps ≥ 2).
    Stepped { steps: u32 },
    /// Piecewise-linear through points (sorted by `x`; clamped outside).
    Points { points: Vec<CurvePoint> },
    /// `inner` mirrored: `1 - inner(1 - x)`.
    Inverted { inner: Box<Curve> },
}

/// How strongly the exponential/logarithmic shapes bend.
const BEND: f64 = 4.0;

impl Curve {
    /// Map `x` (clamped to 0..=1) through the curve; the result is in 0..=1.
    #[must_use]
    pub fn apply(&self, x: f64) -> f64 {
        let x = if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) };
        let y = match self {
            Self::Linear => x,
            Self::Power { exponent } => {
                if *exponent > 0.0 {
                    x.powf(*exponent)
                } else {
                    x
                }
            }
            Self::Exponential => (BEND * x).exp_m1() / BEND.exp_m1(),
            Self::Logarithmic => (BEND * x).ln_1p() / BEND.ln_1p(),
            Self::SmoothStep => x * x * (3.0 - 2.0 * x),
            Self::Toggle { threshold } => {
                if x >= *threshold {
                    1.0
                } else {
                    0.0
                }
            }
            Self::Stepped { steps } => {
                let n = f64::from((*steps).max(2) - 1);
                (x * n).round() / n
            }
            Self::Points { points } => piecewise(points, x),
            Self::Inverted { inner } => 1.0 - inner.apply(1.0 - x),
        };
        y.clamp(0.0, 1.0)
    }

    /// The inverse mapping where one exists (monotone curves), for turning
    /// a stored value back into a control position.
    #[must_use]
    pub fn invert(&self, y: f64) -> f64 {
        let y = if y.is_nan() { 0.0 } else { y.clamp(0.0, 1.0) };
        match self {
            Self::Linear | Self::Stepped { .. } | Self::Toggle { .. } => y,
            Self::Power { exponent } if *exponent > 0.0 => y.powf(1.0 / exponent),
            Self::Power { .. } => y,
            Self::Exponential => (y * BEND.exp_m1()).ln_1p() / BEND,
            Self::Logarithmic => (y * BEND.ln_1p()).exp_m1() / BEND,
            // Monotone but no closed form: bisect.
            Self::SmoothStep | Self::Points { .. } | Self::Inverted { .. } => bisect(self, y),
        }
    }
}

fn piecewise(points: &[CurvePoint], x: f64) -> f64 {
    match points {
        [] => x,
        [only] => only.y,
        _ => {
            if x <= points[0].x {
                return points[0].y;
            }
            for w in points.windows(2) {
                let (a, b) = (w[0], w[1]);
                if x <= b.x {
                    let span = b.x - a.x;
                    return if span <= 0.0 { b.y } else { a.y + (b.y - a.y) * (x - a.x) / span };
                }
            }
            points[points.len() - 1].y
        }
    }
}

fn bisect(curve: &Curve, y: f64) -> f64 {
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if curve.apply(mid) < y {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monotone(c: &Curve) -> bool {
        (0..=100).map(|i| c.apply(f64::from(i) / 100.0)).collect::<Vec<_>>().windows(2).all(|w| w[1] >= w[0] - 1e-12)
    }

    #[test]
    fn endpoints_and_monotonicity() {
        let curves = [
            Curve::Linear,
            Curve::Power { exponent: 2.0 },
            Curve::Power { exponent: 0.5 },
            Curve::Exponential,
            Curve::Logarithmic,
            Curve::SmoothStep,
            Curve::Stepped { steps: 5 },
            Curve::Points { points: vec![CurvePoint { x: 0.0, y: 0.2 }, CurvePoint { x: 0.5, y: 0.9 }, CurvePoint { x: 1.0, y: 1.0 }] },
            Curve::Inverted { inner: Box::new(Curve::Exponential) },
        ];
        for c in &curves {
            assert!(monotone(c), "{c:?} not monotone");
            assert!((c.apply(1.0) - 1.0).abs() < 1e-9, "{c:?} at 1");
        }
        assert_eq!(Curve::Exponential.apply(0.0), 0.0);
        assert_eq!(Curve::Logarithmic.apply(0.0), 0.0);
    }

    #[test]
    fn invert_round_trips() {
        for c in [
            Curve::Linear,
            Curve::Power { exponent: 3.0 },
            Curve::Exponential,
            Curve::Logarithmic,
            Curve::SmoothStep,
            Curve::Inverted { inner: Box::new(Curve::Logarithmic) },
        ] {
            for i in 0..=20 {
                let x = f64::from(i) / 20.0;
                assert!((c.invert(c.apply(x)) - x).abs() < 1e-6, "{c:?} at {x}");
            }
        }
    }

    #[test]
    fn shapes() {
        assert!(Curve::Exponential.apply(0.5) < 0.5);
        assert!(Curve::Logarithmic.apply(0.5) > 0.5);
        assert_eq!(Curve::Toggle { threshold: 0.5 }.apply(0.49), 0.0);
        assert_eq!(Curve::Toggle { threshold: 0.5 }.apply(0.5), 1.0);
        assert_eq!(Curve::Stepped { steps: 3 }.apply(0.3), 0.5);
        assert_eq!(Curve::Linear.apply(f64::NAN), 0.0);
        assert_eq!(Curve::Linear.apply(7.0), 1.0);
    }
}
