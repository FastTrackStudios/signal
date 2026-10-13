//! Media item data structures and parsing for REAPER

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::primitives::Token;

/// Fade curve types for items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum FadeCurveType {
    Linear = 0,
    Square = 1,
    SlowStartEnd = 2,
    FastStart = 3,
    FastEnd = 4,
    Bezier = 5,
    Unknown(i32),
}

impl From<i32> for FadeCurveType {
    fn from(value: i32) -> Self {
        match value {
            0 => FadeCurveType::Linear,
            1 => FadeCurveType::Square,
            2 => FadeCurveType::SlowStartEnd,
            3 => FadeCurveType::FastStart,
            4 => FadeCurveType::FastEnd,
            5 => FadeCurveType::Bezier,
            _ => FadeCurveType::Unknown(value),
        }
    }
}

impl fmt::Display for FadeCurveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FadeCurveType::Linear => write!(f, "Linear"),
            FadeCurveType::Square => write!(f, "Square"),
            FadeCurveType::SlowStartEnd => write!(f, "Slow Start/End"),
            FadeCurveType::FastStart => write!(f, "Fast Start"),
            FadeCurveType::FastEnd => write!(f, "Fast End"),
            FadeCurveType::Bezier => write!(f, "Bezier"),
            FadeCurveType::Unknown(val) => write!(f, "Unknown({})", val),
        }
    }
}

/// Channel mode for takes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum ChannelMode {
    Normal = 0,
    ReverseStereo = 1,
    MonoDownmix = 2,
    MonoLeft = 3,
    MonoRight = 4,
    MonoChannel(u8),  // 5-194 for mono channels 3-128
    StereChannel(u8), // 67-257 for stereo channels 1-128
    Unknown(i32),
}

impl From<i32> for ChannelMode {
    fn from(value: i32) -> Self {
        match value {
            0 => ChannelMode::Normal,
            1 => ChannelMode::ReverseStereo,
            2 => ChannelMode::MonoDownmix,
            3 => ChannelMode::MonoLeft,
            4 => ChannelMode::MonoRight,
            5..=66 => ChannelMode::MonoChannel((value - 2) as u8), // 5-66 -> 3-64
            67..=130 => ChannelMode::StereChannel((value - 66) as u8), // 67-130 -> 1-64
            131..=194 => ChannelMode::MonoChannel((value - 66) as u8), // 131-194 -> 65-128
            195..=257 => ChannelMode::StereChannel((value - 128) as u8), // 195-257 -> 67-128
            _ => ChannelMode::Unknown(value),
        }
    }
}

impl fmt::Display for ChannelMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChannelMode::Normal => write!(f, "Normal"),
            ChannelMode::ReverseStereo => write!(f, "Reverse Stereo"),
            ChannelMode::MonoDownmix => write!(f, "Mono (Downmix)"),
            ChannelMode::MonoLeft => write!(f, "Mono (Left)"),
            ChannelMode::MonoRight => write!(f, "Mono (Right)"),
            ChannelMode::MonoChannel(ch) => write!(f, "Mono (Channel {})", ch),
            ChannelMode::StereChannel(ch) => write!(f, "Stereo (Channels {}/{})", ch, ch + 1),
            ChannelMode::Unknown(val) => write!(f, "Unknown({})", val),
        }
    }
}

/// Pitch shifting and time stretch modes for PLAYRATE field 4
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum PitchMode {
    ProjectDefault = -1,
    SoundTouchPreset1 = 0,
    SoundTouchPreset2 = 1,
    SoundTouchPreset3 = 2,
    DiracLE(u8),              // 0-31 presets (65536-65567)
    LowQualityWindowed(u8),   // 0-47 presets (131072-131119)
    ElastiquePro(u8),         // 0-31 presets (196608-196639)
    ElastiqueEfficient(u8),   // 0-3 presets (262144-262147)
    ElastiqueSoloist(u8),     // 0-3 presets (327680-327683)
    Elastique21Pro(u8),       // 0-31 presets (393216-393247)
    Elastique21Efficient(u8), // 0-3 presets (458752-458755)
    Elastique21Soloist(u8),   // 0-3 presets (524288-524291)
    Unknown(i32),
}

impl From<i32> for PitchMode {
    fn from(value: i32) -> Self {
        match value {
            -1 => PitchMode::ProjectDefault,
            0 => PitchMode::SoundTouchPreset1,
            1 => PitchMode::SoundTouchPreset2,
            2 => PitchMode::SoundTouchPreset3,
            65536..=65567 => PitchMode::DiracLE((value - 65536) as u8),
            131072..=131119 => PitchMode::LowQualityWindowed((value - 131072) as u8),
            196608..=196639 => PitchMode::ElastiquePro((value - 196608) as u8),
            262144..=262147 => PitchMode::ElastiqueEfficient((value - 262144) as u8),
            327680..=327683 => PitchMode::ElastiqueSoloist((value - 327680) as u8),
            393216..=393247 => PitchMode::Elastique21Pro((value - 393216) as u8),
            458752..=458755 => PitchMode::Elastique21Efficient((value - 458752) as u8),
            524288..=524291 => PitchMode::Elastique21Soloist((value - 524288) as u8),
            _ => PitchMode::Unknown(value),
        }
    }
}

impl fmt::Display for PitchMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PitchMode::ProjectDefault => write!(f, "Project Default"),
            PitchMode::SoundTouchPreset1 => write!(f, "Sound Touch (Preset 1)"),
            PitchMode::SoundTouchPreset2 => write!(f, "Sound Touch (Preset 2)"),
            PitchMode::SoundTouchPreset3 => write!(f, "Sound Touch (Preset 3)"),
            PitchMode::DiracLE(preset) => write!(f, "Dirac LE (Preset {})", preset + 1),
            PitchMode::LowQualityWindowed(preset) => {
                write!(f, "Low Quality Windowed (Preset {})", preset + 1)
            }
            PitchMode::ElastiquePro(preset) => write!(f, "élastique Pro (Preset {})", preset + 1),
            PitchMode::ElastiqueEfficient(preset) => {
                write!(f, "élastique Efficient (Preset {})", preset + 1)
            }
            PitchMode::ElastiqueSoloist(preset) => {
                write!(f, "élastique SOLOIST (Preset {})", preset + 1)
            }
            PitchMode::Elastique21Pro(preset) => {
                write!(f, "élastique 2.1 Pro (Preset {})", preset + 1)
            }
            PitchMode::Elastique21Efficient(preset) => {
                write!(f, "élastique 2.1 Efficient (Preset {})", preset + 1)
            }
            PitchMode::Elastique21Soloist(preset) => {
                write!(f, "élastique 2.1 SOLOIST (Preset {})", preset + 1)
            }
            PitchMode::Unknown(val) => write!(f, "Unknown({})", val),
        }
    }
}

/// Solo states for MUTE field 2
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum SoloState {
    NotSoloed = 0,
    Soloed = -1,
    SoloOverridden = 1,
    Unknown(i32),
}

impl From<i32> for SoloState {
    fn from(value: i32) -> Self {
        match value {
            0 => SoloState::NotSoloed,
            -1 => SoloState::Soloed,
            1 => SoloState::SoloOverridden,
            _ => SoloState::Unknown(value),
        }
    }
}

impl fmt::Display for SoloState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SoloState::NotSoloed => write!(f, "Not Soloed"),
            SoloState::Soloed => write!(f, "Soloed"),
            SoloState::SoloOverridden => write!(f, "Solo Overridden"),
            SoloState::Unknown(val) => write!(f, "Unknown({})", val),
        }
    }
}

/// Source types for SOURCE blocks
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceType {
    Wave,
    Mp3,
    Midi,
    Video,
    Section,
    Empty,
    Flac,
    Vorbis,
    OfflineWave,
    Unknown(String),
}

impl From<&str> for SourceType {
    fn from(value: &str) -> Self {
        match value.to_uppercase().as_str() {
            "WAVE" => SourceType::Wave,
            "MP3" => SourceType::Mp3,
            "MIDI" => SourceType::Midi,
            "VIDEO" => SourceType::Video,
            "SECTION" => SourceType::Section,
            "EMPTY" => SourceType::Empty,
            "FLAC" => SourceType::Flac,
            "VORBIS" => SourceType::Vorbis,
            "_OFFLINE_WAVE" => SourceType::OfflineWave,
            _ => SourceType::Unknown(value.to_string()),
        }
    }
}

impl fmt::Display for SourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceType::Wave => write!(f, "Wave"),
            SourceType::Mp3 => write!(f, "MP3"),
            SourceType::Midi => write!(f, "MIDI"),
            SourceType::Video => write!(f, "Video"),
            SourceType::Section => write!(f, "Section"),
            SourceType::Empty => write!(f, "Empty"),
            SourceType::Flac => write!(f, "FLAC"),
            SourceType::Vorbis => write!(f, "Vorbis"),
            SourceType::OfflineWave => write!(f, "Offline Wave"),
            SourceType::Unknown(val) => write!(f, "Unknown({})", val),
        }
    }
}

/// Item timebase for BEAT field
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum ItemTimebase {
    ProjectDefault = -1,
    Time = 0,
    Beats = 1,
    Unknown(i32),
}

impl From<i32> for ItemTimebase {
    fn from(value: i32) -> Self {
        match value {
            -1 => ItemTimebase::ProjectDefault,
            0 => ItemTimebase::Time,
            1 => ItemTimebase::Beats,
            _ => ItemTimebase::Unknown(value),
        }
    }
}

impl fmt::Display for ItemTimebase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemTimebase::ProjectDefault => write!(f, "Project Default"),
            ItemTimebase::Time => write!(f, "Time"),
            ItemTimebase::Beats => write!(f, "Beats"),
            ItemTimebase::Unknown(val) => write!(f, "Unknown({})", val),
        }
    }
}

/// A REAPER media item
/// Item vertical geometry within its track (`YPOS`): fractions of the
/// track height. REAPER writes this for free item positioning and for
/// fixed-lane membership (lane = `y / height` when lanes are uniform).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ItemYPos {
    pub y: f64,
    pub height: f64,
    pub mode: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    // Basic item properties
    pub position: f64,              // POSITION - Position on timeline in seconds
    pub snap_offset: f64,           // SNAPOFFS - Snap offset in seconds
    pub length: f64,                // LENGTH - Item length in seconds
    pub loop_source: bool,          // LOOP - Loop source flag
    pub play_all_takes: bool,       // ALLTAKES - Play all takes flag
    pub color: Option<i32>,         // COLOR - Item color (optional)
    pub beat: Option<ItemTimebase>, // BEAT - Item timebase (optional)
    pub selected: bool,             // SEL - Is item selected

    // Fade settings
    pub fade_in: Option<FadeSettings>,  // FADEIN - Fade in settings
    pub fade_out: Option<FadeSettings>, // FADEOUT - Fade out settings

    // Mute/Solo settings
    pub mute: Option<MuteSettings>, // MUTE - Mute and solo settings

    // Item identification
    pub item_guid: Option<String>, // IGUID - Item GUID
    pub item_id: Option<i32>,      // IID - Item ordinal number (deprecated)

    // Item properties
    pub name: String,                       // NAME - Item name
    pub volpan: Option<VolPanSettings>,     // VOLPAN - Volume and pan settings
    pub slip_offset: f64,                   // SOFFS - Slip offset in seconds
    pub playrate: Option<PlayRateSettings>, // PLAYRATE - Play rate settings
    pub channel_mode: ChannelMode,          // CHANMODE - Channel mode
    pub take_guid: Option<String>,          // GUID - Take GUID
    pub rec_pass: Option<i32>,              // RECPASS - Recording pass number
    /// Fixed item lane index (`LANE`, or derived from `YPOS` on
    /// fixed-lanes tracks), REAPER 7+ comping. `None` for ordinary items.
    pub lane: Option<i32>,
    /// Raw vertical geometry (`YPOS y height [mode]`) — free item
    /// positioning and fixed-lane placement.
    pub y_pos: Option<ItemYPos>,

    // Takes
    pub takes: Vec<Take>,

    // Stretch markers
    pub stretch_markers: Vec<StretchMarker>,

    // Raw content for preservation
    pub raw_content: String,
}

impl Default for Item {
    fn default() -> Self {
        Self {
            position: 0.0,
            snap_offset: 0.0,
            length: 0.0,
            loop_source: false,
            play_all_takes: false,
            color: None,
            beat: None,
            selected: false,
            fade_in: None,
            fade_out: None,
            mute: None,
            item_guid: None,
            item_id: None,
            name: String::new(),
            volpan: None,
            slip_offset: 0.0,
            playrate: None,
            channel_mode: ChannelMode::Normal,
            take_guid: None,
            rec_pass: None,
            lane: None,
            y_pos: None,
            takes: vec![],
            stretch_markers: vec![],
            raw_content: String::new(),
        }
    }
}

/// Fade settings for an item
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FadeSettings {
    pub curve_type: FadeCurveType, // field 1 - fade curve type
    pub time: f64,                 // field 2 - fade time in seconds
    pub unknown_field_3: f64,      // field 3 - unknown
    pub unknown_field_4: i32,      // field 4 - unknown
    pub unknown_field_5: i32,      // field 5 - unknown
    pub unknown_field_6: i32,      // field 6 - unknown
    pub unknown_field_7: i32,      // field 7 - unknown
}

/// Mute and solo settings for an item
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MuteSettings {
    pub muted: bool,           // field 1 - item is muted
    pub solo_state: SoloState, // field 2 - solo state (-1, 0, 1)
}

/// Volume and pan settings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VolPanSettings {
    pub item_trim: f64,    // field 1 - item trim (1.0 = 0 dB)
    pub take_pan: f64,     // field 2 - take pan (-1.0 to 1.0)
    pub take_volume: f64,  // field 3 - take volume
    pub take_pan_law: f64, // field 4 - take pan law
}

/// Play rate settings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayRateSettings {
    pub rate: f64,             // field 1 - play rate
    pub preserve_pitch: bool,  // field 2 - preserve pitch while changing rate
    pub pitch_adjust: f64,     // field 3 - pitch adjust in semitones.cents
    pub pitch_mode: PitchMode, // field 4 - pitch shifting/time stretch mode
    pub unknown_field_5: i32,  // field 5 - unknown
    pub unknown_field_6: f64,  // field 6 - unknown
}

/// A take within a media item
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Take {
    pub is_selected: bool,                  // TAKE SEL - Is this take selected
    pub name: String,                       // NAME - Take name
    pub volpan: Option<VolPanSettings>,     // TAKEVOLPAN - Take volume and pan
    pub slip_offset: f64,                   // SOFFS - Take slip offset
    pub playrate: Option<PlayRateSettings>, // PLAYRATE - Take play rate
    pub channel_mode: ChannelMode,          // CHANMODE - Take channel mode
    pub take_color: Option<i32>,            // TAKECOLOR - Take color
    pub take_guid: Option<String>,          // GUID - Take GUID
    pub rec_pass: Option<i32>,              // RECPASS - Recording pass number
    pub source: Option<SourceBlock>,        // SOURCE block
    /// `SM` lines read under this take. REAPER keys stretch markers per
    /// take; the first take's markers are also mirrored on
    /// `Item::stretch_markers` for callers that predate this field.
    pub stretch_markers: Vec<StretchMarker>,
    /// Nested blocks under this take that are not `<SOURCE>` — chiefly
    /// `<EXT>` (REAPER's per-take extension store, e.g.
    /// `ORIGINAL_FILENAME`). Kept verbatim so a round trip does not drop
    /// them; the serializer re-indents and writes them back after the
    /// source block.
    pub extra_blocks: Vec<String>,
}

impl Default for Take {
    fn default() -> Self {
        Self {
            is_selected: false,
            name: String::new(),
            volpan: None,
            slip_offset: 0.0,
            playrate: None,
            channel_mode: ChannelMode::Normal,
            take_color: None,
            take_guid: None,
            rec_pass: None,
            source: None,
            stretch_markers: Vec::new(),
            extra_blocks: Vec::new(),
        }
    }
}

/// Source block for a take
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceBlock {
    pub source_type: SourceType,       // WAVE, MIDI, etc.
    pub file_path: String,             // FILE - Source file path
    pub midi_data: Option<MidiSource>, // Parsed MIDI data (if source is MIDI)
    pub raw_content: String,           // Raw content for preservation
}

/// Parsed MIDI source data from a `<SOURCE MIDI>` block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiSource {
    /// Whether this source reports any data (`HASDATA` field 1)
    pub has_data: bool,
    /// Ticks per quarter note (from HASDATA line, e.g. 960)
    pub ticks_per_qn: u32,
    /// Timebase label from HASDATA (typically `QN`)
    pub ticks_timebase: Option<String>,
    /// CC interpolation mode (`CCINTERP`)
    pub cc_interp: Option<i32>,
    /// Pooled events GUID (from POOLEDEVTS line)
    pub pooled_evts_guid: Option<String>,
    /// MIDI events
    pub events: Vec<MidiEvent>,
    /// Extended `X/x` event blocks (base64 payload blocks)
    pub extended_events: Vec<MidiExtendedEvent>,
    /// MIDI source events in original file order, preserving timing deltas from both `E` and `<X>` lines.
    pub event_stream: Vec<MidiSourceEvent>,
    /// Ignore project tempo override (`IGNTEMPO`)
    pub ignore_tempo: Option<MidiIgnoreTempo>,
    /// MIDI editor velocity/CC lanes (`VELLANE`)
    pub vel_lanes: Vec<MidiVelLane>,
    /// Optional ReaBank path (`BANKPROGRAMFILE`)
    pub bank_program_file: Option<String>,
    /// Raw CFGEDITVIEW fields (`CFGEDITVIEW`)
    pub cfg_edit_view: Option<Vec<String>>,
    /// Raw CFGEDIT fields (`CFGEDIT`)
    pub cfg_edit: Option<Vec<String>>,
    /// Raw EVTFILTER fields (`EVTFILTER`)
    pub evt_filter: Option<Vec<String>>,
    /// Source GUID
    pub guid: Option<String>,
    /// Unrecognized lines preserved for forward-compatibility.
    pub unknown_lines: Vec<String>,
}

/// A single MIDI event from an RPP `E` line.
///
/// Format: `E <delta_ticks> <status_hex> <data1_hex> [data2_hex] [...]`
/// The status byte determines the event type:
/// - `8x` = Note Off, `9x` = Note On, `Ax` = Aftertouch
/// - `Bx` = Control Change, `Cx` = Program Change, `Dx` = Channel Pressure
/// - `Ex` = Pitch Bend, `Fx` = System (SysEx, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiEvent {
    /// Delta time in ticks from previous event
    pub delta_ticks: u32,
    /// Raw MIDI bytes (status + data, parsed from hex)
    pub bytes: Vec<u8>,
}

/// Extended MIDI event block (`<X ...>` / `<x ...>`) with base64 payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiExtendedEvent {
    /// `true` if lowercase `<x ...>` (selected), `false` for uppercase `<X ...>`.
    pub selected: bool,
    /// Header fields following `X/x`.
    pub fields: Vec<String>,
    /// Base64 payload lines contained by this block.
    pub data_lines: Vec<String>,
}

/// A MIDI source event preserving the original interleaving of standard `E` and extended `<X>` events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MidiSourceEvent {
    Midi(MidiEvent),
    Extended(MidiExtendedEvent),
}

/// `IGNTEMPO` override for MIDI sources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiIgnoreTempo {
    pub enabled: bool,
    pub bpm: f64,
    pub numerator: i32,
    pub denominator: i32,
}

/// `VELLANE` row from a MIDI source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiVelLane {
    pub lane_type: i32,
    pub midi_editor_height: i32,
    pub inline_editor_height: i32,
}

impl MidiEvent {
    /// Get the MIDI status byte (first byte).
    pub fn status(&self) -> u8 {
        self.bytes.first().copied().unwrap_or(0)
    }

    /// Get the event type (upper nibble of status byte).
    pub fn event_type(&self) -> MidiEventType {
        match self.status() >> 4 {
            0x8 => MidiEventType::NoteOff,
            0x9 => {
                // Note On with velocity 0 is actually Note Off
                if self.bytes.get(2).copied().unwrap_or(0) == 0 {
                    MidiEventType::NoteOff
                } else {
                    MidiEventType::NoteOn
                }
            }
            0xA => MidiEventType::Aftertouch,
            0xB => MidiEventType::ControlChange,
            0xC => MidiEventType::ProgramChange,
            0xD => MidiEventType::ChannelPressure,
            0xE => MidiEventType::PitchBend,
            0xF => MidiEventType::System,
            _ => MidiEventType::Unknown,
        }
    }

    /// Get the MIDI channel (0-15, from lower nibble of status byte).
    pub fn channel(&self) -> u8 {
        self.status() & 0x0F
    }
}

impl MidiExtendedEvent {
    /// Delta time in ticks from the previous source event.
    pub fn delta_ticks(&self) -> u32 {
        self.fields
            .first()
            .and_then(|field| field.parse::<u32>().ok())
            .unwrap_or(0)
    }
}

impl MidiSourceEvent {
    /// Delta time in ticks from the previous source event.
    pub fn delta_ticks(&self) -> u32 {
        match self {
            Self::Midi(event) => event.delta_ticks,
            Self::Extended(event) => event.delta_ticks(),
        }
    }
}

/// MIDI event type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MidiEventType {
    NoteOn,
    NoteOff,
    Aftertouch,
    ControlChange,
    ProgramChange,
    ChannelPressure,
    PitchBend,
    System,
    Unknown,
}

/// A stretch marker within a media item.
///
/// From `SM` lines: `SM <position> <source_position> [<rate>]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StretchMarker {
    /// Position within the item (in seconds)
    pub position: f64,
    /// Source position (in seconds)
    pub source_position: f64,
    /// Stretch rate at this marker (optional)
    pub rate: Option<f64>,
}

/// Whether an accumulated take holds anything — a source, a name or a GUID.
///
/// Distinguishes "the item began with an inline take" from "the parser had an
/// empty accumulator when the first `TAKE` marker arrived".
fn take_has_content(take: &Take) -> bool {
    take.source.is_some() || !take.name.is_empty() || take.take_guid.is_some()
}

impl Item {
    fn should_parse_item_raw_line(raw: &str) -> bool {
        let trimmed = raw.trim_start();
        let Some(first) = trimmed.split_whitespace().next() else {
            return false;
        };
        matches!(
            first,
            "POSITION"
                | "SNAPOFFS"
                | "LENGTH"
                | "LOOP"
                | "ALLTAKES"
                | "COLOR"
                | "BEAT"
                | "SEL"
                | "FADEIN"
                | "FADEOUT"
                | "MUTE"
                | "IGUID"
                | "IID"
                | "NAME"
                | "VOLPAN"
                | "SOFFS"
                | "PLAYRATE"
                | "CHANMODE"
                | "GUID"
                | "RECPASS"
                | "LANE"
                | "YPOS"
                | "TAKE"
                | "TAKEVOLPAN"
                | "TAKECOLOR"
                | "SM"
        )
    }

    fn fast_classify_item_token(raw: &str) -> Token {
        if let Some(hex) = raw.strip_prefix("0x")
            && let Ok(v) = u64::from_str_radix(hex, 16)
        {
            return Token::HexInteger(v);
        }

        if let Ok(v) = raw.parse::<i64>() {
            return Token::Integer(v);
        }
        if let Ok(v) = raw.parse::<f64>() {
            return Token::Float(v);
        }
        if raw.contains(',') {
            let normalized = raw.replace(',', ".");
            if let Ok(v) = normalized.parse::<f64>() {
                return Token::Float(v);
            }
        }
        Token::Identifier(raw.to_string())
    }

    fn parse_item_token_line(line: &str) -> Result<Vec<Token>, String> {
        if line.contains('"')
            || line.contains('\'')
            || line.contains('`')
            || line.contains('#')
            || line.contains(';')
        {
            return crate::primitives::token::parse_token_line(line)
                .map(|(_, tokens)| tokens)
                .map_err(|e| format!("{e:?}"));
        }

        let mut parts = line.split_whitespace();
        let Some(first) = parts.next() else {
            return Ok(Vec::new());
        };
        let mut out = Vec::with_capacity(8);
        out.push(Self::fast_classify_item_token(first));
        out.extend(parts.map(Self::fast_classify_item_token));
        Ok(out)
    }

    fn default_take() -> Take {
        Take {
            is_selected: false,
            name: String::new(),
            volpan: None,
            slip_offset: 0.0,
            playrate: None,
            channel_mode: ChannelMode::Normal,
            take_color: None,
            take_guid: None,
            rec_pass: None,
            source: None,
            stretch_markers: Vec::new(),
            extra_blocks: Vec::new(),
        }
    }

    fn apply_item_tokens(
        item: &mut Item,
        tokens: &[Token],
        current_take: &mut Option<Take>,
        in_take_context: &mut bool,
    ) -> Result<(), String> {
        if tokens.is_empty() {
            return Ok(());
        }
        let identifier = match &tokens[0] {
            Token::Identifier(id) => id.as_str(),
            _ => return Ok(()),
        };

        match identifier {
            "POSITION" if tokens.len() > 1 => {
                item.position = Self::parse_float(&tokens[1])?;
            }
            "SNAPOFFS" if tokens.len() > 1 => {
                item.snap_offset = Self::parse_float(&tokens[1])?;
            }
            "LENGTH" if tokens.len() > 1 => {
                item.length = Self::parse_float(&tokens[1])?;
            }
            "LOOP" if tokens.len() > 1 => {
                item.loop_source = Self::parse_bool(&tokens[1])?;
            }
            "ALLTAKES" if tokens.len() > 1 => {
                item.play_all_takes = Self::parse_bool(&tokens[1])?;
            }
            "COLOR" if tokens.len() > 1 => {
                item.color = Some(Self::parse_int(&tokens[1])?);
            }
            "BEAT" if tokens.len() > 1 => {
                item.beat = Some(ItemTimebase::from(Self::parse_int(&tokens[1])?));
            }
            "SEL" if tokens.len() > 1 => {
                item.selected = Self::parse_bool(&tokens[1])?;
            }
            "FADEIN" if tokens.len() >= 3 => {
                item.fade_in = Some(FadeSettings {
                    curve_type: FadeCurveType::from(Self::parse_int(&tokens[1])?),
                    time: Self::parse_float(&tokens[2])?,
                    unknown_field_3: if tokens.len() > 3 {
                        Self::parse_float(&tokens[3])?
                    } else {
                        0.0
                    },
                    unknown_field_4: if tokens.len() > 4 {
                        Self::parse_int(&tokens[4])?
                    } else {
                        0
                    },
                    unknown_field_5: if tokens.len() > 5 {
                        Self::parse_int(&tokens[5])?
                    } else {
                        0
                    },
                    unknown_field_6: if tokens.len() > 6 {
                        Self::parse_int(&tokens[6])?
                    } else {
                        0
                    },
                    unknown_field_7: if tokens.len() > 7 {
                        Self::parse_int(&tokens[7])?
                    } else {
                        0
                    },
                });
            }
            "FADEOUT" if tokens.len() >= 3 => {
                item.fade_out = Some(FadeSettings {
                    curve_type: FadeCurveType::from(Self::parse_int(&tokens[1])?),
                    time: Self::parse_float(&tokens[2])?,
                    unknown_field_3: if tokens.len() > 3 {
                        Self::parse_float(&tokens[3])?
                    } else {
                        0.0
                    },
                    unknown_field_4: if tokens.len() > 4 {
                        Self::parse_int(&tokens[4])?
                    } else {
                        0
                    },
                    unknown_field_5: if tokens.len() > 5 {
                        Self::parse_int(&tokens[5])?
                    } else {
                        0
                    },
                    unknown_field_6: if tokens.len() > 6 {
                        Self::parse_int(&tokens[6])?
                    } else {
                        0
                    },
                    unknown_field_7: if tokens.len() > 7 {
                        Self::parse_int(&tokens[7])?
                    } else {
                        0
                    },
                });
            }
            "MUTE" if tokens.len() >= 3 => {
                item.mute = Some(MuteSettings {
                    muted: Self::parse_bool(&tokens[1])?,
                    solo_state: SoloState::from(Self::parse_int(&tokens[2])?),
                });
            }
            "IGUID" if tokens.len() > 1 => {
                item.item_guid = Some(Self::parse_string(&tokens[1])?);
            }
            "IID" if tokens.len() > 1 => {
                item.item_id = Some(Self::parse_int(&tokens[1])?);
            }
            // REAPER writes the FIRST take's attributes at item level (the
            // implicit take before any `TAKE` statement); later takes carry
            // theirs after their `TAKE` line. These arms therefore route to
            // the current take always, and mirror onto the item fields
            // outside take context (compatibility with existing readers).
            "NAME" if tokens.len() > 1 => {
                let name = Self::parse_string(&tokens[1])?;
                if !*in_take_context {
                    item.name = name.clone();
                }
                if let Some(take) = current_take {
                    take.name = name;
                }
            }
            "VOLPAN" if tokens.len() >= 5 => {
                let vp = VolPanSettings {
                    item_trim: Self::parse_float(&tokens[1])?,
                    take_pan: Self::parse_float(&tokens[2])?,
                    take_volume: Self::parse_float(&tokens[3])?,
                    take_pan_law: Self::parse_float(&tokens[4])?,
                };
                if !*in_take_context {
                    item.volpan = Some(vp.clone());
                }
                if let Some(take) = current_take {
                    take.volpan = Some(vp);
                }
            }
            "SOFFS" if tokens.len() > 1 => {
                let v = Self::parse_float(&tokens[1])?;
                if !*in_take_context {
                    item.slip_offset = v;
                }
                if let Some(take) = current_take {
                    take.slip_offset = v;
                }
            }
            "PLAYRATE" if tokens.len() >= 4 => {
                let pr = PlayRateSettings {
                    rate: Self::parse_float(&tokens[1])?,
                    preserve_pitch: Self::parse_bool(&tokens[2])?,
                    pitch_adjust: Self::parse_float(&tokens[3])?,
                    pitch_mode: PitchMode::from(Self::parse_int(&tokens[4])?),
                    unknown_field_5: if tokens.len() > 5 {
                        Self::parse_int(&tokens[5])?
                    } else {
                        0
                    },
                    unknown_field_6: if tokens.len() > 6 {
                        Self::parse_float(&tokens[6])?
                    } else {
                        0.0
                    },
                };
                if !*in_take_context {
                    item.playrate = Some(pr.clone());
                }
                if let Some(take) = current_take {
                    take.playrate = Some(pr);
                }
            }
            "CHANMODE" if tokens.len() > 1 => {
                let cm = ChannelMode::from(Self::parse_int(&tokens[1])?);
                if !*in_take_context {
                    item.channel_mode = cm;
                }
                if let Some(take) = current_take {
                    take.channel_mode = cm;
                }
            }
            "GUID" if tokens.len() > 1 => {
                let guid = Self::parse_string(&tokens[1])?;
                if *in_take_context {
                    if let Some(take) = current_take {
                        take.take_guid = Some(guid);
                    }
                } else {
                    item.take_guid = Some(guid.clone());
                    if let Some(take) = current_take {
                        take.take_guid = Some(guid);
                    }
                }
            }
            "LANE" if tokens.len() >= 2 => {
                item.lane = Some(Self::parse_int(&tokens[1])?);
            }
            "YPOS" if tokens.len() >= 3 => {
                // Free-positioning / fixed-lane geometry:
                // `YPOS <y> <height> [mode]`, both 0..1 fractions of
                // the track height. On a fixed-lanes track each lane
                // is `1/lane_count` tall, so `lane = round(y/height)`.
                let y = Self::parse_float(&tokens[1])?;
                let height = Self::parse_float(&tokens[2])?;
                item.y_pos = Some(ItemYPos {
                    y,
                    height,
                    mode: tokens.get(3).map(Self::parse_int).transpose()?.unwrap_or(0),
                });
                // Derive the fixed-lane index when not explicitly set
                // via a LANE line.
                if item.lane.is_none() && height > 1e-9 {
                    item.lane = Some((y / height).round() as i32);
                }
            }
            "RECPASS" if tokens.len() > 1 => {
                let rec_pass = Self::parse_int(&tokens[1])?;
                if *in_take_context {
                    if let Some(take) = current_take {
                        take.rec_pass = Some(rec_pass);
                    }
                } else {
                    item.rec_pass = Some(rec_pass);
                }
            }
            "TAKE" => {
                // `TAKE NULL` is an empty comp-lane slot. It is still a take
                // as far as REAPER is concerned — it occupies an index — so
                // it is kept, not skipped: dropping it would silently
                // re-number every take on a comped item.
                let is_selected =
                    matches!(tokens.get(1), Some(Token::Identifier(flag)) if flag == "SEL");
                if let Some(take) = current_take.take() {
                    // An item whose FIRST take is written inline has content
                    // before any `TAKE` marker, and that content is take 0.
                    // A comped item has none — it opens straight with
                    // `TAKE NULL` — and pushing the empty accumulator there
                    // invented a take 0 that REAPER does not count, shifting
                    // every later index by one. Only real inline content
                    // becomes take 0.
                    if *in_take_context || take_has_content(&take) {
                        item.takes.push(take);
                    }
                }
                *current_take = Some(Take {
                    is_selected,
                    ..Self::default_take()
                });
                *in_take_context = true;
            }
            "TAKEVOLPAN" => {
                if let Some(take) = current_take
                    && tokens.len() >= 4
                {
                    take.volpan = Some(VolPanSettings {
                        item_trim: 0.0,
                        take_pan: Self::parse_float(&tokens[1])?,
                        take_volume: Self::parse_float(&tokens[2])?,
                        take_pan_law: Self::parse_float(&tokens[3])?,
                    });
                }
            }
            "TAKECOLOR" => {
                if let Some(take) = current_take
                    && tokens.len() > 1
                {
                    take.take_color = Some(Self::parse_int(&tokens[1])?);
                }
            }
            "SM" if tokens.len() >= 3 => {
                let sm = StretchMarker {
                    position: Self::parse_float(&tokens[1])?,
                    source_position: Self::parse_float(&tokens[2])?,
                    rate: if tokens.len() > 3 {
                        Some(Self::parse_float(&tokens[3])?)
                    } else {
                        None
                    },
                };
                // Markers belong to the take they were read under. Before
                // the first `TAKE` line that is the implicit first take,
                // whose markers are mirrored on the item for compatibility.
                if !*in_take_context {
                    item.stretch_markers.push(sm.clone());
                }
                if let Some(take) = current_take {
                    take.stretch_markers.push(sm);
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Create an Item from a parsed RPP block (legacy method for compatibility)
    // r[impl rpp.parse.item]
    pub fn from_block(block: &crate::primitives::RppBlock) -> Result<Self, String> {
        let mut item = Item {
            position: 0.0,
            snap_offset: 0.0,
            length: 0.0,
            loop_source: false,
            play_all_takes: false,
            color: None,
            beat: None,
            selected: false,
            fade_in: None,
            fade_out: None,
            mute: None,
            item_guid: None,
            item_id: None,
            name: String::new(),
            volpan: None,
            slip_offset: 0.0,
            playrate: None,
            channel_mode: ChannelMode::Normal,
            take_guid: None,
            rec_pass: None,
            lane: None,
            y_pos: None,
            takes: Vec::new(),
            stretch_markers: Vec::new(),
            raw_content: String::new(),
        };
        let mut current_take: Option<Take> = Some(Self::default_take());
        let mut in_take_context = false;

        for child in &block.children {
            match child {
                crate::primitives::RppBlockContent::Content(tokens) => {
                    let reparsed;
                    let tokens = if let [Token::Identifier(raw)] = tokens.as_slice() {
                        if raw.contains(' ') && Self::should_parse_item_raw_line(raw) {
                            reparsed = Self::parse_item_token_line(raw)?;
                            reparsed.as_slice()
                        } else {
                            tokens.as_slice()
                        }
                    } else {
                        tokens.as_slice()
                    };
                    Self::apply_item_tokens(
                        &mut item,
                        tokens,
                        &mut current_take,
                        &mut in_take_context,
                    )?;
                }
                crate::primitives::RppBlockContent::RawLine(raw) => {
                    if Self::should_parse_item_raw_line(raw) {
                        let tokens = Self::parse_item_token_line(raw)?;
                        Self::apply_item_tokens(
                            &mut item,
                            &tokens,
                            &mut current_take,
                            &mut in_take_context,
                        )?;
                    }
                }
                crate::primitives::RppBlockContent::Block(nested_block) => {
                    if nested_block.name == "SOURCE" {
                        let source_block = Self::parse_source_block_from_rpp_block(nested_block)?;
                        if let Some(ref mut take) = current_take {
                            take.source = Some(source_block);
                        }
                    } else if let Some(ref mut take) = current_take {
                        // Anything else under a take (`<EXT>` above all) is
                        // real project content: keep it verbatim rather than
                        // dropping it on the floor.
                        take.extra_blocks.push(nested_block.to_string());
                    }
                }
            }
        }

        if let Some(take) = current_take {
            item.takes.push(take);
        }
        Ok(item)
    }

    /// Create an Item from a raw RPP item block string
    ///
    /// # Example
    /// ```
    /// use dawfile_reaper::Item;
    ///
    /// let rpp_content = r#"<ITEM
    ///   POSITION 0.5
    ///   SNAPOFFS 0
    ///   LENGTH 1.256
    ///   LOOP 1
    ///   ALLTAKES 0
    ///   FADEIN 1 0.01 0 1 0 0 0
    ///   FADEOUT 1 0.01 0 1 0 0 0
    ///   MUTE 0 0
    ///   SEL 1
    ///   IGUID {A6A9CEA1-F124-DEB6-16F0-D1761D2084F4}
    ///   IID 3
    ///   NAME 01-250919_0416.wav
    ///   VOLPAN 1 0 1 -1
    ///   SOFFS 0
    ///   PLAYRATE 1 1 0 -1 0 0.0025
    ///   CHANMODE 0
    ///   GUID {1E52F736-F6BD-D957-F1DD-65CA3C4DE4E9}
    ///   RECPASS 1
    ///   <SOURCE WAVE
    ///     FILE "/home/cody/Documents/REAPER Media/01-250919_0416.wav"
    ///   >
    ///   TAKE SEL
    ///   NAME 01-250919_0416-01.wav
    ///   TAKEVOLPAN 0 1 -1
    ///   SOFFS 0
    ///   PLAYRATE 1 1 0 -1 0 0.0025
    ///   CHANMODE 0
    ///   TAKECOLOR 33319124 B
    ///   GUID {4F3DCC1C-3A7B-8161-9D5E-BCA88432F95F}
    ///   RECPASS 2
    ///   <SOURCE WAVE
    ///     FILE "/home/cody/Documents/REAPER Media/01-250919_0416-01.wav"
    ///   >
    /// >"#;
    ///
    /// let item = Item::from_rpp_block(rpp_content).unwrap();
    /// ```
    pub fn from_rpp_block(block_content: &str) -> Result<Self, String> {
        let mut item = Item {
            position: 0.0,
            snap_offset: 0.0,
            length: 0.0,
            loop_source: false,
            play_all_takes: false,
            color: None,
            beat: None,
            selected: false,
            fade_in: None,
            fade_out: None,
            mute: None,
            item_guid: None,
            item_id: None,
            name: String::new(),
            volpan: None,
            slip_offset: 0.0,
            playrate: None,
            channel_mode: ChannelMode::Normal,
            take_guid: None,
            rec_pass: None,
            lane: None,
            y_pos: None,
            takes: Vec::new(),
            stretch_markers: Vec::new(),
            raw_content: block_content.to_string(),
        };

        let lines: Vec<&str> = block_content.lines().collect();
        let mut i = 0;
        let mut current_take: Option<Take> = Some(Self::default_take());
        let mut in_take_context = false; // Track whether we're parsing take-level fields

        while i < lines.len() {
            let line = lines[i].trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with("//") {
                i += 1;
                continue;
            }

            // The `<ITEM` header is structure, not content.
            if line.starts_with("<ITEM") {
                i += 1;
                continue;
            }

            // Nested blocks: `<SOURCE ...>` and everything else (`<EXT>`).
            if line.starts_with('<') {
                let mut block_lines = Vec::new();
                block_lines.push(line);
                i += 1;
                let mut depth = 1i32;

                while i < lines.len() {
                    let current_line = lines[i].trim();
                    block_lines.push(current_line);

                    if current_line.starts_with('<') {
                        depth += 1;
                    } else if current_line == ">" {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    i += 1;
                }

                if i >= lines.len() {
                    return Err(format!("nested block {line} not properly closed with '>'"));
                }

                let block_content = block_lines.join("\n");
                if line.starts_with("<SOURCE") {
                    let source_block = Self::parse_source_block(&block_content)?;
                    if let Some(ref mut take) = current_take {
                        take.source = Some(source_block);
                    }
                } else if let Some(ref mut take) = current_take {
                    take.extra_blocks.push(block_content);
                }

                // Skip the increment at the end since we already incremented in the loop
                continue;
            }

            // Skip block end markers
            if line == ">" {
                i += 1;
                continue;
            }

            // Parse token line
            let tokens = match Self::parse_item_token_line(line) {
                Ok(tokens) => tokens,
                Err(_) => {
                    i += 1;
                    continue;
                }
            };

            if tokens.is_empty() {
                i += 1;
                continue;
            }

            Self::apply_item_tokens(&mut item, &tokens, &mut current_take, &mut in_take_context)?;

            i += 1;
        }

        // Add the last take if it exists
        if let Some(take) = current_take {
            item.takes.push(take);
        }

        Ok(item)
    }

    /// The media path out of a tokenized `FILE` line, or `None` if this
    /// is not one.
    ///
    /// The single place that knows what a `FILE` line looks like. Both
    /// SOURCE parsers call it, and they did not always: each had its own
    /// copy, both copies were wrong in the same way, and fixing one of
    /// them fixed nothing because the other was the one that ran.
    ///
    /// What they got wrong: the path is the **first** token after
    /// `FILE`, not everything after it. REAPER writes a flag behind the
    /// path —
    ///
    /// ```text
    /// FILE "Media/_NSYNC - Bye Bye Bye (Official Audio) 0.mp3" 1
    /// ```
    ///
    /// — and joining the remaining tokens folds that `1` into the
    /// filename. The source then fails to open with a "no such file"
    /// naming a file that is sitting right there on disk, the project
    /// loads with zero decoded sources, and it plays silence.
    ///
    /// Joining was presumably meant to hold together a path with spaces
    /// in it. It never did that job: the tokenizer keeps a quoted path
    /// as one token, spaces and all.
    fn file_path_from_tokens(tokens: &[Token]) -> Result<Option<String>, String> {
        if !matches!(tokens.first(), Some(Token::Identifier(id)) if id == "FILE") {
            return Ok(None);
        }
        match tokens.get(1) {
            Some(token) => Ok(Some(Self::parse_string(token)?.replace("\\\\", "\\"))),
            None => Ok(None),
        }
    }

    /// Parse a SOURCE block
    fn parse_source_block(block_content: &str) -> Result<SourceBlock, String> {
        let lines: Vec<&str> = block_content.lines().collect();

        if lines.len() < 2 {
            return Err("SOURCE block must have at least 2 lines".to_string());
        }

        // First line should be "<SOURCE TYPE"
        let first_line = lines[0].trim();
        if !first_line.starts_with("<SOURCE") {
            return Err("Expected SOURCE block to start with '<SOURCE'".to_string());
        }

        // Parse source type
        let source_type_str = first_line.replace("<SOURCE", "").trim().to_string();
        let source_type = SourceType::from(source_type_str.as_str());

        // Find FILE line
        let mut file_path = String::new();
        let inner_lines = &lines[1..lines.len().saturating_sub(1)];
        for line in inner_lines {
            let line = line.trim();
            if line.starts_with("FILE") {
                // Tokenize and go through the one shared extractor, so
                // this parser and the token-based one cannot drift apart
                // on what a FILE line means — which is exactly how the
                // trailing-flag bug survived being fixed once.
                let tokens = Self::parse_item_token_line(line)?;
                if let Some(path) = Self::file_path_from_tokens(&tokens)? {
                    file_path = path;
                }
                break;
            }
        }

        // Parse MIDI data if this is a MIDI source
        let midi_data = if source_type == SourceType::Midi {
            Some(Self::parse_midi_source(inner_lines))
        } else {
            None
        };

        Ok(SourceBlock {
            source_type,
            file_path,
            midi_data,
            raw_content: block_content.to_string(),
        })
    }

    /// Parse a SOURCE block directly from parsed block structure.
    /// Falls back to string parser for MIDI/complex nested formats.
    fn parse_source_block_from_rpp_block(
        block: &crate::primitives::RppBlock,
    ) -> Result<SourceBlock, String> {
        if block.name != "SOURCE" {
            return Err("Expected SOURCE block".to_string());
        }

        if block
            .children
            .iter()
            .any(|c| matches!(c, crate::primitives::RppBlockContent::Block(_)))
        {
            return Self::parse_source_block(&block.to_string());
        }

        let source_type = block
            .params
            .first()
            .and_then(Token::as_string)
            .map(SourceType::from)
            .unwrap_or_else(|| SourceType::Unknown(String::new()));

        if source_type == SourceType::Midi {
            return Self::parse_source_block(&block.to_string());
        }

        let mut file_path = String::new();
        for child in &block.children {
            let mut reparsed_tokens: Option<Vec<Token>> = None;
            let tokens: &[Token] = match child {
                crate::primitives::RppBlockContent::Content(tokens) => {
                    if let [Token::Identifier(raw)] = tokens.as_slice() {
                        if raw.contains(' ') {
                            if !raw.trim_start().starts_with("FILE") {
                                continue;
                            }
                            reparsed_tokens = Some(Self::parse_item_token_line(raw)?);
                            reparsed_tokens.as_deref().unwrap_or(tokens.as_slice())
                        } else {
                            tokens.as_slice()
                        }
                    } else {
                        tokens.as_slice()
                    }
                }
                crate::primitives::RppBlockContent::RawLine(raw) => {
                    if !raw.trim_start().starts_with("FILE") {
                        continue;
                    }
                    reparsed_tokens = Some(Self::parse_item_token_line(raw)?);
                    reparsed_tokens.as_deref().unwrap_or(&[])
                }
                crate::primitives::RppBlockContent::Block(_) => continue,
            };
            if !matches!(tokens.first(), Some(Token::Identifier(id)) if id == "FILE") {
                continue;
            }
            let Some(path) = Self::file_path_from_tokens(tokens)? else {
                continue;
            };
            file_path = path;
            break;
        }

        Ok(SourceBlock {
            source_type,
            file_path,
            midi_data: None,
            raw_content: String::new(),
        })
    }

    /// Parse MIDI source data from inner lines of a `<SOURCE MIDI>` block.
    fn parse_midi_source(lines: &[&str]) -> MidiSource {
        let mut midi = MidiSource {
            has_data: false,
            ticks_per_qn: 960,
            ticks_timebase: None,
            cc_interp: None,
            pooled_evts_guid: None,
            events: Vec::new(),
            extended_events: Vec::new(),
            event_stream: Vec::new(),
            ignore_tempo: None,
            vel_lanes: Vec::new(),
            bank_program_file: None,
            cfg_edit_view: None,
            cfg_edit: None,
            evt_filter: None,
            guid: None,
            unknown_lines: Vec::new(),
        };

        let mut i = 0usize;
        while i < lines.len() {
            let line = lines[i].trim();

            if line.starts_with("HASDATA ") {
                // HASDATA 1 960 QN
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    midi.has_data = parts[1] != "0";
                }
                if parts.len() >= 3 {
                    midi.ticks_per_qn = parts[2].parse().unwrap_or(960);
                }
                if parts.len() >= 4 {
                    midi.ticks_timebase = Some(parts[3].to_string());
                }
            } else if line.starts_with("CCINTERP ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    midi.cc_interp = parts[1].parse().ok();
                }
            } else if let Some(stripped) = line.strip_prefix("POOLEDEVTS ") {
                midi.pooled_evts_guid = Some(stripped.trim().to_string());
            } else if let Some(stripped) = line.strip_prefix("GUID ") {
                midi.guid = Some(stripped.trim().to_string());
            } else if line.starts_with("IGNTEMPO ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 5 {
                    midi.ignore_tempo = Some(MidiIgnoreTempo {
                        enabled: parts[1] != "0",
                        bpm: parts[2].parse().unwrap_or(120.0),
                        numerator: parts[3].parse().unwrap_or(4),
                        denominator: parts[4].parse().unwrap_or(4),
                    });
                }
            } else if line.starts_with("VELLANE ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    midi.vel_lanes.push(MidiVelLane {
                        lane_type: parts[1].parse().unwrap_or(0),
                        midi_editor_height: parts[2].parse().unwrap_or(0),
                        inline_editor_height: parts[3].parse().unwrap_or(0),
                    });
                }
            } else if let Some(stripped) = line.strip_prefix("BANKPROGRAMFILE ") {
                let mut v = stripped.trim().to_string();
                if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 {
                    v = v[1..v.len() - 1].to_string();
                }
                v = v.replace("\\\\", "\\");
                midi.bank_program_file = Some(v);
            } else if let Some(stripped) = line.strip_prefix("CFGEDITVIEW ") {
                midi.cfg_edit_view =
                    Some(stripped.split_whitespace().map(|s| s.to_string()).collect());
            } else if let Some(stripped) = line.strip_prefix("CFGEDIT ") {
                midi.cfg_edit = Some(stripped.split_whitespace().map(|s| s.to_string()).collect());
            } else if let Some(stripped) = line.strip_prefix("EVTFILTER ") {
                midi.evt_filter =
                    Some(stripped.split_whitespace().map(|s| s.to_string()).collect());
            } else if line.starts_with("<X ") || line.starts_with("<x ") {
                let selected = line.starts_with("<x ");
                let fields: Vec<String> = line[2..]
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
                let mut data_lines = Vec::new();
                i += 1;
                while i < lines.len() {
                    let payload_line = lines[i].trim();
                    if payload_line == ">" {
                        break;
                    }
                    if !payload_line.is_empty() {
                        data_lines.push(payload_line.to_string());
                    }
                    i += 1;
                }
                let event = MidiExtendedEvent {
                    selected,
                    fields,
                    data_lines,
                };
                midi.extended_events.push(event.clone());
                midi.event_stream.push(MidiSourceEvent::Extended(event));
            } else if line.starts_with("E ") || line.starts_with("e ") {
                // MIDI event: E <delta_ticks> <status_hex> <data1_hex> [data2_hex ...]
                let parts: Vec<&str> = line[2..].split_whitespace().collect();
                if parts.len() >= 2 {
                    let delta_ticks = parts[0].parse::<u32>().unwrap_or(0);
                    let bytes: Vec<u8> = parts[1..]
                        .iter()
                        .filter_map(|s| u8::from_str_radix(s, 16).ok())
                        .collect();
                    if !bytes.is_empty() {
                        let event = MidiEvent { delta_ticks, bytes };
                        midi.events.push(event.clone());
                        midi.event_stream.push(MidiSourceEvent::Midi(event));
                    }
                }
            } else if !line.is_empty() {
                midi.unknown_lines.push(line.to_string());
            }

            i += 1;
        }

        midi
    }

    /// Parse a float from a token
    fn parse_float(token: &Token) -> Result<f64, String> {
        match token {
            Token::Float(f) => Ok(*f),
            Token::Integer(i) => Ok(*i as f64),
            _ => Err(format!("Expected float or integer, got {:?}", token)),
        }
    }

    /// Parse an integer from a token
    fn parse_int(token: &Token) -> Result<i32, String> {
        match token {
            Token::Integer(i) => Ok(*i as i32),
            Token::Float(f) => Ok(*f as i32),
            _ => Err(format!("Expected integer or float, got {:?}", token)),
        }
    }

    /// Parse a boolean from a token
    fn parse_bool(token: &Token) -> Result<bool, String> {
        match token {
            Token::Integer(i) => Ok(*i != 0),
            Token::Float(f) => Ok(*f != 0.0),
            _ => Err(format!(
                "Expected integer or float for boolean, got {:?}",
                token
            )),
        }
    }

    /// Parse a string from a token
    fn parse_string(token: &Token) -> Result<String, String> {
        match token {
            Token::String(s, _) => Ok(s.clone()),
            Token::Identifier(s) => Ok(s.clone()),
            // Purely numeric names appear unquoted in real projects
            // (`LANENAME 1 2 3`, takes named `2`) — stringify them.
            Token::Integer(i) => Ok(i.to_string()),
            Token::Float(f) => Ok(f.to_string()),
            _ => Err(format!("Expected string or identifier, got {:?}", token)),
        }
    }
}

impl fmt::Display for Item {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Item({}) - {} takes", self.name, self.takes.len())
    }
}

/// Helper function to format Option values nicely
fn format_option<T: fmt::Display>(opt: &Option<T>) -> String {
    match opt {
        Some(value) => value.to_string(),
        None => "None".to_string(),
    }
}

/// Helper function to format GUIDs without braces
fn format_guid(guid: &Option<String>) -> String {
    match guid {
        Some(g) => {
            // Remove braces if present
            let cleaned = g.trim_start_matches('{').trim_end_matches('}');
            cleaned.to_string()
        }
        None => "None".to_string(),
    }
}

impl fmt::Display for Take {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Take({})", self.name)
    }
}

impl fmt::Display for FadeSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "curve: {}, time: {:.3}s", self.curve_type, self.time)
    }
}

impl fmt::Display for MuteSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "muted: {}, solo: {}", self.muted, self.solo_state)
    }
}

impl fmt::Display for VolPanSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "trim: {:.2}, pan: {:.2}, vol: {:.2}",
            self.item_trim, self.take_pan, self.take_volume
        )
    }
}

impl fmt::Display for PlayRateSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "rate: {:.3}, preserve_pitch: {}, pitch: {:.2}, mode: {}",
            self.rate, self.preserve_pitch, self.pitch_adjust, self.pitch_mode
        )
    }
}

impl fmt::Display for SourceBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.source_type, self.file_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REAPER writes a flag after the quoted path. Swept into the
    /// filename it produces a "no such file" naming a file that is
    /// there — the project loads with zero decoded sources and plays
    /// silence, which reads as a broken audio device.
    #[test]
    fn a_trailing_flag_is_not_part_of_the_path() {
        let block =
            "<SOURCE MP3\n  FILE \"Media/_NSYNC - Bye Bye Bye (Official Audio) 0.mp3\" 1\n>";
        let source = Item::parse_source_block(block).expect("parses");
        assert_eq!(
            source.file_path,
            "Media/_NSYNC - Bye Bye Bye (Official Audio) 0.mp3"
        );
    }

    /// The same line without a flag, which is what most projects carry
    /// and what used to work — the fix must not regress it.
    #[test]
    fn a_quoted_path_with_no_flag_still_parses() {
        let block = "<SOURCE WAVE\n  FILE \"Media/take one.wav\"\n>";
        let source = Item::parse_source_block(block).expect("parses");
        assert_eq!(source.file_path, "Media/take one.wav");
    }

    /// Unquoted paths cannot contain spaces, so anything after the
    /// first token is a flag rather than the rest of a filename.
    #[test]
    fn an_unquoted_path_stops_at_the_first_token() {
        let block = "<SOURCE WAVE\n  FILE Media/kick.wav 1\n>";
        let source = Item::parse_source_block(block).expect("parses");
        assert_eq!(source.file_path, "Media/kick.wav");
    }

    /// The same file through a whole `<ITEM>`, by **both** entry points.
    ///
    /// This is the test that was missing. The trailing-flag bug was
    /// fixed once in `parse_source_block` and nothing changed, because a
    /// real project goes through `from_block` — the token route — and
    /// that had its own copy of the logic with the same defect. Asserting
    /// both routes agree is what stops the pair drifting again.
    #[test]
    fn both_item_parsers_resolve_the_same_media_path() {
        let rpp = "<ITEM\n  POSITION 0\n  LENGTH 200\n  NAME \"song.mp3\"\n  \
                   <SOURCE MP3\n    FILE \"Media/_NSYNC - Bye Bye Bye (Official Audio) 0.mp3\" 1\n  >\n>";
        let want = "Media/_NSYNC - Bye Bye Bye (Official Audio) 0.mp3";

        let from_text = Item::from_rpp_block(rpp).expect("string route parses");
        let path = |item: &Item| {
            item.takes
                .first()
                .and_then(|t| t.source.as_ref())
                .map(|s| s.file_path.clone())
                .expect("a source with a path")
        };
        assert_eq!(path(&from_text), want, "string route kept the flag");

        let (_, block) = crate::primitives::block::parse_block(rpp).expect("block parses");
        let from_tokens = Item::from_block(&block).expect("token route parses");
        assert_eq!(path(&from_tokens), want, "token route kept the flag");
    }

    #[test]
    fn test_parse_complex_item() {
        let rpp_content = r#"<ITEM
  POSITION 0.5
  SNAPOFFS 0
  LENGTH 1.256
  LOOP 1
  ALLTAKES 0
  FADEIN 1 0.01 0 1 0 0 0
  FADEOUT 1 0.01 0 1 0 0 0
  MUTE 0 0
  SEL 1
  IGUID {A6A9CEA1-F124-DEB6-16F0-D1761D2084F4}
  IID 3
  NAME 01-250919_0416.wav
  VOLPAN 1 0 1 -1
  SOFFS 0
  PLAYRATE 1 1 0 -1 0 0.0025
  CHANMODE 0
  GUID {1E52F736-F6BD-D957-F1DD-65CA3C4DE4E9}
  RECPASS 1
  <SOURCE WAVE
    FILE "/home/cody/Documents/REAPER Media/01-250919_0416.wav"
  >
  TAKE SEL
  NAME 01-250919_0416-01.wav
  TAKEVOLPAN 0 1 -1
  SOFFS 0
  PLAYRATE 1 1 0 -1 0 0.0025
  CHANMODE 0
  TAKECOLOR 33319124 B
  GUID {4F3DCC1C-3A7B-8161-9D5E-BCA88432F95F}
  RECPASS 2
  <SOURCE WAVE
    FILE "/home/cody/Documents/REAPER Media/01-250919_0416-01.wav"
  >
  TAKE
  NAME 01-250919_0416-02.wav
  TAKEVOLPAN 0 1 -1
  SOFFS 0
  PLAYRATE 1 1 0 -1 0 0.0025
  CHANMODE 0
  TAKECOLOR 32163693 B
  GUID {7D9D5C13-2CF0-FF8B-66FD-F9C0D53E2FD5}
  RECPASS 3
  <SOURCE WAVE
    FILE "/home/cody/Documents/REAPER Media/01-250919_0416-02.wav"
  >
  TAKE
  NAME 01-250919_0416-03.wav
  TAKEVOLPAN 0 1 -1
  SOFFS 0
  PLAYRATE 1 1 0 -1 0 0.0025
  CHANMODE 0
  TAKECOLOR 24072829 B
  GUID {A943CE1C-349F-F012-5C9E-1568123B2135}
  RECPASS 4
  <SOURCE WAVE
    FILE "/home/cody/Documents/REAPER Media/01-250919_0416-03.wav"
  >
>"#;

        let item = Item::from_rpp_block(rpp_content).unwrap();

        println!("\n🎵 Parsed Complex Item:");
        println!("{}", "=".repeat(60));
        println!("Item: {}", item);
        println!(
            "  Position: {:.3}s, Length: {:.3}s",
            item.position, item.length
        );
        println!("  Snap Offset: {:.3}s", item.snap_offset);
        println!(
            "  Loop Source: {}, Play All Takes: {}",
            item.loop_source, item.play_all_takes
        );
        println!("  Selected: {}", item.selected);
        println!("  Item GUID: {}", format_guid(&item.item_guid));
        println!("  Item ID: {}", format_option(&item.item_id));
        println!("  Recording Pass: {}", format_option(&item.rec_pass));

        if let Some(ref fade_in) = item.fade_in {
            println!("  Fade In: {}", fade_in);
        }
        if let Some(ref fade_out) = item.fade_out {
            println!("  Fade Out: {}", fade_out);
        }
        if let Some(ref mute) = item.mute {
            println!("  Mute: {}", mute);
        }
        if let Some(ref volpan) = item.volpan {
            println!("  VolPan: {}", volpan);
        }
        if let Some(ref playrate) = item.playrate {
            println!("  PlayRate: {}", playrate);
        }

        println!("\n  Takes ({}):", item.takes.len());
        for (i, take) in item.takes.iter().enumerate() {
            println!("    Take {}: {}", i + 1, take);
            println!("      Selected: {}", take.is_selected);
            println!("      Slip Offset: {:.3}s", take.slip_offset);
            println!("      Channel Mode: {}", take.channel_mode);
            println!("      Take Color: {}", format_option(&take.take_color));
            println!("      Take GUID: {}", format_guid(&take.take_guid));
            println!("      Recording Pass: {}", format_option(&take.rec_pass));

            if let Some(ref volpan) = take.volpan {
                println!("      VolPan: {}", volpan);
            }
            if let Some(ref playrate) = take.playrate {
                println!("      PlayRate: {}", playrate);
            }
            if let Some(ref source) = take.source {
                println!("      Source: {}", source);
            }
            println!();
        }

        // Verify basic item properties
        assert_eq!(item.position, 0.5);
        assert_eq!(item.snap_offset, 0.0);
        assert_eq!(item.length, 1.256);
        assert!(item.loop_source);
        assert!(!item.play_all_takes);
        assert!(item.selected);
        assert_eq!(item.name, "01-250919_0416.wav");
        assert_eq!(item.item_id, Some(3));
        assert_eq!(item.rec_pass, Some(1));

        // Verify fade settings
        assert!(item.fade_in.is_some());
        let fade_in = item.fade_in.unwrap();
        assert_eq!(fade_in.curve_type, FadeCurveType::Square);
        assert_eq!(fade_in.time, 0.01);
        assert_eq!(fade_in.unknown_field_3, 0.0);
        assert_eq!(fade_in.unknown_field_4, 1);
        assert_eq!(fade_in.unknown_field_5, 0);
        assert_eq!(fade_in.unknown_field_6, 0);
        assert_eq!(fade_in.unknown_field_7, 0);

        assert!(item.fade_out.is_some());
        let fade_out = item.fade_out.unwrap();
        assert_eq!(fade_out.curve_type, FadeCurveType::Square);
        assert_eq!(fade_out.time, 0.01);
        assert_eq!(fade_out.unknown_field_3, 0.0);
        assert_eq!(fade_out.unknown_field_4, 1);
        assert_eq!(fade_out.unknown_field_5, 0);
        assert_eq!(fade_out.unknown_field_6, 0);
        assert_eq!(fade_out.unknown_field_7, 0);

        // Verify mute settings
        assert!(item.mute.is_some());
        let mute = item.mute.unwrap();
        assert!(!mute.muted);
        assert_eq!(mute.solo_state, SoloState::NotSoloed);

        // Verify volpan settings
        assert!(item.volpan.is_some());
        let volpan = item.volpan.unwrap();
        assert_eq!(volpan.item_trim, 1.0);
        assert_eq!(volpan.take_pan, 0.0);
        assert_eq!(volpan.take_volume, 1.0);
        assert_eq!(volpan.take_pan_law, -1.0);

        // Verify playrate settings
        assert!(item.playrate.is_some());
        let playrate = item.playrate.unwrap();
        assert_eq!(playrate.rate, 1.0);
        assert!(playrate.preserve_pitch);
        assert_eq!(playrate.pitch_adjust, 0.0);
        assert_eq!(playrate.pitch_mode, PitchMode::ProjectDefault);

        // Verify takes
        assert_eq!(item.takes.len(), 4);

        // First take (implicit, not selected). REAPER writes the first
        // take's attributes at item level — the implicit take inherits them.
        let take1 = &item.takes[0];
        assert!(!take1.is_selected);
        assert_eq!(take1.name, "01-250919_0416.wav");
        assert_eq!(take1.rec_pass, None);
        assert!(take1.source.is_some());
        let source1 = take1.source.as_ref().unwrap();
        assert_eq!(source1.source_type, SourceType::Wave);
        assert_eq!(
            source1.file_path,
            "/home/cody/Documents/REAPER Media/01-250919_0416.wav"
        );

        // Second take (selected)
        let take2 = &item.takes[1];
        assert!(take2.is_selected);
        assert_eq!(take2.name, "01-250919_0416-01.wav");
        assert_eq!(take2.rec_pass, Some(2));

        // Third take
        let take3 = &item.takes[2];
        assert!(!take3.is_selected);
        assert_eq!(take3.name, "01-250919_0416-02.wav");
        assert_eq!(take3.rec_pass, Some(3));

        // Fourth take
        let take4 = &item.takes[3];
        assert!(!take4.is_selected);
        assert_eq!(take4.name, "01-250919_0416-03.wav");
        assert_eq!(take4.rec_pass, Some(4));

        println!(
            "✅ Successfully parsed complex item with {} takes!",
            item.takes.len()
        );
    }

    #[test]
    fn test_parse_simple_item() {
        let rpp_content = r#"<ITEM
  POSITION 0.0
  SNAPOFFS 0.0
  LENGTH 145.5
  LOOP 0
  ALLTAKES 0
  SEL 1
  NAME "Simple Item"
  VOLPAN 1.0 0.0 1.0 -1.0
  SOFFS 0.0
  PLAYRATE 1.0 1 0.0 -1
  CHANMODE 0
  <SOURCE WAVE
    FILE "C:\\Path\\To\\AudioFile.wav"
  >
>"#;

        let item = Item::from_rpp_block(rpp_content).unwrap();

        println!("\n🎵 Parsed Simple Item:");
        println!("{}", "=".repeat(40));
        println!("Item: {}", item);
        println!(
            "  Position: {:.3}s, Length: {:.3}s",
            item.position, item.length
        );
        println!("  Name: {}", item.name);
        println!("  Takes: {}", item.takes.len());

        // Debug: show take details
        for (i, take) in item.takes.iter().enumerate() {
            println!(
                "  Take {}: name='{}', source={}",
                i,
                take.name,
                take.source.is_some()
            );
        }

        // Verify basic properties
        assert_eq!(item.position, 0.0);
        assert_eq!(item.length, 145.5);
        assert!(!item.loop_source);
        assert!(!item.play_all_takes);
        assert!(item.selected);
        assert_eq!(item.name, "Simple Item");
        assert_eq!(item.takes.len(), 1);

        // Verify the single take
        let take = &item.takes[0];
        assert!(!take.is_selected);
        assert!(take.source.is_some());
        let source = take.source.as_ref().unwrap();
        assert_eq!(source.source_type, SourceType::Wave);
        assert_eq!(source.file_path, "C:\\Path\\To\\AudioFile.wav");

        println!("✅ Successfully parsed simple item!");
    }

    #[test]
    fn test_parse_midi_item() {
        let rpp_content = r#"<ITEM
  POSITION 0.0
  SNAPOFFS 0
  LENGTH 4.0
  LOOP 1
  ALLTAKES 0
  SEL 0
  NAME "MIDI Pattern"
  VOLPAN 1 0 1 -1
  SOFFS 0
  PLAYRATE 1 1 0 -1 0 0.0025
  CHANMODE 0
  GUID {AABB0011-2233-4455-6677-8899AABBCCDD}
  <SOURCE MIDI
    HASDATA 1 960 QN
    CCINTERP 32
    POOLEDEVTS {11223344-5566-7788-99AA-BBCCDDEEFF00}
    E 0 90 3c 7f
    E 480 80 3c 00
    E 0 90 40 64
    E 480 80 40 00
    GUID {FFEEDDCC-BBAA-9988-7766-554433221100}
  >
>"#;

        let item = Item::from_rpp_block(rpp_content).unwrap();
        assert_eq!(item.takes.len(), 1);
        assert_eq!(item.name, "MIDI Pattern");

        let take = &item.takes[0];
        let source = take.source.as_ref().unwrap();
        assert_eq!(source.source_type, SourceType::Midi);
        assert!(source.midi_data.is_some());

        let midi = source.midi_data.as_ref().unwrap();
        assert!(midi.has_data);
        assert_eq!(midi.ticks_per_qn, 960);
        assert_eq!(midi.ticks_timebase.as_deref(), Some("QN"));
        assert_eq!(midi.cc_interp, Some(32));
        assert_eq!(
            midi.pooled_evts_guid.as_deref(),
            Some("{11223344-5566-7788-99AA-BBCCDDEEFF00}")
        );
        assert_eq!(
            midi.guid.as_deref(),
            Some("{FFEEDDCC-BBAA-9988-7766-554433221100}")
        );

        // 4 MIDI events: note on C4, note off C4, note on E4, note off E4
        assert_eq!(midi.events.len(), 4);

        // First event: Note On C4 (0x3c = 60) velocity 127
        let ev0 = &midi.events[0];
        assert_eq!(ev0.delta_ticks, 0);
        assert_eq!(ev0.bytes, vec![0x90, 0x3c, 0x7f]);
        assert_eq!(ev0.event_type(), MidiEventType::NoteOn);
        assert_eq!(ev0.channel(), 0);

        // Second event: Note Off C4 after 480 ticks
        let ev1 = &midi.events[1];
        assert_eq!(ev1.delta_ticks, 480);
        assert_eq!(ev1.bytes, vec![0x80, 0x3c, 0x00]);
        assert_eq!(ev1.event_type(), MidiEventType::NoteOff);

        // Third event: Note On E4 (0x40 = 64) velocity 100
        let ev2 = &midi.events[2];
        assert_eq!(ev2.delta_ticks, 0);
        assert_eq!(ev2.bytes, vec![0x90, 0x40, 0x64]);
        assert_eq!(ev2.event_type(), MidiEventType::NoteOn);

        // Fourth event: Note Off E4
        let ev3 = &midi.events[3];
        assert_eq!(ev3.delta_ticks, 480);
        assert_eq!(ev3.bytes, vec![0x80, 0x40, 0x00]);
        assert_eq!(ev3.event_type(), MidiEventType::NoteOff);
    }

    #[test]
    fn test_parse_midi_extended_fields_and_x_blocks() {
        let rpp_content = r#"<ITEM
  POSITION 0
  SNAPOFFS 0
  LENGTH 1
  LOOP 0
  ALLTAKES 0
  SEL 0
  NAME "MIDI Meta"
  VOLPAN 1 0 1 -1
  SOFFS 0
  PLAYRATE 1 1 0 -1 0 0.0025
  CHANMODE 0
  <SOURCE MIDI
    HASDATA 1 960 QN
    CCINTERP 32
    <X 7680 0 0 0 6 Am7
      /wZBbTc=
    >
    IGNTEMPO 1 120.00000000 4 4
    VELLANE 128 97 0
    BANKPROGRAMFILE "C:\\Path\\To\\GM.reabank"
    CFGEDITVIEW 3787.8 0.1 0 48 0 0 0
    CFGEDIT 1 1 0 1
    EVTFILTER 0 -1 -1 -1 -1 0 1
    GUID {12345678-1234-5678-9ABC-DEF012345678}
  >
>"#;

        let item = Item::from_rpp_block(rpp_content).unwrap();
        let midi = item.takes[0]
            .source
            .as_ref()
            .unwrap()
            .midi_data
            .as_ref()
            .unwrap();

        assert!(midi.has_data);
        assert_eq!(midi.cc_interp, Some(32));
        assert_eq!(midi.extended_events.len(), 1);
        assert_eq!(midi.extended_events[0].fields[0], "7680");
        assert_eq!(midi.extended_events[0].fields[5], "Am7");
        assert_eq!(midi.extended_events[0].data_lines, vec!["/wZBbTc="]);

        let ign = midi.ignore_tempo.as_ref().unwrap();
        assert!(ign.enabled);
        assert_eq!(ign.bpm, 120.0);
        assert_eq!(ign.numerator, 4);
        assert_eq!(ign.denominator, 4);

        assert_eq!(midi.vel_lanes.len(), 1);
        assert_eq!(midi.vel_lanes[0].lane_type, 128);
        assert_eq!(
            midi.bank_program_file.as_deref(),
            Some("C:\\Path\\To\\GM.reabank")
        );
        assert_eq!(midi.cfg_edit_view.as_ref().unwrap()[0], "3787.8");
        assert_eq!(midi.cfg_edit.as_ref().unwrap()[0], "1");
        assert_eq!(midi.evt_filter.as_ref().unwrap()[0], "0");
    }

    #[test]
    fn test_parse_stretch_markers() {
        let rpp_content = r#"<ITEM
  POSITION 1.0
  SNAPOFFS 0
  LENGTH 10.0
  LOOP 0
  ALLTAKES 0
  SEL 0
  NAME "Stretched Audio"
  VOLPAN 1 0 1 -1
  SOFFS 0
  PLAYRATE 1 1 0 -1 0 0.0025
  CHANMODE 0
  SM 0 0
  SM 2.5 2.0 1.25
  SM 5.0 4.5
  <SOURCE WAVE
    FILE "audio.wav"
  >
>"#;

        let item = Item::from_rpp_block(rpp_content).unwrap();
        assert_eq!(item.stretch_markers.len(), 3);

        let sm0 = &item.stretch_markers[0];
        assert_eq!(sm0.position, 0.0);
        assert_eq!(sm0.source_position, 0.0);
        assert!(sm0.rate.is_none());

        let sm1 = &item.stretch_markers[1];
        assert_eq!(sm1.position, 2.5);
        assert_eq!(sm1.source_position, 2.0);
        assert_eq!(sm1.rate, Some(1.25));

        let sm2 = &item.stretch_markers[2];
        assert_eq!(sm2.position, 5.0);
        assert_eq!(sm2.source_position, 4.5);
        assert!(sm2.rate.is_none());
    }

    #[test]
    fn test_source_type_variants() {
        assert_eq!(SourceType::from("WAVE"), SourceType::Wave);
        assert_eq!(SourceType::from("MP3"), SourceType::Mp3);
        assert_eq!(SourceType::from("MIDI"), SourceType::Midi);
        assert_eq!(SourceType::from("VIDEO"), SourceType::Video);
        assert_eq!(SourceType::from("SECTION"), SourceType::Section);
        assert_eq!(SourceType::from("EMPTY"), SourceType::Empty);
        assert_eq!(SourceType::from("FLAC"), SourceType::Flac);
        assert_eq!(SourceType::from("VORBIS"), SourceType::Vorbis);
        assert_eq!(SourceType::from("_OFFLINE_WAVE"), SourceType::OfflineWave);
        // Case-insensitive for standard types
        assert_eq!(SourceType::from("wave"), SourceType::Wave);
        assert_eq!(SourceType::from("midi"), SourceType::Midi);
        // Unknown type preserved
        assert_eq!(
            SourceType::from("CUSTOM"),
            SourceType::Unknown("CUSTOM".to_string())
        );
    }

    #[test]
    fn test_parse_empty_source() {
        let rpp_content = r#"<ITEM
  POSITION 0.0
  SNAPOFFS 0
  LENGTH 2.0
  LOOP 0
  ALLTAKES 0
  SEL 0
  NAME "Empty Item"
  VOLPAN 1 0 1 -1
  SOFFS 0
  PLAYRATE 1 1 0 -1 0 0.0025
  CHANMODE 0
  <SOURCE EMPTY
  >
>"#;

        let item = Item::from_rpp_block(rpp_content).unwrap();
        assert_eq!(item.takes.len(), 1);
        let source = item.takes[0].source.as_ref().unwrap();
        assert_eq!(source.source_type, SourceType::Empty);
        assert!(source.file_path.is_empty());
        assert!(source.midi_data.is_none());
    }

    #[test]
    fn test_midi_event_types() {
        // Control Change on channel 5
        let cc = MidiEvent {
            delta_ticks: 0,
            bytes: vec![0xB5, 0x07, 0x7F],
        };
        assert_eq!(cc.event_type(), MidiEventType::ControlChange);
        assert_eq!(cc.channel(), 5);
        assert_eq!(cc.status(), 0xB5);

        // Program Change on channel 0
        let pc = MidiEvent {
            delta_ticks: 0,
            bytes: vec![0xC0, 0x05],
        };
        assert_eq!(pc.event_type(), MidiEventType::ProgramChange);
        assert_eq!(pc.channel(), 0);

        // Pitch Bend on channel 3
        let pb = MidiEvent {
            delta_ticks: 0,
            bytes: vec![0xE3, 0x00, 0x40],
        };
        assert_eq!(pb.event_type(), MidiEventType::PitchBend);
        assert_eq!(pb.channel(), 3);

        // Channel Pressure (Aftertouch)
        let cp = MidiEvent {
            delta_ticks: 0,
            bytes: vec![0xD0, 0x64],
        };
        assert_eq!(cp.event_type(), MidiEventType::ChannelPressure);

        // Poly Aftertouch
        let at = MidiEvent {
            delta_ticks: 0,
            bytes: vec![0xA1, 0x3C, 0x40],
        };
        assert_eq!(at.event_type(), MidiEventType::Aftertouch);

        // System message (0xF0+)
        let sys = MidiEvent {
            delta_ticks: 0,
            bytes: vec![0xF0, 0x7E, 0x7F],
        };
        assert_eq!(sys.event_type(), MidiEventType::System);
        // System messages: channel nibble is 0 (0xF0 & 0x0F)
        assert_eq!(sys.channel(), 0);

        // Empty event — status() returns 0, event_type() returns Unknown
        let empty = MidiEvent {
            delta_ticks: 0,
            bytes: vec![],
        };
        assert_eq!(empty.event_type(), MidiEventType::Unknown);
        assert_eq!(empty.status(), 0);
        assert_eq!(empty.channel(), 0);
    }

    #[test]
    fn test_parse_midi_with_multiple_takes() {
        let rpp_content = r#"<ITEM
  POSITION 0.0
  SNAPOFFS 0
  LENGTH 4.0
  LOOP 1
  ALLTAKES 0
  SEL 0
  NAME "Multi-take MIDI"
  VOLPAN 1 0 1 -1
  SOFFS 0
  PLAYRATE 1 1 0 -1 0 0.0025
  CHANMODE 0
  <SOURCE MIDI
    HASDATA 1 960 QN
    E 0 90 3c 7f
    E 960 80 3c 00
  >
  TAKE SEL
  NAME "Alternate Take"
  SOFFS 0
  CHANMODE 0
  GUID {11111111-2222-3333-4444-555555555555}
  <SOURCE WAVE
    FILE "alt-audio.wav"
  >
>"#;

        let item = Item::from_rpp_block(rpp_content).unwrap();
        assert_eq!(item.takes.len(), 2);

        // First take has MIDI source
        let take1 = &item.takes[0];
        let src1 = take1.source.as_ref().unwrap();
        assert_eq!(src1.source_type, SourceType::Midi);
        let midi = src1.midi_data.as_ref().unwrap();
        assert_eq!(midi.events.len(), 2);

        // Second take has audio source
        let take2 = &item.takes[1];
        assert!(take2.is_selected);
        assert_eq!(take2.name, "Alternate Take");
        let src2 = take2.source.as_ref().unwrap();
        assert_eq!(src2.source_type, SourceType::Wave);
        assert!(src2.midi_data.is_none());
    }
}
