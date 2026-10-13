//! Numeric conversions for the audio-scheduling math in this crate.
//!
//! Every value converted here is audio-domain-bounded (sample counts, beat
//! indices, block sizes) — nowhere near the range where `usize`/`i64`→`f64`
//! precision loss or `f64`→int truncation would matter in practice — but
//! `std` has no non-`as` conversion for any of these pairs, so each helper
//! documents the bound and does the cast in one place instead of scattering
//! justified `#[allow]`s through the DSP code.

/// `x` as `f64`. Sample/beat counts never approach `f64`'s 52-bit mantissa
/// limit, so this never loses precision in practice.
#[allow(clippy::as_conversions, clippy::cast_precision_loss)]
pub const fn f64_from_usize(x: usize) -> f64 {
    x as f64
}

/// `x` as `f64`. Beat-grid indices never approach `f64`'s 52-bit mantissa
/// limit, so this never loses precision in practice.
#[allow(clippy::as_conversions, clippy::cast_precision_loss)]
pub const fn f64_from_i64(x: i64) -> f64 {
    x as f64
}

/// `x.round()` as `i64`, saturating instead of wrapping if `x` is outside
/// `i64`'s range (never happens for real sample offsets, but keeps the
/// conversion honest rather than assuming it).
///
/// Bounds are `2.0f64.powi(63)` (exactly representable) rather than a cast
/// of `i64::MIN`/`i64::MAX`, so the comparison itself needs no `as`.
#[allow(clippy::as_conversions, clippy::cast_possible_truncation)]
pub fn i64_from_f64_round(x: f64) -> i64 {
    const I64_MIN_F64: f64 = -9_223_372_036_854_775_808.0; // -2^63, exact
    const I64_MAX_BOUND_F64: f64 = 9_223_372_036_854_775_808.0; // 2^63, exact

    if !x.is_finite() {
        return 0;
    }
    let rounded = x.round();
    if rounded <= I64_MIN_F64 {
        i64::MIN
    } else if rounded >= I64_MAX_BOUND_F64 {
        i64::MAX
    } else {
        rounded as i64
    }
}

/// `x`, clamped to non-negative, as `usize` (saturating at `usize::MAX` on
/// the rare 32-bit target where `i64` exceeds `usize`'s range).
pub fn usize_from_i64_nonneg(x: i64) -> usize {
    usize::try_from(x.max(0)).unwrap_or(usize::MAX)
}

/// A finite `f64` clamped to `i32`'s range, as an `i32`.
#[allow(clippy::as_conversions, clippy::cast_possible_truncation)]
pub fn i32_from_f64_saturating(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    x.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

/// `x.round()`, clamped to non-negative and to `u64::MAX`, as `usize`
/// (`i64_from_f64_round`'s `u64` counterpart — sample/frame counts are
/// always non-negative).
pub fn usize_from_f64_round(x: f64) -> usize {
    usize::try_from(u64_from_f64_round(x)).unwrap_or(usize::MAX)
}

/// `x.round()`, clamped to `0.0..=u64::MAX`, as `u64`.
///
/// Bounds are `2.0f64.powi(64)` (exactly representable) rather than a cast
/// of `u64::MAX`, so the comparison itself needs no `as`.
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn u64_from_f64_round(x: f64) -> u64 {
    const U64_MAX_BOUND_F64: f64 = 18_446_744_073_709_551_616.0; // 2^64, exact

    if !x.is_finite() || x < 0.0 {
        return 0;
    }
    let rounded = x.round();
    if rounded >= U64_MAX_BOUND_F64 {
        u64::MAX
    } else {
        rounded as u64
    }
}

/// `x.floor()`, clamped to non-negative, as `usize`.
pub fn usize_from_f64_floor_nonneg(x: f64) -> usize {
    usize_from_f64_round(x.floor().max(0.0))
}

/// A finite `f64` clamped to `f32`'s range, as an `f32`.
#[allow(clippy::as_conversions, clippy::cast_possible_truncation)]
pub fn f32_from_f64_saturating(x: f64) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    x.clamp(f64::from(f32::MIN), f64::from(f32::MAX)) as f32
}
