//! Conversion traits for durations.
//!
//! These traits provide a unified interface for converting between different
//! duration representations.

use super::Ticks;
use crate::core::time::TimeSignature;

/// Convert to tick representation at a given PPQ resolution.
///
/// This trait allows any duration type to be converted to ticks at any PPQ.
///
/// # Example
///
/// ```
/// use keyflow_proto::core::{NoteValue, StandardTicks, ToTicks};
///
/// let quarter: StandardTicks = NoteValue::Quarter.to_ticks();
/// assert_eq!(quarter.0, 480);
/// ```
pub trait ToTicks<const PPQ: u32> {
    /// Convert to ticks at the given PPQ resolution.
    fn to_ticks(&self) -> Ticks<PPQ>;
}

/// Create a duration from ticks at a given PPQ resolution.
///
/// This trait allows creating duration types from tick values.
pub trait FromTicks<const PPQ: u32>: Sized {
    /// Create from ticks at the given PPQ resolution.
    fn from_ticks(ticks: Ticks<PPQ>) -> Self;
}

/// Convert to beats relative to a time signature.
///
/// Beats are defined relative to the time signature's beat unit.
/// In 4/4 time, one beat equals one quarter note.
/// In 6/8 time, one beat equals one eighth note (dotted quarter for compound time).
///
/// # Example
///
/// ```
/// use keyflow_proto::core::{NoteValue, ToBeats, TimeSignature};
///
/// let time_sig = TimeSignature::new(4, 4);
/// let quarter = NoteValue::Quarter;
/// assert!((quarter.to_beats(time_sig) - 1.0).abs() < 0.001);
/// ```
pub trait ToBeats {
    /// Convert to beats relative to the given time signature.
    fn to_beats(&self, time_sig: TimeSignature) -> f64;
}

// Implement ToBeats for NoteValue
use super::NoteValue;

impl ToBeats for NoteValue {
    fn to_beats(&self, time_sig: TimeSignature) -> f64 {
        // In any time signature, the beat unit is the denominator
        // A quarter note in 4/4 = 1 beat
        // A quarter note in 6/8 = 2 beats (since eighth is the beat)
        // A half note in 2/2 = 1 beat (since half is the beat)

        // Calculate how many "denominator notes" fit in this note value
        // denominator 4 = quarter = 480 ticks
        // denominator 8 = eighth = 240 ticks
        let beat_ticks = 1920 / i32::from(time_sig.denominator); // ticks per beat
        let note_ticks = self.base_ticks_480();
        f64::from(note_ticks) / f64::from(beat_ticks)
    }
}

// Implement ToBeats for Ticks
impl<const PPQ: u32> ToBeats for Ticks<PPQ> {
    fn to_beats(&self, time_sig: TimeSignature) -> f64 {
        // Convert to standard 480 PPQ first
        let standard = self.to_standard();
        // Then calculate beats
        let beat_ticks = 1920 / i32::from(time_sig.denominator);
        standard.0 as f64 / beat_ticks as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::StandardTicks;

    #[test]
    fn test_note_value_to_beats_4_4() {
        let time_sig = TimeSignature::new(4, 4);

        assert!((NoteValue::Whole.to_beats(time_sig) - 4.0).abs() < 0.001);
        assert!((NoteValue::Half.to_beats(time_sig) - 2.0).abs() < 0.001);
        assert!((NoteValue::Quarter.to_beats(time_sig) - 1.0).abs() < 0.001);
        assert!((NoteValue::Eighth.to_beats(time_sig) - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_note_value_to_beats_6_8() {
        let time_sig = TimeSignature::new(6, 8);

        // In 6/8, the beat unit is the eighth note
        assert!((NoteValue::Quarter.to_beats(time_sig) - 2.0).abs() < 0.001);
        assert!((NoteValue::Eighth.to_beats(time_sig) - 1.0).abs() < 0.001);
        assert!((NoteValue::Sixteenth.to_beats(time_sig) - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_note_value_to_beats_2_2() {
        let time_sig = TimeSignature::new(2, 2);

        // In 2/2, the beat unit is the half note
        assert!((NoteValue::Whole.to_beats(time_sig) - 2.0).abs() < 0.001);
        assert!((NoteValue::Half.to_beats(time_sig) - 1.0).abs() < 0.001);
        assert!((NoteValue::Quarter.to_beats(time_sig) - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_ticks_to_beats() {
        let time_sig = TimeSignature::new(4, 4);

        let quarter = StandardTicks::new(480);
        assert!((quarter.to_beats(time_sig) - 1.0).abs() < 0.001);

        let half = StandardTicks::new(960);
        assert!((half.to_beats(time_sig) - 2.0).abs() < 0.001);
    }
}
