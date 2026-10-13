//! Unified notation duration for cross-system conversion.
//!
//! This module provides [`NotationDuration`], a unified representation
//! that can be converted between keyflow's rhythm types and engraver's
//! duration types without losing information.

use super::{NoteValue, Ticks, TupletRatio};
use crate::core::time::TimeSignature;
use facet::Facet;
use serde::{Deserialize, Serialize};

/// Unified duration for conversion between notation systems.
///
/// This type captures all the information needed to represent a musical
/// duration in a notation-agnostic way, enabling lossless conversion between
/// different rhythm representations (keyflow `ChordRhythm`, engraver duration, etc.).
///
/// # Example
///
/// ```
/// use keyflow_proto::core::duration::{NotationDuration, NoteValue, RhythmType};
///
/// // A dotted quarter note chord
/// let duration = NotationDuration::new(NoteValue::Quarter)
///     .with_dots(1)
///     .as_chord();
///
/// assert_eq!(duration.note_value, NoteValue::Quarter);
/// assert_eq!(duration.dots, 1);
/// assert!(matches!(duration.rhythm_type, RhythmType::Chord));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Facet)]
pub struct NotationDuration {
    /// The base note value (quarter, eighth, etc.)
    pub note_value: NoteValue,

    /// Number of augmentation dots (0, 1, or 2)
    pub dots: u8,

    /// Optional tuplet ratio (e.g., 3:2 for triplets)
    pub tuplet: Option<TupletRatio>,

    /// The type of rhythm element
    pub rhythm_type: RhythmType,

    /// Optional multiplier for repeated durations (e.g., s1*8)
    pub multiplier: Option<u16>,

    /// Whether this duration is tied to the next
    pub tied: bool,
}

/// Type of rhythm element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum RhythmType {
    /// A sounding chord
    #[default]
    Chord,
    /// A rest (silence)
    Rest,
    /// A space (invisible, for spacing)
    Space,
    /// Slash notation (each slash = one beat)
    Slashes(u8),
}

impl NotationDuration {
    /// Create a new notation duration with the given note value.
    ///
    /// Defaults to: no dots, no tuplet, chord type, no multiplier, not tied.
    #[must_use]
    pub const fn new(note_value: NoteValue) -> Self {
        Self {
            note_value,
            dots: 0,
            tuplet: None,
            rhythm_type: RhythmType::Chord,
            multiplier: None,
            tied: false,
        }
    }

    /// Create a duration representing one full measure.
    ///
    /// This is typically used as the default duration when none is specified.
    #[must_use]
    pub fn one_measure(time_sig: TimeSignature) -> Self {
        // In 4/4 this is a whole note, in 3/4 a dotted half, etc.
        // For simplicity, we represent this as slashes equal to beats per measure
        Self {
            note_value: NoteValue::Quarter, // Ignored for Slashes
            dots: 0,
            tuplet: None,
            rhythm_type: RhythmType::Slashes(time_sig.numerator),
            multiplier: None,
            tied: false,
        }
    }

    /// Whole note duration.
    #[must_use]
    pub const fn whole() -> Self {
        Self::new(NoteValue::Whole)
    }

    /// Half note duration.
    #[must_use]
    pub const fn half() -> Self {
        Self::new(NoteValue::Half)
    }

    /// Quarter note duration.
    #[must_use]
    pub const fn quarter() -> Self {
        Self::new(NoteValue::Quarter)
    }

    /// Eighth note duration.
    #[must_use]
    pub const fn eighth() -> Self {
        Self::new(NoteValue::Eighth)
    }

    /// Sixteenth note duration.
    #[must_use]
    pub const fn sixteenth() -> Self {
        Self::new(NoteValue::Sixteenth)
    }

    /// Thirty-second note duration.
    #[must_use]
    pub const fn thirty_second() -> Self {
        Self::new(NoteValue::ThirtySecond)
    }

    /// Add dots to this duration.
    #[must_use]
    pub const fn with_dots(mut self, dots: u8) -> Self {
        self.dots = dots;
        self
    }

    /// Make this a dotted duration (one dot).
    #[must_use]
    pub const fn dotted(self) -> Self {
        self.with_dots(1)
    }

    /// Make this a double-dotted duration.
    #[must_use]
    pub const fn double_dotted(self) -> Self {
        self.with_dots(2)
    }

    /// Make this a triplet duration.
    #[must_use]
    pub const fn as_triplet(mut self) -> Self {
        self.tuplet = Some(TupletRatio::TRIPLET);
        self
    }

    /// Set a custom tuplet ratio.
    #[must_use]
    pub const fn with_tuplet(mut self, tuplet: TupletRatio) -> Self {
        self.tuplet = Some(tuplet);
        self
    }

    /// Mark this as a rest.
    #[must_use]
    pub const fn as_rest(mut self) -> Self {
        self.rhythm_type = RhythmType::Rest;
        self
    }

    /// Mark this as a space.
    #[must_use]
    pub const fn as_space(mut self) -> Self {
        self.rhythm_type = RhythmType::Space;
        self
    }

    /// Mark this as a chord (sounding).
    #[must_use]
    pub const fn as_chord(mut self) -> Self {
        self.rhythm_type = RhythmType::Chord;
        self
    }

    /// Set a multiplier for repeated durations.
    #[must_use]
    pub const fn with_multiplier(mut self, mult: u16) -> Self {
        self.multiplier = Some(mult);
        self
    }

    /// Mark this as tied to the next duration.
    #[must_use]
    pub const fn tied(mut self) -> Self {
        self.tied = true;
        self
    }

    /// Check if this is a sounding duration (chord, not rest/space).
    #[must_use]
    pub const fn is_sounding(&self) -> bool {
        matches!(self.rhythm_type, RhythmType::Chord | RhythmType::Slashes(_))
    }

    /// Check if this is a rest.
    #[must_use]
    pub const fn is_rest(&self) -> bool {
        matches!(self.rhythm_type, RhythmType::Rest)
    }

    /// Check if this is a space.
    #[must_use]
    pub const fn is_space(&self) -> bool {
        matches!(self.rhythm_type, RhythmType::Space)
    }

    /// Check if this is slash notation.
    #[must_use]
    pub const fn is_slashes(&self) -> bool {
        matches!(self.rhythm_type, RhythmType::Slashes(_))
    }

    /// Check if this is a triplet.
    #[must_use]
    pub fn is_triplet(&self) -> bool {
        self.tuplet.is_some_and(|t| t.is_triplet())
    }

    /// Get the base tick duration at 480 PPQ (without dots, tuplets, or multiplier).
    #[must_use]
    pub fn base_ticks_480(&self) -> i32 {
        self.note_value.base_ticks_480()
    }

    /// Get the full tick duration at 480 PPQ (with dots, tuplets, and multiplier).
    #[must_use]
    pub fn total_ticks_480(&self) -> i32 {
        let base = self.base_ticks_480();

        // Apply dots: each dot adds half the previous value
        let dotted = match self.dots {
            0 => base,
            1 => base + base / 2,            // 1.5x
            2 => base + base / 2 + base / 4, // 1.75x
            _ => base + base / 2 + base / 4, // Cap at double-dotted
        };

        // Apply tuplet
        let tuplet_applied = match self.tuplet {
            Some(ratio) => ratio.apply_to_ticks(dotted),
            None => dotted,
        };

        // Apply multiplier
        match self.multiplier {
            Some(mult) => tuplet_applied * i32::from(mult),
            None => tuplet_applied,
        }
    }

    /// Get the full tick duration at a given PPQ.
    #[must_use]
    pub fn total_ticks_at_ppq<const PPQ: u32>(&self) -> Ticks<PPQ> {
        let ticks_480 = self.total_ticks_480();
        let scaled = i64::from(ticks_480) * i64::from(PPQ) / 480;
        Ticks(scaled)
    }

    /// Get the duration in beats relative to a time signature.
    #[must_use]
    pub fn to_beats(&self, time_sig: TimeSignature) -> f64 {
        // For slash notation, each slash is one beat
        if let RhythmType::Slashes(count) = self.rhythm_type {
            return f64::from(count);
        }

        // Calculate ticks at 480 PPQ
        let total_ticks = self.total_ticks_480();

        // Ticks per beat depends on time signature denominator
        // denominator 4 = quarter = 480 ticks per beat
        // denominator 8 = eighth = 240 ticks per beat
        let ticks_per_beat = 1920 / i32::from(time_sig.denominator);

        f64::from(total_ticks) / f64::from(ticks_per_beat)
    }
}

impl Default for NotationDuration {
    fn default() -> Self {
        Self::quarter()
    }
}

/// Context for duration conversion.
///
/// Provides all the information needed to convert between different
/// duration representations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DurationContext {
    /// The time signature for beat calculations
    pub time_signature: TimeSignature,

    /// Pulses per quarter note (PPQ) for tick calculations
    pub ppq: i32,

    /// Whether this is compound meter (6/8, 9/8, 12/8, etc.)
    pub compound_meter: bool,
}

impl DurationContext {
    /// Create a new duration context.
    #[must_use]
    pub const fn new(time_signature: TimeSignature, ppq: i32) -> Self {
        Self {
            time_signature,
            ppq,
            compound_meter: false,
        }
    }

    /// Create a context with standard 480 PPQ.
    #[must_use]
    pub const fn standard(time_signature: TimeSignature) -> Self {
        Self::new(time_signature, 480)
    }

    /// Create a context for REAPER (960 PPQ).
    #[must_use]
    pub const fn reaper(time_signature: TimeSignature) -> Self {
        Self::new(time_signature, 960)
    }

    /// Mark this as compound meter.
    #[must_use]
    pub const fn with_compound_meter(mut self, compound: bool) -> Self {
        self.compound_meter = compound;
        self
    }

    /// Auto-detect compound meter from time signature.
    ///
    /// A time signature is compound if:
    /// - The numerator is divisible by 3 (but not 3 itself in some conventions)
    /// - Common compound signatures: 6/8, 9/8, 12/8, 6/4, 9/4, 12/4
    #[must_use]
    pub fn detect_compound_meter(mut self) -> Self {
        let n = self.time_signature.numerator;
        self.compound_meter = n >= 6 && n.is_multiple_of(3);
        self
    }
}

impl Default for DurationContext {
    fn default() -> Self {
        Self::standard(TimeSignature::default())
    }
}

/// Trait for converting to [`NotationDuration`].
pub trait ToNotationDuration {
    /// Convert to a notation duration using the given context.
    fn to_notation_duration(&self, ctx: &DurationContext) -> NotationDuration;
}

// Implement ToNotationDuration for NoteValue (simple conversion)
impl ToNotationDuration for NoteValue {
    fn to_notation_duration(&self, _ctx: &DurationContext) -> NotationDuration {
        NotationDuration::new(*self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notation_duration_new() {
        let duration = NotationDuration::quarter();

        assert_eq!(duration.note_value, NoteValue::Quarter);
        assert_eq!(duration.dots, 0);
        assert!(duration.tuplet.is_none());
        assert!(duration.is_sounding());
    }

    #[test]
    fn test_notation_duration_builder() {
        let duration = NotationDuration::eighth().dotted().as_triplet().as_rest();

        assert_eq!(duration.note_value, NoteValue::Eighth);
        assert_eq!(duration.dots, 1);
        assert!(duration.is_triplet());
        assert!(duration.is_rest());
    }

    #[test]
    fn test_base_ticks() {
        assert_eq!(NotationDuration::whole().base_ticks_480(), 1920);
        assert_eq!(NotationDuration::half().base_ticks_480(), 960);
        assert_eq!(NotationDuration::quarter().base_ticks_480(), 480);
        assert_eq!(NotationDuration::eighth().base_ticks_480(), 240);
    }

    #[test]
    fn test_dotted_ticks() {
        // Dotted quarter = 480 + 240 = 720
        assert_eq!(NotationDuration::quarter().dotted().total_ticks_480(), 720);

        // Double-dotted quarter = 480 + 240 + 120 = 840
        assert_eq!(
            NotationDuration::quarter()
                .double_dotted()
                .total_ticks_480(),
            840
        );
    }

    #[test]
    fn test_triplet_ticks() {
        // Triplet eighth = 240 * 2/3 = 160
        assert_eq!(
            NotationDuration::eighth().as_triplet().total_ticks_480(),
            160
        );
    }

    #[test]
    fn test_multiplier() {
        // Quarter with multiplier of 4 = 480 * 4 = 1920
        assert_eq!(
            NotationDuration::quarter()
                .with_multiplier(4)
                .total_ticks_480(),
            1920
        );
    }

    #[test]
    fn test_to_beats_4_4() {
        let time_sig = TimeSignature::new(4, 4);

        assert!((NotationDuration::whole().to_beats(time_sig) - 4.0).abs() < 0.001);
        assert!((NotationDuration::half().to_beats(time_sig) - 2.0).abs() < 0.001);
        assert!((NotationDuration::quarter().to_beats(time_sig) - 1.0).abs() < 0.001);
        assert!((NotationDuration::eighth().to_beats(time_sig) - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_to_beats_6_8() {
        let time_sig = TimeSignature::new(6, 8);

        // In 6/8, eighth note = 1 beat
        assert!((NotationDuration::quarter().to_beats(time_sig) - 2.0).abs() < 0.001);
        assert!((NotationDuration::eighth().to_beats(time_sig) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_duration_context_compound_detection() {
        let ctx_4_4 = DurationContext::standard(TimeSignature::new(4, 4)).detect_compound_meter();
        assert!(!ctx_4_4.compound_meter);

        let ctx_6_8 = DurationContext::standard(TimeSignature::new(6, 8)).detect_compound_meter();
        assert!(ctx_6_8.compound_meter);

        let ctx_12_8 = DurationContext::standard(TimeSignature::new(12, 8)).detect_compound_meter();
        assert!(ctx_12_8.compound_meter);
    }

    #[test]
    fn test_rhythm_type_checks() {
        let chord = NotationDuration::quarter().as_chord();
        assert!(chord.is_sounding());
        assert!(!chord.is_rest());
        assert!(!chord.is_space());

        let rest = NotationDuration::quarter().as_rest();
        assert!(!rest.is_sounding());
        assert!(rest.is_rest());

        let space = NotationDuration::quarter().as_space();
        assert!(!space.is_sounding());
        assert!(space.is_space());
    }
}
