//! Index and coordinate conversions that say what they do at the edges.
//!
//! A lane index becomes a pixel offset and a pixel span becomes a
//! character count all over the painter. The `as` casts that did it
//! truncated, lost the sign or wrapped silently; these saturate, clamp
//! and treat NaN as zero, and say so.

/// An index, as a coordinate.
///
/// Lossless up to `u32::MAX`, which is more indices than anything
/// drawn will have; past that it saturates rather than wrapping.
#[must_use]
pub fn coord(index: usize) -> f64 {
    f64::from(u32::try_from(index).unwrap_or(u32::MAX))
}

/// A coordinate, as an index.
///
/// Clamped into range rather than truncated into nonsense: a negative
/// value is zero, anything past `u32::MAX` is that, and NaN is zero
/// because a NaN index is a bug upstream and the start is the
/// recoverable answer.
#[must_use]
pub fn index(coord: f64) -> usize {
    if !coord.is_finite() || coord <= 0.0 {
        return 0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::as_conversions,
        reason = "no non-`as` f64->integer conversion exists; bounded and NaN-checked above"
    )]
    let index = coord.min(f64::from(u32::MAX)) as usize;
    index
}

/// A level, as the single-precision sample the peaks are kept in.
#[must_use]
pub const fn sample(level: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "no non-`as` f64->f32 conversion exists; a peak is a level, not a count"
    )]
    let sample = level.clamp(-1.0, 1.0) as f32;
    sample
}
