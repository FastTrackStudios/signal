//! Time signature representation.
//!
//! A single canonical `TimeSignature` type that can be used across the codebase.

/// Time signature representation.
///
/// # Fields
///
/// - `numerator`: The number of beats per measure (top number)
/// - `denominator`: The note value that gets one beat (bottom number)
///
/// # Common Time Signatures
///
/// | Name | Signature | Description |
/// |------|-----------|-------------|
/// | Common time | 4/4 | 4 quarter notes per measure |
/// | Cut time | 2/2 | 2 half notes per measure |
/// | Waltz time | 3/4 | 3 quarter notes per measure |
/// | Compound duple | 6/8 | 6 eighth notes per measure (2 dotted quarters) |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TimeSignature {
    /// Beats per measure (top number)
    pub numerator: u8,
    /// Note value per beat (bottom number): 4 = quarter, 8 = eighth, etc.
    pub denominator: u8,
}

impl TimeSignature {
    /// Create a new time signature.
    #[must_use]
    pub const fn new(numerator: u8, denominator: u8) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    /// Common time (4/4).
    pub const COMMON: Self = Self::new(4, 4);

    /// Returns common time (4/4). Alias for `COMMON` for compatibility.
    #[must_use]
    pub const fn common_time() -> Self {
        Self::COMMON
    }

    /// Cut time (2/2).
    pub const CUT: Self = Self::new(2, 2);

    /// Waltz time (3/4).
    pub const WALTZ: Self = Self::new(3, 4);

    /// Compound duple (6/8).
    pub const COMPOUND_DUPLE: Self = Self::new(6, 8);

    /// Compound triple (9/8).
    pub const COMPOUND_TRIPLE: Self = Self::new(9, 8);

    /// Compound quadruple (12/8).
    pub const COMPOUND_QUADRUPLE: Self = Self::new(12, 8);

    /// Get the number of ticks in one measure (at 480 PPQ).
    ///
    /// Formula: `(1920 / denominator) * numerator`
    /// where 1920 = ticks per whole note at 480 PPQ
    #[must_use]
    pub const fn measure_ticks_480(&self) -> i32 {
        let beat_ticks = 1920 / self.denominator as i32;
        beat_ticks * self.numerator as i32
    }

    /// Get the number of ticks per beat (at 480 PPQ).
    ///
    /// For 4/4: quarter note = 480 ticks
    /// For 6/8: eighth note = 240 ticks
    #[must_use]
    pub const fn beat_ticks_480(&self) -> i32 {
        1920 / self.denominator as i32
    }

    /// Get the number of ticks in one measure at any PPQ.
    #[must_use]
    pub const fn measure_ticks_at_ppq(&self, ppq: u32) -> i64 {
        let whole_note_ticks = ppq as i64 * 4; // whole note = 4 quarter notes
        whole_note_ticks * self.numerator as i64 / self.denominator as i64
    }

    /// Get the number of ticks per beat at any PPQ.
    #[must_use]
    pub const fn beat_ticks_at_ppq(&self, ppq: u32) -> i64 {
        let whole_note_ticks = ppq as i64 * 4;
        whole_note_ticks / self.denominator as i64
    }

    /// Check if this is a compound time signature (6/8, 9/8, 12/8, etc.).
    ///
    /// Compound time signatures have a numerator divisible by 3 (and > 3)
    /// with a denominator of 8 or smaller subdivision.
    #[must_use]
    pub const fn is_compound(&self) -> bool {
        self.numerator.is_multiple_of(3) && self.numerator > 3 && self.denominator >= 8
    }

    /// Check if this is a simple time signature.
    #[must_use]
    pub const fn is_simple(&self) -> bool {
        !self.is_compound()
    }

    /// Get beam groupings for this time signature.
    ///
    /// Returns a list of tick counts for each beam group at 480 PPQ.
    #[must_use]
    pub fn beam_groups_480(&self) -> Vec<i32> {
        let beat = self.beat_ticks_480();
        match (self.numerator, self.denominator) {
            // 4/4: beam in groups of 1 beat each
            (4, 4) => vec![beat, beat, beat, beat],
            // 3/4: beam each beat separately
            (3, 4) => vec![beat, beat, beat],
            // 6/8: beam in groups of 3 eighths (compound meter)
            (6, 8) => vec![beat * 3, beat * 3],
            // 9/8: beam in groups of 3 eighths
            (9, 8) => vec![beat * 3, beat * 3, beat * 3],
            // 12/8: beam in groups of 3 eighths
            (12, 8) => vec![beat * 3, beat * 3, beat * 3, beat * 3],
            // 2/4: beam in groups of 1 beat
            (2, 4) => vec![beat, beat],
            // 2/2: beam in groups of 1 beat (half notes)
            (2, 2) => vec![beat, beat],
            // Default: beam each beat
            _ => vec![beat; self.numerator as usize],
        }
    }

    /// Get the number of "felt beats" per measure.
    ///
    /// For simple time, this is the numerator.
    /// For compound time, this is numerator / 3 (e.g., 6/8 has 2 felt beats).
    #[must_use]
    pub const fn felt_beats(&self) -> u8 {
        if self.is_compound() {
            self.numerator / 3
        } else {
            self.numerator
        }
    }
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self::COMMON
    }
}

impl std::fmt::Display for TimeSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

// Conversion from DAW's i32-based TimeSignature
impl From<(i32, i32)> for TimeSignature {
    fn from((num, denom): (i32, i32)) -> Self {
        Self::new(num.clamp(1, 255) as u8, denom.clamp(1, 255) as u8)
    }
}

// Conversion to tuple
impl From<TimeSignature> for (u8, u8) {
    fn from(ts: TimeSignature) -> Self {
        (ts.numerator, ts.denominator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_time() {
        let ts = TimeSignature::COMMON;
        assert_eq!(ts.numerator, 4);
        assert_eq!(ts.denominator, 4);
        assert_eq!(ts.measure_ticks_480(), 1920);
        assert_eq!(ts.beat_ticks_480(), 480);
    }

    #[test]
    fn test_6_8() {
        let ts = TimeSignature::new(6, 8);
        assert_eq!(ts.measure_ticks_480(), 1440); // 6 eighth notes
        assert_eq!(ts.beat_ticks_480(), 240); // eighth note
        assert!(ts.is_compound());
        assert_eq!(ts.felt_beats(), 2);
    }

    #[test]
    fn test_3_4() {
        let ts = TimeSignature::WALTZ;
        assert_eq!(ts.measure_ticks_480(), 1440); // 3 quarter notes
        assert_eq!(ts.beat_ticks_480(), 480); // quarter note
        assert!(ts.is_simple());
        assert_eq!(ts.felt_beats(), 3);
    }

    #[test]
    fn test_compound_detection() {
        assert!(TimeSignature::new(6, 8).is_compound());
        assert!(TimeSignature::new(9, 8).is_compound());
        assert!(TimeSignature::new(12, 8).is_compound());

        assert!(!TimeSignature::new(3, 4).is_compound()); // 3 is not > 3
        assert!(!TimeSignature::new(4, 4).is_compound());
        assert!(!TimeSignature::new(6, 4).is_compound()); // denominator not >= 8
    }

    #[test]
    fn test_beam_groups() {
        let ts = TimeSignature::new(4, 4);
        assert_eq!(ts.beam_groups_480(), vec![480, 480, 480, 480]);

        let ts = TimeSignature::new(6, 8);
        assert_eq!(ts.beam_groups_480(), vec![720, 720]); // 3 eighths each
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", TimeSignature::new(4, 4)), "4/4");
        assert_eq!(format!("{}", TimeSignature::new(6, 8)), "6/8");
    }

    #[test]
    fn test_measure_ticks_at_ppq() {
        let ts = TimeSignature::new(4, 4);
        assert_eq!(ts.measure_ticks_at_ppq(480), 1920);
        assert_eq!(ts.measure_ticks_at_ppq(960), 3840);
    }
}
