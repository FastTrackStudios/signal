//! Track events for reactive subscriptions

use super::Track;
use facet::Facet;

/// Events emitted when track state changes
// Wire/domain type: `Added` carries a whole Track by design; boxing the
// event payload would ripple through every subscriber match arm.
#[allow(clippy::large_enum_variant)]
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum TrackEvent {
    /// A track was added
    Added(Track),
    /// A track was removed (GUID)
    Removed(String),
    /// A track was renamed
    Renamed { guid: String, name: String },
    /// Track mute state changed
    MuteChanged { guid: String, muted: bool },
    /// Track solo state changed
    SoloChanged { guid: String, soloed: bool },
    /// Track arm state changed
    ArmChanged { guid: String, armed: bool },
    /// Track selection changed
    SelectionChanged { guid: String, selected: bool },
    /// Track volume changed
    VolumeChanged { guid: String, volume: f64 },
    /// Track pan changed
    PanChanged { guid: String, pan: f64 },
    /// Track color changed
    ColorChanged { guid: String, color: Option<u32> },
    /// Track TCP visibility changed
    TcpVisibilityChanged { guid: String, visible: bool },
    /// Track mixer visibility changed
    MixerVisibilityChanged { guid: String, visible: bool },
    /// Polarity / phase invert toggled
    PhaseInvertedChanged { guid: String, inverted: bool },
    /// Track automation mode changed
    AutomationModeChanged {
        guid: String,
        mode: crate::primitives::AutomationMode,
    },
    /// Record-input monitoring changed
    InputMonitorChanged {
        guid: String,
        monitor: super::InputMonitoringMode,
    },
    /// The track's record input changed — which physical channel, or
    /// which MIDI device and channel, it records from.
    ///
    /// Added because something already writes it: applying a patch list
    /// sets every source track's input in one undo step, and without an
    /// event a second client keeps showing whatever the inputs were
    /// when it last read them. An engineer looking at two screens that
    /// disagree about where a take is coming from is the worst possible
    /// moment for a stale field.
    RecordInputChanged {
        guid: String,
        input: super::RecordInput,
    },
    /// The track's group membership changed.
    ///
    /// Published by the **writers** rather than diffed by the track
    /// poller, and that is a cost decision worth stating. Reading a
    /// track's grouping is tens of FFI calls (ten flag families across
    /// four slot windows); doing that for every track on a 30 Hz timer
    /// would be hundreds of thousands of calls a second on REAPER's
    /// main thread, which is the one thread that must never be busy.
    ///
    /// So every change made *through the facade* is reported the instant
    /// it is made — which is every change FTS makes, since the grouping
    /// watcher is what manages groups. A change made by hand in REAPER's
    /// own matrix dialog reaches no setter of ours, and is covered
    /// separately by a sweep that reads a fixed slice of the tracks each
    /// tick: flat cost whatever the session's size, a second or so of lag
    /// on an edit a human made in a dialog.
    GroupingChanged {
        guid: String,
        grouping: super::TrackGrouping,
    },
    /// The track's panel got taller or shorter.
    ///
    /// A view concern that lives in the PROJECT, not in the viewer: a
    /// height set here is a height REAPER opens with. So a second
    /// window showing the same session has to be told, or the two
    /// disagree about a value they both draw from the same file.
    HeightChanged { guid: String, height: Option<u32> },
    /// The track's place in the folder tree changed.
    ///
    /// Structural, and the most structural of all: depth is relative,
    /// so one track's change moves every track BELOW it between
    /// levels. A client cannot patch this into the tree it holds — the
    /// parent of a track it never heard about may have changed — which
    /// is why the event carries the new depth and not a new tree.
    FolderDepthChanged { guid: String, folder_depth: i32 },
    /// The track's fixed lanes changed: how many, which ones play,
    /// what they are called, how they are drawn.
    ///
    /// One event for the four together because they are read together
    /// and drawn together. A comp view that learned the lane count
    /// without the names would draw the right number of empty labels,
    /// which is worse than not redrawing at all.
    LanesChanged {
        guid: String,
        lane_count: u32,
        lane_play_mask: u64,
        lane_names: Vec<String>,
        lane_display: super::LaneDisplay,
    },
    /// Track was moved (index changed)
    Moved {
        guid: String,
        old_index: u32,
        new_index: u32,
    },
    /// The track stopped or started sending to its parent / master.
    ///
    /// The IO indicator on every strip shows this, and it used to be a
    /// per-track routing call — so a mixer paid N round trips for it and
    /// then never heard about a change.
    ParentSendChanged { guid: String, enabled: bool },
    /// The track gained or lost a send, or a receive.
    ///
    /// Both counts ride one event for the reason the FX counts do: a
    /// strip draws the two lanes side by side, and one arriving without
    /// the other is a frame where the indicator contradicts itself.
    ///
    /// A count and not the routes. What changed about a route — its
    /// level, its destination — travels on the routing stream, which a
    /// strip does not subscribe to because it does not draw any of it.
    RouteCountsChanged {
        guid: String,
        send_count: u32,
        receive_count: u32,
    },
    /// The track's FX chains gained or lost plugins.
    ///
    /// `Track::fx_count` and `input_fx_count` were seeded by the bulk read
    /// and then never updated by anything: a mixer's FX buttons were
    /// correct when it opened and wrong from the first plugin the user
    /// added. This is the event that was missing.
    ///
    /// Both counts ride together because a backend computing one has
    /// already computed the other, and a strip showing the input indicator
    /// separately from the chain needs them consistent — two events could
    /// leave it briefly showing a track with input FX and no chain when it
    /// has both.
    ///
    /// Deliberately on the *track* stream rather than a new `#[subscribe]`
    /// on `Effects`: a lit button is track state. What should force a
    /// chain-level stream is a surface that renders the chain.
    FxCountChanged {
        guid: String,
        fx_count: u32,
        input_fx_count: u32,
    },
}

// SelfRef compatibility: TrackEvent has no lifetime parameters, so Ref<'a> = Self.
#[allow(unsafe_code)]
unsafe impl vox_types::Reborrow for TrackEvent {
    type Ref<'a> = TrackEvent;
}

/// Streaming envelope — pairs a [`TrackEvent`] with the project it
/// applies to.
#[derive(Debug, Clone, Facet)]
pub struct TrackStreamEvent {
    pub project_guid: String,
    pub event: TrackEvent,
}

// Trivial Reborrow impls for owned types — lets `SelfRef<T>::get()`
// hand subscribers `&T` for ergonomic field access. Safe because these
// types have no borrowed lifetimes.
#[cfg(feature = "vox")]
#[allow(unsafe_code)]
mod reborrow_impls {
    use super::TrackStreamEvent;
    unsafe impl vox_types::Reborrow for TrackStreamEvent {
        type Ref<'a> = TrackStreamEvent;
    }
}
