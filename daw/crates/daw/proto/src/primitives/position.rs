//! Position types for representing points in time
//!
//! Positions can be negative (for pre-roll, count-in, offsets).
//! For strictly non-negative time spans, use Duration types instead.

use super::{Duration, Tempo, TimeSignature};
use facet::Facet;
use std::fmt;

/// Position in seconds (can be negative for pre-roll, count-in, etc.)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Facet)]
pub struct PositionInSeconds {
    seconds: f64,
}

impl PositionInSeconds {
    /// Create a position from seconds
    pub fn from_seconds(seconds: f64) -> Self {
        Self { seconds }
    }

    /// Get the position in seconds
    pub fn as_seconds(&self) -> f64 {
        self.seconds
    }

    /// Zero position
    pub const ZERO: Self = Self { seconds: 0.0 };

    /// Convert to absolute duration (takes absolute value)
    pub fn abs(self) -> Duration {
        Duration::from_seconds(self.seconds.abs())
    }

    /// Convert to musical position using tempo and time signature
    pub fn to_musical(&self, tempo: Tempo, time_signature: TimeSignature) -> MusicalPosition {
        let beats_per_measure = time_signature.numerator() as f64;
        let total_beats = self.seconds * (tempo.bpm() / 60.0);
        let measure = (total_beats / beats_per_measure).floor() as i32;
        let beats_in_measure = total_beats % beats_per_measure;
        let beat = beats_in_measure.floor() as i32;
        let subdivision = ((beats_in_measure - beat as f64) * 1000.0).round() as i32;

        MusicalPosition::new(measure, beat, subdivision.clamp(0, 999))
    }

    /// Convert to quarter notes using tempo
    pub fn to_quarter_notes(&self, tempo: Tempo) -> PositionInQuarterNotes {
        let quarter_notes = self.seconds * (tempo.bpm() / 60.0);
        PositionInQuarterNotes::from_quarter_notes(quarter_notes)
    }
}

impl Default for PositionInSeconds {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for PositionInSeconds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let abs_seconds = self.seconds.abs();
        let sign = if self.seconds < 0.0 { "-" } else { "" };
        let minutes = (abs_seconds / 60.0).floor() as i32;
        let secs = abs_seconds % 60.0;
        write!(f, "{}{}:{:06.3}", sign, minutes, secs)
    }
}

/// Position in beats (can be negative)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Facet)]
pub struct PositionInBeats {
    beats: f64,
}

impl PositionInBeats {
    /// Create a position from beats
    pub fn from_beats(beats: f64) -> Self {
        Self { beats }
    }

    /// Get the position in beats
    pub fn as_beats(&self) -> f64 {
        self.beats
    }

    /// Zero position
    pub const ZERO: Self = Self { beats: 0.0 };

    /// Convert to seconds using tempo
    pub fn to_seconds(&self, tempo: Tempo) -> PositionInSeconds {
        let seconds = self.beats * (60.0 / tempo.bpm());
        PositionInSeconds::from_seconds(seconds)
    }
}

impl Default for PositionInBeats {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for PositionInBeats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3} beats", self.beats)
    }
}

/// Position in quarter notes (can be negative)
///
/// Quarter notes are central to REAPER's time mapping system.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Facet)]
pub struct PositionInQuarterNotes {
    quarter_notes: f64,
}

impl PositionInQuarterNotes {
    /// Create a position from quarter notes
    pub fn from_quarter_notes(quarter_notes: f64) -> Self {
        Self { quarter_notes }
    }

    /// Get the position in quarter notes
    pub fn as_quarter_notes(&self) -> f64 {
        self.quarter_notes
    }

    /// Zero position
    pub const ZERO: Self = Self { quarter_notes: 0.0 };

    /// Convert to seconds using tempo
    pub fn to_seconds(&self, tempo: Tempo) -> PositionInSeconds {
        let seconds = self.quarter_notes * (60.0 / tempo.bpm());
        PositionInSeconds::from_seconds(seconds)
    }

    /// Convert to musical position using time signature
    pub fn to_musical(&self, time_signature: TimeSignature) -> MusicalPosition {
        let beats_per_measure = time_signature.numerator() as f64;
        let measure = (self.quarter_notes / beats_per_measure).floor() as i32;
        let beats_in_measure = self.quarter_notes % beats_per_measure;
        let beat = beats_in_measure.floor() as i32;
        let subdivision = ((beats_in_measure - beat as f64) * 1000.0).round() as i32;

        MusicalPosition::new(measure, beat, subdivision.clamp(0, 999))
    }
}

impl Default for PositionInQuarterNotes {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for PositionInQuarterNotes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3} qn", self.quarter_notes)
    }
}

/// Musical position (measure.beat.subdivision)
///
/// Can be negative for positions before project start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct MusicalPosition {
    pub measure: i32,
    pub beat: i32,
    pub subdivision: i32, // 0-999
}

impl MusicalPosition {
    /// Create a new musical position
    ///
    /// Subdivision must be in range 0-999
    pub fn new(measure: i32, beat: i32, subdivision: i32) -> Self {
        assert!(
            (0..=999).contains(&subdivision),
            "Subdivision must be 0-999, got {}",
            subdivision
        );
        Self {
            measure,
            beat,
            subdivision,
        }
    }

    /// Zero position
    pub const ZERO: Self = Self {
        measure: 0,
        beat: 0,
        subdivision: 0,
    };

    /// Convert to seconds using tempo and time signature
    pub fn to_seconds(&self, tempo: Tempo, time_signature: TimeSignature) -> PositionInSeconds {
        let beats_per_measure = time_signature.numerator() as f64;
        let total_beats = self.measure as f64 * beats_per_measure
            + self.beat as f64
            + self.subdivision as f64 / 1000.0;
        let seconds = total_beats * (60.0 / tempo.bpm());
        PositionInSeconds::from_seconds(seconds)
    }

    /// Convert to quarter notes using time signature
    pub fn to_quarter_notes(&self, time_signature: TimeSignature) -> PositionInQuarterNotes {
        let beats_per_measure = time_signature.numerator() as f64;
        let total_qn = self.measure as f64 * beats_per_measure
            + self.beat as f64
            + self.subdivision as f64 / 1000.0;
        PositionInQuarterNotes::from_quarter_notes(total_qn)
    }
}

impl Default for MusicalPosition {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for MusicalPosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.measure < 0 || self.beat < 0 {
            "-"
        } else {
            ""
        };
        write!(
            f,
            "{}{}.{}.{:03}",
            sign,
            self.measure.abs() + 1,
            self.beat.abs() + 1,
            self.subdivision.abs()
        )
    }
}

/// MIDI position in PPQ ticks (can be negative)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Facet)]
pub struct PositionInPpq {
    ppq: i64,
}

impl PositionInPpq {
    /// Create a position from PPQ ticks
    pub fn from_ppq(ppq: i64) -> Self {
        Self { ppq }
    }

    /// Get the position in PPQ ticks
    pub fn as_ppq(&self) -> i64 {
        self.ppq
    }

    /// Zero position
    pub const ZERO: Self = Self { ppq: 0 };

    /// Convert to quarter notes using PPQ resolution
    pub fn to_quarter_notes(&self, ppq_resolution: f64) -> PositionInQuarterNotes {
        let qn = self.ppq as f64 / ppq_resolution;
        PositionInQuarterNotes::from_quarter_notes(qn)
    }

    /// Convert to seconds using tempo and PPQ resolution
    pub fn to_seconds(&self, tempo: Tempo, ppq_resolution: f64) -> PositionInSeconds {
        let qn = self.to_quarter_notes(ppq_resolution);
        qn.to_seconds(tempo)
    }
}

impl Default for PositionInPpq {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for PositionInPpq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ppq", self.ppq)
    }
}
