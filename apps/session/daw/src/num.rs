//! The two numeric conversions this window cannot avoid.
//!
//! Indices are `usize` and geometry is `f64`, and every layout crosses
//! between them: "which strip is at this x" goes one way, "where does
//! strip `i` start" goes the other. `std` has a safe conversion for
//! neither, so without this they are `as` casts scattered through the
//! layout code — which is exactly where a silent truncation would be
//! hardest to see and easiest to write.
//!
//! So both live here, once, with their bounds stated. The scoped
//! `expect` on the float conversion is not a lint being silenced: there
//! is no non-`as` route from `f64` to an integer in the language, and
//! the alternative is the same cast written in six places without the
//! clamping.

/// Strip or row `index`, as a coordinate.
///
/// Exact: `u32` is the widest index this converts, and every integer up
/// to 2^24 is exact in `f64` — a project with sixteen million tracks is
/// not one this would be the problem with. An index too large to convert
/// saturates rather than wrapping, so it lands past the end of the
/// content and draws nothing.
#[must_use]
pub fn coord(index: usize) -> f64 {
    f64::from(u32::try_from(index).unwrap_or(u32::MAX))
}

/// A coordinate, as an index.
///
/// Clamped into range rather than truncated into nonsense: a negative
/// scroll is the first item, and anything past `usize` is the last. NaN
/// becomes zero, because a NaN index is a bug upstream and drawing from
/// the start is the recoverable answer.
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

/// A value quantised to `steps` per unit, as a key.
///
/// For memoising on a setting: a drag produces hundreds of values a
/// pixel apart, and a cache keyed on the exact float would miss every
/// one. Rounded to nearest; saturates far out of range rather than
/// wrapping, and NaN keys as zero.
#[must_use]
pub fn quantise(value: f64, steps: f64) -> i32 {
    let scaled = (value * steps).round();
    if !scaled.is_finite() {
        return 0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "no non-`as` f64->integer conversion exists; clamped into range on this line"
    )]
    let key = scaled.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
    key
}

/// A `f64` reading, narrowed to the `f32` a waveform is drawn from.
///
/// Peaks arrive over the wire as `f64` and are drawn as `f32`: the
/// values are linear `-1..1` and a single rounding at the boundary costs
/// nothing a pixel could show. Non-finite becomes zero — a NaN peak
/// would poison a min/max fold for a whole column rather than for one
/// block.
#[must_use]
pub fn narrow(value: f64) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "no non-`as` f64->f32 conversion exists; finite-checked above"
    )]
    let narrowed = value as f32;
    narrowed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Narrowing keeps a peak's value and refuses to carry a NaN into a
    /// fold.
    #[test]
    fn narrows_a_reading() {
        assert!((narrow(0.5) - 0.5_f32).abs() < f32::EPSILON);
        assert!((narrow(-1.0) + 1.0_f32).abs() < f32::EPSILON);
        assert!((narrow(f64::NAN) - 0.0_f32).abs() < f32::EPSILON);
        assert!((narrow(f64::INFINITY) - 0.0_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn round_trips_an_index() {
        assert_eq!(index(coord(1234)), 1234);
    }

    /// The cases the `as` cast would get wrong on its own.
    #[test]
    fn quantises_to_a_step() {
        assert_eq!(quantise(1.804, 20.0), 36);
        assert_eq!(quantise(1.81, 20.0), 36);
        assert_eq!(quantise(-0.5, 100.0), -50);
        assert_eq!(quantise(f64::NAN, 10.0), 0);
    }

    #[test]
    fn clamps_rather_than_truncating() {
        assert_eq!(index(-5.0), 0);
        assert_eq!(index(f64::NAN), 0);
        assert_eq!(index(f64::INFINITY), 0);
        assert_eq!(index(3.9), 3);
    }
}
