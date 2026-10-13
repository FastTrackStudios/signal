//! Envelope types for automation

use crate::primitives::{AutomationMode, Duration, PositionInSeconds};
use crate::track::TrackRef;
use facet::Facet;

/// Type of envelope (which parameter it controls)
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Facet)]
pub enum EnvelopeType {
    /// Track volume
    #[default]
    Volume = 0,
    /// Track volume (pre-FX)
    VolumePrefx = 1,
    /// Track pan
    Pan = 2,
    /// Track pan (pre-FX)
    PanPrefx = 3,
    /// Track width
    Width = 4,
    /// Track width (pre-FX)
    WidthPrefx = 5,
    /// Track mute
    Mute = 6,
    /// FX parameter (uses fx_guid + param_index)
    FxParam = 7,
    /// Take playback rate (REAPER's `SPEEDENV`).
    PlayRate = 8,
    /// Take pitch in semitones (REAPER's `PITCHENV`).
    Pitch = 9,
    /// An envelope kind this enum does not name yet.
    ///
    /// The `.rpp` corpus carries envelope chunks the facade has no variant
    /// for, and #156 forbids silently dropping what a real project holds.
    /// The originating chunk name is kept in
    /// [`Envelope::name`], so a `Custom` envelope still round-trips exactly
    /// and promoting one to its own variant later is a pure addition.
    Custom = 255,
}

/// Which property of a send the envelope automates.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Facet)]
pub enum SendEnvelopeKind {
    /// Send volume (linear multiplier).
    #[default]
    Volume = 0,
    /// Send pan (0..=1 with 0.5 = center).
    Pan = 1,
    /// Send mute (>0.5 = muted).
    Mute = 2,
}

/// Which property of a *take* the envelope automates. REAPER models
/// these as per-take envelopes that operate in item-relative time
/// (0 at item start, item.length at item end).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Facet)]
pub enum TakeEnvelopeKind {
    /// Take volume (linear multiplier on the item's audio output).
    #[default]
    Volume = 0,
    /// Take pan (0..=1 with 0.5 = center).
    Pan = 1,
    /// Take mute (>0.5 = silence the item while in range).
    Mute = 2,
    /// Take pitch (semitones, additive on top of `Take.pitch`).
    /// Converted to a rate multiplier inside the renderer.
    Pitch = 3,
}

/// Reference to an envelope
#[repr(C)]
#[derive(Clone, Debug, PartialEq, Facet)]
pub enum EnvelopeRef {
    /// Reference by envelope type (for track envelopes)
    Type(EnvelopeType),
    /// Reference by FX parameter
    FxParam { fx_guid: String, param_index: u32 },
    /// Reference by send + automated property. `send_index` is the
    /// index into the source track's send list.
    Send {
        send_index: u32,
        kind: SendEnvelopeKind,
    },
    /// Reference a per-take envelope. `EnvelopeLocation.track` is
    /// ignored for take envelopes — the take's identity carries
    /// both the project + track context.
    Take {
        item_guid: String,
        take_guid: String,
        kind: TakeEnvelopeKind,
    },
    /// Reference by display name
    ByName(String),
}

/// Complete envelope state
#[derive(Clone, Debug, Facet)]
pub struct Envelope {
    /// Track this envelope belongs to
    pub track_guid: String,
    /// Type of envelope
    pub envelope_type: EnvelopeType,
    /// Display name
    pub name: String,

    // FX param envelope specific
    /// FX GUID (if FxParam envelope)
    pub fx_guid: Option<String>,
    /// Parameter index (if FxParam envelope)
    pub param_index: Option<u32>,

    // State
    /// Whether envelope is visible in arrange view
    pub visible: bool,
    /// Whether envelope is armed for recording
    pub armed: bool,
    /// Automation playback/recording mode
    pub automation_mode: AutomationMode,

    // Lane
    /// Whether the envelope has a **lane of its own** under the track,
    /// rather than being drawn over the track's own lane.
    ///
    /// REAPER's per-envelope choice, and the one that matters for FX
    /// parameter automation: a parameter envelope is unreadable overlaid
    /// on a waveform, so it gets a lane. In REAPER's envelope chunk this
    /// is the second field of `VIS`.
    pub in_own_lane: bool,
    /// The lane's height in pixels when it has one, `0` when the host
    /// has not laid it out yet.
    ///
    /// REAPER's `I_TCPH` via `GetEnvelopeInfo_Value`. The floor is the
    /// theme's `envcp_min_height` (27 in the FTS theme) — a UI drawing
    /// this lane must clamp, because REAPER reports the *used* height
    /// and a collapsed lane reports below its own minimum.
    pub lane_height: u32,
    /// Number of automation items on this envelope. Fetch them with
    /// [`Automation::automation_items`][crate::automation::Automation].
    pub automation_item_count: u32,

    // Points
    /// Number of points in the envelope
    pub point_count: u32,
}

/// A windowed piece of automation on an envelope — REAPER's automation
/// item, which behaves like a media item: it moves, loops, stretches,
/// and can be **pooled** so every instance edits one source.
///
/// Fields map onto `GetSetAutomationItemInfo`'s descriptors, named for
/// what they mean rather than for the string.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct AutomationItem {
    /// Index on the envelope — the handle every setter takes.
    pub index: u32,
    /// `P_POOL_ID`. Instances sharing a pool id share their source, so
    /// editing one edits all — the fact a UI must surface before a user
    /// edits the wrong copy.
    pub pool_id: i32,
    /// `P_POOL_NAME`, the pooled source's display name.
    pub name: String,
    /// `D_POS` — position on the timeline.
    pub position: PositionInSeconds,
    /// `D_LENGTH`.
    pub length: Duration,
    /// `D_STARTOFFS` — offset into the pooled source.
    pub start_offset: Duration,
    /// `D_PLAYRATE`.
    pub play_rate: f64,
    /// `D_BASELINE` — moves the item's curve: `0.5` leaves it where it
    /// was drawn (REAPER's 0%), `0` and `1` are −100% and +100%.
    pub baseline: f64,
    /// `D_AMPLITUDE` — the curve's scale, and it may be negative
    /// (inverted), which is why this is not a `0..1`.
    pub amplitude: f64,
    /// `D_LOOPSRC` — whether the source repeats to fill the length.
    pub loop_source: bool,
    /// `D_UISEL` — selected in the arrange.
    pub selected: bool,
}

impl Default for AutomationItem {
    fn default() -> Self {
        Self {
            index: 0,
            pool_id: -1,
            name: String::new(),
            position: PositionInSeconds::ZERO,
            length: Duration::ZERO,
            start_offset: Duration::ZERO,
            play_rate: 1.0,
            baseline: 0.5,
            amplitude: 1.0,
            loop_source: false,
            selected: false,
        }
    }
}

/// Shape of automation curve between points
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Facet)]
pub enum EnvelopeShape {
    /// Linear interpolation
    #[default]
    Linear = 0,
    /// Square (instant jump)
    Square = 1,
    /// Slow start and end (S-curve)
    SlowStartEnd = 2,
    /// Fast start (logarithmic)
    FastStart = 3,
    /// Fast end (exponential)
    FastEnd = 4,
    /// Bezier curve (uses tension)
    Bezier = 5,
}

/// A point on an automation envelope
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct EnvelopePoint {
    /// Index of this point in the envelope
    pub index: u32,
    /// Time position
    pub time: PositionInSeconds,
    /// Value (0.0-1.0 normalized)
    pub value: f64,
    /// Curve shape to next point
    pub shape: EnvelopeShape,
    /// Bezier tension (-1.0 to 1.0)
    pub tension: f64,
    /// Whether this point is selected
    pub selected: bool,
}

/// Location of an envelope (track + envelope reference)
#[derive(Clone, Debug, Facet)]
pub struct EnvelopeLocation {
    /// The track containing the envelope
    pub track: TrackRef,
    /// Reference to the envelope
    pub envelope: EnvelopeRef,
}

impl EnvelopeLocation {
    /// Create a new envelope location
    pub fn new(track: TrackRef, envelope: EnvelopeRef) -> Self {
        Self { track, envelope }
    }

    /// Create a volume envelope location
    pub fn volume(track: TrackRef) -> Self {
        Self::new(track, EnvelopeRef::Type(EnvelopeType::Volume))
    }

    /// Create a pan envelope location
    pub fn pan(track: TrackRef) -> Self {
        Self::new(track, EnvelopeRef::Type(EnvelopeType::Pan))
    }

    /// Create an FX parameter envelope location
    pub fn fx_param(track: TrackRef, fx_guid: String, param_index: u32) -> Self {
        Self::new(
            track,
            EnvelopeRef::FxParam {
                fx_guid,
                param_index,
            },
        )
    }
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            track_guid: String::new(),
            envelope_type: EnvelopeType::Volume,
            name: String::new(),
            fx_guid: None,
            param_index: None,
            visible: false,
            armed: false,
            automation_mode: AutomationMode::TrimRead,
            in_own_lane: false,
            lane_height: 0,
            automation_item_count: 0,
            point_count: 0,
        }
    }
}

impl Default for EnvelopePoint {
    fn default() -> Self {
        Self {
            index: 0,
            time: PositionInSeconds::ZERO,
            value: 0.0,
            shape: EnvelopeShape::Linear,
            tension: 0.0,
            selected: false,
        }
    }
}
