//! Domain types for Ableton Live set data.
//!
//! These types represent the parsed contents of an Ableton Live set file.
//! They are format-specific (not yet mapped to `daw_proto` types).

use crate::devices::BuiltinParams;
use std::path::PathBuf;

/// A parsed Ableton Live set (.als file).
#[derive(Debug, Clone)]
pub struct AbletonLiveSet {
    /// Ableton Live version that created this set.
    pub version: AbletonVersion,
    /// Tempo in BPM (from master track).
    pub tempo: f64,
    /// Time signature.
    pub time_signature: TimeSignature,
    /// Key signature (if detected, v11+ only).
    pub key_signature: Option<KeySignature>,
    /// Audio tracks.
    pub audio_tracks: Vec<AudioTrack>,
    /// MIDI tracks.
    pub midi_tracks: Vec<MidiTrack>,
    /// Return (aux/send) tracks.
    pub return_tracks: Vec<ReturnTrack>,
    /// Group tracks.
    pub group_tracks: Vec<GroupTrack>,
    /// Master track mixer state.
    pub master_track: Option<MasterTrack>,
    /// Arrangement locators (markers).
    pub locators: Vec<Locator>,
    /// Scenes (session view).
    pub scenes: Vec<Scene>,
    /// Tempo automation events.
    pub tempo_automation: Vec<AutomationPoint>,
    /// Transport state (loop, punch, metronome).
    pub transport: TransportState,
    /// Furthest bar position (derived from max CurrentEnd values).
    pub furthest_bar: f64,
    /// Groove pool templates.
    pub groove_pool: Vec<Groove>,
    /// Tuning system (v12, opaque preservation).
    pub tuning_system: Option<TuningSystem>,
    /// Pre-hear (cue) track.
    pub pre_hear_track: Option<PreHearTrack>,
}

/// The pre-hear (cue) track for previewing audio.
#[derive(Debug, Clone)]
pub struct PreHearTrack {
    /// Audio output routing target.
    pub audio_output: RoutingTarget,
}

/// Ableton version info parsed from the `MinorVersion` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbletonVersion {
    /// Major version (8, 9, 10, 11, 12, ...).
    pub major: u32,
    /// Minor version.
    pub minor: u32,
    /// Patch/build number.
    pub patch: u32,
    /// Whether this is a beta release.
    pub beta: bool,
    /// The raw `Creator` attribute value.
    pub creator: String,
}

impl AbletonVersion {
    /// Check if this version is at least the given major.minor.
    pub fn at_least(&self, major: u32, minor: u32) -> bool {
        (self.major, self.minor) >= (major, minor)
    }
}

impl std::fmt::Display for AbletonVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if self.beta {
            write!(f, " (beta)")?;
        }
        Ok(())
    }
}

/// Time signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

/// Key signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySignature {
    pub root_note: Tonic,
    pub scale: String,
}

/// Chromatic root note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tonic {
    C,
    CSharp,
    D,
    DSharp,
    E,
    F,
    FSharp,
    G,
    GSharp,
    A,
    ASharp,
    B,
}

impl Tonic {
    /// Convert from MIDI note number mod 12.
    pub fn from_midi(note: u32) -> Self {
        match note % 12 {
            0 => Self::C,
            1 => Self::CSharp,
            2 => Self::D,
            3 => Self::DSharp,
            4 => Self::E,
            5 => Self::F,
            6 => Self::FSharp,
            7 => Self::G,
            8 => Self::GSharp,
            9 => Self::A,
            10 => Self::ASharp,
            11 => Self::B,
            _ => unreachable!(),
        }
    }

    /// Convert to MIDI note number (0-11).
    pub fn to_midi(self) -> u32 {
        match self {
            Self::C => 0,
            Self::CSharp => 1,
            Self::D => 2,
            Self::DSharp => 3,
            Self::E => 4,
            Self::F => 5,
            Self::FSharp => 6,
            Self::G => 7,
            Self::GSharp => 8,
            Self::A => 9,
            Self::ASharp => 10,
            Self::B => 11,
        }
    }
}

impl std::fmt::Display for Tonic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::C => "C",
            Self::CSharp => "C#",
            Self::D => "D",
            Self::DSharp => "D#",
            Self::E => "E",
            Self::F => "F",
            Self::FSharp => "F#",
            Self::G => "G",
            Self::GSharp => "G#",
            Self::A => "A",
            Self::ASharp => "A#",
            Self::B => "B",
        };
        f.write_str(s)
    }
}

// ─── Routing types ─────────────────────────────────────────────────────────

/// A routing target with display strings and MPE settings.
#[derive(Debug, Clone, Default)]
pub struct RoutingTarget {
    /// Routing target identifier string.
    pub target: String,
    /// Upper display string (shown in Ableton UI).
    pub upper_display_string: String,
    /// Lower display string (shown in Ableton UI).
    pub lower_display_string: String,
    /// MPE settings for this routing.
    pub mpe_settings: Option<MpeSettings>,
}

/// MPE (MIDI Polyphonic Expression) settings for a routing target.
#[derive(Debug, Clone)]
pub struct MpeSettings {
    /// Zone type.
    pub zone_type: i32,
    /// First note channel.
    pub first_note_channel: i32,
    /// Last note channel.
    pub last_note_channel: i32,
}

// ─── Track types ────────────────────────────────────────────────────────────

/// Common track properties shared by all track types.
#[derive(Debug, Clone)]
pub struct TrackCommon {
    /// Track ID (from the `Id` attribute on the XML element).
    pub id: i32,
    /// User-given name (empty if not renamed).
    pub user_name: String,
    /// Effective (displayed) name.
    pub effective_name: String,
    /// Annotation / notes.
    pub annotation: String,
    /// Color index (0-69 in Ableton's palette).
    pub color: i32,
    /// Group track ID this track belongs to (-1 = none).
    pub group_id: i32,
    /// Whether the track is folded in the arrangement.
    pub folded: bool,
    /// Mixer state.
    pub mixer: MixerState,
    /// Devices (plugins and built-in effects) on this track.
    pub devices: Vec<Device>,
    /// Per-track automation envelopes.
    pub automation_envelopes: Vec<AutomationEnvelope>,
    /// Track delay compensation settings.
    pub track_delay: Option<TrackDelay>,
    /// Linked track group ID (-1 = none).
    pub linked_track_group_id: i32,
    /// Track view/UI state (preserved for round-trip).
    pub view_state: Option<TrackViewState>,
    /// Memorized first clip name (from Name element).
    pub memorized_first_clip_name: String,
}

/// An audio track.
#[derive(Debug, Clone)]
pub struct AudioTrack {
    pub common: TrackCommon,
    /// Audio clips in the arrangement.
    pub arrangement_clips: Vec<AudioClip>,
    /// Audio clips in session view clip slots (slot_index, clip).
    pub session_clips: Vec<SessionClip<AudioClip>>,
    /// Audio input routing target.
    pub audio_input: RoutingTarget,
    /// Audio output routing target.
    pub audio_output: RoutingTarget,
    /// Monitoring mode: 0=off, 1=in, 2=auto.
    pub monitoring: i32,
    /// Whether the track is armed for recording.
    pub is_armed: bool,
    /// Take counter for recording.
    pub take_counter: i32,
    /// Take lanes for comping.
    pub take_lanes: Vec<TakeLane>,
}

/// A MIDI track.
#[derive(Debug, Clone)]
pub struct MidiTrack {
    pub common: TrackCommon,
    /// MIDI clips in the arrangement.
    pub arrangement_clips: Vec<MidiClip>,
    /// MIDI clips in session view clip slots (slot_index, clip).
    pub session_clips: Vec<SessionClip<MidiClip>>,
    /// MIDI input routing target.
    pub midi_input: RoutingTarget,
    /// Audio output routing target.
    pub audio_output: RoutingTarget,
    /// Monitoring mode.
    pub monitoring: i32,
    /// Whether the track is armed for recording.
    pub is_armed: bool,
    /// Take counter for recording.
    pub take_counter: i32,
    /// Take lanes for comping.
    pub take_lanes: Vec<TakeLane>,
    /// Pitchbend range in semitones (default 48).
    pub pitchbend_range: i32,
}

/// A return (send) track.
#[derive(Debug, Clone)]
pub struct ReturnTrack {
    pub common: TrackCommon,
}

/// A group track.
#[derive(Debug, Clone)]
pub struct GroupTrack {
    pub common: TrackCommon,
}

/// The master track.
#[derive(Debug, Clone)]
pub struct MasterTrack {
    pub mixer: MixerState,
    /// Audio output routing target.
    pub audio_output: RoutingTarget,
    /// Devices on the master.
    pub devices: Vec<Device>,
}

/// A clip in a session view slot, tracking its slot index (scene row).
#[derive(Debug, Clone)]
pub struct SessionClip<T> {
    /// Clip slot index (corresponds to scene row).
    pub slot_index: usize,
    /// The clip data.
    pub clip: T,
    /// Whether this slot has a stop button.
    pub has_stop: bool,
}

/// Mixer channel strip state.
#[derive(Debug, Clone)]
pub struct MixerState {
    /// Volume (0.0 = -inf, 1.0 = 0dB, ~1.4 = +6dB).
    pub volume: f64,
    /// Pan (-1.0 = full left, 0.0 = center, 1.0 = full right).
    pub pan: f64,
    /// Send levels, indexed by return track order.
    pub sends: Vec<SendLevel>,
    /// Whether the track is solo'd.
    pub solo: bool,
    /// Whether the track's speaker (output) is enabled.
    pub speaker_on: bool,
    /// Crossfade assignment: 0=none, 1=A, 2=B.
    pub crossfade_state: i32,
    /// Split stereo pan left value (when pan_mode == 1).
    pub split_stereo_pan_l: Option<f64>,
    /// Split stereo pan right value (when pan_mode == 1).
    pub split_stereo_pan_r: Option<f64>,
    /// Pan mode: 0=stereo, 1=split stereo.
    pub pan_mode: i32,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pan: 0.0,
            sends: Vec::new(),
            solo: false,
            speaker_on: true,
            crossfade_state: 0,
            split_stereo_pan_l: None,
            split_stereo_pan_r: None,
            pan_mode: 0,
        }
    }
}

/// A send level on the mixer.
#[derive(Debug, Clone)]
pub struct SendLevel {
    /// Send level (same scale as volume).
    pub level: f64,
    /// Whether this send is enabled.
    pub enabled: bool,
}

// ─── Clip types ─────────────────────────────────────────────────────────────

/// Common clip properties.
#[derive(Debug, Clone)]
pub struct ClipCommon {
    /// Clip ID.
    pub id: i32,
    /// Position on the timeline in beats.
    pub time: f64,
    /// Start of the clip's active region in beats.
    pub current_start: f64,
    /// End of the clip's active region in beats.
    pub current_end: f64,
    /// Clip name.
    pub name: String,
    /// Color index.
    pub color: i32,
    /// Whether the clip is disabled.
    pub disabled: bool,
    /// Loop settings.
    pub loop_settings: Option<LoopSettings>,
    /// Follow action (session view).
    pub follow_action: Option<FollowAction>,
    /// Clip automation envelopes.
    pub envelopes: Vec<ClipEnvelope>,
    /// Launch mode: 0=trigger, 1=gate, 2=toggle, 3=repeat.
    pub launch_mode: i32,
    /// Launch quantisation: 0=none, 1=8bars, 2=4bars, etc.
    pub launch_quantisation: i32,
    /// Grid settings for the clip.
    pub grid: Option<ClipGrid>,
    /// Legato mode.
    pub legato: bool,
    /// RAM mode (load sample into RAM).
    pub ram: bool,
    /// Velocity amount.
    pub velocity_amount: f64,
    /// Groove ID (-1 = none).
    pub groove_id: i32,
    /// Freeze start position.
    pub freeze_start: f64,
    /// Freeze end position.
    pub freeze_end: f64,
    /// Take ID.
    pub take_id: i32,
}

/// Grid settings for a clip.
#[derive(Debug, Clone)]
pub struct ClipGrid {
    pub fixed_numerator: i32,
    pub fixed_denominator: i32,
    pub grid_interval_pixel: i32,
    pub ntoles: i32,
    pub snap_to_grid: bool,
    pub fixed: bool,
}

/// Loop settings for a clip.
#[derive(Debug, Clone)]
pub struct LoopSettings {
    pub loop_start: f64,
    pub loop_end: f64,
    pub loop_on: bool,
    pub start_relative: f64,
}

/// Follow action settings for session view clips.
#[derive(Debug, Clone)]
pub struct FollowAction {
    /// Follow time in beats.
    pub follow_time: f64,
    /// Whether linked to clip length.
    pub is_linked: bool,
    /// Number of loop iterations before triggering.
    pub loop_iterations: i32,
    /// Follow action A type (0=None, 1=Stop, 3=Previous, 4=Next, etc.).
    pub action_a: i32,
    /// Follow action B type.
    pub action_b: i32,
    /// Chance for action A (0-100).
    pub chance_a: i32,
    /// Chance for action B (0-100).
    pub chance_b: i32,
    /// Whether follow actions are enabled on this clip.
    pub enabled: bool,
}

/// An audio clip.
#[derive(Debug, Clone)]
pub struct AudioClip {
    pub common: ClipCommon,
    /// Sample file reference.
    pub sample_ref: Option<SampleRef>,
    /// Warp mode (0=Beats, 1=Tones, 2=Texture, 3=Re-Pitch, 4=Complex, 6=ComplexPro).
    pub warp_mode: i32,
    /// Whether time-warping is enabled.
    pub is_warped: bool,
    /// Warp markers.
    pub warp_markers: Vec<WarpMarker>,
    /// Pitch adjustment in semitones.
    pub pitch_coarse: f64,
    /// Fine pitch adjustment in cents.
    pub pitch_fine: f64,
    /// Sample volume (gain).
    pub sample_volume: f64,
    /// Fade settings.
    pub fades: Option<Fades>,
    /// Granularity tones (Tones warp mode).
    pub granularity_tones: f64,
    /// Granularity texture (Texture warp mode).
    pub granularity_texture: f64,
    /// Fluctuation texture (Texture warp mode).
    pub fluctuation_texture: f64,
    /// Transient resolution (Beats warp mode).
    pub transient_resolution: i32,
    /// Transient loop mode (Beats warp mode).
    pub transient_loop_mode: i32,
    /// Transient envelope (Beats warp mode).
    pub transient_envelope: f64,
    /// Complex Pro formants.
    pub complex_pro_formants: f64,
    /// Complex Pro envelope.
    pub complex_pro_envelope: f64,
    /// Whether fading is enabled (boolean flag, separate from Fades struct).
    pub fade_on: bool,
    /// High quality interpolation.
    pub hiq: bool,
    /// Whether this clip is the song tempo leader.
    pub is_song_tempo_leader: bool,
}

/// A MIDI clip.
#[derive(Debug, Clone)]
pub struct MidiClip {
    pub common: ClipCommon,
    /// Key tracks containing MIDI notes.
    pub key_tracks: Vec<KeyTrack>,
    /// Key/scale info (v11+).
    pub scale_info: Option<KeySignature>,
    /// Bank select coarse (-1 = none).
    pub bank_select_coarse: i32,
    /// Bank select fine (-1 = none).
    pub bank_select_fine: i32,
    /// Program change (-1 = none).
    pub program_change: i32,
    /// Note spelling preference.
    pub note_spelling_preference: i32,
    /// Expression grid settings.
    pub expression_grid: Option<ClipGrid>,
}

/// A key track groups notes by MIDI key within a clip.
#[derive(Debug, Clone)]
pub struct KeyTrack {
    /// MIDI note number for this key track.
    pub midi_key: u8,
    /// Notes in this key track.
    pub notes: Vec<MidiNote>,
}

/// A single MIDI note event.
#[derive(Debug, Clone, Copy)]
pub struct MidiNote {
    /// Time position in beats (relative to clip).
    pub time: f64,
    /// Duration in beats.
    pub duration: f64,
    /// Velocity (0-127).
    pub velocity: u8,
    /// Velocity deviation (-127 to 127).
    pub velocity_deviation: i8,
    /// Whether this note is enabled.
    pub is_enabled: bool,
    /// Note probability (0.0-1.0).
    pub probability: f64,
}

// ─── Sample references ──────────────────────────────────────────────────────

/// A reference to a sample file.
#[derive(Debug, Clone)]
pub struct SampleRef {
    /// Absolute path to the sample file.
    pub path: Option<PathBuf>,
    /// Relative path (from project root).
    pub relative_path: Option<String>,
    /// Filename only (pre-v11 fallback).
    pub name: Option<String>,
    /// File size in bytes.
    pub file_size: Option<u64>,
    /// CRC checksum.
    pub crc: Option<u32>,
    /// Last modification timestamp (unix epoch).
    pub last_mod_date: Option<u64>,
    /// Default sample duration in frames.
    pub default_duration: Option<u64>,
    /// Default sample rate.
    pub default_sample_rate: Option<u32>,
    /// Live Pack name (empty if not from a pack).
    pub live_pack_name: Option<String>,
    /// Live Pack ID.
    pub live_pack_id: Option<String>,
}

/// A warp marker mapping a time in seconds to a beat position.
#[derive(Debug, Clone, Copy)]
pub struct WarpMarker {
    /// Time in seconds in the audio file.
    pub sec_time: f64,
    /// Corresponding beat position.
    pub beat_time: f64,
}

/// Fade settings for an audio clip.
#[derive(Debug, Clone, Copy)]
pub struct Fades {
    pub fade_in_length: f64,
    pub fade_out_length: f64,
    pub fade_in_curve_skew: f64,
    pub fade_in_curve_slope: f64,
    pub fade_out_curve_skew: f64,
    pub fade_out_curve_slope: f64,
}

// ─── Devices / Plugins ─────────────────────────────────────────────────────

/// A device (plugin or built-in effect) on a track.
#[derive(Debug, Clone)]
pub struct Device {
    /// Device name.
    pub name: String,
    /// Device format/type.
    pub format: DeviceFormat,
    /// Device identifier string (e.g. `device:vst3:audiofx:<uuid>`).
    pub device_id: Option<String>,
    /// Whether the device is enabled.
    pub is_on: bool,
    /// Parsed/structured form of the device_id.
    pub parsed_id: Option<DevicePluginId>,
    /// Base64-encoded processor state (plugin preset data).
    pub processor_state: Option<String>,
    /// Base64-encoded controller state.
    pub controller_state: Option<String>,
    /// Typed parameters for recognised built-in devices.
    pub builtin_params: Option<BuiltinParams>,
}

/// The format/type of a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceFormat {
    /// Ableton built-in device.
    Builtin,
    /// VST2 plugin.
    Vst2,
    /// VST3 plugin.
    Vst3,
    /// Audio Unit plugin (macOS).
    AudioUnit,
    /// Max for Live device.
    MaxForLive,
    /// Note algorithm / note transform (v12).
    NoteAlgorithm,
    /// Unknown or unrecognized format.
    Unknown,
}

// ─── Automation ─────────────────────────────────────────────────────────────

/// An automation envelope attached to a track parameter.
#[derive(Debug, Clone)]
pub struct AutomationEnvelope {
    /// The automation target's pointee ID.
    pub pointee_id: i32,
    /// Automation points.
    pub events: Vec<AutomationEvent>,
}

/// An automation event (supports float, bool, and enum types).
#[derive(Debug, Clone, Copy)]
pub enum AutomationEvent {
    Float {
        time: f64,
        value: f64,
        /// Bezier control handle 1 (x, y).
        curve_control_1: Option<(f64, f64)>,
        /// Bezier control handle 2 (x, y).
        curve_control_2: Option<(f64, f64)>,
    },
    Bool {
        time: f64,
        value: bool,
    },
    Enum {
        time: f64,
        value: i32,
    },
}

impl AutomationEvent {
    pub fn time(&self) -> f64 {
        match self {
            Self::Float { time, .. } => *time,
            Self::Bool { time, .. } => *time,
            Self::Enum { time, .. } => *time,
        }
    }
}

/// A clip-level automation envelope.
#[derive(Debug, Clone)]
pub struct ClipEnvelope {
    /// Parameter target ID.
    pub pointee_id: i32,
    /// Automation events within the clip.
    pub events: Vec<AutomationEvent>,
}

/// An automation point (simplified, for tempo automation).
#[derive(Debug, Clone, Copy)]
pub struct AutomationPoint {
    /// Time in beats.
    pub time: f64,
    /// Automation value.
    pub value: f64,
    /// Bezier control handle 1 (x, y).
    pub curve_control_1: Option<(f64, f64)>,
    /// Bezier control handle 2 (x, y).
    pub curve_control_2: Option<(f64, f64)>,
}

// ─── Arrangement ────────────────────────────────────────────────────────────

/// An arrangement locator (marker).
#[derive(Debug, Clone)]
pub struct Locator {
    /// Position in beats.
    pub time: f64,
    /// Marker name.
    pub name: String,
}

/// A session view scene.
#[derive(Debug, Clone)]
pub struct Scene {
    /// Scene ID.
    pub id: i32,
    /// Scene name.
    pub name: String,
    /// Color index.
    pub color: i32,
    /// Scene tempo override (if enabled).
    pub tempo: Option<f64>,
    /// Annotation / notes.
    pub annotation: String,
    /// Follow action for the scene.
    pub follow_action: Option<FollowAction>,
    /// Time signature ID.
    pub time_signature_id: i32,
    /// Whether the time signature override is enabled.
    pub is_time_signature_enabled: bool,
}

/// Transport state.
#[derive(Debug, Clone)]
pub struct TransportState {
    /// Whether the loop is enabled.
    pub loop_on: bool,
    /// Loop start position in beats.
    pub loop_start: f64,
    /// Loop length in beats.
    pub loop_length: f64,
    /// Whether loop start is also the song start.
    pub loop_is_song_start: bool,
    /// Current playback position in beats.
    pub current_time: f64,
    /// Punch-in enabled.
    pub punch_in: bool,
    /// Punch-out enabled.
    pub punch_out: bool,
    /// Metronome tick duration.
    pub metronome_tick_duration: i32,
    /// Draw mode (0=off, 1=on).
    pub draw_mode: i32,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            loop_on: false,
            loop_start: 0.0,
            loop_length: 16.0,
            loop_is_song_start: false,
            current_time: 0.0,
            punch_in: false,
            punch_out: false,
            metronome_tick_duration: 0,
            draw_mode: 0,
        }
    }
}

// ─── Track delay / view state ──────────────────────────────────────────────

/// Track delay compensation settings.
#[derive(Debug, Clone, Copy)]
pub struct TrackDelay {
    /// Delay value (in ms or samples depending on `is_sample_based`).
    pub value: f64,
    /// Whether the delay value is in samples (true) or milliseconds (false).
    pub is_sample_based: bool,
}

/// Track view/UI state (preserved for round-trip fidelity).
#[derive(Debug, Clone)]
pub struct TrackViewState {
    /// Session view track width.
    pub session_track_width: Option<i32>,
    /// Arrangement lane height.
    pub arrangement_lane_height: Option<i32>,
    /// Raw JSON view data.
    pub view_data_json: Option<String>,
}

// ─── Groove pool ───────────────────────────────────────────────────────────

/// A groove template from the groove pool.
#[derive(Debug, Clone)]
pub struct Groove {
    /// Groove ID.
    pub id: i32,
    /// Groove name.
    pub name: String,
    /// File path (empty if not loaded from a file).
    pub path: String,
    /// Base note division.
    pub base: f64,
    /// Quantize amount (0.0-1.0).
    pub quantize_amount: f64,
    /// Timing amount (0.0-1.0).
    pub timing_amount: f64,
    /// Random amount (0.0-1.0).
    pub random_amount: f64,
    /// Velocity amount (0.0-1.0).
    pub velocity_amount: f64,
}

// ─── Tuning system ─────────────────────────────────────────────────────────

/// Tuning system (v12, preserved as raw XML for round-trip fidelity).
#[derive(Debug, Clone)]
pub struct TuningSystem {
    /// The raw XML content of the tuning system element.
    pub raw_xml: String,
}

// ─── Take lanes ────────────────────────────────────────────────────────────

/// A take lane for comping (recording multiple takes).
#[derive(Debug, Clone)]
pub struct TakeLane {
    /// Take lane ID.
    pub id: i32,
    /// Take lane name.
    pub name: String,
    /// Whether this take lane is the active (selected) one.
    pub is_active: bool,
    /// Audio clips in this take lane.
    pub audio_clips: Vec<AudioClip>,
    /// MIDI clips in this take lane.
    pub midi_clips: Vec<MidiClip>,
}

// ─── Device plugin ID ──────────────────────────────────────────────────────

/// Parsed/structured form of a device plugin identifier string.
#[derive(Debug, Clone)]
pub struct DevicePluginId {
    /// Plugin format.
    pub format: DeviceFormat,
    /// Plugin category (instrument, effect, etc.).
    pub category: DeviceCategory,
    /// UUID (VST3, AU).
    pub uuid: Option<String>,
    /// Numeric plugin ID (VST2).
    pub numeric_id: Option<i64>,
    /// Name hint from the identifier string.
    pub name_hint: Option<String>,
}

/// Category of a device plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceCategory {
    /// Instrument (virtual instrument / synth).
    Instrument,
    /// Audio effect.
    AudioEffect,
    /// MIDI effect.
    MidiEffect,
    /// Unknown category.
    Unknown,
}
