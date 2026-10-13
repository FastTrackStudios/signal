//! Duration types for representing time spans
//!
//! Durations are strictly non-negative (>= 0).
//! For positions that can be negative, use Position types instead.

use super::{PositionInQuarterNotes, PositionInSeconds, Tempo};
use facet::Facet;
use std::fmt;

/// Duration in seconds (must be >= 0)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Facet)]
pub struct Duration {
    seconds: f64,
}

impl Duration {
    /// Create a duration from seconds
    ///
    /// # Panics
    /// Panics if seconds is negative
    pub fn from_seconds(seconds: f64) -> Self {
        assert!(seconds >= 0.0, "Duration cannot be negative: {}", seconds);
        Self { seconds }
    }

    /// Try to create a duration from seconds
    pub fn try_from_seconds(seconds: f64) -> Result<Self, String> {
        if seconds < 0.0 {
            Err(format!("Duration cannot be negative: {}", seconds))
        } else {
            Ok(Self { seconds })
        }
    }

    /// Get the duration in seconds
    pub fn as_seconds(&self) -> f64 {
        self.seconds
    }

    /// Zero duration
    pub const ZERO: Self = Self { seconds: 0.0 };

    /// Maximum duration
    pub const MAX: Self = Self { seconds: f64::MAX };

    /// Saturating subtraction (returns ZERO if result would be negative)
    pub fn saturating_sub(self, other: Self) -> Self {
        if self.seconds >= other.seconds {
            Self {
                seconds: self.seconds - other.seconds,
            }
        } else {
            Self::ZERO
        }
    }

    /// Convert to quarter notes using tempo
    pub fn to_quarter_notes(&self, tempo: Tempo) -> DurationInQuarterNotes {
        let qn = self.seconds * (tempo.bpm() / 60.0);
        DurationInQuarterNotes::from_quarter_notes(qn)
    }

    /// Convert to beats using tempo
    pub fn to_beats(&self, tempo: Tempo) -> DurationInBeats {
        let beats = self.seconds * (tempo.bpm() / 60.0);
        DurationInBeats::from_beats(beats)
    }
}

impl Default for Duration {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let minutes = (self.seconds / 60.0).floor() as i32;
        let secs = self.seconds % 60.0;
        write!(f, "{}:{:06.3}", minutes, secs)
    }
}

/// Try to convert a position to a duration
impl TryFrom<PositionInSeconds> for Duration {
    type Error = String;

    fn try_from(pos: PositionInSeconds) -> Result<Self, Self::Error> {
        Self::try_from_seconds(pos.as_seconds())
    }
}

/// Duration in beats (must be >= 0)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Facet)]
pub struct DurationInBeats {
    beats: f64,
}

impl DurationInBeats {
    /// Create a duration from beats
    ///
    /// # Panics
    /// Panics if beats is negative
    pub fn from_beats(beats: f64) -> Self {
        assert!(beats >= 0.0, "Duration cannot be negative: {}", beats);
        Self { beats }
    }

    /// Get the duration in beats
    pub fn as_beats(&self) -> f64 {
        self.beats
    }

    /// Zero duration
    pub const ZERO: Self = Self { beats: 0.0 };

    /// Maximum duration
    pub const MAX: Self = Self { beats: f64::MAX };

    /// Convert to seconds using tempo
    pub fn to_seconds(&self, tempo: Tempo) -> Duration {
        let seconds = self.beats * (60.0 / tempo.bpm());
        Duration::from_seconds(seconds)
    }
}

impl Default for DurationInBeats {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for DurationInBeats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3} beats", self.beats)
    }
}

/// Duration in quarter notes (must be >= 0)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Facet)]
pub struct DurationInQuarterNotes {
    quarter_notes: f64,
}

impl DurationInQuarterNotes {
    /// Create a duration from quarter notes
    ///
    /// # Panics
    /// Panics if quarter_notes is negative
    pub fn from_quarter_notes(quarter_notes: f64) -> Self {
        assert!(
            quarter_notes >= 0.0,
            "Duration cannot be negative: {}",
            quarter_notes
        );
        Self { quarter_notes }
    }

    /// Get the duration in quarter notes
    pub fn as_quarter_notes(&self) -> f64 {
        self.quarter_notes
    }

    /// Zero duration
    pub const ZERO: Self = Self { quarter_notes: 0.0 };

    /// Maximum duration
    pub const MAX: Self = Self {
        quarter_notes: f64::MAX,
    };

    /// Convert to seconds using tempo
    pub fn to_seconds(&self, tempo: Tempo) -> Duration {
        let seconds = self.quarter_notes * (60.0 / tempo.bpm());
        Duration::from_seconds(seconds)
    }

    /// Convert to beats (quarter notes are beats in 4/4 time)
    pub fn to_beats(&self) -> DurationInBeats {
        DurationInBeats::from_beats(self.quarter_notes)
    }
}

impl Default for DurationInQuarterNotes {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for DurationInQuarterNotes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3} qn", self.quarter_notes)
    }
}

/// Try to convert a quarter note position to a duration
impl TryFrom<PositionInQuarterNotes> for DurationInQuarterNotes {
    type Error = String;

    fn try_from(pos: PositionInQuarterNotes) -> Result<Self, Self::Error> {
        let qn = pos.as_quarter_notes();
        if qn < 0.0 {
            Err(format!("Duration cannot be negative: {}", qn))
        } else {
            Ok(Self::from_quarter_notes(qn))
        }
    }
}
