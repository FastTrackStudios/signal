//! Chord degrees - semantic scale positions within a chord
//!
//! Represents the semantic function of each note in a chord (e.g., 3rd, 7th, 9th)
//! independent of the actual interval quality (major vs minor, etc.)

use crate::chord::quality::ChordQuality;
use crate::primitives::Interval;
use facet::Facet;

/// Semantic degree within a chord
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Facet)]
#[repr(u8)]
pub enum ChordDegree {
    Root = 1,
    Second = 2,
    Third = 3,
    Fourth = 4,
    Fifth = 5,
    Sixth = 6,
    Seventh = 7,
    Ninth = 9,
    Eleventh = 11,
    Thirteenth = 13,
}

impl ChordDegree {
    /// Get the numeric value of this degree
    pub fn value(&self) -> u8 {
        *self as u8
    }

    /// Get the semantic interval (1-7) for letter name calculation
    ///
    /// This maps extended degrees (9th, 11th, 13th) back to their base scale degree:
    /// - 9th -> 2nd
    /// - 11th -> 4th
    /// - 13th -> 6th
    ///
    /// This is used for determining the correct letter name in enharmonic spelling.
    pub fn semantic_interval(&self) -> u8 {
        match self {
            ChordDegree::Root => 1,
            ChordDegree::Second | ChordDegree::Ninth => 2,
            ChordDegree::Third => 3,
            ChordDegree::Fourth | ChordDegree::Eleventh => 4,
            ChordDegree::Fifth => 5,
            ChordDegree::Sixth | ChordDegree::Thirteenth => 6,
            ChordDegree::Seventh => 7,
        }
    }

    /// Get the expected "natural" interval for this degree in a given quality
    ///
    /// For example:
    /// - 3rd in Major quality -> MajorThird
    /// - 3rd in Minor quality -> MinorThird
    /// - 7th in Major quality -> MajorSeventh (for maj7 chords)
    /// - 5th in Diminished quality -> DiminishedFifth
    pub fn to_expected_interval(&self, quality: ChordQuality) -> Interval {
        match self {
            ChordDegree::Root => Interval::Unison,

            ChordDegree::Second => Interval::MajorSecond,

            ChordDegree::Third => match quality {
                ChordQuality::Major | ChordQuality::Augmented => Interval::MajorThird,
                ChordQuality::Minor | ChordQuality::Diminished => Interval::MinorThird,
                ChordQuality::Suspended(_) | ChordQuality::Power => {
                    // Suspended/power chords don't have a third by default
                    // But if forced to provide one, use major
                    Interval::MajorThird
                }
            },

            ChordDegree::Fourth => Interval::PerfectFourth,

            ChordDegree::Fifth => match quality {
                ChordQuality::Diminished => Interval::DiminishedFifth,
                ChordQuality::Augmented => Interval::AugmentedFifth,
                _ => Interval::PerfectFifth,
            },

            ChordDegree::Sixth => Interval::MajorSixth,

            ChordDegree::Seventh => {
                // Default to dominant 7th (MinorSeventh)
                // ChordFamily will override this for maj7, dim7, etc.
                Interval::MinorSeventh
            }

            ChordDegree::Ninth => Interval::Ninth,

            ChordDegree::Eleventh => Interval::Eleventh,

            ChordDegree::Thirteenth => Interval::Thirteenth,
        }
    }

    /// Extract the semantic degree from any interval
    ///
    /// Examples:
    /// - MajorThird -> Third
    /// - MinorThird -> Third
    /// - MajorSeventh -> Seventh
    /// - MinorSeventh -> Seventh
    pub fn from_interval(interval: Interval) -> ChordDegree {
        use Interval::*;
        match interval {
            Unison | PerfectOctave | Octave => ChordDegree::Root,

            MinorSecond | MajorSecond | AugmentedSecond => ChordDegree::Second,

            MinorThird | MajorThird | AugmentedThird => ChordDegree::Third,

            PerfectFourth | AugmentedFourth => ChordDegree::Fourth,

            DiminishedFifth | PerfectFifth | AugmentedFifth => ChordDegree::Fifth,

            MinorSixth | MajorSixth | AugmentedSixth => ChordDegree::Sixth,

            DiminishedSeventh | MinorSeventh | MajorSeventh | AugmentedSeventh => {
                ChordDegree::Seventh
            }

            FlatNinth | Ninth | SharpNinth => ChordDegree::Ninth,

            Eleventh | SharpEleventh => ChordDegree::Eleventh,

            FlatThirteenth | Thirteenth => ChordDegree::Thirteenth,
        }
    }

    /// Parse a degree from a number string
    pub fn from_number(n: u8) -> Option<ChordDegree> {
        match n {
            1 => Some(ChordDegree::Root),
            2 => Some(ChordDegree::Second),
            3 => Some(ChordDegree::Third),
            4 => Some(ChordDegree::Fourth),
            5 => Some(ChordDegree::Fifth),
            6 => Some(ChordDegree::Sixth),
            7 => Some(ChordDegree::Seventh),
            9 => Some(ChordDegree::Ninth),
            11 => Some(ChordDegree::Eleventh),
            13 => Some(ChordDegree::Thirteenth),
            _ => None,
        }
    }

    /// Get all degrees as a sorted vec
    pub fn all() -> Vec<ChordDegree> {
        vec![
            ChordDegree::Root,
            ChordDegree::Second,
            ChordDegree::Third,
            ChordDegree::Fourth,
            ChordDegree::Fifth,
            ChordDegree::Sixth,
            ChordDegree::Seventh,
            ChordDegree::Ninth,
            ChordDegree::Eleventh,
            ChordDegree::Thirteenth,
        ]
    }
}

impl std::fmt::Display for ChordDegree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_degree_values() {
        assert_eq!(ChordDegree::Root.value(), 1);
        assert_eq!(ChordDegree::Third.value(), 3);
        assert_eq!(ChordDegree::Seventh.value(), 7);
        assert_eq!(ChordDegree::Ninth.value(), 9);
    }

    #[test]
    fn test_expected_interval_major() {
        let quality = ChordQuality::Major;
        assert_eq!(
            ChordDegree::Third.to_expected_interval(quality),
            Interval::MajorThird
        );
        assert_eq!(
            ChordDegree::Fifth.to_expected_interval(quality),
            Interval::PerfectFifth
        );
    }

    #[test]
    fn test_expected_interval_minor() {
        let quality = ChordQuality::Minor;
        assert_eq!(
            ChordDegree::Third.to_expected_interval(quality),
            Interval::MinorThird
        );
        assert_eq!(
            ChordDegree::Fifth.to_expected_interval(quality),
            Interval::PerfectFifth
        );
    }

    #[test]
    fn test_expected_interval_diminished() {
        let quality = ChordQuality::Diminished;
        assert_eq!(
            ChordDegree::Third.to_expected_interval(quality),
            Interval::MinorThird
        );
        assert_eq!(
            ChordDegree::Fifth.to_expected_interval(quality),
            Interval::DiminishedFifth
        );
    }

    #[test]
    fn test_expected_interval_augmented() {
        let quality = ChordQuality::Augmented;
        assert_eq!(
            ChordDegree::Third.to_expected_interval(quality),
            Interval::MajorThird
        );
        assert_eq!(
            ChordDegree::Fifth.to_expected_interval(quality),
            Interval::AugmentedFifth
        );
    }

    #[test]
    fn test_from_interval() {
        assert_eq!(
            ChordDegree::from_interval(Interval::MajorThird),
            ChordDegree::Third
        );
        assert_eq!(
            ChordDegree::from_interval(Interval::MinorThird),
            ChordDegree::Third
        );
        assert_eq!(
            ChordDegree::from_interval(Interval::MajorSeventh),
            ChordDegree::Seventh
        );
        assert_eq!(
            ChordDegree::from_interval(Interval::MinorSeventh),
            ChordDegree::Seventh
        );
    }

    #[test]
    fn test_from_number() {
        assert_eq!(ChordDegree::from_number(1), Some(ChordDegree::Root));
        assert_eq!(ChordDegree::from_number(3), Some(ChordDegree::Third));
        assert_eq!(ChordDegree::from_number(7), Some(ChordDegree::Seventh));
        assert_eq!(ChordDegree::from_number(9), Some(ChordDegree::Ninth));
        assert_eq!(ChordDegree::from_number(8), None);
        assert_eq!(ChordDegree::from_number(10), None);
    }

    #[test]
    fn test_ordering() {
        assert!(ChordDegree::Root < ChordDegree::Third);
        assert!(ChordDegree::Fifth < ChordDegree::Seventh);
        assert!(ChordDegree::Seventh < ChordDegree::Ninth);
    }
}
