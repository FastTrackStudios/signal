//! Easing curves for parameter interpolation and morph transitions.

use facet::Facet;
use serde::{Deserialize, Serialize};

/// Easing curve for smooth transitions between parameter values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum EasingCurve {
    /// Constant-speed interpolation.
    Linear,
    /// Slow start, fast end (quadratic).
    EaseIn,
    /// Fast start, slow end (quadratic).
    EaseOut,
    /// Slow start and end, fast middle (quadratic).
    EaseInOut,
    /// Cubic ease-in for heavier acceleration.
    CubicIn,
    /// Cubic ease-out for heavier deceleration.
    CubicOut,
    /// Cubic ease-in-out for pronounced S-curve.
    CubicInOut,
}

impl Default for EasingCurve {
    fn default() -> Self {
        Self::Linear
    }
}

impl EasingCurve {
    /// Apply the easing function to a normalized time value `t` in `[0.0, 1.0]`.
    ///
    /// Returns a normalized output in `[0.0, 1.0]` representing the eased position.
    pub fn apply(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t,
            Self::EaseOut => t * (2.0 - t),
            Self::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    -1.0 + (4.0 - 2.0 * t) * t
                }
            }
            Self::CubicIn => t * t * t,
            Self::CubicOut => {
                let t1 = t - 1.0;
                t1 * t1 * t1 + 1.0
            }
            Self::CubicInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let t1 = 2.0 * t - 2.0;
                    0.5 * t1 * t1 * t1 + 1.0
                }
            }
        }
    }

    /// All available easing curves.
    pub const ALL: &'static [EasingCurve] = &[
        Self::Linear,
        Self::EaseIn,
        Self::EaseOut,
        Self::EaseInOut,
        Self::CubicIn,
        Self::CubicOut,
        Self::CubicInOut,
    ];

    /// Human-readable display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::EaseIn => "Ease In",
            Self::EaseOut => "Ease Out",
            Self::EaseInOut => "Ease In/Out",
            Self::CubicIn => "Cubic In",
            Self::CubicOut => "Cubic Out",
            Self::CubicInOut => "Cubic In/Out",
        }
    }
}

/// Linearly interpolate between two values using an eased `t`.
pub fn lerp_eased(from: f64, to: f64, t: f64, curve: EasingCurve) -> f64 {
    let eased = curve.apply(t);
    from + (to - from) * eased
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_is_identity() {
        assert_eq!(EasingCurve::Linear.apply(0.0), 0.0);
        assert_eq!(EasingCurve::Linear.apply(0.5), 0.5);
        assert_eq!(EasingCurve::Linear.apply(1.0), 1.0);
    }

    #[test]
    fn all_curves_start_at_zero_end_at_one() {
        for curve in EasingCurve::ALL {
            let start = curve.apply(0.0);
            let end = curve.apply(1.0);
            assert!(
                (start - 0.0).abs() < 1e-10,
                "{:?} start = {}",
                curve,
                start
            );
            assert!((end - 1.0).abs() < 1e-10, "{:?} end = {}", curve, end);
        }
    }

    #[test]
    fn ease_in_is_slower_than_linear_at_start() {
        let linear = EasingCurve::Linear.apply(0.25);
        let ease_in = EasingCurve::EaseIn.apply(0.25);
        assert!(ease_in < linear);
    }

    #[test]
    fn ease_out_is_faster_than_linear_at_start() {
        let linear = EasingCurve::Linear.apply(0.25);
        let ease_out = EasingCurve::EaseOut.apply(0.25);
        assert!(ease_out > linear);
    }

    #[test]
    fn clamps_out_of_range() {
        assert_eq!(EasingCurve::Linear.apply(-0.5), 0.0);
        assert_eq!(EasingCurve::Linear.apply(1.5), 1.0);
    }

    #[test]
    fn lerp_eased_interpolates_correctly() {
        let result = lerp_eased(10.0, 20.0, 0.5, EasingCurve::Linear);
        assert!((result - 15.0).abs() < 1e-10);
    }

    #[test]
    fn serde_round_trip() {
        let curve = EasingCurve::CubicInOut;
        let json = serde_json::to_string(&curve).unwrap();
        let parsed: EasingCurve = serde_json::from_str(&json).unwrap();
        assert_eq!(curve, parsed);
    }
}
