//! Canonical note value enum.
//!
//! `NoteValue` represents the base duration of a note without dots or tuplet
//! modifications. This is the single source of truth for note value definitions.

use super::{Ticks, ToTicks};
use facet::Facet;

/// Canonical note value (duration kind).
///
/// Represents the base duration of a note without dots or tuplet modifications.
/// All other duration types should convert to/from this canonical representation.
///
/// # Tick Values (at 480 PPQ)
///
/// | Value | Ticks | Quarters |
/// |-------|-------|----------|
/// | Whole | 1920 | 4.0 |
/// | Half | 960 | 2.0 |
/// | Quarter | 480 | 1.0 |
/// | Eighth | 240 | 0.5 |
/// | Sixteenth | 120 | 0.25 |
/// | ThirtySecond | 60 | 0.125 |
/// | SixtyFourth | 30 | 0.0625 |
#[repr(u8)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Facet, Default,
)]
pub enum NoteValue {
    /// Whole note (semibreve) - 4 beats in 4/4
    Whole,
    /// Half note (minim) - 2 beats in 4/4
    Half,
    /// Quarter note (crotchet) - 1 beat in 4/4
    #[default]
    Quarter,
    /// Eighth note (quaver) - 1/2 beat in 4/4
    Eighth,
    /// Sixteenth note (semiquaver) - 1/4 beat in 4/4
    Sixteenth,
    /// Thirty-second note (demisemiquaver) - 1/8 beat in 4/4
    ThirtySecond,
    /// Sixty-fourth note (hemidemisemiquaver) - 1/16 beat in 4/4
    SixtyFourth,
}

impl NoteValue {
    /// Get the base tick value at 480 PPQ (standard MIDI resolution).
    ///
    /// This is the canonical tick value without dots or tuplet modifications.
    #[must_use]
    pub const fn base_ticks_480(self) -> i32 {
        match self {
            Self::Whole => 1920,
            Self::Half => 960,
            Self::Quarter => 480,
            Self::Eighth => 240,
            Self::Sixteenth => 120,
            Self::ThirtySecond => 60,
            Self::SixtyFourth => 30,
        }
    }

    /// Get the duration in quarter notes (1.0 = quarter note).
    #[must_use]
    pub const fn quarters(self) -> f64 {
        match self {
            Self::Whole => 4.0,
            Self::Half => 2.0,
            Self::Quarter => 1.0,
            Self::Eighth => 0.5,
            Self::Sixteenth => 0.25,
            Self::ThirtySecond => 0.125,
            Self::SixtyFourth => 0.0625,
        }
    }

    /// Get the number of flags/beams for this note value.
    ///
    /// Returns 0 for whole, half, and quarter notes.
    #[must_use]
    pub const fn flags(self) -> u8 {
        match self {
            Self::Whole | Self::Half | Self::Quarter => 0,
            Self::Eighth => 1,
            Self::Sixteenth => 2,
            Self::ThirtySecond => 3,
            Self::SixtyFourth => 4,
        }
    }

    /// Get the numeric denominator value (1, 2, 4, 8, 16, 32, 64).
    ///
    /// This is the value used in LilyPond-style notation.
    #[must_use]
    pub const fn denominator(self) -> u8 {
        match self {
            Self::Whole => 1,
            Self::Half => 2,
            Self::Quarter => 4,
            Self::Eighth => 8,
            Self::Sixteenth => 16,
            Self::ThirtySecond => 32,
            Self::SixtyFourth => 64,
        }
    }

    /// Create from a numeric denominator string ("1", "2", "4", "8", "16", "32", "64").
    ///
    /// Returns `None` for invalid values.
    #[must_use]
    pub fn from_denominator_str(s: &str) -> Option<Self> {
        match s {
            "1" => Some(Self::Whole),
            "2" => Some(Self::Half),
            "4" => Some(Self::Quarter),
            "8" => Some(Self::Eighth),
            "16" => Some(Self::Sixteenth),
            "32" => Some(Self::ThirtySecond),
            "64" => Some(Self::SixtyFourth),
            _ => None,
        }
    }

    /// Create from a numeric denominator (1, 2, 4, 8, 16, 32, 64).
    ///
    /// Returns `None` for invalid values.
    #[must_use]
    pub const fn from_denominator(n: u8) -> Option<Self> {
        match n {
            1 => Some(Self::Whole),
            2 => Some(Self::Half),
            4 => Some(Self::Quarter),
            8 => Some(Self::Eighth),
            16 => Some(Self::Sixteenth),
            32 => Some(Self::ThirtySecond),
            64 => Some(Self::SixtyFourth),
            _ => None,
        }
    }

    /// Get the base tick value at any PPQ resolution.
    ///
    /// Performs the conversion: `base_ticks_480 * target_ppq / 480`
    #[must_use]
    pub const fn base_ticks_at_ppq<const PPQ: u32>(self) -> i32 {
        (self.base_ticks_480() as i64 * PPQ as i64 / 480) as i32
    }
}

impl<const PPQ: u32> ToTicks<PPQ> for NoteValue {
    fn to_ticks(&self) -> Ticks<PPQ> {
        Ticks(self.base_ticks_at_ppq::<PPQ>() as i64)
    }
}

impl std::fmt::Display for NoteValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Whole => write!(f, "whole"),
            Self::Half => write!(f, "half"),
            Self::Quarter => write!(f, "quarter"),
            Self::Eighth => write!(f, "eighth"),
            Self::Sixteenth => write!(f, "sixteenth"),
            Self::ThirtySecond => write!(f, "32nd"),
            Self::SixtyFourth => write!(f, "64th"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::StandardTicks;

    #[test]
    fn test_base_ticks_480() {
        assert_eq!(NoteValue::Whole.base_ticks_480(), 1920);
        assert_eq!(NoteValue::Half.base_ticks_480(), 960);
        assert_eq!(NoteValue::Quarter.base_ticks_480(), 480);
        assert_eq!(NoteValue::Eighth.base_ticks_480(), 240);
        assert_eq!(NoteValue::Sixteenth.base_ticks_480(), 120);
        assert_eq!(NoteValue::ThirtySecond.base_ticks_480(), 60);
        assert_eq!(NoteValue::SixtyFourth.base_ticks_480(), 30);
    }

    #[test]
    fn test_quarters() {
        assert_eq!(NoteValue::Whole.quarters(), 4.0);
        assert_eq!(NoteValue::Half.quarters(), 2.0);
        assert_eq!(NoteValue::Quarter.quarters(), 1.0);
        assert_eq!(NoteValue::Eighth.quarters(), 0.5);
        assert_eq!(NoteValue::Sixteenth.quarters(), 0.25);
    }

    #[test]
    fn test_flags() {
        assert_eq!(NoteValue::Whole.flags(), 0);
        assert_eq!(NoteValue::Half.flags(), 0);
        assert_eq!(NoteValue::Quarter.flags(), 0);
        assert_eq!(NoteValue::Eighth.flags(), 1);
        assert_eq!(NoteValue::Sixteenth.flags(), 2);
        assert_eq!(NoteValue::ThirtySecond.flags(), 3);
        assert_eq!(NoteValue::SixtyFourth.flags(), 4);
    }

    #[test]
    fn test_denominator_roundtrip() {
        for note in [
            NoteValue::Whole,
            NoteValue::Half,
            NoteValue::Quarter,
            NoteValue::Eighth,
            NoteValue::Sixteenth,
            NoteValue::ThirtySecond,
            NoteValue::SixtyFourth,
        ] {
            let denom = note.denominator();
            let recovered = NoteValue::from_denominator(denom);
            assert_eq!(recovered, Some(note));
        }
    }

    #[test]
    fn test_base_ticks_at_ppq_960() {
        // REAPER uses 960 PPQ
        assert_eq!(NoteValue::Whole.base_ticks_at_ppq::<960>(), 3840);
        assert_eq!(NoteValue::Half.base_ticks_at_ppq::<960>(), 1920);
        assert_eq!(NoteValue::Quarter.base_ticks_at_ppq::<960>(), 960);
        assert_eq!(NoteValue::Eighth.base_ticks_at_ppq::<960>(), 480);
    }

    #[test]
    fn test_to_ticks_standard() {
        let quarter: StandardTicks = NoteValue::Quarter.to_ticks();
        assert_eq!(quarter.0, 480);
    }
}
