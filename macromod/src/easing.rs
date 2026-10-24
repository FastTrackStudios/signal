//! Easing curves for parameter interpolation and morph transitions.

use facet::Facet;
use serde::{Deserialize, Serialize};

/// Easing curve for smooth transitions between parameter values.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum EasingCurve {
    /// Constant-speed interpolation.
    #[default]
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

impl EasingCurve {
    /// Apply the easing function to a normalized time value `t` in `[0.0, 1.0]`.
    ///
    /// Returns a normalized output in `[0.0, 1.0]` representing the eased position.
    #[must_use]
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
                    2.0f64.mul_add(-t, 4.0).mul_add(t, -1.0)
                }
            }
            Self::CubicIn => t * t * t,
            Self::CubicOut => {
                let t1 = t - 1.0;
                (t1 * t1).mul_add(t1, 1.0)
            }
            Self::CubicInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let t1 = 2.0f64.mul_add(t, -2.0);
                    (0.5 * t1 * t1).mul_add(t1, 1.0)
                }
            }
        }
    }

    /// All available easing curves.
    pub const ALL: &'static [Self] = &[
        Self::Linear,
        Self::EaseIn,
        Self::EaseOut,
        Self::EaseInOut,
        Self::CubicIn,
        Self::CubicOut,
        Self::CubicInOut,
    ];

    /// Human-readable display name.
    #[must_use]
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
#[must_use]
pub fn lerp_eased(from: f64, to: f64, t: f64, curve: EasingCurve) -> f64 {
    let eased = curve.apply(t);
    (to - from).mul_add(eased, from)
}
