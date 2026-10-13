//! Domain types for DawProject data.
//!
//! These types represent the parsed contents of a `.dawproject` file.
//! They are format-specific (not yet mapped to `daw_proto` types).

#![allow(clippy::large_enum_variant, clippy::should_implement_trait)]

use std::path::PathBuf;

// ─── Top-level ───────────────────────────────────────────────────────────────

/// A parsed DawProject file.
#[derive(Debug, Clone)]
pub struct DawProject {
    /// DawProject format version (e.g. "1.0").
    pub version: String,
    /// Application that created this project.
    pub application: Option<Application>,
    /// Project metadata (from metadata.xml, if present).
    pub metadata: Option<ProjectMetadata>,
    /// Global transport settings (tempo, time signature).
    pub transport: Transport,
    /// Track hierarchy (the `<Structure>` element).
    pub tracks: Vec<Track>,
    /// The main arrangement timeline.
    pub arrangement: Option<Arrangement>,
    /// Clip launcher scenes.
    pub scenes: Vec<Scene>,
}

/// Application info embedded in the project.
#[derive(Debug, Clone)]
pub struct Application {
    pub name: String,
    pub version: String,
}

/// Project metadata from `metadata.xml`.
#[derive(Debug, Clone, Default)]
pub struct ProjectMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub composer: Option<String>,
    pub songwriter: Option<String>,
    pub producer: Option<String>,
    pub original_artist: Option<String>,
    pub arranger: Option<String>,
    pub year: Option<String>,
    pub genre: Option<String>,
    pub copyright: Option<String>,
    pub website: Option<String>,
    pub comment: Option<String>,
}

// ─── Transport ───────────────────────────────────────────────────────────────

/// Global transport settings.
#[derive(Debug, Clone)]
pub struct Transport {
    /// Project tempo in BPM.
    pub tempo: f64,
    /// Time signature numerator.
    pub numerator: u8,
    /// Time signature denominator (power of 2: 2, 4, 8, 16).
    pub denominator: u8,
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            tempo: 120.0,
            numerator: 4,
            denominator: 4,
        }
    }
}

// ─── Track hierarchy ─────────────────────────────────────────────────────────

/// A track in the project structure.
#[derive(Debug, Clone)]
pub struct Track {
    /// Unique XML ID used for cross-references.
    pub id: String,
    /// Track name.
    pub name: String,
    /// Track color (hex RGB string, e.g. "#FF8800").
    pub color: Option<String>,
    /// Annotation / comment text.
    pub comment: Option<String>,
    /// Content types this track holds (space-separated in XML, e.g. "audio notes").
    pub content_types: Vec<ContentType>,
    /// Whether the track's content is currently loaded in memory.
    pub loaded: bool,
    /// Mixer channel for this track (absent on group/folder-only tracks).
    pub channel: Option<Channel>,
    /// Child tracks (for group/folder tracks).
    pub children: Vec<Track>,
}

/// A single content-type token a track holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentType {
    Audio,
    Notes,
    Automation,
    Video,
    Markers,
    /// Nested tracks (folder / group tracks).
    Tracks,
    /// Unrecognised token.
    Unknown(String),
}

impl ContentType {
    /// Parse a single token (not the full space-separated string).
    pub fn from_token(s: &str) -> Self {
        match s {
            "audio" => Self::Audio,
            "notes" => Self::Notes,
            "automation" => Self::Automation,
            "video" => Self::Video,
            "markers" => Self::Markers,
            "tracks" => Self::Tracks,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// Parse a space-separated contentType attribute into a list.
    pub fn parse_list(s: &str) -> Vec<Self> {
        s.split_whitespace().map(Self::from_token).collect()
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Audio => "audio",
            Self::Notes => "notes",
            Self::Automation => "automation",
            Self::Video => "video",
            Self::Markers => "markers",
            Self::Tracks => "tracks",
            Self::Unknown(s) => s.as_str(),
        }
    }
}

/// Mixer channel strip.
#[derive(Debug, Clone)]
pub struct Channel {
    /// Channel ID for automation targeting.
    pub id: String,
    /// Channel role.
    pub role: ChannelRole,
    /// Number of audio channels (1 = mono, 2 = stereo).
    pub audio_channels: u32,
    /// IDREF to the destination channel (master/bus this feeds into).
    pub destination: Option<String>,
    /// Blend mode for this channel (e.g. "normal", "multiply").
    pub blend_mode: Option<String>,
    /// Volume (linear amplitude: 1.0 = 0 dB).
    pub volume: f64,
    /// Pan (-1.0 = full left, 0.0 = center, 1.0 = full right).
    pub pan: f64,
    /// Whether the channel is muted.
    pub muted: bool,
    /// Whether the channel is soloed.
    pub solo: bool,
    /// Send routing entries.
    pub sends: Vec<Send>,
    /// Devices (plugins and built-in effects).
    pub devices: Vec<Device>,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            id: String::new(),
            role: ChannelRole::Regular,
            audio_channels: 2,
            destination: None,
            blend_mode: None,
            volume: 1.0,
            pan: 0.0,
            muted: false,
            solo: false,
            sends: Vec::new(),
            devices: Vec::new(),
        }
    }
}

/// Role of a mixer channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelRole {
    Regular,
    Master,
    Effect,
    /// Sub-mix / group bus.
    Submix,
    /// VCA-style fader.
    Vca,
    /// Polyphonic voice channel.
    Voice,
}

impl ChannelRole {
    pub fn from_str(s: &str) -> Self {
        match s {
            "master" => Self::Master,
            "effect" => Self::Effect,
            "submix" => Self::Submix,
            "vca" => Self::Vca,
            "voice" => Self::Voice,
            _ => Self::Regular,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Regular => "regular",
            Self::Master => "master",
            Self::Effect => "effect",
            Self::Submix => "submix",
            Self::Vca => "vca",
            Self::Voice => "voice",
        }
    }
}

/// A send from one channel to another.
#[derive(Debug, Clone)]
pub struct Send {
    /// IDREF to the destination channel.
    pub destination: String,
    /// Send level (linear amplitude).
    pub volume: f64,
    /// Pan for this send (-1.0..1.0).
    pub pan: f64,
    /// Whether this send is active.
    pub enabled: bool,
    /// Whether this is a pre-fader send.
    pub pre_fader: bool,
}

// ─── Devices ─────────────────────────────────────────────────────────────────

/// A device (plugin or built-in effect) on a channel.
#[derive(Debug, Clone)]
pub struct Device {
    /// Device name.
    pub name: String,
    /// Device format (plugin type).
    pub format: DeviceFormat,
    /// Functional role of this device in the signal chain.
    pub device_role: Option<DeviceRole>,
    /// Format-specific plugin identifier (VST2 integer, VST3 GUID, CLAP domain-reverse ID).
    pub plugin_id: Option<String>,
    /// Plugin vendor / manufacturer name.
    pub vendor: Option<String>,
    /// Plugin version string.
    pub plugin_version: Option<String>,
    /// Additional device identifier (secondary ID used by some DAWs).
    pub device_id: Option<String>,
    /// Plugin file path (for external plugins).
    pub plugin_path: Option<PathBuf>,
    /// Whether the device is enabled (not bypassed).
    pub enabled: bool,
    /// Whether the device's content is currently loaded.
    pub loaded: bool,
    /// Structured parameters exposed by this device.
    pub parameters: Vec<DeviceParameter>,
    /// Structured content for built-in device types (EQ bands, compressor knobs, etc.).
    pub builtin_content: BuiltinDeviceContent,
    /// Raw state blob (base64 or file reference, if any).
    pub state: Option<DeviceState>,
}

/// The format/type of a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceFormat {
    Vst2,
    Vst3,
    Clap,
    Au,
    /// Generic built-in device (tag: `BuiltinDevice`).
    Builtin,
    /// Built-in equalizer (tag: `Equalizer`).
    Equalizer,
    /// Built-in compressor (tag: `Compressor`).
    Compressor,
    /// Built-in limiter (tag: `Limiter`).
    Limiter,
    /// Built-in noise gate (tag: `NoiseGate`).
    NoiseGate,
    Unknown,
}

/// The functional role of a device in the signal chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceRole {
    Instrument,
    NoteFx,
    AudioFx,
    Analyzer,
}

impl DeviceRole {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "instrument" => Some(Self::Instrument),
            "noteFX" => Some(Self::NoteFx),
            "audioFX" => Some(Self::AudioFx),
            "analyzer" => Some(Self::Analyzer),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Instrument => "instrument",
            Self::NoteFx => "noteFX",
            Self::AudioFx => "audioFX",
            Self::Analyzer => "analyzer",
        }
    }
}

/// A structured parameter value on a device.
#[derive(Debug, Clone)]
pub struct DeviceParameter {
    /// The `parameterID` attribute (links to automation targets).
    pub id: String,
    /// The parameter name or key.
    pub name: Option<String>,
    /// The typed value.
    pub value: DeviceParameterValue,
}

/// Typed value of a device parameter.
#[derive(Debug, Clone)]
pub enum DeviceParameterValue {
    /// Continuous real-valued parameter.
    Real {
        value: f64,
        min: Option<f64>,
        max: Option<f64>,
        unit: Option<AutomationUnit>,
    },
    /// Boolean on/off parameter.
    Bool(bool),
    /// Integer parameter.
    Integer {
        value: i64,
        min: Option<i64>,
        max: Option<i64>,
    },
    /// Enumeration parameter.
    Enum {
        value: u32,
        count: u32,
        labels: Vec<String>,
    },
    /// Time signature parameter.
    TimeSignature { numerator: u8, denominator: u8 },
}

/// Opaque device state blob.
#[derive(Debug, Clone)]
pub enum DeviceState {
    /// Inline base64-encoded state.
    Base64(String),
    /// Path to a state file inside the archive.
    File(String),
}

// ─── Built-in device structured content ──────────────────────────────────────

/// Structured content for built-in device types.
///
/// External plugins store opaque state; built-in devices expose named parameters
/// in structured form defined by the DawProject spec.
#[derive(Debug, Clone, Default)]
pub enum BuiltinDeviceContent {
    /// No structured content (external plugin or generic built-in).
    #[default]
    None,
    /// Parametric EQ with typed bands.
    Equalizer(Vec<EqBand>),
    /// Dynamics compressor.
    Compressor(CompressorParams),
    /// Brick-wall limiter.
    Limiter(LimiterParams),
    /// Noise gate.
    NoiseGate(NoiseGateParams),
}

/// A single band in a built-in Equalizer.
#[derive(Debug, Clone)]
pub struct EqBand {
    /// Band XML ID.
    pub id: String,
    /// Filter type.
    pub band_type: EqBandType,
    /// Filter order (slope steepness, e.g. 1 = 6 dB/oct, 2 = 12 dB/oct).
    pub order: Option<u32>,
    /// Center / corner frequency in Hz.
    pub freq: Option<f64>,
    /// Gain in dB (shelving/bell only).
    pub gain: Option<f64>,
    /// Q factor (bandwidth).
    pub q: Option<f64>,
    /// Whether this band is active.
    pub enabled: bool,
}

/// Filter type for an EQ band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqBandType {
    HighPass,
    LowPass,
    BandPass,
    HighShelf,
    LowShelf,
    Bell,
    Notch,
}

impl EqBandType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "highPass" => Some(Self::HighPass),
            "lowPass" => Some(Self::LowPass),
            "bandPass" => Some(Self::BandPass),
            "highShelf" => Some(Self::HighShelf),
            "lowShelf" => Some(Self::LowShelf),
            "bell" => Some(Self::Bell),
            "notch" => Some(Self::Notch),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::HighPass => "highPass",
            Self::LowPass => "lowPass",
            Self::BandPass => "bandPass",
            Self::HighShelf => "highShelf",
            Self::LowShelf => "lowShelf",
            Self::Bell => "bell",
            Self::Notch => "notch",
        }
    }
}

/// Structured parameters for the built-in Compressor device.
#[derive(Debug, Clone, Default)]
pub struct CompressorParams {
    pub threshold: Option<f64>,
    pub ratio: Option<f64>,
    pub attack: Option<f64>,
    pub release: Option<f64>,
    pub input_gain: Option<f64>,
    pub output_gain: Option<f64>,
    pub auto_makeup: Option<bool>,
}

/// Structured parameters for the built-in Limiter device.
#[derive(Debug, Clone, Default)]
pub struct LimiterParams {
    pub threshold: Option<f64>,
    pub attack: Option<f64>,
    pub release: Option<f64>,
    pub input_gain: Option<f64>,
    pub output_gain: Option<f64>,
}

/// Structured parameters for the built-in NoiseGate device.
#[derive(Debug, Clone, Default)]
pub struct NoiseGateParams {
    pub threshold: Option<f64>,
    pub range: Option<f64>,
    pub ratio: Option<f64>,
    pub attack: Option<f64>,
    pub release: Option<f64>,
}

// ─── Arrangement ─────────────────────────────────────────────────────────────

/// The main arrangement timeline.
#[derive(Debug, Clone)]
pub struct Arrangement {
    pub id: String,
    /// Optional display name.
    pub name: Option<String>,
    /// Optional color.
    pub color: Option<String>,
    /// Optional comment / annotation.
    pub comment: Option<String>,
    /// Default time unit for lanes that don't specify their own.
    pub time_unit: TimeUnit,
    /// Per-track content lanes.
    pub lanes: Vec<Lane>,
    /// Global arrangement markers (separate from per-track marker lanes).
    pub markers: Vec<Marker>,
    /// Tempo automation (BPM changes over time).
    pub tempo_automation: Vec<TempoPoint>,
    /// Time signature automation.
    pub time_sig_automation: Vec<TimeSignaturePoint>,
}

/// A tempo automation point.
#[derive(Debug, Clone, Copy)]
pub struct TempoPoint {
    pub time: f64,
    pub bpm: f64,
    pub interpolation: Interpolation,
}

/// A time signature automation point.
#[derive(Debug, Clone, Copy)]
pub struct TimeSignaturePoint {
    pub time: f64,
    pub numerator: u8,
    pub denominator: u8,
}

/// Time unit used for positioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    Beats,
    Seconds,
}

impl TimeUnit {
    pub fn from_str(s: &str) -> Self {
        match s {
            "seconds" => Self::Seconds,
            _ => Self::Beats,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Beats => "beats",
            Self::Seconds => "seconds",
        }
    }
}

/// A lane within the arrangement, associated with a track.
#[derive(Debug, Clone)]
pub struct Lane {
    pub id: String,
    /// IDREF of the track this lane belongs to.
    pub track: String,
    /// Time unit override for this lane (falls back to arrangement).
    pub time_unit: Option<TimeUnit>,
    /// Content stored in this lane.
    pub content: LaneContent,
}

/// Content stored in a lane.
#[derive(Debug, Clone)]
pub enum LaneContent {
    Clips(Vec<Clip>),
    Notes(Vec<Note>),
    Automation(AutomationPoints),
    Markers(Vec<Marker>),
}

/// A clip on the arrangement timeline.
#[derive(Debug, Clone)]
pub struct Clip {
    pub id: String,
    /// Timeline position.
    pub time: f64,
    /// Clip duration (absent = play to end of content).
    pub duration: f64,
    /// Time unit for `time` and `duration`.
    pub time_unit: Option<TimeUnit>,
    /// Time unit used inside the clip's content.
    pub content_time_unit: Option<TimeUnit>,
    /// Clip name.
    pub name: Option<String>,
    /// Clip color.
    pub color: Option<String>,
    /// Annotation / comment text.
    pub comment: Option<String>,
    /// Fade-in duration (in `fade_time_unit`).
    pub fade_in_time: Option<f64>,
    /// Fade-out duration (in `fade_time_unit`).
    pub fade_out_time: Option<f64>,
    /// Time unit for fade_in_time / fade_out_time (defaults to the clip's time unit).
    pub fade_time_unit: Option<TimeUnit>,
    /// Whether this clip is active (false = muted/disabled).
    pub enabled: bool,
    /// Playback start offset inside the clip's content.
    pub play_start: Option<f64>,
    /// Playback stop offset inside the clip's content.
    pub play_stop: Option<f64>,
    /// IDREF to another clip whose content this clip shares.
    pub reference: Option<String>,
    /// Loop parameters.
    pub loop_settings: Option<LoopSettings>,
    /// Clip content.
    pub content: ClipContent,
}

/// Loop settings for a clip.
#[derive(Debug, Clone, Copy)]
pub struct LoopSettings {
    pub loop_start: f64,
    pub loop_end: f64,
    pub play_start: f64,
}

/// Content inside a clip.
#[derive(Debug, Clone)]
pub enum ClipContent {
    Audio(AudioContent),
    Video(VideoContent),
    Notes(Vec<Note>),
    Empty,
}

/// A reference to a media file (path + embedded/external flag).
///
/// Corresponds to the `<File>` child element of `<Audio>` / `<Video>` in DawProject XML.
/// `external = false` means the file is embedded in the `.dawproject` ZIP archive;
/// `external = true` means the file lives outside the archive at an absolute or
/// project-relative path.
#[derive(Debug, Clone)]
pub struct FileReference {
    pub path: String,
    /// When `true` the file lives outside the ZIP archive.
    pub external: bool,
}

/// An audio file reference inside a clip.
#[derive(Debug, Clone)]
pub struct AudioContent {
    /// The file reference (`<File>` child element). `None` when no file is linked.
    pub file: Option<FileReference>,
    /// Sample rate of the audio file.
    pub sample_rate: Option<u32>,
    /// Number of channels.
    pub channels: Option<u32>,
    /// Duration in seconds (xs:double, required by spec but stored as Option for robustness).
    pub duration: Option<f64>,
    /// Time-stretching algorithm name.
    pub algorithm: Option<String>,
    /// Warp/time-stretch anchors.
    pub warps: Warps,
}

/// A video file reference inside a clip.
#[derive(Debug, Clone)]
pub struct VideoContent {
    pub file: Option<FileReference>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub duration: Option<f64>,
    pub algorithm: Option<String>,
    pub warps: Warps,
}

/// Warp anchor collection with its time unit declaration.
#[derive(Debug, Clone, Default)]
pub struct Warps {
    /// Time unit used for content-side timestamps inside each warp.
    pub content_time_unit: Option<TimeUnit>,
    pub warps: Vec<Warp>,
}

/// A warp anchor mapping content time to arrangement time.
#[derive(Debug, Clone, Copy)]
pub struct Warp {
    /// Time in the arrangement (beats or seconds).
    pub time: f64,
    /// Corresponding time in the content (beats or seconds).
    pub content_time: f64,
}

/// A MIDI note.
#[derive(Debug, Clone, Copy)]
pub struct Note {
    /// Time position relative to clip start.
    pub time: f64,
    /// Note duration.
    pub duration: f64,
    /// MIDI channel (0-15).
    pub channel: u8,
    /// MIDI note number (0-127).
    pub key: u8,
    /// Velocity (0.0-1.0).
    pub velocity: f64,
    /// Release velocity (0.0-1.0).
    pub release_velocity: Option<f64>,
}

// ─── Automation ──────────────────────────────────────────────────────────────

/// An automation curve targeting a specific parameter or expression.
///
/// Corresponds to the `<Points>` element in DawProject XML.
#[derive(Debug, Clone)]
pub struct AutomationPoints {
    pub id: String,
    /// What this automation targets.
    pub target: AutomationTarget,
    /// Physical unit of the values.
    pub unit: Option<AutomationUnit>,
    pub points: Vec<AutomationPoint>,
}

/// What an automation curve is targeting.
#[derive(Debug, Clone, Default)]
pub struct AutomationTarget {
    /// IDREF to a device parameter (mutually exclusive with `expression`).
    pub parameter: Option<String>,
    /// Per-note expression type (mutually exclusive with `parameter`).
    pub expression: Option<ExpressionType>,
    /// MIDI channel for expression targets (0-15).
    pub channel: Option<u8>,
    /// MIDI key for per-note expression targets (0-127).
    pub key: Option<u8>,
    /// MIDI CC number for `channelController` expression targets.
    pub controller: Option<u8>,
}

/// The type of per-note or channel expression being automated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpressionType {
    Gain,
    Pan,
    Transpose,
    Timbre,
    Formant,
    Pressure,
    ChannelController,
    ChannelPressure,
    PolyPressure,
    PitchBend,
    ProgramChange,
}

impl ExpressionType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "gain" => Some(Self::Gain),
            "pan" => Some(Self::Pan),
            "transpose" => Some(Self::Transpose),
            "timbre" => Some(Self::Timbre),
            "formant" => Some(Self::Formant),
            "pressure" => Some(Self::Pressure),
            "channelController" => Some(Self::ChannelController),
            "channelPressure" => Some(Self::ChannelPressure),
            "polyPressure" => Some(Self::PolyPressure),
            "pitchBend" => Some(Self::PitchBend),
            "programChange" => Some(Self::ProgramChange),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gain => "gain",
            Self::Pan => "pan",
            Self::Transpose => "transpose",
            Self::Timbre => "timbre",
            Self::Formant => "formant",
            Self::Pressure => "pressure",
            Self::ChannelController => "channelController",
            Self::ChannelPressure => "channelPressure",
            Self::PolyPressure => "polyPressure",
            Self::PitchBend => "pitchBend",
            Self::ProgramChange => "programChange",
        }
    }
}

/// Physical unit associated with an automation curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationUnit {
    Linear,
    Normalized,
    Percent,
    Decibel,
    Hertz,
    Semitones,
    Seconds,
    Beats,
    Bpm,
}

impl AutomationUnit {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "linear" => Some(Self::Linear),
            "normalized" => Some(Self::Normalized),
            "percent" => Some(Self::Percent),
            "decibel" => Some(Self::Decibel),
            "hertz" => Some(Self::Hertz),
            "semitones" => Some(Self::Semitones),
            "seconds" => Some(Self::Seconds),
            "beats" => Some(Self::Beats),
            "bpm" => Some(Self::Bpm),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Normalized => "normalized",
            Self::Percent => "percent",
            Self::Decibel => "decibel",
            Self::Hertz => "hertz",
            Self::Semitones => "semitones",
            Self::Seconds => "seconds",
            Self::Beats => "beats",
            Self::Bpm => "bpm",
        }
    }
}

/// A single automation point.
#[derive(Debug, Clone, Copy)]
pub struct AutomationPoint {
    pub time: f64,
    pub value: f64,
    pub interpolation: Interpolation,
}

/// Interpolation mode between automation points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Hold,
    Linear,
    /// Smooth cubic spline interpolation.
    Cubic,
    /// Instant jump to the next value (step/staircase).
    Jump,
}

impl Interpolation {
    pub fn from_str(s: &str) -> Self {
        match s {
            "linear" => Self::Linear,
            "cubic" => Self::Cubic,
            "jump" => Self::Jump,
            _ => Self::Hold,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Hold => "hold",
            Self::Cubic => "cubic",
            Self::Jump => "jump",
        }
    }
}

// ─── Scenes ──────────────────────────────────────────────────────────────────

/// A scene in the clip launcher.
///
/// Per the DawProject spec a `<Scene>` directly contains a single piece of timeline
/// content (typically `<Lanes>` for a multi-track scene row, or `<ClipSlot>` for a
/// single slot).
#[derive(Debug, Clone)]
pub struct Scene {
    pub id: String,
    pub name: Option<String>,
    pub color: Option<String>,
    pub comment: Option<String>,
    /// Timeline content for this scene (usually Lanes containing ClipSlots).
    pub content: Option<SceneContent>,
}

/// Content held by a scene.
#[derive(Debug, Clone)]
pub enum SceneContent {
    /// Multiple per-track lanes (the typical clip-launcher layout).
    Lanes(Vec<Lane>),
    /// A single clip slot.
    Slot(ClipSlot),
    /// Bare clip list.
    Clips(Vec<Clip>),
    /// MIDI notes.
    Notes(Vec<Note>),
    /// Markers.
    Markers(Vec<Marker>),
}

/// A clip slot in the clip launcher.
#[derive(Debug, Clone)]
pub struct ClipSlot {
    pub id: String,
    pub has_stop: bool,
    /// Timeline position for sync-grid placement.
    pub time: Option<f64>,
    /// Duration for sync-grid placement.
    pub duration: Option<f64>,
    pub clip: Option<Clip>,
}

// ─── Markers ─────────────────────────────────────────────────────────────────

/// A timeline marker.
#[derive(Debug, Clone)]
pub struct Marker {
    pub time: f64,
    pub name: String,
    pub color: Option<String>,
    pub comment: Option<String>,
}
