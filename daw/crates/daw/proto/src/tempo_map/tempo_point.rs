//! Tempo and time signature change point
//!
//! Represents a point in the timeline where tempo and/or time signature changes.

use crate::{Position, TimePosition, TimeSignature};
use facet::Facet;

/// A tempo/time signature change point in the timeline
///
/// These markers define where tempo and/or time signature changes occur.
/// The tempo map is constructed from a series of these points.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct TempoPoint {
    /// Position in the timeline
    pub position: Position,
    /// Tempo in BPM at this point
    pub bpm: f64,
    /// Time signature at this point (if changed)
    pub time_signature: Option<TimeSignature>,
    /// Envelope shape for tempo transition (0=linear, 1=square)
    pub shape: Option<i32>,
    /// Bezier curve tension for smooth transitions
    pub bezier_tension: Option<f64>,
    /// Whether this point is selected in the UI
    pub selected: Option<bool>,
    /// Linear tempo (true) vs. envelope/ramping (false)
    pub linear: Option<bool>,
}

impl TempoPoint {
    /// Create a new tempo point with just position and tempo
    pub fn new(position: Position, bpm: f64) -> Self {
        Self {
            position,
            bpm,
            time_signature: None,
            shape: None,
            bezier_tension: None,
            selected: None,
            linear: None,
        }
    }

    /// Create a tempo point from seconds and BPM
    pub fn from_seconds(seconds: f64, bpm: f64) -> Self {
        Self::new(
            Position::from_time(TimePosition::from_seconds(seconds)),
            bpm,
        )
    }

    /// Create a tempo point with a time signature change
    pub fn with_time_signature(
        position: Position,
        bpm: f64,
        time_signature: TimeSignature,
    ) -> Self {
        Self {
            position,
            bpm,
            time_signature: Some(time_signature),
            shape: None,
            bezier_tension: None,
            selected: None,
            linear: None,
        }
    }

    /// Get the position in seconds
    pub fn position_seconds(&self) -> f64 {
        self.position
            .time
            .as_ref()
            .map(|t| t.as_seconds())
            .unwrap_or(0.0)
    }

    /// Check if this point includes a time signature change
    pub fn has_time_signature_change(&self) -> bool {
        self.time_signature.is_some()
    }
}

impl Default for TempoPoint {
    fn default() -> Self {
        Self::new(Position::start(), 120.0)
    }
}
