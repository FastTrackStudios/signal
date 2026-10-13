//! Time System - Musical durations and positions
//!
//! This module re-exports DAW primitives for time-related types.
//! All time primitives come from the DAW module to avoid duplication.

pub mod duration;

// Re-export DAW primitives (re-exported at daw_proto root)
pub use daw_proto::{
    MidiPosition, MusicalPosition, Position, Tempo, TimePosition, TimeRange, TimeSignature,
};

// Type aliases for backward compatibility
pub type MusicalDuration = MusicalPosition;
pub type PPQPosition = MidiPosition;
pub type PPQDuration = MidiPosition;
pub type TimeDuration = TimePosition;
pub type Duration = Position;

use facet::Facet;
use std::fmt;

/// Chart-specific position that includes section index
/// This wraps DAW's Position with section tracking for chart parsing
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct AbsolutePosition {
    /// The total duration from the start of the song (using DAW's MusicalPosition)
    pub total_duration: MusicalPosition,
    /// Which section this position is in
    pub section_index: usize,
}

impl AbsolutePosition {
    pub fn new(total_duration: MusicalPosition, section_index: usize) -> Self {
        Self {
            total_duration,
            section_index,
        }
    }

    /// Create a position at the start of a section
    pub fn at_section_start(total_duration: MusicalPosition, section_index: usize) -> Self {
        Self {
            total_duration,
            section_index,
        }
    }

    /// Create a position at the very beginning of the song
    pub fn at_beginning() -> Self {
        Self {
            total_duration: MusicalPosition::ZERO,
            section_index: 0,
        }
    }

    /// Get measures as u32 (for backward compatibility)
    pub fn measures(&self) -> u32 {
        self.total_duration.measure.max(0) as u32
    }

    /// Get beats as u32 (for backward compatibility)
    pub fn beats(&self) -> u32 {
        self.total_duration.beat.max(0) as u32
    }

    /// Get subdivisions as u32 (for backward compatibility)
    pub fn subdivisions(&self) -> u32 {
        self.total_duration.subdivision.clamp(0, 999) as u32
    }
}

impl fmt::Display for AbsolutePosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Section {} @ {}",
            self.section_index, self.total_duration
        )
    }
}

/// Compatibility extensions for daw-proto types
mod compat {
    use super::*;

    /// Extension methods for MusicalPosition
    pub trait MusicalPositionExt {
        fn start() -> MusicalPosition;
        fn try_new(measure: i32, beat: i32, subdivision: i32) -> Result<MusicalPosition, String>;
        fn zero() -> MusicalPosition;
        fn from_beats(total_beats: f64, time_sig: TimeSignature) -> MusicalPosition;
        fn to_beats(&self, time_sig: TimeSignature) -> f64;
    }

    impl MusicalPositionExt for MusicalPosition {
        fn start() -> MusicalPosition {
            MusicalPosition::ZERO
        }

        fn try_new(measure: i32, beat: i32, subdivision: i32) -> Result<MusicalPosition, String> {
            if !(0..=999).contains(&subdivision) {
                return Err(format!("Subdivision must be 0-999, got {}", subdivision));
            }
            Ok(MusicalPosition::new(measure, beat, subdivision))
        }

        fn zero() -> MusicalPosition {
            MusicalPosition::ZERO
        }

        fn from_beats(total_beats: f64, time_sig: TimeSignature) -> MusicalPosition {
            let beats_per_measure = time_sig.numerator() as f64;
            let measures = (total_beats / beats_per_measure).floor() as i32;
            let remaining_beats = total_beats - (measures as f64 * beats_per_measure);
            let beats = remaining_beats.floor() as i32;
            let subdivisions = ((remaining_beats - beats as f64) * 1000.0).round() as i32;
            MusicalPosition::new(measures, beats, subdivisions.clamp(0, 999))
        }

        fn to_beats(&self, time_sig: TimeSignature) -> f64 {
            let beats_per_measure = time_sig.numerator() as f64;
            self.measure.max(0) as f64 * beats_per_measure
                + self.beat.max(0) as f64
                + self.subdivision.clamp(0, 999) as f64 / 1000.0
        }
    }

    /// Extension methods for TimeSignature
    pub trait TimeSignatureExt {
        fn common_time() -> TimeSignature;
    }

    impl TimeSignatureExt for TimeSignature {
        fn common_time() -> TimeSignature {
            TimeSignature::new(4, 4)
        }
    }
}

pub use compat::*;
