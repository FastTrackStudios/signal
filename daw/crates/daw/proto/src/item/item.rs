//! Item types
//!
//! Items are media containers on tracks that hold one or more takes.

use crate::primitives::{BeatAttachMode, Duration, PositionInSeconds};
use facet::Facet;

/// Reference to an item
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum ItemRef {
    /// Reference by GUID
    Guid(String),
    /// Reference by index within track
    Index(u32),
    /// Reference by global index in project
    ProjectIndex(u32),
}

/// Complete item state
// PartialEq so a Vec<Item> can be a Dioxus prop (memoisation compares
// props), the same reason Track carries it.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct Item {
    /// Unique identifier
    pub guid: String,
    /// Track this item belongs to
    pub track_guid: String,
    /// Index within track
    pub index: u32,

    // Position & length
    /// Position on timeline
    pub position: PositionInSeconds,
    /// Duration of the item
    pub length: Duration,
    /// Snap offset for alignment
    pub snap_offset: Duration,

    // State
    /// Whether the item is muted
    pub muted: bool,
    /// Whether the item is selected
    pub selected: bool,
    /// Whether the item is locked (can't be moved/edited)
    pub locked: bool,

    // Audio
    /// Item volume (1.0 = 0dB)
    pub volume: f64,
    /// Fade in duration
    pub fade_in_length: Duration,
    /// Fade out duration
    pub fade_out_length: Duration,
    /// Fade in curve shape
    pub fade_in_shape: FadeShape,
    /// Fade out curve shape
    pub fade_out_shape: FadeShape,

    // Timing
    /// How the item attaches to the timeline
    pub beat_attach_mode: BeatAttachMode,
    /// Whether to loop the source
    pub loop_source: bool,
    /// Auto-stretch at tempo changes
    pub auto_stretch: bool,

    // Visual
    /// Custom color (0xRRGGBB format)
    pub color: Option<u32>,
    /// Group ID for linked items
    pub group_id: Option<u32>,
    /// The item's on-screen label (REAPER's `P_NOTES`) — where an item
    /// says what it *means* (a chord symbol, a key change) in a form that
    /// is visible, hand-editable and moves with the item.
    ///
    /// Not MIDI notes. See [`Items::label`](crate::item::Items::label).
    pub label: Option<String>,
    /// Fixed item lane this item sits on (REAPER 7 comping lanes).
    /// `None` when the track has no fixed lanes.
    pub fixed_lane: Option<u32>,

    // Takes
    /// How far into the source the ACTIVE take starts, in seconds.
    ///
    /// The take's own `start_offset`, lifted onto the item because a
    /// window that draws the waveform needs it for every item it draws
    /// and cannot afford a take fetch each — the same reason
    /// `send_count` and `receive_count` sit on `Track`. Zero when there
    /// is no take.
    ///
    /// Read-only here: `TakeService::set_start_offset` is what changes
    /// it, and this follows on the next read.
    pub start_offset: Duration,
    /// Number of takes in this item
    pub take_count: u32,
    /// Index of the currently active take
    pub active_take_index: u32,
}

/// Fade shape for item fade in/out
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Facet)]
pub enum FadeShape {
    /// Linear fade
    #[default]
    Linear = 0,
    /// Fast start (logarithmic)
    FastStart = 1,
    /// Fast end (exponential)
    FastEnd = 2,
    /// Fast start, steep curve
    FastStartSteep = 3,
    /// Fast end, steep curve
    FastEndSteep = 4,
    /// Slow start and end (S-curve)
    SlowStartEnd = 5,
    /// Slow start and end, steep curve
    SlowStartEndSteep = 6,
}

impl Item {
    /// Get the end position of the item
    pub fn end_position(&self) -> PositionInSeconds {
        PositionInSeconds::from_seconds(self.position.as_seconds() + self.length.as_seconds())
    }

    /// Check if a position is within this item
    pub fn contains_position(&self, pos: PositionInSeconds) -> bool {
        let start = self.position.as_seconds();
        let end = start + self.length.as_seconds();
        let p = pos.as_seconds();
        p >= start && p < end
    }

    /// Check if this item overlaps with a time range
    pub fn overlaps(&self, start: PositionInSeconds, end: PositionInSeconds) -> bool {
        let item_start = self.position.as_seconds();
        let item_end = item_start + self.length.as_seconds();
        item_start < end.as_seconds() && start.as_seconds() < item_end
    }
}

impl Default for Item {
    fn default() -> Self {
        Self {
            guid: String::new(),
            track_guid: String::new(),
            index: 0,
            position: PositionInSeconds::ZERO,
            length: Duration::ZERO,
            snap_offset: Duration::ZERO,
            muted: false,
            selected: false,
            locked: false,
            volume: 1.0,
            fade_in_length: Duration::ZERO,
            fade_out_length: Duration::ZERO,
            fade_in_shape: FadeShape::Linear,
            fade_out_shape: FadeShape::Linear,
            beat_attach_mode: BeatAttachMode::Time,
            loop_source: false,
            auto_stretch: false,
            color: None,
            group_id: None,
            label: None,
            fixed_lane: None,
            start_offset: Duration::ZERO,
            take_count: 0,
            active_take_index: 0,
        }
    }
}
