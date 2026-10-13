//! Region data type
//!
//! A region represents a named time range within a project.

use crate::TimeRange;
use facet::Facet;

/// A region spanning a time range in the project timeline
///
/// Regions are named spans of time that can be used for organizing sections,
/// defining loop areas, or marking sections for export.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Region {
    /// Unique identifier for the region (assigned by the DAW)
    pub id: Option<u32>,
    /// Time range of the region (start to end)
    pub time_range: TimeRange,
    /// Display name of the region
    pub name: String,
    /// Color in native format (0xRRGGBB, or None for default)
    pub color: Option<u32>,
    /// GUID for stable identification across sessions
    pub guid: Option<String>,
    /// Ruler lane index (v7.62+). None = default lane.
    pub lane: Option<u32>,
}

impl Region {
    /// Create a new region with the given time range and name
    pub fn new(time_range: TimeRange, name: String) -> Self {
        Self {
            id: None,
            time_range,
            name,
            color: None,
            guid: None,
            lane: None,
        }
    }

    /// Create a region from start and end positions in seconds
    pub fn from_seconds(start: f64, end: f64, name: String) -> Self {
        Self::new(TimeRange::from_seconds(start, end), name)
    }

    /// Create a region with all metadata
    pub fn new_full(
        id: Option<u32>,
        time_range: TimeRange,
        name: String,
        color: Option<u32>,
        guid: Option<String>,
    ) -> Self {
        Self {
            id,
            time_range,
            name,
            color,
            guid,
            lane: None,
        }
    }

    /// Get the start position in seconds
    pub fn start_seconds(&self) -> f64 {
        self.time_range.start_seconds()
    }

    /// Get the end position in seconds
    pub fn end_seconds(&self) -> f64 {
        self.time_range.end_seconds()
    }

    /// Get the duration in seconds
    pub fn duration_seconds(&self) -> f64 {
        self.time_range.duration_seconds()
    }

    /// Check if a position (in seconds) is within this region
    pub fn contains_position(&self, seconds: f64) -> bool {
        self.time_range.contains(seconds)
    }

    /// Check if this region intersects with a time range
    pub fn intersects_range(&self, start: f64, end: f64) -> bool {
        self.start_seconds() <= end && self.end_seconds() >= start
    }

    /// Check if this region overlaps with another region
    pub fn overlaps_with(&self, other: &Region) -> bool {
        self.time_range.overlaps(&other.time_range)
    }
}

impl Default for Region {
    fn default() -> Self {
        Self::new(TimeRange::default(), String::new())
    }
}
