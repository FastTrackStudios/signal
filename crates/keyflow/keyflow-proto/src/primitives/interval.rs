//! Musical intervals - adapted from v1 with improvements
//!
//! Comprehensive interval system supporting all common and extended intervals

use facet::Facet;
use std::fmt::Display;

/// Enum representing all possible intervals of a chord
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, Facet)]
#[repr(u8)]
pub enum Interval {
    Unison,
    MinorSecond,
    MajorSecond,
    AugmentedSecond,
    MinorThird,
    MajorThird,
    AugmentedThird,
    PerfectFourth,
    AugmentedFourth,
    DiminishedFifth,
    PerfectFifth,
    AugmentedFifth,
    MinorSixth,
    MajorSixth,
    AugmentedSixth,
    DiminishedSeventh,
    MinorSeventh,
    MajorSeventh,
    AugmentedSeventh,
    Octave,
    PerfectOctave,
    FlatNinth,
    Ninth,
    SharpNinth,
    Eleventh,
    SharpEleventh,
    FlatThirteenth,
    Thirteenth,
}

impl Interval {
    /// Returns the semitone representation of the interval
    pub fn semitones(&self) -> u8 {
        match self {
            Interval::Unison => 0,
            Interval::MinorSecond => 1,
            Interval::MajorSecond => 2,
            Interval::AugmentedSecond => 3,
            Interval::MinorThird => 3,
            Interval::MajorThird => 4,
            Interval::AugmentedThird => 5,
            Interval::PerfectFourth => 5,
            Interval::AugmentedFourth => 6,
            Interval::DiminishedFifth => 6,
            Interval::PerfectFifth => 7,
            Interval::AugmentedFifth => 8,
            Interval::MinorSixth => 8,
            Interval::MajorSixth => 9,
            Interval::AugmentedSixth => 10,
            Interval::DiminishedSeventh => 9,
            Interval::MinorSeventh => 10,
            Interval::MajorSeventh => 11,
            Interval::AugmentedSeventh => 12,
            Interval::Octave => 12,
            Interval::PerfectOctave => 12,
            Interval::FlatNinth => 13,
            Interval::Ninth => 14,
            Interval::SharpNinth => 15,
            Interval::Eleventh => 17,
            Interval::SharpEleventh => 18,
            Interval::FlatThirteenth => 20,
            Interval::Thirteenth => 21,
        }
    }

    /// Transforms the interval into its semantic form
    pub fn to_semantic_interval(&self) -> SemInterval {
        match self {
            Interval::Unison => SemInterval::Root,
            Interval::MinorSecond | Interval::MajorSecond | Interval::AugmentedSecond => {
                SemInterval::Second
            }
            Interval::MinorThird | Interval::MajorThird | Interval::AugmentedThird => {
                SemInterval::Third
            }
            Interval::PerfectFourth | Interval::AugmentedFourth => SemInterval::Fourth,
            Interval::DiminishedFifth | Interval::PerfectFifth | Interval::AugmentedFifth => {
                SemInterval::Fifth
            }
            Interval::MinorSixth | Interval::MajorSixth | Interval::AugmentedSixth => {
                SemInterval::Sixth
            }
            Interval::DiminishedSeventh
            | Interval::MinorSeventh
            | Interval::MajorSeventh
            | Interval::AugmentedSeventh => SemInterval::Seventh,
            Interval::Octave | Interval::PerfectOctave => SemInterval::Root,
            Interval::FlatNinth | Interval::Ninth | Interval::SharpNinth => SemInterval::Ninth,
            Interval::Eleventh | Interval::SharpEleventh => SemInterval::Eleventh,
            Interval::FlatThirteenth | Interval::Thirteenth => SemInterval::Thirteenth,
        }
    }

    /// Transforms given interval into its chord notation form
    pub fn to_chord_notation(&self) -> String {
        match self {
            Interval::Unison => "1".to_string(),
            Interval::MinorSecond => "b2".to_string(),
            Interval::MajorSecond => "2".to_string(),
            Interval::AugmentedSecond => "#2".to_string(),
            Interval::MinorThird => "b3".to_string(),
            Interval::MajorThird => "3".to_string(),
            Interval::AugmentedThird => "#3".to_string(),
            Interval::PerfectFourth => "4".to_string(),
            Interval::AugmentedFourth => "#4".to_string(),
            Interval::DiminishedFifth => "b5".to_string(),
            Interval::PerfectFifth => "5".to_string(),
            Interval::AugmentedFifth => "#5".to_string(),
            Interval::MinorSixth => "b6".to_string(),
            Interval::MajorSixth => "6".to_string(),
            Interval::AugmentedSixth => "#6".to_string(),
            Interval::DiminishedSeventh => "bb7".to_string(),
            Interval::MinorSeventh => "7".to_string(),
            Interval::MajorSeventh => "maj7".to_string(),
            Interval::AugmentedSeventh => "#7".to_string(),
            Interval::Octave => "8".to_string(),
            Interval::PerfectOctave => "8".to_string(),
            Interval::FlatNinth => "b9".to_string(),
            Interval::Ninth => "9".to_string(),
            Interval::SharpNinth => "#9".to_string(),
            Interval::Eleventh => "11".to_string(),
            Interval::SharpEleventh => "#11".to_string(),
            Interval::FlatThirteenth => "b13".to_string(),
            Interval::Thirteenth => "13".to_string(),
        }
    }

    /// Parse interval from chord notation
    pub fn from_chord_notation(i: &str) -> Option<Interval> {
        match i {
            "1" => Some(Interval::Unison),
            "b2" => Some(Interval::MinorSecond),
            "2" => Some(Interval::MajorSecond),
            "b3" => Some(Interval::MinorThird),
            "3" => Some(Interval::MajorThird),
            "4" => Some(Interval::PerfectFourth),
            "#4" => Some(Interval::AugmentedFourth),
            "b5" => Some(Interval::DiminishedFifth),
            "5" => Some(Interval::PerfectFifth),
            "#5" => Some(Interval::AugmentedFifth),
            "b6" => Some(Interval::MinorSixth),
            "6" => Some(Interval::MajorSixth),
            "bb7" => Some(Interval::DiminishedSeventh),
            "7" => Some(Interval::MinorSeventh),
            "maj7" => Some(Interval::MajorSeventh),
            "8" => Some(Interval::Octave),
            "b9" => Some(Interval::FlatNinth),
            "9" => Some(Interval::Ninth),
            "#9" => Some(Interval::SharpNinth),
            "11" => Some(Interval::Eleventh),
            "#11" => Some(Interval::SharpEleventh),
            "b13" => Some(Interval::FlatThirteenth),
            "13" => Some(Interval::Thirteenth),
            _ => None,
        }
    }

    /// Convert semitones to the most common interval representation
    pub fn from_semitones(semitones: u8) -> Option<Interval> {
        match semitones {
            0 => Some(Interval::Unison),
            1 => Some(Interval::MinorSecond),
            2 => Some(Interval::MajorSecond),
            3 => Some(Interval::MinorThird),
            4 => Some(Interval::MajorThird),
            5 => Some(Interval::PerfectFourth),
            6 => Some(Interval::DiminishedFifth),
            7 => Some(Interval::PerfectFifth),
            8 => Some(Interval::MinorSixth),
            9 => Some(Interval::MajorSixth),
            10 => Some(Interval::MinorSeventh),
            11 => Some(Interval::MajorSeventh),
            _ => None,
        }
    }

    // Convenience constructors
    pub fn unison() -> Self {
        Interval::Unison
    }
    pub fn minor_second() -> Self {
        Interval::MinorSecond
    }
    pub fn major_second() -> Self {
        Interval::MajorSecond
    }
    pub fn minor_third() -> Self {
        Interval::MinorThird
    }
    pub fn major_third() -> Self {
        Interval::MajorThird
    }
    pub fn perfect_fourth() -> Self {
        Interval::PerfectFourth
    }
    pub fn tritone() -> Self {
        Interval::DiminishedFifth
    }
    pub fn perfect_fifth() -> Self {
        Interval::PerfectFifth
    }
    pub fn minor_sixth() -> Self {
        Interval::MinorSixth
    }
    pub fn major_sixth() -> Self {
        Interval::MajorSixth
    }
    pub fn minor_seventh() -> Self {
        Interval::MinorSeventh
    }
    pub fn major_seventh() -> Self {
        Interval::MajorSeventh
    }
}

impl Display for Interval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_chord_notation())
    }
}

/// Semantic intervals - abstract interval degrees independent of quality
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SemInterval {
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

impl SemInterval {
    /// Get numeric representation of the semantic interval
    pub fn numeric(&self) -> u8 {
        *self as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interval_semitones() {
        assert_eq!(Interval::Unison.semitones(), 0);
        assert_eq!(Interval::MajorThird.semitones(), 4);
        assert_eq!(Interval::PerfectFifth.semitones(), 7);
        assert_eq!(Interval::MajorSeventh.semitones(), 11);
    }

    #[test]
    fn test_from_semitones() {
        assert_eq!(Interval::from_semitones(0), Some(Interval::Unison));
        assert_eq!(Interval::from_semitones(4), Some(Interval::MajorThird));
        assert_eq!(Interval::from_semitones(7), Some(Interval::PerfectFifth));
    }

    #[test]
    fn test_chord_notation() {
        assert_eq!(Interval::MajorThird.to_chord_notation(), "3");
        assert_eq!(Interval::MinorSeventh.to_chord_notation(), "7");
        assert_eq!(Interval::MajorSeventh.to_chord_notation(), "maj7");
    }

    #[test]
    fn test_from_chord_notation() {
        assert_eq!(
            Interval::from_chord_notation("3"),
            Some(Interval::MajorThird)
        );
        assert_eq!(
            Interval::from_chord_notation("7"),
            Some(Interval::MinorSeventh)
        );
        assert_eq!(
            Interval::from_chord_notation("maj7"),
            Some(Interval::MajorSeventh)
        );
    }

    #[test]
    fn test_semantic_interval() {
        assert_eq!(
            Interval::MinorThird.to_semantic_interval(),
            SemInterval::Third
        );
        assert_eq!(
            Interval::MajorThird.to_semantic_interval(),
            SemInterval::Third
        );
        assert_eq!(
            Interval::AugmentedThird.to_semantic_interval(),
            SemInterval::Third
        );
    }
}
