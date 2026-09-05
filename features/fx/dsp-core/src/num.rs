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
//! The processing crates must be able to use this on native, in an
//! `AudioWorklet`, and on an embedded target, so it stays `no_std` and
//! dependency-free — a conversion boundary only the test harness can reach
//! would be no boundary at all.

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

/// A finite, non-negative `f64` as a sample index, rounding toward zero.
///
/// The `f64` counterpart of [`f32_to_index`], with the same decided answers:
/// NaN and negatives give `0`, and anything past `usize::MAX` saturates.
#[must_use]
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the audited boundary: float-to-int `as` saturates in Rust, and every case is enumerated above"
)]
pub fn f64_to_index(x: f64) -> usize {
    if x.is_nan() || x <= 0.0 {
        return 0;
    }
    x as usize
}

/// An `i32` as `f32`, exact within ±2^24 and clamped beyond.
#[must_use]
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "the audited boundary: the input is range-checked against the f32 mantissa on the lines above"
)]
pub const fn i32_to_f32(n: i32) -> f32 {
    const EXACT: i32 = 1 << 24;
    if n > EXACT {
        return EXACT as f32;
    }
    if n < -EXACT {
        return -EXACT as f32;
    }
    n as f32
}

/// The greatest integer not above `x`, as `i32` — `f32::floor` for crates that
/// cannot reach `std` or `libm`.
///
/// NaN gives `0`, and magnitudes past the `i32` range saturate, matching the
/// float-to-int conversion this is built on.
#[must_use]
pub fn floor_to_i32(x: f32) -> i32 {
    let truncated = trunc_to_i32(x);
    // Truncation rounds toward zero, so it overshoots upward for negatives.
    if i32_to_f32(truncated) > x {
        truncated.saturating_sub(1)
    } else {
        truncated
    }
}

/// `x` truncated toward zero, as `i32`.
///
/// NaN gives `0` and out-of-range magnitudes saturate — Rust's float-to-int
/// `as` is already defined this way, and naming it says the call site knows.
#[must_use]
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "the audited boundary: saturating float-to-int truncation is this function's stated purpose"
)]
pub const fn trunc_to_i32(x: f32) -> i32 {
    x as i32
}

/// The greatest integer not above `x` — `f64::floor` for crates that cannot
/// reach `std` or `libm`.
///
/// Magnitudes past the `i64` range are already integral in `f64`, so they are
/// returned unchanged rather than saturated.
#[must_use]
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "the audited boundary: the round trip through i64 is the floor, and magnitudes past its range are handled above"
)]
pub fn floor_f64(x: f64) -> f64 {
    const INTEGRAL: f64 = (1_u64 << 53) as f64;
    if !x.is_finite() || x.abs() >= INTEGRAL {
        return x;
    }
    let truncated = x as i64 as f64;
    if x < truncated { truncated - 1.0 } else { truncated }
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
    fn both_widths_agree_on_hostile_floats() {
        assert_eq!(f64_to_index(f64::NAN), 0);
        assert_eq!(f64_to_index(-1.0), 0);
        assert_eq!(f64_to_index(2.9), 2);
        assert_eq!(f64_to_index(f64::INFINITY), usize::MAX);
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
    fn flooring_matches_the_mathematical_definition() {
        for (input, expected) in [
            (0.0_f32, 0),
            (2.9, 2),
            (3.0, 3),
            (-0.1, -1),
            (-2.9, -3),
            (-3.0, -3),
        ] {
            assert_eq!(floor_to_i32(input), expected, "floor({input})");
        }
    }

    #[test]
    fn flooring_saturates_rather_than_wrapping() {
        assert_eq!(floor_to_i32(f32::NAN), 0);
        assert_eq!(floor_to_i32(f32::INFINITY), i32::MAX);
        assert_eq!(floor_to_i32(f32::NEG_INFINITY), i32::MIN);
    }

    #[test]
    fn truncation_rounds_toward_zero_on_both_sides() {
        assert_eq!(trunc_to_i32(2.9), 2);
        assert_eq!(trunc_to_i32(-2.9), -2);
    }

    #[test]
    fn signed_conversion_round_trips_within_the_mantissa() {
        for n in [0_i32, 1, -1, 48_000, -48_000, (1 << 24) - 1, -((1 << 24) - 1)] {
            assert_eq!(trunc_to_i32(i32_to_f32(n)), n);
        }
    }

    #[test]
    fn flooring_f64_matches_the_mathematical_definition() {
        for (input, expected) in [(0.0_f64, 0.0), (2.9, 2.0), (3.0, 3.0), (-0.1, -1.0), (-2.9, -3.0)] {
            assert!((floor_f64(input) - expected).abs() < f64::EPSILON, "floor({input})");
        }
    }

    #[test]
    fn flooring_f64_passes_through_what_is_already_integral() {
        assert!(floor_f64(f64::INFINITY).is_infinite());
        assert!(floor_f64(f64::NAN).is_nan());
        assert!((floor_f64(f64::MAX) - f64::MAX).abs() < f64::EPSILON);
    }

    #[test]
    fn narrowing_keeps_representable_values_intact() {
        assert_eq!(narrow(0.5_f64).to_bits(), 0.5_f32.to_bits());
        assert!(narrow(f64::MAX).is_infinite());
        assert!(narrow(f64::NAN).is_nan());
    }
}
