//! Tuplet ratio representation.
//!
//! A tuplet is a rhythmic grouping where a certain number of notes are played
//! in the time normally occupied by a different number of notes.

use facet::Facet;

/// Tuplet ratio (e.g., 3:2 for triplet).
///
/// The ratio indicates how many notes (numerator) are played in the time
/// normally occupied by (denominator) notes. For example:
/// - 3:2 (triplet): 3 notes in the space of 2
/// - 5:4 (quintuplet): 5 notes in the space of 4
/// - 6:4 (sextuplet): 6 notes in the space of 4
/// - 7:8 (septuplet): 7 notes in the space of 8 (MuseScore convention)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Facet)]
pub struct TupletRatio {
    /// How many notes in the tuplet group
    pub numerator: u8,
    /// How many notes they replace (the time they occupy)
    pub denominator: u8,
}

impl TupletRatio {
    /// Create a new tuplet ratio.
    #[must_use]
    pub const fn new(numerator: u8, denominator: u8) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    /// Triplet ratio (3:2) - 3 notes in the space of 2.
    pub const TRIPLET: Self = Self::new(3, 2);

    /// Quintuplet ratio (5:4) - 5 notes in the space of 4.
    pub const QUINTUPLET: Self = Self::new(5, 4);

    /// Sextuplet ratio (6:4) - 6 notes in the space of 4.
    pub const SEXTUPLET: Self = Self::new(6, 4);

    /// Septuplet ratio (7:8) - 7 notes in the space of 8 (MuseScore convention).
    pub const SEPTUPLET: Self = Self::new(7, 8);

    /// Create a triplet (3:2).
    #[must_use]
    pub const fn triplet() -> Self {
        Self::TRIPLET
    }

    /// Create a quintuplet (5:4).
    #[must_use]
    pub const fn quintuplet() -> Self {
        Self::QUINTUPLET
    }

    /// Create a sextuplet (6:4).
    #[must_use]
    pub const fn sextuplet() -> Self {
        Self::SEXTUPLET
    }

    /// Create a septuplet (7:8).
    #[must_use]
    pub const fn septuplet() -> Self {
        Self::SEPTUPLET
    }

    /// Get the duration multiplier as a fraction.
    ///
    /// For a triplet (3:2), each note is 2/3 of its normal duration.
    /// Returns (denominator, numerator) for: duration * denom / numer
    #[must_use]
    pub const fn duration_multiplier(&self) -> (i32, i32) {
        (self.denominator as i32, self.numerator as i32)
    }

    /// Apply this tuplet ratio to a tick duration.
    ///
    /// For a triplet (3:2), the result is: ticks * 2 / 3
    #[must_use]
    pub const fn apply_to_ticks(&self, ticks: i32) -> i32 {
        ticks * self.denominator as i32 / self.numerator as i32
    }

    /// Get the multiplier as f64.
    ///
    /// For a triplet (3:2), returns 2/3 ≈ 0.667
    #[must_use]
    pub fn as_f64(&self) -> f64 {
        f64::from(self.denominator) / f64::from(self.numerator)
    }

    /// Check if this is a triplet (3:2).
    #[must_use]
    pub const fn is_triplet(&self) -> bool {
        self.numerator == 3 && self.denominator == 2
    }
}

impl Default for TupletRatio {
    fn default() -> Self {
        Self::TRIPLET
    }
}

impl std::fmt::Display for TupletRatio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.numerator, self.denominator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triplet() {
        let triplet = TupletRatio::triplet();
        assert_eq!(triplet.numerator, 3);
        assert_eq!(triplet.denominator, 2);
        assert!(triplet.is_triplet());

        // Triplet eighth = 2/3 of regular eighth
        // 240 ticks * 2/3 = 160 ticks
        assert_eq!(triplet.apply_to_ticks(240), 160);
    }

    #[test]
    fn test_quintuplet() {
        let quint = TupletRatio::quintuplet();
        assert_eq!(quint.numerator, 5);
        assert_eq!(quint.denominator, 4);

        // Quintuplet eighth = 4/5 of regular eighth
        // 240 ticks * 4/5 = 192 ticks
        assert_eq!(quint.apply_to_ticks(240), 192);
    }

    #[test]
    fn test_as_f64() {
        let triplet = TupletRatio::triplet();
        let expected = 2.0 / 3.0;
        assert!((triplet.as_f64() - expected).abs() < 0.001);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", TupletRatio::triplet()), "3:2");
        assert_eq!(format!("{}", TupletRatio::quintuplet()), "5:4");
    }
}
