//! Stereo helpers: pan laws and mid/side width.
//!
//! Consolidates the equal-power pan and width blocks that were
//! open-coded (with drifting laws) across the fx crates.

use core::f64::consts::{FRAC_PI_4, SQRT_2};

/// Equal-power pan gains, normalized so a centered source contributes
/// unity to BOTH sides — a panned element at `pan = 0` stays
/// bit-identical to dual-mono routing. Hard pans put the full-power
/// (+3 dB) signal on one side, standard for constant-power laws with a
/// 0 dB center.
#[inline]
pub fn pan_equal_power(pan: f64) -> (f64, f64) {
    let theta = (pan.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
    (SQRT_2 * theta.cos(), SQRT_2 * theta.sin())
}

/// Mid/side stereo width: `width` 0.0 collapses to mono, 1.0 is
/// unchanged, above 1.0 widens (side boosted). Returns the processed
/// (left, right).
#[inline]
pub fn width(left: f64, right: f64, width: f64) -> (f64, f64) {
    let mid = (left + right) * 0.5;
    let side = (left - right) * 0.5 * width;
    (mid + side, mid - side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pan_center_is_unity_both_sides() {
        let (l, r) = pan_equal_power(0.0);
        assert!((l - 1.0).abs() < 1e-12);
        assert!((r - 1.0).abs() < 1e-12);
    }

    #[test]
    fn pan_is_constant_power() {
        for i in 0..=20 {
            let p = -1.0 + i as f64 * 0.1;
            let (l, r) = pan_equal_power(p);
            assert!((l * l + r * r - 2.0).abs() < 1e-9, "at pan {p}");
        }
    }

    #[test]
    fn width_zero_is_mono_and_one_is_identity() {
        let (l, r) = width(0.8, -0.2, 0.0);
        assert!((l - r).abs() < 1e-12);
        let (l1, r1) = width(0.8, -0.2, 1.0);
        assert!((l1 - 0.8).abs() < 1e-12 && (r1 + 0.2).abs() < 1e-12);
    }
}
