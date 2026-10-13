//! Chord quality - the fundamental harmonic character of a chord
//!
//! Defines basic triad qualities and their intervals

use crate::primitives::Interval;
use facet::Facet;

/// Basic chord quality - the fundamental triad structure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet, Default)]
#[repr(u8)]
pub enum ChordQuality {
    /// Major triad (1, 3, 5) - C, E, G
    #[default]
    Major,
    /// Minor triad (1, b3, 5) - C, Eb, G
    Minor,
    /// Diminished triad (1, b3, b5) - C, Eb, Gb
    Diminished,
    /// Augmented triad (1, 3, #5) - C, E, G#
    Augmented,
    /// Suspended - replaces the third with either 2nd or 4th
    Suspended(SuspendedType),
    /// Power chord (1, 5) - C, G (no third)
    Power,
}

/// Type of suspended chord
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet, Default)]
#[repr(u8)]
pub enum SuspendedType {
    /// Sus2 - suspended 2nd (1, 2, 5) - C, D, G
    Second,
    /// Sus4 - suspended 4th (1, 4, 5) - C, F, G (default)
    #[default]
    Fourth,
}

impl ChordQuality {
    /// Get the intervals that define this quality
    pub fn intervals(&self) -> Vec<Interval> {
        match self {
            ChordQuality::Major => vec![Interval::MajorThird, Interval::PerfectFifth],
            ChordQuality::Minor => vec![Interval::MinorThird, Interval::PerfectFifth],
            ChordQuality::Diminished => vec![Interval::MinorThird, Interval::DiminishedFifth],
            ChordQuality::Augmented => vec![Interval::MajorThird, Interval::AugmentedFifth],
            ChordQuality::Suspended(sus_type) => match sus_type {
                SuspendedType::Second => vec![Interval::MajorSecond, Interval::PerfectFifth],
                SuspendedType::Fourth => vec![Interval::PerfectFourth, Interval::PerfectFifth],
            },
            ChordQuality::Power => vec![Interval::PerfectFifth],
        }
    }

    /// Check if this quality is major (has a major third)
    pub fn is_major(&self) -> bool {
        matches!(self, ChordQuality::Major | ChordQuality::Augmented)
    }

    /// Check if this quality is minor (has a minor third)
    pub fn is_minor(&self) -> bool {
        matches!(self, ChordQuality::Minor | ChordQuality::Diminished)
    }

    /// Check if this quality is suspended (no third)
    pub fn is_suspended(&self) -> bool {
        matches!(self, ChordQuality::Suspended(_) | ChordQuality::Power)
    }

    /// Get a short symbol for this quality
    pub fn symbol(&self) -> &'static str {
        match self {
            ChordQuality::Major => "", // Major is implied
            ChordQuality::Minor => "m",
            ChordQuality::Diminished => "dim",
            ChordQuality::Augmented => "+",
            ChordQuality::Suspended(sus_type) => match sus_type {
                SuspendedType::Second => "sus2",
                SuspendedType::Fourth => "sus4",
            },
            ChordQuality::Power => "5",
        }
    }

    /// Convenience constructor for sus4 (most common)
    pub fn sus() -> Self {
        ChordQuality::Suspended(SuspendedType::Fourth)
    }

    /// Convenience constructor for sus2
    pub fn sus2() -> Self {
        ChordQuality::Suspended(SuspendedType::Second)
    }

    /// Convenience constructor for sus4 (explicit)
    pub fn sus4() -> Self {
        ChordQuality::Suspended(SuspendedType::Fourth)
    }
}

impl std::fmt::Display for ChordQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display outputs the chord notation symbol, not the full name
        write!(f, "{}", self.symbol())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_major_intervals() {
        let quality = ChordQuality::Major;
        let intervals = quality.intervals();
        assert_eq!(intervals.len(), 2);
        assert_eq!(intervals[0].semitones(), 4); // Major third
        assert_eq!(intervals[1].semitones(), 7); // Perfect fifth
    }

    #[test]
    fn test_minor_intervals() {
        let quality = ChordQuality::Minor;
        let intervals = quality.intervals();
        assert_eq!(intervals.len(), 2);
        assert_eq!(intervals[0].semitones(), 3); // Minor third
        assert_eq!(intervals[1].semitones(), 7); // Perfect fifth
    }

    #[test]
    fn test_quality_checks() {
        assert!(ChordQuality::Major.is_major());
        assert!(ChordQuality::Minor.is_minor());
        assert!(ChordQuality::sus4().is_suspended());
        assert!(!ChordQuality::Major.is_minor());
    }

    #[test]
    fn test_symbols() {
        assert_eq!(ChordQuality::Major.symbol(), "");
        assert_eq!(ChordQuality::Minor.symbol(), "m");
        assert_eq!(ChordQuality::Diminished.symbol(), "dim");
        assert_eq!(ChordQuality::Augmented.symbol(), "+");
        assert_eq!(ChordQuality::Power.symbol(), "5");
        assert_eq!(ChordQuality::sus4().symbol(), "sus4");
        assert_eq!(ChordQuality::sus2().symbol(), "sus2");
    }

    #[test]
    fn test_suspended_intervals() {
        let sus4 = ChordQuality::sus4();
        let intervals = sus4.intervals();
        assert_eq!(intervals[0].semitones(), 5); // Perfect fourth
        assert_eq!(intervals[1].semitones(), 7); // Perfect fifth

        let sus2 = ChordQuality::sus2();
        let intervals = sus2.intervals();
        assert_eq!(intervals[0].semitones(), 2); // Major second
        assert_eq!(intervals[1].semitones(), 7); // Perfect fifth
    }

    #[test]
    fn test_sus_convenience_constructors() {
        assert_eq!(ChordQuality::sus(), ChordQuality::sus4());
        assert_eq!(
            ChordQuality::sus(),
            ChordQuality::Suspended(SuspendedType::Fourth)
        );
        assert_eq!(
            ChordQuality::sus2(),
            ChordQuality::Suspended(SuspendedType::Second)
        );
    }
}
