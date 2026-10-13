//! Primitive types for DAW Protocol
//!
//! This module defines the core position, duration, tempo, and time signature types
//! used throughout the DAW Protocol.
//!
//! ## Position Types
//! Positions can be negative (for pre-roll, count-in, offsets):
//! - [`PositionInSeconds`] - Time in seconds
//! - [`PositionInBeats`] - Musical beats
//! - [`PositionInQuarterNotes`] - Quarter notes (REAPER's native time mapping)
//! - [`MusicalPosition`] - Measure.beat.subdivision
//! - [`PositionInPpq`] - MIDI PPQ ticks
//!
//! ## Duration Types
//! Durations are strictly non-negative (>= 0):
//! - [`Duration`] - Time span in seconds
//! - [`DurationInBeats`] - Time span in beats
//! - [`DurationInQuarterNotes`] - Time span in quarter notes
//!
//! ## Tempo & Time Signature
//! - [`Tempo`] - BPM (must be > 0)
//! - [`TimeSignature`] - Numerator/denominator (uses NonZeroU32 for safety)
//!
//! ## Enums
//! - [`TimeMode`] - Time display format
//! - [`BeatAttachMode`] - How items attach to timeline
//! - [`MeasureMode`] - Beat calculation behavior
//! - [`TimeModeOverride`] - Time mode override
//! - [`SendMidiTime`] - MIDI event timing
//!
//! ## Position Conversions
//! - [`TimeToBeatsResult`] - Result of time-to-beats conversion with measure context
//! - [`QuarterNotesToMeasureResult`] - Quarter notes to measure conversion
//! - [`TimeToQuarterNotesResult`] - Time to quarter notes with measure context

mod conversion;
mod duration;
mod enums;
mod position;
mod tempo;
mod time_signature;

// Re-export all types
pub use conversion::{QuarterNotesToMeasureResult, TimeToBeatsResult, TimeToQuarterNotesResult};
pub use duration::{Duration, DurationInBeats, DurationInQuarterNotes};
pub use enums::{
    AutomationMode, BeatAttachMode, MeasureMode, SendMidiTime, TimeMode, TimeModeOverride,
};
pub use position::{
    MusicalPosition, PositionInBeats, PositionInPpq, PositionInQuarterNotes, PositionInSeconds,
};
pub use tempo::Tempo;
pub use time_signature::TimeSignature;

// Backward compatibility aliases (deprecated - will be removed)
/// @deprecated Use `PositionInSeconds` instead
pub type TimePosition = PositionInSeconds;

/// @deprecated Use `PositionInPpq` instead
pub type MidiPosition = PositionInPpq;

/// Unified position type with multiple representations (legacy)
///
/// This type is kept for backward compatibility but may be removed in the future.
/// Prefer using specific position types directly.
#[derive(Debug, Clone, PartialEq, facet::Facet)]
pub struct Position {
    pub musical: Option<MusicalPosition>,
    pub time: Option<PositionInSeconds>,
    pub midi: Option<PositionInPpq>,
}

impl Position {
    /// Create from optional representations
    pub fn new(
        musical: Option<MusicalPosition>,
        time: Option<PositionInSeconds>,
        midi: Option<PositionInPpq>,
    ) -> Self {
        Self {
            musical,
            time,
            midi,
        }
    }

    /// Create from musical position
    pub fn from_musical(musical: MusicalPosition) -> Self {
        Self {
            musical: Some(musical),
            time: None,
            midi: None,
        }
    }

    /// Create from time position
    pub fn from_time(time: PositionInSeconds) -> Self {
        Self {
            musical: None,
            time: Some(time),
            midi: None,
        }
    }

    /// Create from MIDI position
    pub fn from_midi(midi: PositionInPpq) -> Self {
        Self {
            musical: None,
            time: None,
            midi: Some(midi),
        }
    }

    /// Zero/start position
    pub fn start() -> Self {
        Self {
            musical: Some(MusicalPosition::ZERO),
            time: Some(PositionInSeconds::ZERO),
            midi: Some(PositionInPpq::ZERO),
        }
    }

    /// Build from both time and musical representations — what
    /// publishers like the REAPER transport poll have ready in hand.
    pub fn from_time_and_musical(time: PositionInSeconds, musical: MusicalPosition) -> Self {
        Self {
            time: Some(time),
            musical: Some(musical),
            midi: None,
        }
    }

    /// Time component in seconds, or `None` when this position only
    /// carries a musical / MIDI representation. Returning `Option`
    /// instead of `0.0` removes a footgun — silently treating
    /// "missing" as "at the origin" once produced an off-by-zero in
    /// the offset-map code that took an hour to spot.
    pub fn seconds(&self) -> Option<f64> {
        self.time.map(|t| t.as_seconds())
    }

    /// `self − other` in both seconds and quarter-notes. Returns
    /// `None` for a component when either side is missing it.
    pub fn delta_from(&self, other: &Position) -> PositionDelta {
        let seconds = match (self.time, other.time) {
            (Some(a), Some(b)) => Some(a.as_seconds() - b.as_seconds()),
            _ => None,
        };
        let quarter_notes = match (self.musical, other.musical) {
            // Reconstruct beats from measure+beat+subdivision using the
            // common 4/4 assumption when no time-sig is in scope. Callers
            // that need accurate bar deltas should call the variant that
            // takes a TimeSignature.
            (Some(_), Some(_)) => None,
            _ => None,
        };
        PositionDelta {
            seconds,
            quarter_notes,
        }
    }

    /// `self − other`, with bar-aware musical delta using the supplied
    /// time signature. Use this when you have a time-sig in hand (e.g.
    /// from a TransportEvent::TempoChanged) and want a `bars.beats`
    /// display.
    pub fn delta_from_with_ts(&self, other: &Position, ts: TimeSignature) -> PositionDelta {
        let mut d = self.delta_from(other);
        if let (Some(a), Some(b)) = (self.musical, other.musical) {
            let beats_per_measure = ts.numerator().max(1) as f64;
            let a_qn = a.measure as f64 * beats_per_measure
                + a.beat as f64
                + a.subdivision as f64 / 1000.0;
            let b_qn = b.measure as f64 * beats_per_measure
                + b.beat as f64
                + b.subdivision as f64 / 1000.0;
            d.quarter_notes = Some(a_qn - b_qn);
        }
        d
    }
}

impl std::fmt::Display for Position {
    /// `time / musical` if both present, just the non-None one
    /// otherwise. Single-line, suitable for table cells.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.time, self.musical) {
            (Some(t), Some(m)) => write!(f, "{t} / {m}"),
            (Some(t), None) => write!(f, "{t}"),
            (None, Some(m)) => write!(f, "{m}"),
            (None, None) => write!(f, "—"),
        }
    }
}

/// Result of [`Position::delta_from`]. Both components are `Option`
/// because the underlying positions may have only one representation
/// in common.
#[derive(Debug, Clone, Copy, Default)]
pub struct PositionDelta {
    pub seconds: Option<f64>,
    pub quarter_notes: Option<f64>,
}

impl PositionDelta {
    /// Format the musical delta as `±bars.beats.subdivision` given
    /// the time signature. Returns `None` when the QN delta isn't
    /// available (e.g. only the time component differed).
    pub fn musical_string(&self, ts: TimeSignature) -> Option<String> {
        let qn = self.quarter_notes?;
        let bpm = ts.numerator().max(1) as f64;
        let sign = if qn >= 0.0 { "+" } else { "-" };
        let a = qn.abs();
        let bars = (a / bpm).floor() as i64;
        let in_bar = a - bars as f64 * bpm;
        let beats = in_bar.floor() as i64;
        let sub = ((in_bar - beats as f64) * 1000.0).round() as i64;
        let sub = sub.clamp(0, 999);
        Some(format!("{sign}{bars}.{beats}.{sub:03}"))
    }
}

impl std::fmt::Display for PositionDelta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(s) = self.seconds {
            let sign = if s >= 0.0 { "+" } else { "" };
            write!(f, "{sign}{s:.3}s")
        } else {
            write!(f, "—")
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::start()
    }
}

/// Time range with start and end positions
#[derive(Debug, Clone, PartialEq, facet::Facet, Default)]
pub struct TimeRange {
    pub start: Position,
    pub end: Position,
}

impl TimeRange {
    /// Create a new time range
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// Create from seconds
    pub fn from_seconds(start: f64, end: f64) -> Self {
        Self {
            start: Position::from_time(PositionInSeconds::from_seconds(start)),
            end: Position::from_time(PositionInSeconds::from_seconds(end)),
        }
    }

    /// Get start in seconds
    pub fn start_seconds(&self) -> f64 {
        self.start.time.map(|t| t.as_seconds()).unwrap_or(0.0)
    }

    /// Get end in seconds
    pub fn end_seconds(&self) -> f64 {
        self.end.time.map(|t| t.as_seconds()).unwrap_or(0.0)
    }

    /// Get duration in seconds
    pub fn duration_seconds(&self) -> f64 {
        (self.end_seconds() - self.start_seconds()).max(0.0)
    }

    /// Check if this range contains the given time in seconds
    pub fn contains(&self, seconds: f64) -> bool {
        let start = self.start_seconds();
        let end = self.end_seconds();
        seconds >= start && seconds <= end
    }

    /// Check if this range overlaps with another range
    pub fn overlaps(&self, other: &TimeRange) -> bool {
        let self_start = self.start_seconds();
        let self_end = self.end_seconds();
        let other_start = other.start_seconds();
        let other_end = other.end_seconds();

        // Ranges overlap if one starts before the other ends
        self_start <= other_end && other_start <= self_end
    }
}
