//! Marker data type
//!
//! A marker represents a named point in time within a project.

use crate::Position;
use facet::Facet;

/// A marker at a specific position in the project timeline
///
/// Markers are named reference points that can be used for navigation,
/// synchronization, or structural organization of a project.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Marker {
    /// Unique identifier for the marker (assigned by the DAW)
    pub id: Option<u32>,
    /// Position of the marker in the timeline
    pub position: Position,
    /// Display name of the marker
    pub name: String,
    /// Color in native format (0xRRGGBB, or None for default)
    pub color: Option<u32>,
    /// GUID for stable identification across sessions
    pub guid: Option<String>,
    /// Ruler lane index (v7.62+). None = default lane.
    pub lane: Option<u32>,
}

impl Marker {
    /// Create a marker at the given position with a name. All other
    /// fields take their `Default` values; override individual ones
    /// with the `..Marker::new(pos, name)` struct-update pattern:
    ///
    /// ```ignore
    /// Marker {
    ///     id: Some(42),
    ///     color: Some(0xFF0000),
    ///     ..Marker::new(pos, "loud".into())
    /// }
    /// ```
    pub fn new(position: Position, name: String) -> Self {
        Self {
            position,
            name,
            ..Self::default()
        }
    }

    /// Position in seconds, or `0.0` if no time component is set.
    pub fn position_seconds(&self) -> f64 {
        self.position
            .time
            .as_ref()
            .map(|t| t.as_seconds())
            .unwrap_or(0.0)
    }

    /// Whether the marker's position falls within `[start, end]`.
    pub fn is_in_range(&self, start: f64, end: f64) -> bool {
        let pos = self.position_seconds();
        pos >= start && pos <= end
    }

    /// Whether the marker is within `tolerance` seconds of `seconds`.
    pub fn is_at_position(&self, seconds: f64, tolerance: f64) -> bool {
        (self.position_seconds() - seconds).abs() <= tolerance
    }
}

// Manual impl rather than derived so the position uses the explicit
// `Position::start()` constructor; semantically equivalent to
// `Position::default()` today but makes the "marker starts at the
// session origin" intent obvious.
#[allow(clippy::derivable_impls)]
impl Default for Marker {
    fn default() -> Self {
        Self {
            id: None,
            position: Position::start(),
            name: String::new(),
            color: None,
            guid: None,
            lane: None,
        }
    }
}
