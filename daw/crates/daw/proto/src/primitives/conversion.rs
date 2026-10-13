//! Position conversion result types
//!
//! These types hold the results of position conversions that require
//! tempo map context from the project.

use super::{PositionInBeats, PositionInQuarterNotes, TimeSignature};
use facet::Facet;

/// Result of converting time to beats
///
/// Includes the full beat position, measure information, and time signature context
#[derive(Debug, Clone, PartialEq, Facet, Default)]
pub struct TimeToBeatsResult {
    /// Position in beats since project start
    pub full_beats: PositionInBeats,
    /// Index of the measure in which the given position is located (0-based)
    pub measure_index: i32,
    /// Position in beats within that measure (0-based)
    pub beats_since_measure: PositionInBeats,
    /// Time signature of that measure
    pub time_signature: TimeSignature,
}

/// Result of converting quarter notes to measure information
///
/// Provides the measure index and the start/end positions in quarter notes
#[derive(Debug, Clone, PartialEq, Facet, Default)]
pub struct QuarterNotesToMeasureResult {
    /// Measure index in project (0-based)
    pub measure_index: i32,
    /// Start position of the measure in quarter notes
    pub start: PositionInQuarterNotes,
    /// End position of the measure in quarter notes
    pub end: PositionInQuarterNotes,
    /// Time signature of that measure
    pub time_signature: TimeSignature,
}

/// Result of converting time to quarter notes with measure context
#[derive(Debug, Clone, PartialEq, Facet, Default)]
pub struct TimeToQuarterNotesResult {
    /// Position in quarter notes since project start
    pub quarter_notes: PositionInQuarterNotes,
    /// Index of the measure in which the given position is located (0-based)
    pub measure_index: i32,
    /// Position in quarter notes within that measure (0-based)
    pub quarter_notes_since_measure: PositionInQuarterNotes,
    /// Time signature of that measure
    pub time_signature: TimeSignature,
}
