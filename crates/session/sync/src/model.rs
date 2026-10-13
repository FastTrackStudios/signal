//! The session, as plain data: what the document holds and what the
//! engine is told.
//!
//! Deliberately not daw-proto's types. The document is a file format —
//! it outlives every refactor of the service types — so what goes into
//! it is written down here, field by field, and nothing lands in it by
//! accident because a wire struct grew a member.

use std::collections::BTreeMap;

/// Everything collaborators share about one song's session.
///
/// Selection, the edit cursor and the play position are NOT here: they
/// are one person's, and travel as presence.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionModel {
    /// Project order, folders included — a depth-first walk of the
    /// track tree, each track naming its folder.
    pub tracks: Vec<TrackState>,
    /// By item guid.
    pub items: BTreeMap<String, ItemState>,
    /// By marker guid.
    pub markers: BTreeMap<String, MarkerState>,
    /// By region guid.
    pub regions: BTreeMap<String, RegionState>,
    /// Tempo and meter changes, in time order.
    pub tempo: Vec<TempoPoint>,
    /// The song's keyflow chart, as text.
    pub chart: String,
    /// Take markers, by [`TakeMarkerState::key`]: a recording's markers —
    /// its drum hits among them — whichever items are cut from it.
    pub take_markers: BTreeMap<String, TakeMarkerState>,
    /// What the project holds beyond its tracks, items, marks and tempo:
    /// the master bus, the monitoring chain, ruler lanes, ext-state.
    pub project: ProjectSettings,
}

/// The project's own settings, the master bus and the monitoring chain.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSettings {
    pub master: MasterState,
    /// What extensions wrote on the project (REAPER's `ProjExtState`):
    /// section → key → value.
    pub ext: BTreeMap<String, BTreeMap<String, String>>,
    /// The ruler's lanes by index: what markers and regions sit on.
    pub ruler_lanes: BTreeMap<u32, RulerLaneState>,
    /// The global automation override, a mode name ([`EnvelopeState::mode`]);
    /// `None` when off.
    pub automation_override: Option<String>,
    /// The monitoring (control room) FX chain, in order.
    pub monitoring_fx: Vec<FxState>,
    /// The media bay's own state (its retained paths and folders), as
    /// the engine serializes it; `None` until the bay is first used.
    pub bay: Option<Vec<u8>>,
    /// Whether the loop is on.
    pub looping: bool,
    pub metronome: bool,
    /// The project's play rate (1 is normal speed).
    pub playrate: f64,
    /// What recording does: `normal`, `time_selection`, `item`.
    pub record_mode: String,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            master: MasterState::default(),
            ext: BTreeMap::new(),
            ruler_lanes: BTreeMap::new(),
            automation_override: None,
            monitoring_fx: Vec::new(),
            bay: None,
            looping: false,
            metronome: false,
            playrate: 1.0,
            record_mode: "normal".to_owned(),
        }
    }
}

/// The master bus.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterState {
    /// A gain, 1.0 = unity.
    pub volume: f64,
    pub pan: f64,
    pub muted: bool,
    /// Its FX, in chain order.
    pub fx: Vec<FxState>,
    /// Its automation, keyed as a track's is.
    pub envelopes: BTreeMap<String, EnvelopeState>,
    /// Its hardware outputs, in order.
    pub hw_outputs: Vec<SendState>,
}

impl Default for MasterState {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pan: 0.0,
            muted: false,
            fx: Vec::new(),
            envelopes: BTreeMap::new(),
            hw_outputs: Vec::new(),
        }
    }
}

/// One ruler lane.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RulerLaneState {
    pub name: String,
    /// REAPER's lane flags (4: new markers land here, 8: new regions do).
    pub flags: u32,
}

impl SessionModel {
    /// The track with this guid.
    #[must_use]
    pub fn track(&self, guid: &str) -> Option<&TrackState> {
        self.tracks.iter().find(|t| t.guid == guid)
    }
}

// The engine's own booleans, one field each: the doc merges them one by
// one, which an enum of states could not do.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackState {
    pub guid: String,
    /// The folder this track sits in; `None` at the top level.
    pub parent: Option<String>,
    pub name: String,
    /// `0xRRGGBB`; `None` is the theme's default.
    pub color: Option<u32>,
    /// A gain, 1.0 = unity — the engine's own unit.
    pub volume: f64,
    /// -1.0 (left) to 1.0 (right).
    pub pan: f64,
    pub muted: bool,
    pub soloed: bool,
    pub phase_inverted: bool,
    /// Whether it feeds its folder.
    pub parent_send: bool,
    pub visible_in_tcp: bool,
    pub visible_in_mixer: bool,
    /// REAPER 7's fixed lanes: how many, what each is called, which play.
    pub lanes: LaneState,
    /// Its part in each track group: a family's lead or follow mask by
    /// name (`recarm_lead`, `media_edit_follow`, …), the non-zero ones.
    pub grouping: BTreeMap<String, u128>,
    /// What extensions and the template wrote on it (`FTS:kind` →
    /// `piece`): REAPER's `P_EXT`, by key.
    pub ext: BTreeMap<String, String>,
    /// Its FX, in chain order.
    pub fx: Vec<FxState>,
    /// Its sends, in order.
    pub sends: Vec<SendState>,
    /// Its automation, by what is automated ([`EnvelopeState`]).
    pub envelopes: BTreeMap<String, EnvelopeState>,
    /// Armed to record.
    pub armed: bool,
    /// Its automation mode, named as [`EnvelopeState::mode`].
    pub automation_mode: String,
    /// Input monitoring: `off`, `normal`, `not_when_playing`.
    pub monitor: String,
    /// What it records from: `none`, `audio:<channel>`,
    /// `midi:<device>:<channel>` (`*` for every device or channel), or
    /// `raw:<n>` (REAPER's own number).
    pub record_input: String,
    /// How many channels it carries (2: stereo).
    pub channels: u32,
    /// Its panel's height and its mixer strip's width, pixels; `None` is
    /// the default.
    pub height: Option<u32>,
    pub width: Option<u32>,
    /// How its fixed lanes are drawn: `small`, `big`, `one`.
    pub lane_display: String,
    /// Fixed-lane comping: which lane records, which is the comp, and the
    /// comp's areas.
    pub comping: CompingState,
    /// Ext-state written under a named section (the unnamed one is
    /// [`Self::ext`]): section → key → value.
    pub ext_sections: BTreeMap<String, BTreeMap<String, String>>,
    /// Its input (recording) FX, in chain order.
    pub input_fx: Vec<FxState>,
    /// Its hardware outputs, in order; each one's `dest` is the output's
    /// index.
    pub hw_outputs: Vec<SendState>,
}

/// A track's comping state (REAPER's `LANEREC` and `LINKEDLANE`s).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompingState {
    /// The lane new recordings land on, when one is pinned.
    pub record_lane: Option<u32>,
    /// The comping lane: the comp that plays.
    pub comp_lane: Option<u32>,
    /// The comping lane before it.
    pub last_comp_lane: Option<u32>,
    /// Every comp area, in order.
    pub areas: Vec<CompAreaState>,
}

/// One stretch of a comp lane, taken from a source lane.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CompAreaState {
    /// Seconds, project time.
    pub start: f64,
    pub end: f64,
    /// The lane the audio is taken from.
    pub source_lane: u32,
    /// The comp lane it is placed on.
    pub comp_lane: u32,
    /// Crossfades, seconds.
    pub fade_in: f64,
    pub fade_out: f64,
}

/// A track's fixed lanes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaneState {
    pub count: u32,
    pub names: Vec<String>,
    /// Bit `n`: lane `n` plays.
    pub play_mask: u64,
}

/// One FX on a track.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FxState {
    pub guid: String,
    /// What the engine adds it by: a built-in's name, a plugin's id or
    /// bundle.
    pub plugin: String,
    /// As shown.
    pub name: String,
    pub enabled: bool,
    pub offline: bool,
    /// Everything it holds (its parameters, a sampler's kit, a rig's
    /// capture), as a blob's name in the project's store — kept once,
    /// however many versions hold it.
    pub state: Option<String>,
    /// Its parameters by id, as the plugin reads them (plain values; a
    /// built-in without a plugin keeps 0..1 by slot) — what a plugin
    /// with no state of its own is.
    pub params: BTreeMap<u32, f64>,
    /// How many parameters it shows.
    pub param_count: u32,
    /// The preset last chosen, by index, and the name it shows.
    pub preset: Option<u32>,
    pub preset_name: Option<String>,
    /// Its named config values (REAPER's `TrackFX_GetNamedConfigParm`),
    /// by key.
    pub config: BTreeMap<String, String>,
}

/// One send from a track (or one hardware output, its `dest` the
/// output's index).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SendState {
    /// The destination track's guid.
    pub dest: String,
    /// `post_fader`, `pre_fx`, `post_fx`.
    pub mode: String,
    pub volume: f64,
    pub pan: f64,
    pub muted: bool,
    /// The source's first channel (0: its main pair, 2: its 3/4, 4: 5/6).
    pub from: u32,
    /// The destination's first channel (2: its sidechain).
    pub to: u32,
    pub mono: bool,
    pub phase_inverted: bool,
    /// Its automation mode, named as [`EnvelopeState::mode`].
    pub automation_mode: String,
    /// How many channels it takes and gives.
    pub from_count: u32,
    pub to_count: u32,
    /// Its MIDI channels, `<from>><to>`: a channel number or `*` (every
    /// channel, as they were). `None`: no MIDI mapping.
    pub midi: Option<String>,
    /// The far end's name as the engine shows it.
    pub label: Option<String>,
}

/// One automation envelope: what plays it, and its points.
///
/// Keyed on its track by what it automates: `volume`, `pan`, `mute`,
/// `volume_prefx`, `pan_prefx`, `width`, or `fx:<fx guid>:<param>`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EnvelopeState {
    /// `read`, `trim`, `touch`, `latch`, `latch_preview`, `write`, `off`.
    pub mode: String,
    pub visible: bool,
    pub armed: bool,
    pub points: Vec<EnvelopePointState>,
    /// Drawn in a lane of its own under the track, that lane's height.
    pub own_lane: bool,
    pub lane_height: u32,
    /// Its automation items, in order, each with its own curve.
    pub items: Vec<AutomationItemState>,
}

/// An automation item: a curve of its own placed on an envelope.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AutomationItemState {
    /// Its pool: items of one pool share a curve.
    pub pool: i32,
    pub name: String,
    /// Seconds.
    pub position: f64,
    pub length: f64,
    pub start_offset: f64,
    pub playrate: f64,
    pub baseline: f64,
    pub amplitude: f64,
    pub looped: bool,
    pub points: Vec<EnvelopePointState>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EnvelopePointState {
    /// Seconds.
    pub at: f64,
    pub value: f64,
    /// REAPER's shape number: 0 linear, 1 square, 2 slow start/end, 3
    /// fast start, 4 fast end, 5 bezier.
    pub shape: u8,
    pub tension: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemState {
    /// The guid of the track it is on.
    pub track: String,
    /// Seconds from the project start.
    pub position: f64,
    /// Seconds.
    pub length: f64,
    pub snap_offset: f64,
    pub muted: bool,
    pub locked: bool,
    pub volume: f64,
    pub fade_in: f64,
    pub fade_out: f64,
    pub fade_in_shape: String,
    pub fade_out_shape: String,
    /// The item's note — what the ruler lanes name chords, keys and
    /// lyric lines by.
    pub label: String,
    pub color: Option<u32>,
    pub take: TakeState,
    /// The fixed lane it sits on, on a track with lanes.
    pub fixed_lane: Option<u32>,
    /// Its source loops across it.
    pub loop_source: bool,
    /// How it follows tempo, its group, its own offset.
    pub timing: ItemTiming,
    /// Its other takes, in order: [`Self::take`] sits among them at
    /// [`Self::active_take`].
    pub other_takes: Vec<TakeState>,
    pub active_take: u32,
}

/// How an item follows tempo, the group it is in, its own offset.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemTiming {
    /// `time`, `beats`, `beats_position`.
    pub beat_attach: String,
    pub auto_stretch: bool,
    /// The item group it is in.
    pub group: Option<u32>,
    /// The item's own start offset, seconds.
    pub start_offset: f64,
}

impl Default for ItemTiming {
    fn default() -> Self {
        Self {
            beat_attach: "time".to_owned(),
            auto_stretch: false,
            group: None,
            start_offset: 0.0,
        }
    }
}

impl ItemState {
    /// Every take, in order, the active one in its place.
    #[must_use]
    pub fn takes(&self) -> Vec<&TakeState> {
        let mut all: Vec<&TakeState> = self.other_takes.iter().collect();
        let at = usize::try_from(self.active_take)
            .unwrap_or(usize::MAX)
            .min(all.len());
        all.insert(at, &self.take);
        all
    }
}

/// One take of an item; [`ItemState::take`] is the one it plays.
#[derive(Debug, Clone, PartialEq)]
pub struct TakeState {
    pub name: String,
    /// The media file, relative to the session folder. `None` for an
    /// empty (label-only) or MIDI take.
    pub source: Option<String>,
    /// Seconds into the source where the item starts.
    pub start_offset: f64,
    pub playrate: f64,
    /// A MIDI take (its notes are [`Self::notes`]), not audio.
    pub midi: bool,
    /// Its stretch markers, in order.
    pub stretch: Vec<StretchPoint>,
    /// Its volume envelope — a leveled drum's stroke-by-stroke gain — in
    /// order.
    pub volume: Vec<GainPoint>,
    /// A MIDI take's notes, in order.
    pub notes: Vec<NoteState>,
    pub color: Option<u32>,
    /// The take's own gain (1 is unity).
    pub gain: f64,
    /// Semitones.
    pub pitch: f64,
    pub preserve_pitch: bool,
    /// REAPER's channel mode (0 normal, 1 reversed, 2 mono mix, 3 left,
    /// 4 right).
    pub channel_mode: u32,
    /// `audio`, `midi`, `empty`, `video`, `unknown`.
    pub source_type: String,
    /// What the engine found the source to be: its length (seconds),
    /// sample rate and channels.
    pub source_length: Option<f64>,
    pub source_rate: Option<u32>,
    pub source_channels: Option<u32>,
    /// A MIDI take's note count, as the engine keeps it.
    pub note_count: Option<u32>,
    /// Its stretch algorithm, REAPER's number: 0 the project's, 1
    /// balanced, 2 tonal, 4 transient, 5 no pre-echo.
    pub stretch_mode: u8,
    /// Its envelopes other than volume (`pan`, `mute`, `pitch`), by kind.
    pub envelopes: BTreeMap<String, EnvelopeState>,
    /// A MIDI take's events other than notes.
    pub midi_events: MidiEvents,
}

impl Default for TakeState {
    fn default() -> Self {
        Self {
            name: String::new(),
            source: None,
            start_offset: 0.0,
            playrate: 1.0,
            midi: false,
            stretch: Vec::new(),
            volume: Vec::new(),
            notes: Vec::new(),
            color: None,
            gain: 1.0,
            pitch: 0.0,
            preserve_pitch: false,
            channel_mode: 0,
            source_type: String::new(),
            source_length: None,
            source_rate: None,
            source_channels: None,
            note_count: None,
            stretch_mode: 0,
            envelopes: BTreeMap::new(),
            midi_events: MidiEvents::default(),
        }
    }
}

/// A MIDI take's events other than its notes, each kind in order. Times
/// are in the take's ticks, as [`NoteState::start`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MidiEvents {
    pub ccs: Vec<CcState>,
    pub pitch_bends: Vec<PitchBendState>,
    pub program_changes: Vec<ProgramChangeState>,
    pub sysex: Vec<SysexState>,
    pub channel_pressures: Vec<ChannelPressureState>,
    pub poly_pressures: Vec<PolyPressureState>,
    pub note_expressions: Vec<NoteExpressionState>,
}

impl MidiEvents {
    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CcState {
    pub at: f64,
    pub channel: u8,
    pub controller: u8,
    pub value: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PitchBendState {
    pub at: f64,
    pub channel: u8,
    /// -8192..8191, 0 centred.
    pub value: i16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProgramChangeState {
    pub at: f64,
    pub channel: u8,
    pub program: u8,
}

/// A system-exclusive message, whole (`F0 … F7`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SysexState {
    pub at: f64,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ChannelPressureState {
    pub at: f64,
    pub channel: u8,
    pub pressure: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PolyPressureState {
    pub at: f64,
    pub channel: u8,
    pub note: u8,
    pub pressure: u8,
}

/// A per-note expression (MPE-style): one dimension of one voice.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NoteExpressionState {
    pub at: f64,
    pub channel: u8,
    pub note: u8,
    /// 0 volume, 1 pan, 2 tuning, 3 vibrato, 4 expression, 5 brightness,
    /// 6 pressure.
    pub dimension: u8,
    pub value: f64,
}

/// A point on a take's volume envelope.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GainPoint {
    /// Seconds into the item.
    pub at: f64,
    /// Linear gain (1 is unity).
    pub value: f64,
}

/// A stretch marker: where in the take it sits and what source moment
/// it holds there.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StretchPoint {
    /// Seconds into the take.
    pub at: f64,
    /// Seconds into the source.
    pub source: f64,
    pub slope: f64,
}

/// One MIDI note, in the take's quarter notes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NoteState {
    pub start: f64,
    pub length: f64,
    pub pitch: u8,
    pub velocity: u8,
    pub channel: u8,
    pub muted: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarkerState {
    pub at: f64,
    pub name: String,
    pub color: Option<u32>,
    /// Which ruler lane it sits on (none: the default).
    pub lane: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegionState {
    pub start: f64,
    pub end: f64,
    pub name: String,
    pub color: Option<u32>,
    /// Which ruler lane it sits on (none: the default) — a song's region
    /// on one, its sections on another.
    pub lane: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TempoPoint {
    /// Seconds.
    pub at: f64,
    pub bpm: f64,
    pub beats_per_bar: u32,
    pub beat_unit: u32,
}

/// A marker on a recording (a take's source), drum hits among them.
///
/// Kept by recording rather than by item: a slip edit cuts one take into
/// hundreds of items, and every one of them shows the same hits. One
/// stroke on a piece is a marker on each of its mics' recordings, all
/// with the hit's `id`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TakeMarkerState {
    /// The recording, relative to the session folder (as
    /// [`TakeState::source`]).
    pub source: String,
    /// What the marker is, whatever its place among the others.
    pub id: String,
    /// Seconds into the recording.
    pub at: f64,
    pub name: String,
    pub color: Option<u32>,
    /// A drum hit's properties, when it is one.
    pub hit: Option<HitState>,
}

impl TakeMarkerState {
    /// Where the doc keeps it: its recording and id.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}#{}", self.source, self.id)
    }
}

/// Which of a piece's inputs reported a hit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sources {
    /// Its trigger.
    pub trigger: bool,
    /// Its close mic.
    pub mic: bool,
}

/// A drum hit, as a take marker carries it (`daw_proto::HitMark`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HitState {
    pub piece: String,
    /// `auto`, `manual` or `approved`.
    pub origin: String,
    /// The detector run that placed it; empty when placed by hand.
    pub run: String,
    /// The attack's peak on the close mic, dBFS.
    pub peak_db: f64,
    pub confidence: f64,
    /// Which inputs reported it.
    pub heard: Sources,
    pub ghost: bool,
    pub bleed: bool,
    /// The other stroke of a flam: that marker's id.
    pub flam_with: Option<String>,
    pub note: String,
}
