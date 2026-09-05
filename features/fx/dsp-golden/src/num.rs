//! The audited conversion boundary.
//!
//! Under the workspace lint policy `as` is denied everywhere, and for good
//! reason: a silent `as` in DSP code is how a sample index wraps, how a
//! negative delay time becomes a four-gigabyte offset, and how a buffer length
//! quietly loses precision on the way into a float.
//!
//! The fix is not to suppress the lint per call site — it is to have exactly
//! one place where a conversion may happen, where the precondition is written
//! down and the out-of-range case has a decided answer. That place is this
//! module; everything else calls these functions.
//!
//! This is the shape the DSP crates are being moved onto, so it lives in the
//! harness where they can all reach it.

/// Largest integer `f32` represents exactly: 2^24, or about 5.8 minutes of
/// samples at 48 kHz.
const EXACT_MAX: usize = 1 << 24;

/// A sample count or buffer length as `f32`, exactly.
///
/// Past 2^24 the value is clamped rather than silently rounded — a buffer that
/// long in a per-block path is a bug, not a number to approximate.
#[must_use]
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "the audited boundary: the input is range-checked against the f32 mantissa on the line above"
)]
pub const fn count_to_f32(n: usize) -> f32 {
    if n > EXACT_MAX {
        return EXACT_MAX as f32;
    }
    n as f32
}

/// A finite, non-negative `f32` as a sample index, rounding toward zero.
///
/// NaN and negative inputs give `0`; `+inf` and anything past `usize::MAX`
/// saturate. Each of those is a decided answer, which is the point — `x as
/// usize` on a NaN also yields 0, but without anyone having chosen that.
#[must_use]
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the audited boundary: float-to-int `as` saturates in Rust, and every case is enumerated above"
)]
pub fn f32_to_index(x: f32) -> usize {
    if x.is_nan() || x <= 0.0 {
        return 0;
    }
    x as usize
}

/// A `u32` as `f32`, exact below 2^24 and clamped above it.
#[must_use]
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "the audited boundary: f64 holds every u32 exactly, and the narrowing is range-checked"
)]
pub fn u32_to_f32(n: u32) -> f32 {
    const EXACT_MAX_U32: u32 = 1 << 24;
    if n > EXACT_MAX_U32 {
        return EXACT_MAX_U32 as f32;
    }
    f64::from(n) as f32
}

/// An `f64` narrowed to `f32` — the conversion DSP code makes constantly, with
/// coefficients designed in double and processed in single.
///
/// Non-finite inputs pass through and out-of-range magnitudes saturate to
/// infinity, exactly as `as` already does. This exists so the call site reads
/// as a deliberate narrowing rather than an incidental one.
#[must_use]
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "the audited boundary: f64-to-f32 narrowing is this function's stated purpose"
)]
pub const fn narrow(x: f64) -> f32 {
    x as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_round_trip_below_the_mantissa_limit() {
        for n in [0_usize, 1, 512, 48_000, EXACT_MAX - 1] {
            assert_eq!(f32_to_index(count_to_f32(n)), n);
        }
    }

    #[test]
    fn oversized_counts_clamp_rather_than_round() {
        assert_eq!(f32_to_index(count_to_f32(EXACT_MAX + 1)), EXACT_MAX);
    }

    #[test]
    fn hostile_floats_have_decided_indices() {
        assert_eq!(f32_to_index(f32::NAN), 0);
        assert_eq!(f32_to_index(-1.0), 0);
        assert_eq!(f32_to_index(-0.0), 0);
        assert_eq!(f32_to_index(2.9), 2);
        assert_eq!(f32_to_index(f32::INFINITY), usize::MAX);
    }

    #[test]
    fn the_two_integer_conversions_agree() {
        for n in [0_u32, 1, 44_100, (1 << 24) - 1] {
            assert_eq!(f32_to_index(u32_to_f32(n)), f32_to_index(count_to_f32(n.try_into().unwrap())));
        }
    }

    #[test]
    fn narrowing_keeps_representable_values_intact() {
        assert_eq!(narrow(0.5_f64).to_bits(), 0.5_f32.to_bits());
        assert!(narrow(f64::MAX).is_infinite());
        assert!(narrow(f64::NAN).is_nan());
    }
}
