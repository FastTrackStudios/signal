//! Take types
//!
//! Takes are alternative recordings or sources within an item.
//! An item can have multiple takes, but only one active take at a time.

use crate::primitives::Duration;
use facet::Facet;

/// Reference to a take within an item
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum TakeRef {
    /// Reference by GUID
    Guid(String),
    /// Reference by index within item
    Index(u32),
    /// Reference the currently active take
    Active,
}

/// Complete take state
#[derive(Clone, Debug, Facet)]
pub struct Take {
    /// Unique identifier
    pub guid: String,
    /// Item this take belongs to
    pub item_guid: String,
    /// Index within the item
    pub index: u32,
    /// Whether this is the active take
    pub is_active: bool,

    // Metadata
    /// Display name
    pub name: String,
    /// Custom color (0xRRGGBB format)
    pub color: Option<u32>,

    // Playback
    /// Take volume (1.0 = 0dB)
    pub volume: f64,
    /// Playback speed (1.0 = normal, 0.5 = half speed, 2.0 = double)
    pub play_rate: f64,
    /// Pitch adjustment in semitones
    pub pitch: f64,
    /// Whether to preserve pitch when changing playback rate
    pub preserve_pitch: bool,
    /// REAPER channel mode (`CHANMODE`): 0 normal, 1 reverse stereo,
    /// 2 mono downmix, 3 mono-left, 4 mono-right, 5–66 mono of
    /// source channel N−2 (1-based), 67+ stereo pair at N−66.
    pub channel_mode: u32,
    /// Offset into the source (start position)
    pub start_offset: Duration,

    // Source info
    /// Type of source (audio, MIDI, etc.)
    pub source_type: SourceType,
    /// Absolute path to the source file on disk (None for MIDI or empty takes)
    pub source_file_path: Option<String>,
    /// Length of the source media
    pub source_length: Option<Duration>,
    /// Sample rate of the source
    pub source_sample_rate: Option<u32>,
    /// Number of channels in the source
    pub source_channels: Option<u32>,

    // MIDI-specific
    /// Whether this is a MIDI take
    pub is_midi: bool,
    /// Number of MIDI notes (if MIDI take)
    pub midi_note_count: Option<u32>,
}

/// Type of media source
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Facet)]
pub enum SourceType {
    /// Audio file (WAV, MP3, FLAC, etc.)
    #[default]
    Audio = 0,
    /// MIDI data
    Midi = 1,
    /// Empty/no source
    Empty = 2,
    /// Video file
    Video = 3,
    /// Unknown source type
    Unknown = 255,
}

impl Take {
    /// Check if this take has a valid source
    pub fn has_source(&self) -> bool {
        !matches!(self.source_type, SourceType::Empty | SourceType::Unknown)
    }

    /// Get the effective playback duration considering play rate
    pub fn effective_duration(&self) -> Option<Duration> {
        self.source_length.map(|len| {
            let effective_seconds = len.as_seconds() / self.play_rate;
            Duration::from_seconds(effective_seconds)
        })
    }
}

impl Default for Take {
    fn default() -> Self {
        Self {
            guid: String::new(),
            item_guid: String::new(),
            index: 0,
            is_active: false,
            name: String::new(),
            color: None,
            volume: 1.0,
            play_rate: 1.0,
            pitch: 0.0,
            preserve_pitch: true,
            channel_mode: 0,
            start_offset: Duration::ZERO,
            source_type: SourceType::Empty,
            source_file_path: None,
            source_length: None,
            source_sample_rate: None,
            source_channels: None,
            is_midi: false,
            midi_note_count: None,
        }
    }
}

// ─── Take markers ───────────────────────────────────────────────────────────
//
// REAPER take markers (`GetTakeMarker`, `SetTakeMarker`, etc.) — markers
// attached to a take's source media at a specific PPQ position. Used for
// review/comp workflows; REAPER 7.17+ also exposes built-in actions that
// stamp star-rating markers onto the active take.

/// One marker on a take's source.
///
/// A marker belongs to the **source** — the recording — at a position in
/// it, so every item cut from that recording shows it, wherever the
/// cuts and slips put them. (The standalone engine keys them so; REAPER
/// keeps them per take.) A marker can be a drum hit ([`HitMark`]): the
/// detector writes one per stroke, and an edit reads them instead of
/// detecting again.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct TakeMarker {
    /// Stable enumeration index. REAPER renumbers when markers are inserted
    /// or deleted, so the index is only meaningful within a single read.
    pub index: u32,
    /// What the marker is, whatever its index: kept through inserts,
    /// deletes and moves. Empty where a backend keeps none (REAPER).
    pub id: String,
    /// Display name. Convention for REAPER's built-in rating markers is
    /// e.g. `"5"` / `"4"` / etc. — keyflow / fts treat any leading digit
    /// 1..=5 as a star rating.
    pub name: String,
    /// Position in source PPQ (NOT take-time, NOT absolute project time).
    pub source_position_seconds: f64,
    /// Custom color (0xRRGGBB). `None` falls back to REAPER's default.
    pub color: Option<u32>,
    /// A drum hit's properties, when the marker is one.
    pub hit: Option<HitMark>,
}

/// Who put a hit marker where it is.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Facet)]
pub enum MarkOrigin {
    /// A detector run placed it, and nobody has checked it.
    #[default]
    Auto = 0,
    /// A person or an agent placed it (or moved it).
    Manual = 1,
    /// A detector placed it and someone checked it.
    Approved = 2,
}

/// A drum hit, as a take marker carries it.
///
/// The marker's position is the hit's attack start; this is what is
/// known about it. A detector run replaces only the `Auto` markers in its
/// range — `Manual` and `Approved` ones are never moved or removed by a
/// run.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct HitMark {
    /// The piece it was played on (`Kick`, `Snare`, `Tom 1`…).
    pub piece: String,
    pub origin: MarkOrigin,
    /// The detector run that placed it (empty when placed by hand).
    pub run: String,
    /// The attack's peak on the close mic, dBFS.
    pub peak_db: f64,
    /// How sure the detector was, 0..=1.
    pub confidence: f64,
    /// The piece's trigger reported it.
    pub from_trigger: bool,
    /// The piece's close mic reported it.
    pub from_mic: bool,
    /// Much quieter than the piece usually is.
    pub ghost: bool,
    /// Possibly another drum's bleed rather than a stroke of this one.
    pub bleed: bool,
    /// The other stroke of a flam: that marker's id.
    pub flam_with: Option<String>,
    /// Anything whoever looked at it wants the next to know.
    pub note: String,
}

/// Create-only payload for adding a new take marker.
#[derive(Clone, Debug, Default, Facet)]
pub struct TakeMarkerCreate {
    /// Its id; one is made when none is given.
    pub id: Option<String>,
    pub name: String,
    pub source_position_seconds: f64,
    pub color: Option<u32>,
    pub hit: Option<HitMark>,
}

/// Update payload for modifying an existing take marker. `index` selects
/// the marker; the rest are optional partial updates.
#[derive(Clone, Debug, Default, Facet)]
pub struct TakeMarkerUpdate {
    pub index: u32,
    pub name: Option<String>,
    pub source_position_seconds: Option<f64>,
    /// `Some(Some(c))` sets a color, `Some(None)` clears it, `None` leaves
    /// the existing color untouched.
    pub color: Option<Option<u32>>,
    /// `Some(Some(h))` makes it a hit with these properties, `Some(None)`
    /// a plain marker again, `None` leaves it as it is.
    pub hit: Option<Option<HitMark>>,
}

/// Place a take marker at a project-timeline position. The service
/// converts the project position to source-time (accounting for the
/// item's start position, the take's start offset and play rate). If
/// the resulting source position is out of bounds — the project position
/// falls before/after the item, or the source has a known length and the
/// computed source-time exceeds it — the call is a no-op (`Ok(None)`).
#[derive(Clone, Debug, Facet)]
pub struct AddTakeMarkerAtPositionRequest {
    pub name: String,
    /// Project-timeline position. May carry musical / time / MIDI forms;
    /// the implementation prefers `time` if present, otherwise resolves
    /// `musical` via the project's tempo map.
    pub position: crate::primitives::Position,
    pub color: Option<u32>,
}

// ─── Take ratings (REAPER 7.17+) ───────────────────────────────────────────
//
// REAPER 7.17 added "take ranking" — a comp/review system built on top of
// take markers. Ratings are encoded as specially-named take markers; the
// daw service surfaces the *actions* that REAPER provides for manipulating
// them, plus a detector that reads them back from `TakeMarker::name`.
//
// REAPER's encoding (verified empirically by firing action 42680 and
// reading the resulting take marker):
//   up-rank   level N  →  marker name `":" + ")" * N`   (`:)`, `:))`, `:)))`)
//   down-rank          →  marker name `":("`
// REAPER's prefs configure the maximum up-rank level (1–5, default 3) and
// whether down-ranking is enabled (0 or 1 max, default 1). We clamp
// `UpRank` to 1..=5 and treat down-rank as a flag (just `DownRank` —
// REAPER never assigns higher levels). Neutral / unranked takes have no
// rank marker at all.

/// A take's ranking, derived from its take markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum TakeRating {
    /// Up-ranked — level 1..=5. Encoded by REAPER as a `:` followed by
    /// N `)` characters (`":)"` = 1, `":))"` = 2, `":)))"` = 3, …).
    /// REAPER's default max is 3 (configurable up to 5).
    UpRank(u8),
    /// Down-ranked. REAPER only supports a single down-rank level
    /// (`":("`); the variant is a flag, not a count.
    DownRank,
}

impl TakeRating {
    /// Parse a take marker name as a rating, if it matches REAPER's
    /// smiley/frowny format. Returns `None` for ordinary markers.
    pub fn from_marker_name(name: &str) -> Option<Self> {
        let rest = name.trim().strip_prefix(':')?;
        if rest.is_empty() {
            return None;
        }
        let bytes = rest.as_bytes();
        if bytes.iter().all(|b| *b == b')') {
            Some(Self::UpRank(bytes.len().clamp(1, 5) as u8))
        } else if bytes.iter().all(|b| *b == b'(') {
            // REAPER only emits a single down-rank level; treat any run
            // of frownies as a generic DownRank.
            Some(Self::DownRank)
        } else {
            None
        }
    }

    /// Serialize into REAPER's smiley/frowny marker-name format.
    pub fn to_marker_name(self) -> String {
        match self {
            Self::UpRank(level) => {
                let n = level.clamp(1, 5) as usize;
                let mut s = String::with_capacity(1 + n);
                s.push(':');
                for _ in 0..n {
                    s.push(')');
                }
                s
            }
            Self::DownRank => ":(".to_string(),
        }
    }
}

/// REAPER built-in action command IDs for the take-ranking system added in
/// 7.17. We surface them as constants so service code can call
/// `Main_OnCommand` with a meaningful name. Selection state must be set up
/// by the caller — most of these operate on the current item selection or
/// the take/item under mouse.
pub mod take_rating_actions {
    /// `Item: Up-rank active take or last recording pass` — bumps the
    /// active take up one level (cycles back to neutral after the max).
    pub const UP_RANK_ACTIVE_TAKE: u32 = 42680;

    /// `Item: Cycle through up-rank/down-rank levels for active take or
    /// last recording pass` — cycles up→max→down→neutral→up→...
    pub const CYCLE_RANK_ACTIVE_TAKE: u32 = 43197;

    /// `Item: Clear up-rank/down-rank markers` — wipes ranking on
    /// selected items' active takes.
    pub const CLEAR_RANK_SELECTED_ITEMS: u32 = 43161;

    /// `Item: Clear up-rank/down-rank markers for take under mouse`.
    pub const CLEAR_RANK_TAKE_UNDER_MOUSE: u32 = 43162;

    /// `Item: Up-rank take or comp area under mouse`.
    pub const UP_RANK_TAKE_UNDER_MOUSE: u32 = 43155;

    /// `Item: Cycle through up-rank/down-rank levels for take or comp area
    /// under mouse`.
    pub const CYCLE_RANK_TAKE_UNDER_MOUSE: u32 = 43198;

    /// `Item: Up-rank take marker at play position or edit cursor`.
    pub const UP_RANK_AT_PLAY_POSITION: u32 = 43157;

    /// `Item: Up-rank take marker at mouse position`.
    pub const UP_RANK_AT_MOUSE_POSITION: u32 = 43159;

    /// `Item: Delete takes that are not up-ranked (no confirm)`.
    pub const DELETE_NON_UPRANKED_TAKES: u32 = 40229;

    /// `Track: Clear up-rank/down-rank markers for all items on track`.
    pub const CLEAR_RANK_ALL_ITEMS_ON_TRACK: u32 = 43165;

    /// `Track: Delete takes for all items on track that are not up-ranked
    /// (no confirm)`.
    pub const DELETE_NON_UPRANKED_TAKES_ON_TRACK: u32 = 43166;
}
