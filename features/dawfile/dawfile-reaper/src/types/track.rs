//! Track data structures and parsing for REAPER

use serde::{Deserialize, Serialize};
use std::fmt;

use daw_proto::routing::{MidiChannelMapping, MidiDestinationChannel, MidiSourceChannel};

use crate::primitives::{BlockType, RppBlock, Token};
use crate::types::envelope::Envelope;
use crate::types::fx_chain::FxChain;
use crate::types::item::Item;

/// Automation mode for tracks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum AutomationMode {
    TrimRead = 0,
    Read = 1,
    Touch = 2,
    Write = 3,
    Latch = 4,
    Unknown(i32),
}

impl From<i32> for AutomationMode {
    fn from(value: i32) -> Self {
        match value {
            0 => AutomationMode::TrimRead,
            1 => AutomationMode::Read,
            2 => AutomationMode::Touch,
            3 => AutomationMode::Write,
            4 => AutomationMode::Latch,
            _ => AutomationMode::Unknown(value),
        }
    }
}

impl fmt::Display for AutomationMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AutomationMode::TrimRead => write!(f, "Trim/Read"),
            AutomationMode::Read => write!(f, "Read"),
            AutomationMode::Touch => write!(f, "Touch"),
            AutomationMode::Write => write!(f, "Write"),
            AutomationMode::Latch => write!(f, "Latch"),
            AutomationMode::Unknown(val) => write!(f, "Unknown({})", val),
        }
    }
}

/// Solo state for tracks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum TrackSoloState {
    NoSolo = 0,
    Solo = 1,
    SoloInPlace = 2,
    Unknown(i32),
}

impl From<i32> for TrackSoloState {
    fn from(value: i32) -> Self {
        match value {
            0 => TrackSoloState::NoSolo,
            1 => TrackSoloState::Solo,
            2 => TrackSoloState::SoloInPlace,
            _ => TrackSoloState::Unknown(value),
        }
    }
}

impl fmt::Display for TrackSoloState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrackSoloState::NoSolo => write!(f, "No Solo"),
            TrackSoloState::Solo => write!(f, "Solo"),
            TrackSoloState::SoloInPlace => write!(f, "Solo In Place"),
            TrackSoloState::Unknown(val) => write!(f, "Unknown({})", val),
        }
    }
}

/// Folder state for tracks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum FolderState {
    Regular = 0,
    FolderParent = 1,
    LastInFolder = 2,
    Unknown(i32),
}

impl From<i32> for FolderState {
    fn from(value: i32) -> Self {
        match value {
            0 => FolderState::Regular,
            1 => FolderState::FolderParent,
            2 => FolderState::LastInFolder,
            _ => FolderState::Unknown(value),
        }
    }
}

impl fmt::Display for FolderState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FolderState::Regular => write!(f, "Regular"),
            FolderState::FolderParent => write!(f, "Folder Parent"),
            FolderState::LastInFolder => write!(f, "Last In Folder"),
            FolderState::Unknown(val) => write!(f, "Unknown({})", val),
        }
    }
}

/// Record mode for tracks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum RecordMode {
    Input = 0,
    OutputStereo = 1,
    DisableMonitor = 2,
    OutputStereoLatencyComp = 3,
    OutputMidi = 4,
    OutputMono = 5,
    OutputMonoLatencyComp = 6,
    MidiOverdub = 7,
    MidiReplace = 8,
    MidiTouchReplace = 9,
    OutputMultichannel = 10,
    OutputMultichannelLatencyComp = 11,
    Unknown(i32),
}

impl From<i32> for RecordMode {
    fn from(value: i32) -> Self {
        match value {
            0 => RecordMode::Input,
            1 => RecordMode::OutputStereo,
            2 => RecordMode::DisableMonitor,
            3 => RecordMode::OutputStereoLatencyComp,
            4 => RecordMode::OutputMidi,
            5 => RecordMode::OutputMono,
            6 => RecordMode::OutputMonoLatencyComp,
            7 => RecordMode::MidiOverdub,
            8 => RecordMode::MidiReplace,
            9 => RecordMode::MidiTouchReplace,
            10 => RecordMode::OutputMultichannel,
            11 => RecordMode::OutputMultichannelLatencyComp,
            _ => RecordMode::Unknown(value),
        }
    }
}

impl fmt::Display for RecordMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordMode::Input => write!(f, "Input"),
            RecordMode::OutputStereo => write!(f, "Output (Stereo)"),
            RecordMode::DisableMonitor => write!(f, "Disable (Monitor)"),
            RecordMode::OutputStereoLatencyComp => write!(f, "Output (Stereo, Latency Comp)"),
            RecordMode::OutputMidi => write!(f, "Output (MIDI)"),
            RecordMode::OutputMono => write!(f, "Output (Mono)"),
            RecordMode::OutputMonoLatencyComp => write!(f, "Output (Mono, Latency Comp)"),
            RecordMode::MidiOverdub => write!(f, "MIDI Overdub"),
            RecordMode::MidiReplace => write!(f, "MIDI Replace"),
            RecordMode::MidiTouchReplace => write!(f, "MIDI Touch Replace"),
            RecordMode::OutputMultichannel => write!(f, "Output (Multichannel)"),
            RecordMode::OutputMultichannelLatencyComp => {
                write!(f, "Output (Multichannel, Latency Comp)")
            }
            RecordMode::Unknown(val) => write!(f, "Unknown({})", val),
        }
    }
}

/// Monitor mode for tracks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum MonitorMode {
    Off = 0,
    On = 1,
    Auto = 2,
    Unknown(i32),
}

impl From<i32> for MonitorMode {
    fn from(value: i32) -> Self {
        match value {
            0 => MonitorMode::Off,
            1 => MonitorMode::On,
            2 => MonitorMode::Auto,
            _ => MonitorMode::Unknown(value),
        }
    }
}

impl fmt::Display for MonitorMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MonitorMode::Off => write!(f, "Off"),
            MonitorMode::On => write!(f, "On"),
            MonitorMode::Auto => write!(f, "Auto"),
            MonitorMode::Unknown(val) => write!(f, "Unknown({})", val),
        }
    }
}

/// Free item positioning mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum FreeMode {
    Disabled = 0,
    FreeItemPositioning = 1,
    FixedItemLanes = 2,
    Unknown(i32),
}

impl From<i32> for FreeMode {
    fn from(value: i32) -> Self {
        match value {
            0 => FreeMode::Disabled,
            1 => FreeMode::FreeItemPositioning,
            2 => FreeMode::FixedItemLanes,
            _ => FreeMode::Unknown(value),
        }
    }
}

impl fmt::Display for FreeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FreeMode::Disabled => write!(f, "Disabled"),
            FreeMode::FreeItemPositioning => write!(f, "Free Item Positioning"),
            FreeMode::FixedItemLanes => write!(f, "Fixed Item Lanes"),
            FreeMode::Unknown(val) => write!(f, "Unknown({})", val),
        }
    }
}

/// Volume and pan settings for tracks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VolPanSettings {
    pub volume: f64,  // field 1 - track volume
    pub pan: f64,     // field 2 - track pan
    pub pan_law: f64, // field 3 - track pan law
}

/// Mute and solo settings for tracks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MuteSoloSettings {
    pub mute: bool,           // field 1 - mute
    pub solo: TrackSoloState, // field 2 - solo state
    pub solo_defeat: bool,    // field 3 - solo defeat
}

/// Folder settings for tracks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderSettings {
    pub folder_state: FolderState, // field 1 - folder state
    pub indentation: i32,          // field 2 - track indentation
}

/// Bus compact settings for tracks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BusCompactSettings {
    pub arrange_collapse: i32, // field 1 - collapse state in Arrange
    pub mixer_collapse: i32,   // field 2 - collapse state in Mixer
    pub wiring_collapse: i32,  // field 3 - collapse state in track wiring
    pub wiring_x: i32,         // field 4 - track wiring routing window x position
    pub wiring_y: i32,         // field 5 - track wiring routing window y position
}

/// Show in mixer settings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowInMixerSettings {
    pub show_in_mixer: bool,      // field 1 - show in mixer
    pub unknown_field_2: f64,     // field 2 - unknown
    pub unknown_field_3: f64,     // field 3 - unknown
    pub show_in_track_list: bool, // field 4 - show in track list
    pub unknown_field_5: f64,     // field 5 - unknown
    pub unknown_field_6: i32,     // field 6 - unknown
    pub unknown_field_7: i32,     // field 7 - unknown
    pub unknown_field_8: i32,     // field 8 - unknown
}

/// Input quantize settings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputQuantizeSettings {
    pub quantize_midi: bool,      // field 1 - quantize midi
    pub quantize_to_pos: i32,     // field 2 - quantize to pos (-1=prev, 0=nearest, 1=next)
    pub quantize_note_offs: bool, // field 3 - quantize note-offs
    pub quantize_to: f64,         // field 4 - quantize to (fraction of beat)
    pub quantize_strength: i32,   // field 5 - quantize strength (%)
    pub swing_strength: i32,      // field 6 - swing strength (%)
    pub quantize_range_min: i32,  // field 7 - quantize range min (%)
    pub quantize_range_max: i32,  // field 8 - quantize range max (%)
}

/// Record settings for tracks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordSettings {
    pub armed: bool,                // field 1 - armed
    pub input: i32,                 // field 2 - input (device + channel coded)
    pub monitor: MonitorMode,       // field 3 - monitor mode
    pub record_mode: RecordMode,    // field 4 - record mode
    pub monitor_track_media: bool,  // field 5 - monitor track media while recording
    pub preserve_pdc_delayed: bool, // field 6 - preserve PDC delayed monitoring
    pub record_path: i32,           // field 7 - record path (0=primary, 1=secondary, 2=both)
}

/// Track height settings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackHeightSettings {
    pub height: i32,           // field 1 - height in pixels
    pub folder_override: bool, // field 2 - folder override (collapsed)
}

/// Fixed lanes settings (REAPER 7+)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixedLanesSettings {
    pub bitfield: i32,             // field 1 - bitfield for various options
    pub allow_editing: bool,       // field 2 - allow editing source media while comping
    pub show_play_only_lane: bool, // field 3 - show/play only lane
    pub mask_playback: bool,       // field 4 - media items in higher numbered lanes mask playback
    pub recording_behavior: i32,   // field 5 - recording behavior bitfield
}

/// Lane solo settings (REAPER 7+)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaneSoloSettings {
    pub playing_lanes: i32,   // field 1 - bitfield of playing lanes
    pub unknown_field_2: i32, // field 2 - unknown
    pub unknown_field_3: i32, // field 3 - unknown
    pub unknown_field_4: i32, // field 4 - unknown
    pub unknown_field_5: i32, // field 5 - unknown
    pub unknown_field_6: i32, // field 6 - unknown
    pub unknown_field_7: i32, // field 7 - unknown
    pub unknown_field_8: i32, // field 8 - unknown
}

/// Lane record settings (REAPER 7+)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaneRecordSettings {
    pub record_enabled_lane: i32, // field 1 - 0-based index of record enabled lane
    pub comping_enabled_lane: i32, // field 2 - 0-based index of comping enabled lane
    pub last_comping_lane: i32,   // field 3 - 0-based index of last comping enabled lane
}

/// Lane name settings (REAPER 7+)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaneNameSettings {
    pub lane_count: i32, // derived: number of lanes (NOT serialized — REAPER emits names only)
    pub lane_names: Vec<String>, // every LANENAME token is a name
}

/// One comp area (REAPER 7+): a `LINKEDLANE` line under `<TRACK>`.
///
/// `LINKEDLANE <start> <end> <source_lane> <comp_lane> -1 <fade_in> <fade_out>`:
/// the stretch `start..end` of `comp_lane` is taken from `source_lane`,
/// crossfaded by the two fade lengths. Decoded from
/// `resources/Template-with-takes-lanes.RPP`, where lane 4 (named `3`)
/// feeds comp lanes 0 and 1 (`"Custom Lane Name"`, `C1`) and both areas
/// span the one item on lane 4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompAreaSettings {
    pub start: f64,           // field 1 - area start, project seconds
    pub end: f64,             // field 2 - area end, project seconds
    pub source_lane: i32,     // field 3 - 0-based lane the audio comes from
    pub comp_lane: i32,       // field 4 - 0-based comp lane the audio lands on
    pub unknown_field_5: i32, // field 5 - always -1 in REAPER-saved projects
    pub fade_in: f64,         // field 6 - crossfade in, seconds
    pub fade_out: f64,        // field 7 - crossfade out, seconds
}

/// `C_LANESETTINGS` bits this tree reads. The SDK's full list is in
/// `reaper_plugin_functions.h`; these are the two anything here acts on.
pub mod lane_settings {
    /// Auto-remove empty lanes at the bottom.
    pub const AUTO_REMOVE_EMPTY: i32 = 1;
    /// Big lanes (else small lanes).
    pub const BIG_LANES: i32 = 8;
}

/// The `LANEREC` / `ITEMLANES` / `LINKEDLANE` lines for a comping state,
/// as REAPER writes them — the one place their text form lives, so the
/// project-file exporters and the live backend's chunk patcher cannot
/// drift apart. `LANEREC` is omitted for a track that has never comped,
/// which is what REAPER itself writes.
pub fn comping_lines(comping: &daw_proto::track::LaneComping, lane_count: u32) -> Vec<Vec<String>> {
    let idx = |l: Option<u32>| l.map_or(-1i64, i64::from).to_string();
    let mut lines = Vec::with_capacity(2 + comping.areas.len());
    if *comping != daw_proto::track::LaneComping::default() {
        lines.push(vec![
            "LANEREC".to_string(),
            idx(comping.record_lane),
            idx(comping.comp_lane),
            idx(comping.last_comp_lane),
        ]);
    }
    lines.push(vec!["ITEMLANES".to_string(), lane_count.to_string()]);
    for area in &comping.areas {
        let a = comp_area_from_proto(area);
        lines.push(vec![
            "LINKEDLANE".to_string(),
            a.start.to_string(),
            a.end.to_string(),
            a.source_lane.to_string(),
            a.comp_lane.to_string(),
            a.unknown_field_5.to_string(),
            a.fade_in.to_string(),
            a.fade_out.to_string(),
        ]);
    }
    lines
}

/// The comping state those lines describe — the inverse of
/// [`comping_lines`], over already-tokenized lines.
pub fn comping_from_lines<'a>(
    lines: impl Iterator<Item = &'a [String]>,
) -> daw_proto::track::LaneComping {
    let mut comping = daw_proto::track::LaneComping::default();
    let lane = |t: &[String], i: usize| {
        t.get(i)
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|&l| l >= 0.0)
            .map(|l| l as u32)
    };
    let secs = |t: &[String], i: usize| t.get(i).and_then(|v| v.parse::<f64>().ok());
    for tokens in lines {
        match tokens.first().map(String::as_str) {
            Some("LANEREC") => {
                comping.record_lane = lane(tokens, 1);
                comping.comp_lane = lane(tokens, 2);
                comping.last_comp_lane = lane(tokens, 3);
            }
            Some("LINKEDLANE") => {
                if let (Some(start), Some(end), Some(source_lane), Some(comp_lane)) = (
                    secs(tokens, 1),
                    secs(tokens, 2),
                    lane(tokens, 3),
                    lane(tokens, 4),
                ) {
                    comping.areas.push(comp_area_to_proto(&CompAreaSettings {
                        start,
                        end,
                        source_lane: source_lane as i32,
                        comp_lane: comp_lane as i32,
                        unknown_field_5: -1,
                        fade_in: secs(tokens, 6).unwrap_or(0.0),
                        fade_out: secs(tokens, 7).unwrap_or(0.0),
                    }));
                }
            }
            _ => {}
        }
    }
    comping
}

/// A comp area in the facade's terms.
pub fn comp_area_to_proto(a: &CompAreaSettings) -> daw_proto::track::CompArea {
    use daw_proto::primitives::{Duration, PositionInSeconds};
    daw_proto::track::CompArea {
        start: PositionInSeconds::from_seconds(a.start),
        end: PositionInSeconds::from_seconds(a.end),
        source_lane: a.source_lane.max(0) as u32,
        comp_lane: a.comp_lane.max(0) as u32,
        fade_in: Duration::from_seconds(a.fade_in.max(0.0)),
        fade_out: Duration::from_seconds(a.fade_out.max(0.0)),
    }
}

/// A comp area as REAPER writes it, from the facade's terms.
pub fn comp_area_from_proto(a: &daw_proto::track::CompArea) -> CompAreaSettings {
    CompAreaSettings {
        start: a.start.as_seconds(),
        end: a.end.as_seconds(),
        source_lane: a.source_lane as i32,
        comp_lane: a.comp_lane as i32,
        unknown_field_5: -1,
        fade_in: a.fade_in.as_seconds(),
        fade_out: a.fade_out.as_seconds(),
    }
}

/// The decoded meaning of a track's fixed-lane lines, in the facade's
/// terms.
///
/// This is THE decoding of `FIXEDLANES` / `LANESOLO` / `LANENAME`, shared
/// by every loader so they cannot disagree: `FIXEDLANES` field 1 is the
/// SDK's `C_LANESETTINGS` bitfield (&8 = big lanes), field 3 is "show
/// only the playing lane"; the lane count is the number of `LANENAME`
/// tokens; the play mask is `LANESOLO`'s first 64 bits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FixedLaneState {
    pub lane_count: u32,
    pub lane_play_mask: u64,
    pub lane_names: Vec<String>,
    pub lane_display: daw_proto::track::LaneDisplay,
}

/// The raw lane fields a loader has in hand, before decoding.
///
/// `dawfile-reaper`'s typed [`Track`] fills this from its parsed
/// settings; `dawfile-standalone` fills it straight from the token tree.
/// Both then call [`FixedLaneFields::decode`], which is the one place the
/// meanings live.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FixedLaneFields {
    /// Whether a `FIXEDLANES` line is present at all.
    pub has_fixed_lanes: bool,
    /// `FIXEDLANES` field 1 — `C_LANESETTINGS`.
    pub settings: i32,
    /// `FIXEDLANES` field 3 — show only the playing lane.
    pub show_play_only_lane: bool,
    /// `LANESOLO`'s first two 32-bit words, when the line is present.
    pub lane_solo: Option<(u32, u32)>,
    /// Every `LANENAME` token.
    pub lane_names: Vec<String>,
    /// `ITEMLANES`, when the track carries one.
    pub item_lanes: Option<u32>,
    /// The highest lane any item on the track sits on.
    pub max_item_lane: Option<u32>,
}

impl FixedLaneFields {
    pub fn decode(&self) -> FixedLaneState {
        use daw_proto::track::LaneDisplay;
        // `ITEMLANES` is the count REAPER itself wrote, so it wins. A
        // track saved before it existed falls back to the `LANENAME`
        // token count, then to the largest lane an item sits on — a
        // named lane and an occupied lane both prove the lane is there.
        let lane_count = if !self.has_fixed_lanes {
            0
        } else {
            let named = self.lane_names.len() as u32;
            let occupied = self.max_item_lane.map_or(0, |l| l + 1);
            self.item_lanes.unwrap_or(0).max(named).max(occupied)
        };
        // Lanes on, no LANESOLO (or an all-zero one) → lane 0 plays.
        let lane_play_mask = if lane_count == 0 {
            0
        } else {
            self.lane_solo
                .map(|(lo, hi)| (lo as u64) | ((hi as u64) << 32))
                .filter(|&m| m != 0)
                .unwrap_or(1)
        };
        let lane_display = if !self.has_fixed_lanes {
            LaneDisplay::default()
        } else if self.show_play_only_lane {
            LaneDisplay::One
        } else if self.settings & lane_settings::BIG_LANES != 0 {
            LaneDisplay::Big
        } else {
            LaneDisplay::Small
        };
        FixedLaneState {
            lane_count,
            lane_play_mask,
            lane_names: self.lane_names.clone(),
            lane_display,
        }
    }
}

/// MIDI hardware output settings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiOutputSettings {
    pub device: i32,  // device index (floor(val / 32))
    pub channel: i32, // channel number (val & 0x1F)
}

/// Master/parent send settings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MasterSendSettings {
    pub enabled: bool,        // field 1 - enabled
    pub unknown_field_2: i32, // field 2 - unknown
}

/// Hardware output settings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareOutputSettings {
    pub output_index: i32,        // field 1 - output index
    pub send_mode: i32,           // field 2 - send mode
    pub volume: f64,              // field 3 - volume
    pub pan: f64,                 // field 4 - pan
    pub mute: bool,               // field 5 - mute
    pub invert_polarity: bool,    // field 6 - invert polarity
    pub send_source_channel: i32, // field 7 - send source channel
    pub unknown_field_8: i32,     // field 8 - unknown
    pub automation_mode: i32,     // field 9 - automation mode
}

/// A REAPER track
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    // Basic track properties
    pub name: String,                    // NAME - Track name
    pub selected: bool,                  // SEL - Track selected in TCP/MCP
    pub locked: bool,                    // LOCK - Track controls are locked
    pub peak_color: Option<i32>,         // PEAKCOL - Peak colour
    pub beat: Option<i32>,               // BEAT - Track timebase (-1 = project default)
    pub automation_mode: AutomationMode, // AUTOMODE - Automation mode

    // Volume and pan
    pub volpan: Option<VolPanSettings>, // VOLPAN - Volume/Pan settings

    // Mute and solo
    pub mutesolo: Option<MuteSoloSettings>, // MUTESOLO - Mute/solo settings

    // Phase and folder settings
    pub invert_phase: bool,                      // IPHASE - Invert phase
    pub folder: Option<FolderSettings>,          // ISBUS - Folder settings
    pub bus_compact: Option<BusCompactSettings>, // BUSCOMP - Collapse folder settings

    // Show in mixer
    pub show_in_mixer: Option<ShowInMixerSettings>, // SHOWINMIX - Show in mixer settings

    // Free item positioning
    pub free_mode: Option<FreeMode>, // FREEMODE - Free item positioning mode

    /// `GROUP_FLAGS` — track grouping matrix membership, groups 1–32.
    /// One bitmask per grouping parameter, in REAPER's documented
    /// order (1 = Volume Master, 2 = Volume Follow, …, 21 = VCA
    /// Master, 22 = VCA Follow, 23 = VCA pre-FX Follow, 24/25 =
    /// Media-Edit Lead/Follow). Trailing zero fields are omitted by
    /// REAPER, so the Vec may be short.
    pub group_flags: Option<Vec<u32>>,
    /// `GROUP_FLAGS_HIGH` — same layout for groups 33–64.
    pub group_flags_high: Option<Vec<u32>>,

    // Fixed lanes (REAPER 7+)
    pub fixed_lanes: Option<FixedLanesSettings>, // FIXEDLANES - Fixed lanes settings
    pub lane_solo: Option<LaneSoloSettings>,     // LANESOLO - Lane solo settings
    pub lane_record: Option<LaneRecordSettings>, // LANEREC - Lane record settings
    pub lane_names: Option<LaneNameSettings>,    // LANENAME - Lane names
    pub item_lanes: Option<i32>, // ITEMLANES - lane count the comp areas were written against
    pub comp_areas: Vec<CompAreaSettings>, // LINKEDLANE - comp areas, in file order

    // Record settings
    pub record: Option<RecordSettings>, // REC - Record settings

    // Track height
    pub track_height: Option<TrackHeightSettings>, // TRACKHEIGHT - Height in TCP

    // Input quantize
    pub input_quantize: Option<InputQuantizeSettings>, // INQ - Input quantize settings

    // Channel count
    pub channel_count: u32, // NCHAN - Number of track channels

    // Recording format
    pub rec_cfg: Option<String>, // RECCFG - Recording format data

    // MIDI color map
    pub midi_color_map_fn: Option<String>, // MIDICOLORMAPFN - Path to note color map file

    // FX state
    pub fx_enabled: bool, // FX - FX state (0=bypassed, 1=active)

    // Track ID
    pub track_id: Option<String>, // TRACKID - REAPER track id

    // Performance options
    pub perf: Option<i32>, // PERF - Performance options (bitwise)

    // Layouts
    pub layouts: Option<(String, String)>, // LAYOUTS - Active TCP and MCP layouts

    // Extension data
    pub extension_data: Vec<(String, String)>, // EXT - Extension-specific persistent data

    // Receives
    pub receives: Vec<ReceiveSettings>, // AUXRECV - Track receives

    // MIDI output
    pub midi_output: Option<MidiOutputSettings>, // MIDIOUT - MIDI hardware output settings

    // Custom note order
    pub custom_note_order: Option<Vec<i32>>, // CUSTOM_NOTE_ORDER - Custom note order

    // MIDI note names
    pub midi_note_names: Vec<MidiNoteName>, // MIDINOTENAMES - Custom MIDI note names

    // Master send
    pub master_send: Option<MasterSendSettings>, // MAINSEND - Master/parent send

    // Hardware outputs
    pub hardware_outputs: Vec<HardwareOutputSettings>, // HWOUT - Hardware send data

    // Nested content
    pub items: Vec<Item>,           // ITEM blocks
    pub envelopes: Vec<Envelope>,   // Envelope blocks
    pub fx_chain: Option<FxChain>,  // FXCHAIN - FX section
    pub freeze: Option<FreezeData>, // FREEZE - Freeze data
    pub input_fx: Option<FxChain>,  // FXCHAIN_REC - Input FX section

    // Raw content for preservation
    pub raw_content: String,
}

/// Controls nested parsing depth when decoding tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackParseOptions {
    pub parse_items: bool,
    pub parse_envelopes: bool,
    pub parse_fx_chain: bool,
}

impl TrackParseOptions {
    pub const fn full() -> Self {
        Self {
            parse_items: true,
            parse_envelopes: true,
            parse_fx_chain: true,
        }
    }

    pub const fn summary() -> Self {
        Self {
            parse_items: false,
            parse_envelopes: false,
            parse_fx_chain: false,
        }
    }
}

/// Receive settings for tracks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReceiveSettings {
    pub source_track_index: i32, // field 1 - source track index (zero based)
    pub mode: i32,               // field 2 - mode (0=Post Fader, 1=Pre FX, 3=Pre Fader)
    pub volume: f64,             // field 3 - volume
    pub pan: f64,                // field 4 - pan
    pub mute: bool,              // field 5 - mute
    pub mono_sum: bool,          // field 6 - mono sum
    pub invert_polarity: bool,   // field 7 - invert polarity
    pub source_audio_channels: i32, // field 8 - source audio channels
    pub dest_audio_channels: i32, // field 9 - dest audio channels
    pub pan_law: f64,            // field 10 - pan law
    pub midi_channels: i32,      // field 11 - midi channels
    pub automation_mode: i32,    // field 12 - automation mode (-1 = use track mode)
}

impl ReceiveSettings {
    /// Decode REAPER's packed MIDI routing flags into a typed channel mapping.
    pub fn midi_channel_mapping(&self) -> MidiChannelMapping {
        let src_bits = self.midi_channels & 0x1f;
        let dst_bits = (self.midi_channels >> 5) & 0x1f;

        let source = match src_bits {
            1..=16 => MidiSourceChannel::Channel(src_bits as u8),
            _ => MidiSourceChannel::All,
        };
        let destination = match dst_bits {
            1..=16 => MidiDestinationChannel::Channel(dst_bits as u8),
            _ => MidiDestinationChannel::Original,
        };

        MidiChannelMapping {
            source,
            destination,
        }
    }
}

/// MIDI note name
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiNoteName {
    pub channel: i32,         // field 1 - MIDI channel number (-1 = Omni)
    pub note_number: i32,     // field 2 - 0-based note number
    pub note_name: String,    // field 3 - note name
    pub unknown_field_4: i32, // field 4 - unknown
    pub note_number_2: i32,   // field 5 - note number
}

// FxChain is now defined in fx_chain.rs and imported above.

/// Freeze data (placeholder - will be implemented separately)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FreezeData {
    pub raw_content: String,
}

impl Default for Track {
    fn default() -> Self {
        Self {
            name: String::new(),
            selected: false,
            locked: false,
            peak_color: None,
            beat: None,
            automation_mode: AutomationMode::TrimRead,
            volpan: None,
            mutesolo: None,
            invert_phase: false,
            folder: None,
            bus_compact: None,
            show_in_mixer: None,
            free_mode: None,
            group_flags: None,
            group_flags_high: None,
            fixed_lanes: None,
            lane_solo: None,
            lane_record: None,
            lane_names: None,
            item_lanes: None,
            comp_areas: Vec::new(),
            record: None,
            track_height: None,
            input_quantize: None,
            channel_count: 2,
            rec_cfg: None,
            midi_color_map_fn: None,
            fx_enabled: true,
            track_id: None,
            perf: None,
            layouts: None,
            extension_data: vec![],
            receives: vec![],
            midi_output: None,
            custom_note_order: None,
            midi_note_names: vec![],
            master_send: None,
            hardware_outputs: vec![],
            items: vec![],
            envelopes: vec![],
            fx_chain: None,
            freeze: None,
            input_fx: None,
            raw_content: String::new(),
        }
    }
}

impl Track {
    /// The raw fixed-lane fields, ready for [`FixedLaneFields::decode`].
    pub fn fixed_lane_fields(&self) -> FixedLaneFields {
        FixedLaneFields {
            has_fixed_lanes: self.fixed_lanes.is_some(),
            settings: self.fixed_lanes.as_ref().map(|f| f.bitfield).unwrap_or(0),
            show_play_only_lane: self
                .fixed_lanes
                .as_ref()
                .is_some_and(|f| f.show_play_only_lane),
            lane_solo: self
                .lane_solo
                .as_ref()
                .map(|ls| (ls.playing_lanes as u32, ls.unknown_field_2 as u32)),
            lane_names: self
                .lane_names
                .as_ref()
                .map(|ln| ln.lane_names.clone())
                .unwrap_or_default(),
            item_lanes: self.item_lanes.filter(|&n| n >= 0).map(|n| n as u32),
            max_item_lane: self
                .items
                .iter()
                .filter_map(|i| i.lane)
                .filter(|&l| l >= 0)
                .max()
                .map(|l| l as u32),
        }
    }

    /// The track's fixed lanes in the facade's terms.
    pub fn fixed_lane_state(&self) -> FixedLaneState {
        self.fixed_lane_fields().decode()
    }

    /// The track's comping state in the facade's terms: `LANEREC` plus
    /// every `LINKEDLANE`. A negative `LANEREC` index is "unset".
    pub fn lane_comping(&self) -> daw_proto::track::LaneComping {
        let lane = |i: i32| (i >= 0).then_some(i as u32);
        daw_proto::track::LaneComping {
            record_lane: self
                .lane_record
                .as_ref()
                .and_then(|r| lane(r.record_enabled_lane)),
            comp_lane: self
                .lane_record
                .as_ref()
                .and_then(|r| lane(r.comping_enabled_lane)),
            last_comp_lane: self
                .lane_record
                .as_ref()
                .and_then(|r| lane(r.last_comping_lane)),
            areas: self.comp_areas.iter().map(comp_area_to_proto).collect(),
        }
    }

    fn fast_classify_track_token(raw: &str) -> Token {
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

    fn parse_track_token_line(line: &str) -> Result<Vec<Token>, String> {
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
        out.push(Self::fast_classify_track_token(first));
        out.extend(parts.map(Self::fast_classify_track_token));
        Ok(out)
    }

    fn should_parse_track_raw_line(raw: &str) -> bool {
        let key = raw.split_whitespace().next().unwrap_or_default();
        matches!(
            key,
            "NAME"
                | "EXT"
                | "LOCK"
                | "SEL"
                | "PEAKCOL"
                | "BEAT"
                | "AUTOMODE"
                | "VOLPAN"
                | "MUTESOLO"
                | "IPHASE"
                | "ISBUS"
                | "BUSCOMP"
                | "SHOWINMIX"
                | "FREEMODE"
                | "GROUP_FLAGS"
                | "GROUP_FLAGS_HIGH"
                | "FIXEDLANES"
                | "LANEREC"
                | "LANENAME"
                | "LANESOLO"
                | "ITEMLANES"
                | "LINKEDLANE"
                | "REC"
                | "TRACKHEIGHT"
                | "INQ"
                | "PERF"
                | "LAYOUTS"
                | "AUXRECV"
                | "MIDIOUT"
                | "MAINSEND"
                | "HWOUT"
                | "NCHAN"
                | "FX"
                | "TRACKID"
        )
    }

    fn parse_identifier_number(s: &str) -> Option<f64> {
        if let Ok(v) = s.parse::<f64>() {
            return Some(v);
        }
        let prefix = s.split(':').next().unwrap_or_default();
        prefix.parse::<f64>().ok()
    }

    /// Parse a float from a token
    fn parse_float(token: &Token) -> Result<f64, String> {
        match token {
            Token::Float(f) => Ok(*f),
            Token::Integer(i) => Ok(*i as f64),
            Token::Identifier(s) => Self::parse_identifier_number(s)
                .ok_or_else(|| format!("Expected float or integer, got {:?}", token)),
            _ => Err(format!("Expected float or integer, got {:?}", token)),
        }
    }

    /// Parse an integer from a token
    fn parse_int(token: &Token) -> Result<i32, String> {
        match token {
            Token::Integer(i) => Ok(*i as i32),
            Token::Float(f) => Ok(*f as i32),
            Token::Identifier(s) => Self::parse_identifier_number(s)
                .map(|v| v as i32)
                .ok_or_else(|| format!("Expected integer or float, got {:?}", token)),
            _ => Err(format!("Expected integer or float, got {:?}", token)),
        }
    }

    /// Parse a boolean from a token
    fn parse_bool(token: &Token) -> Result<bool, String> {
        match token {
            Token::Integer(i) => Ok(*i != 0),
            Token::Float(f) => Ok(*f != 0.0),
            Token::Identifier(s) => Self::parse_identifier_number(s)
                .map(|v| v != 0.0)
                .ok_or_else(|| format!("Expected integer or float for boolean, got {:?}", token)),
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

    fn apply_tokens(track: &mut Track, tokens: &[Token]) -> Result<(), String> {
        if tokens.is_empty() {
            return Ok(());
        }
        let identifier = match &tokens[0] {
            Token::Identifier(id) => id,
            _ => return Ok(()),
        };

        match identifier.as_str() {
            "NAME" if tokens.len() > 1 => {
                let new_name = Self::parse_string(&tokens[1])?;
                if track.name.is_empty() {
                    track.name = new_name;
                }
            }
            "LOCK" if tokens.len() > 1 => {
                track.locked = Self::parse_bool(&tokens[1])?;
            }
            // An extension's key and its value, kept as written: the value
            // may hold spaces (`EXT FTS:group Drums/Drum Kit/Kick`).
            "EXT" if tokens.len() > 1 => {
                let value = tokens[2..]
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" ");
                track.extension_data.push((tokens[1].to_string(), value));
            }
            "SEL" if tokens.len() > 1 => {
                track.selected = Self::parse_bool(&tokens[1])?;
            }
            "PEAKCOL" if tokens.len() > 1 => {
                track.peak_color = Some(Self::parse_int(&tokens[1])?);
            }
            "BEAT" if tokens.len() > 1 => {
                track.beat = Some(Self::parse_int(&tokens[1])?);
            }
            "AUTOMODE" if tokens.len() > 1 => {
                track.automation_mode = AutomationMode::from(Self::parse_int(&tokens[1])?);
            }
            "VOLPAN" if tokens.len() >= 4 => {
                track.volpan = Some(VolPanSettings {
                    volume: Self::parse_float(&tokens[1])?,
                    pan: Self::parse_float(&tokens[2])?,
                    pan_law: Self::parse_float(&tokens[3])?,
                });
            }
            "MUTESOLO" if tokens.len() >= 4 => {
                track.mutesolo = Some(MuteSoloSettings {
                    mute: Self::parse_bool(&tokens[1])?,
                    solo: TrackSoloState::from(Self::parse_int(&tokens[2])?),
                    solo_defeat: Self::parse_bool(&tokens[3])?,
                });
            }
            "IPHASE" if tokens.len() > 1 => {
                track.invert_phase = Self::parse_bool(&tokens[1])?;
            }
            "ISBUS" if tokens.len() >= 3 => {
                track.folder = Some(FolderSettings {
                    folder_state: FolderState::from(Self::parse_int(&tokens[1])?),
                    indentation: Self::parse_int(&tokens[2])?,
                });
            }
            "BUSCOMP" if tokens.len() >= 6 => {
                track.bus_compact = Some(BusCompactSettings {
                    arrange_collapse: Self::parse_int(&tokens[1])?,
                    mixer_collapse: Self::parse_int(&tokens[2])?,
                    wiring_collapse: Self::parse_int(&tokens[3])?,
                    wiring_x: Self::parse_int(&tokens[4])?,
                    wiring_y: Self::parse_int(&tokens[5])?,
                });
            }
            "SHOWINMIX" if tokens.len() >= 9 => {
                track.show_in_mixer = Some(ShowInMixerSettings {
                    show_in_mixer: Self::parse_bool(&tokens[1])?,
                    unknown_field_2: Self::parse_float(&tokens[2])?,
                    unknown_field_3: Self::parse_float(&tokens[3])?,
                    show_in_track_list: Self::parse_bool(&tokens[4])?,
                    unknown_field_5: Self::parse_float(&tokens[5])?,
                    unknown_field_6: Self::parse_int(&tokens[6])?,
                    unknown_field_7: Self::parse_int(&tokens[7])?,
                    unknown_field_8: Self::parse_int(&tokens[8])?,
                });
            }
            "FREEMODE" if tokens.len() > 1 => {
                track.free_mode = Some(FreeMode::from(Self::parse_int(&tokens[1])?));
            }
            "GROUP_FLAGS" | "GROUP_FLAGS_HIGH" if tokens.len() >= 2 => {
                // Per-parameter group bitmasks (trailing zeros omitted
                // by REAPER). Values can exceed i32 textually
                // (4294967295) — `as u32` keeps the bits.
                let masks: Vec<u32> = tokens[1..]
                    .iter()
                    .map(|t| Self::parse_int(t).map(|v| v as u32))
                    .collect::<Result<_, _>>()?;
                if identifier == "GROUP_FLAGS" {
                    track.group_flags = Some(masks);
                } else {
                    track.group_flags_high = Some(masks);
                }
            }
            "FIXEDLANES" if tokens.len() >= 6 => {
                track.fixed_lanes = Some(FixedLanesSettings {
                    bitfield: Self::parse_int(&tokens[1])?,
                    allow_editing: Self::parse_bool(&tokens[2])?,
                    show_play_only_lane: Self::parse_bool(&tokens[3])?,
                    mask_playback: Self::parse_bool(&tokens[4])?,
                    recording_behavior: Self::parse_int(&tokens[5])?,
                });
            }
            "LANEREC" if tokens.len() >= 4 => {
                track.lane_record = Some(LaneRecordSettings {
                    record_enabled_lane: Self::parse_int(&tokens[1])?,
                    comping_enabled_lane: Self::parse_int(&tokens[2])?,
                    last_comping_lane: Self::parse_int(&tokens[3])?,
                });
            }
            "ITEMLANES" if tokens.len() >= 2 => {
                track.item_lanes = Some(Self::parse_int(&tokens[1])?);
            }
            "LINKEDLANE" if tokens.len() >= 8 => {
                track.comp_areas.push(CompAreaSettings {
                    start: Self::parse_float(&tokens[1])?,
                    end: Self::parse_float(&tokens[2])?,
                    source_lane: Self::parse_int(&tokens[3])?,
                    comp_lane: Self::parse_int(&tokens[4])?,
                    unknown_field_5: Self::parse_int(&tokens[5])?,
                    fade_in: Self::parse_float(&tokens[6])?,
                    fade_out: Self::parse_float(&tokens[7])?,
                });
            }
            "LANENAME" if tokens.len() >= 2 => {
                // EVERY token after the keyword is a lane name —
                // `LANENAME 1 2 3` is a 3-lane track whose lanes are
                // named "1", "2", "3" (REAPER's defaults), not a count
                // followed by names.
                let lane_names = tokens[1..]
                    .iter()
                    .map(Self::parse_string)
                    .collect::<Result<Vec<_>, _>>()?;
                track.lane_names = Some(LaneNameSettings {
                    lane_count: lane_names.len() as i32,
                    lane_names,
                });
            }
            "LANESOLO" if tokens.len() >= 2 => {
                // Bitfield of PLAYING lanes, stored as up to 8 32-bit
                // words (256 lanes). Values like 4294967295 overflow
                // i32 textually but wrap bit-exact via i64 → i32.
                let word = |i: usize| -> Result<i32, String> {
                    tokens.get(i).map(Self::parse_int).unwrap_or(Ok(0))
                };
                track.lane_solo = Some(LaneSoloSettings {
                    playing_lanes: word(1)?,
                    unknown_field_2: word(2)?,
                    unknown_field_3: word(3)?,
                    unknown_field_4: word(4)?,
                    unknown_field_5: word(5)?,
                    unknown_field_6: word(6)?,
                    unknown_field_7: word(7)?,
                    unknown_field_8: word(8)?,
                });
            }
            "REC" if tokens.len() >= 8 => {
                track.record = Some(RecordSettings {
                    armed: Self::parse_bool(&tokens[1])?,
                    input: Self::parse_int(&tokens[2])?,
                    monitor: MonitorMode::from(Self::parse_int(&tokens[3])?),
                    record_mode: RecordMode::from(Self::parse_int(&tokens[4])?),
                    monitor_track_media: Self::parse_bool(&tokens[5])?,
                    preserve_pdc_delayed: Self::parse_bool(&tokens[6])?,
                    record_path: Self::parse_int(&tokens[7])?,
                });
            }
            "TRACKHEIGHT" if tokens.len() >= 3 => {
                track.track_height = Some(TrackHeightSettings {
                    height: Self::parse_int(&tokens[1])?,
                    folder_override: Self::parse_bool(&tokens[2])?,
                });
            }
            "INQ" if tokens.len() >= 9 => {
                track.input_quantize = Some(InputQuantizeSettings {
                    quantize_midi: Self::parse_bool(&tokens[1])?,
                    quantize_to_pos: Self::parse_int(&tokens[2])?,
                    quantize_note_offs: Self::parse_bool(&tokens[3])?,
                    quantize_to: Self::parse_float(&tokens[4])?,
                    quantize_strength: Self::parse_int(&tokens[5])?,
                    swing_strength: Self::parse_int(&tokens[6])?,
                    quantize_range_min: Self::parse_int(&tokens[7])?,
                    quantize_range_max: Self::parse_int(&tokens[8])?,
                });
            }
            "PERF" if tokens.len() > 1 => {
                track.perf = Some(Self::parse_int(&tokens[1])?);
            }
            "LAYOUTS" if tokens.len() >= 3 => {
                let tcp_layout = Self::parse_string(&tokens[1])?;
                let mcp_layout = Self::parse_string(&tokens[2])?;
                track.layouts = Some((tcp_layout, mcp_layout));
            }
            "AUXRECV" if tokens.len() >= 13 => {
                track.receives.push(ReceiveSettings {
                    source_track_index: Self::parse_int(&tokens[1])?,
                    mode: Self::parse_int(&tokens[2])?,
                    volume: Self::parse_float(&tokens[3])?,
                    pan: Self::parse_float(&tokens[4])?,
                    mute: Self::parse_bool(&tokens[5])?,
                    mono_sum: Self::parse_bool(&tokens[6])?,
                    invert_polarity: Self::parse_bool(&tokens[7])?,
                    source_audio_channels: Self::parse_int(&tokens[8])?,
                    dest_audio_channels: Self::parse_int(&tokens[9])?,
                    pan_law: Self::parse_float(&tokens[10])?,
                    midi_channels: Self::parse_int(&tokens[11])?,
                    automation_mode: Self::parse_int(&tokens[12])?,
                });
            }
            "MIDIOUT" if tokens.len() > 1 => {
                let value = Self::parse_int(&tokens[1])?;
                track.midi_output = Some(MidiOutputSettings {
                    device: value / 32,
                    channel: value & 0x1F,
                });
            }
            "MAINSEND" if tokens.len() >= 3 => {
                track.master_send = Some(MasterSendSettings {
                    enabled: Self::parse_bool(&tokens[1])?,
                    unknown_field_2: Self::parse_int(&tokens[2])?,
                });
            }
            "HWOUT" if tokens.len() >= 10 => {
                track.hardware_outputs.push(HardwareOutputSettings {
                    output_index: Self::parse_int(&tokens[1])?,
                    send_mode: Self::parse_int(&tokens[2])?,
                    volume: Self::parse_float(&tokens[3])?,
                    pan: Self::parse_float(&tokens[4])?,
                    mute: Self::parse_bool(&tokens[5])?,
                    invert_polarity: Self::parse_bool(&tokens[6])?,
                    send_source_channel: Self::parse_int(&tokens[7])?,
                    unknown_field_8: Self::parse_int(&tokens[8])?,
                    automation_mode: Self::parse_int(&tokens[9])?,
                });
            }
            "NCHAN" if tokens.len() > 1 => {
                track.channel_count = Self::parse_int(&tokens[1])? as u32;
            }
            "FX" if tokens.len() > 1 => {
                track.fx_enabled = Self::parse_bool(&tokens[1])?;
            }
            "TRACKID" if tokens.len() > 1 => {
                track.track_id = Some(Self::parse_string(&tokens[1])?);
            }
            _ => {}
        }

        Ok(())
    }

    /// Create a Track from a parsed RPP block
    // r[impl rpp.parse.track]
    pub fn from_block(block: &RppBlock) -> Result<Self, String> {
        Self::from_block_with_options(block, TrackParseOptions::full())
    }

    /// Create a Track from a parsed RPP block with configurable nested parsing.
    pub fn from_block_with_options(
        block: &RppBlock,
        options: TrackParseOptions,
    ) -> Result<Self, String> {
        if block.block_type != BlockType::Track {
            return Err(format!("Expected Track block, got {:?}", block.block_type));
        }
        let mut track = Track {
            name: String::new(),
            selected: false,
            locked: false,
            peak_color: None,
            beat: None,
            automation_mode: AutomationMode::TrimRead,
            volpan: None,
            mutesolo: None,
            invert_phase: false,
            folder: None,
            bus_compact: None,
            show_in_mixer: None,
            free_mode: None,
            group_flags: None,
            group_flags_high: None,
            fixed_lanes: None,
            lane_solo: None,
            lane_record: None,
            lane_names: None,
            item_lanes: None,
            comp_areas: Vec::new(),
            record: None,
            track_height: None,
            input_quantize: None,
            channel_count: 2,
            rec_cfg: None,
            midi_color_map_fn: None,
            fx_enabled: false,
            track_id: None,
            perf: None,
            layouts: None,
            extension_data: Vec::new(),
            receives: Vec::new(),
            midi_output: None,
            custom_note_order: None,
            midi_note_names: Vec::new(),
            master_send: None,
            hardware_outputs: Vec::new(),
            items: Vec::new(),
            envelopes: Vec::new(),
            fx_chain: None,
            freeze: None,
            input_fx: None,
            raw_content: String::new(),
        };
        for child in &block.children {
            match child {
                crate::primitives::RppBlockContent::Content(tokens) => {
                    let reparsed;
                    let tokens = if let [Token::Identifier(raw)] = tokens.as_slice() {
                        if raw.contains(' ') && Self::should_parse_track_raw_line(raw) {
                            reparsed = Self::parse_track_token_line(raw)?;
                            reparsed.as_slice()
                        } else {
                            tokens.as_slice()
                        }
                    } else {
                        tokens.as_slice()
                    };
                    Self::apply_tokens(&mut track, tokens)?;
                }
                crate::primitives::RppBlockContent::RawLine(raw) => {
                    if Self::should_parse_track_raw_line(raw) {
                        let tokens = Self::parse_track_token_line(raw)?;
                        Self::apply_tokens(&mut track, &tokens)?;
                    }
                }
                crate::primitives::RppBlockContent::Block(nested_block) => {
                    if options.parse_items && nested_block.block_type == BlockType::Item {
                        if let Ok(item) = Item::from_block(nested_block) {
                            track.items.push(item);
                        }
                        continue;
                    }

                    if options.parse_envelopes && nested_block.block_type == BlockType::Envelope {
                        if let Ok(envelope) = Envelope::from_block(nested_block) {
                            track.envelopes.push(envelope);
                        }
                        continue;
                    }

                    if options.parse_fx_chain
                        && (nested_block.name == "FXCHAIN" || nested_block.name == "FXCHAIN_REC")
                    {
                        match FxChain::from_block(nested_block) {
                            Ok(parsed) => track.fx_chain = Some(parsed),
                            Err(_) => {
                                track.fx_chain = Some(FxChain {
                                    window_rect: None,
                                    show: 0,
                                    last_sel: 0,
                                    docked: false,
                                    nodes: Vec::new(),
                                    raw_content: nested_block.to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(track)
    }

    /// Create a Track from a raw RPP track block string
    pub fn from_rpp_block(block_content: &str) -> Result<Self, String> {
        Self::from_rpp_block_with_options(block_content, TrackParseOptions::full())
    }

    /// Create a Track from a raw RPP track block string with configurable nested parsing.
    pub fn from_rpp_block_with_options(
        block_content: &str,
        options: TrackParseOptions,
    ) -> Result<Self, String> {
        let mut track = Track {
            name: String::new(),
            selected: false,
            locked: false,
            peak_color: None,
            beat: None,
            automation_mode: AutomationMode::TrimRead,
            volpan: None,
            mutesolo: None,
            invert_phase: false,
            folder: None,
            bus_compact: None,
            show_in_mixer: None,
            free_mode: None,
            group_flags: None,
            group_flags_high: None,
            fixed_lanes: None,
            lane_solo: None,
            lane_record: None,
            lane_names: None,
            item_lanes: None,
            comp_areas: Vec::new(),
            record: None,
            track_height: None,
            input_quantize: None,
            channel_count: 2,
            rec_cfg: None,
            midi_color_map_fn: None,
            fx_enabled: false,
            track_id: None,
            perf: None,
            layouts: None,
            extension_data: Vec::new(),
            receives: Vec::new(),
            midi_output: None,
            custom_note_order: None,
            midi_note_names: Vec::new(),
            master_send: None,
            hardware_outputs: Vec::new(),
            items: Vec::new(),
            envelopes: Vec::new(),
            fx_chain: None,
            freeze: None,
            input_fx: None,
            raw_content: block_content.to_string(),
        };

        let lines: Vec<&str> = block_content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with("//") {
                i += 1;
                continue;
            }

            // Skip the opening <TRACK line
            if line.starts_with("<TRACK") {
                i += 1;
                continue;
            }

            // Handle nested blocks
            if line.starts_with('<') {
                if line.starts_with("<ITEM") {
                    // Parse item block with proper nested block handling
                    let item_start = i;
                    i += 1;
                    let mut depth = 1; // Track nesting depth

                    while i < lines.len() && depth > 0 {
                        let current_line = lines[i].trim();
                        if current_line.starts_with('<') {
                            depth += 1; // Entering nested block
                        } else if current_line == ">" {
                            depth -= 1; // Exiting block
                        }
                        i += 1;
                    }

                    // Extract item block content
                    let item_lines = &lines[item_start..i];
                    let item_content = item_lines.join("\n");

                    // Parse the item (optional in summary mode)
                    if options.parse_items
                        && let Ok(item) = Item::from_rpp_block(&item_content)
                    {
                        track.items.push(item);
                    }
                    continue;
                } else if line.starts_with("<VOLENV")
                    || line.starts_with("<PANENV")
                    || line.starts_with("<PARMENV")
                {
                    // Parse envelope block with proper nested block handling
                    let env_start = i;
                    i += 1;
                    let mut depth = 1; // Track nesting depth

                    while i < lines.len() && depth > 0 {
                        let current_line = lines[i].trim();
                        if current_line.starts_with('<') {
                            depth += 1; // Entering nested block
                        } else if current_line == ">" {
                            depth -= 1; // Exiting block
                        }
                        i += 1;
                    }

                    // Extract envelope block content
                    let env_lines = &lines[env_start..i];
                    let env_content = env_lines.join("\n");

                    // Parse the envelope (optional in summary mode)
                    if options.parse_envelopes
                        && let Ok((_, block)) = crate::primitives::block::parse_block(&env_content)
                        && let Ok(envelope) = Envelope::from_block(&block)
                    {
                        track.envelopes.push(envelope);
                    }
                    continue;
                } else if line.starts_with("<FXCHAIN") {
                    // Parse FX chain block with proper nested block handling
                    let fx_start = i;
                    i += 1;
                    let mut depth = 1; // Track nesting depth

                    while i < lines.len() && depth > 0 {
                        let current_line = lines[i].trim();
                        if current_line.starts_with('<') {
                            depth += 1; // Entering nested block
                        } else if current_line == ">" {
                            depth -= 1; // Exiting block
                        }
                        i += 1;
                    }

                    // Extract FX chain block content
                    let fx_lines = &lines[fx_start..i];
                    let fx_content = fx_lines.join("\n");

                    if options.parse_fx_chain {
                        // Parse the FX chain using the full parser
                        match FxChain::parse(&fx_content) {
                            Ok(parsed) => track.fx_chain = Some(parsed),
                            Err(_) => {
                                // Fallback: store with raw content only
                                track.fx_chain = Some(FxChain {
                                    window_rect: None,
                                    show: 0,
                                    last_sel: 0,
                                    docked: false,
                                    nodes: Vec::new(),
                                    raw_content: fx_content,
                                });
                            }
                        }
                    }
                    continue;
                } else {
                    // Skip other nested blocks with proper depth handling
                    i += 1;
                    let mut depth = 1; // Track nesting depth

                    while i < lines.len() && depth > 0 {
                        let current_line = lines[i].trim();
                        if current_line.starts_with('<') {
                            depth += 1; // Entering nested block
                        } else if current_line == ">" {
                            depth -= 1; // Exiting block
                        }
                        i += 1;
                    }
                    continue;
                }
            }

            // Skip block end markers
            if line == ">" {
                i += 1;
                continue;
            }

            // Parse token line
            let tokens = match Self::parse_track_token_line(line) {
                Ok(tokens) => tokens,
                Err(_) => {
                    i += 1;
                    continue;
                }
            };

            Self::apply_tokens(&mut track, &tokens)?;

            i += 1;
        }

        Ok(track)
    }
}

impl fmt::Display for Track {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Track: {}", self.name)?;

        if self.locked {
            writeln!(f, "  Locked: true")?;
        }

        if let Some(ref volpan) = self.volpan {
            writeln!(
                f,
                "  VolPan: volume: {:.2}, pan: {:.2}, pan_law: {:.2}",
                volpan.volume, volpan.pan, volpan.pan_law
            )?;
        }

        if let Some(ref mutesolo) = self.mutesolo {
            writeln!(
                f,
                "  MuteSolo: mute: {}, solo: {}, solo_defeat: {}",
                mutesolo.mute, mutesolo.solo, mutesolo.solo_defeat
            )?;
        }

        writeln!(f, "  Automation Mode: {}", self.automation_mode)?;
        writeln!(f, "  Channels: {}", self.channel_count)?;
        writeln!(f, "  FX Enabled: {}", self.fx_enabled)?;

        if let Some(ref id) = self.track_id {
            writeln!(f, "  Track ID: {}", id)?;
        }

        if let Some(ref folder) = self.folder {
            writeln!(
                f,
                "  Folder: {}, Indentation: {}",
                folder.folder_state, folder.indentation
            )?;
        }

        if let Some(ref free_mode) = self.free_mode {
            writeln!(f, "  Free Mode: {}", free_mode)?;
        }

        if let Some(ref record) = self.record {
            writeln!(
                f,
                "  Record: armed: {}, mode: {}, monitor: {}",
                record.armed, record.record_mode, record.monitor
            )?;
        }

        writeln!(
            f,
            "  Items: {}, Envelopes: {}, Receives: {}",
            self.items.len(),
            self.envelopes.len(),
            self.receives.len()
        )?;

        Ok(())
    }
}

impl fmt::Display for VolPanSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "volume: {:.2}, pan: {:.2}, pan_law: {:.2}",
            self.volume, self.pan, self.pan_law
        )
    }
}

impl fmt::Display for MuteSoloSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "mute: {}, solo: {}, solo_defeat: {}",
            self.mute, self.solo, self.solo_defeat
        )
    }
}

impl fmt::Display for FolderSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "state: {}, indentation: {}",
            self.folder_state, self.indentation
        )
    }
}

impl fmt::Display for RecordSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "armed: {}, input: {}, mode: {}, monitor: {}",
            self.armed, self.input, self.record_mode, self.monitor
        )
    }
}

impl fmt::Display for ReceiveSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "source: {}, mode: {}, volume: {:.2}, pan: {:.2}",
            self.source_track_index, self.mode, self.volume, self.pan
        )
    }
}

impl fmt::Display for MidiNoteName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ch: {}, note: {}, name: \"{}\"",
            self.channel, self.note_number, self.note_name
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A template's ext-state comes back as written: the key, and the
    /// value with its spaces.
    #[test]
    fn extension_lines_are_kept() {
        let track = Track::from_rpp_block(
            "<TRACK\n  NAME \"Kick\"\n  EXT FTS:kind piece\n  EXT FTS:group Drums/Drum Kit/Kick\n>",
        )
        .expect("a track");
        assert_eq!(
            track.extension_data,
            [
                ("FTS:kind".to_owned(), "piece".to_owned()),
                ("FTS:group".to_owned(), "Drums/Drum Kit/Kick".to_owned()),
            ]
        );
    }

    #[test]
    fn test_parse_simple_track() {
        let track_content = r#"<TRACK
  NAME "Test Track"
  VOLPAN 0.8 0.0 -1.0
  MUTESOLO 0 0 0
  AUTOMODE 0
  NCHAN 2
  FX 1
  TRACKID {12345678-1234-1234-1234-123456789ABC}
>"#;

        let track = Track::from_rpp_block(track_content).expect("Failed to parse track");

        assert_eq!(track.name, "Test Track");
        assert!(track.volpan.is_some());
        assert!(track.mutesolo.is_some());
        assert_eq!(track.automation_mode, AutomationMode::TrimRead);
        assert_eq!(track.channel_count, 2);
        assert!(track.fx_enabled);
        assert!(track.track_id.is_some());

        println!("🎵 Parsed Simple Track:");
        println!("==========================================");
        println!("{}", track);
    }

    #[test]
    fn test_parse_complex_track() {
        let track_content = r#"<TRACK
  NAME ""
  PEAKCOL 16576
  BEAT -1
  AUTOMODE 0
  PANLAWFLAGS 3
  VOLPAN 0.11521806013581 0 -1 -1 1
  MUTESOLO 0 0 0
  IPHASE 0
  PLAYOFFS 0 1
  ISBUS 0 0
  BUSCOMP 0 0 0 0 0
  SHOWINMIX 1 0.6667 0.5 1 0.5 0 0 0
  FIXEDLANES 9 0 0 0 0
  REC 1 0 1 0 0 0 0 0
  VU 64
  TRACKHEIGHT 0 0 0 0 0 0 0
  INQ 0 0 0 0.5 100 0 0 100
  NCHAN 2
  FX 1
  TRACKID {4830A644-41E5-C39E-2F21-0E4D38CF4670}
  PERF 0
  MIDIOUT -1
  MAINSEND 1 0
  <VOLENV2
    EGUID {E1A0A26B-6BF4-C6BD-873D-DB57E2C20775}
    ACT 0 -1
    VIS 1 1 1
    LANEHEIGHT 0 0
    ARM 0
    DEFSHAPE 0 -1 -1
    VOLTYPE 1
    POOLEDENVINST 1 5 1.5 0 1 0 0.5 1 1 0 0 1 0 0 0.012 0
    PT 0 1 0
    PT 1.9995 0.02077026 0
    PT 2.4995 0.04468678 0
    PT 4.5005 0.19575294 0
    PT 5.0005 0.25703958 0
    PT 6.4995 0.25703958 0
    PT 6.5 0.02073425 5 1 0 0 0.3030303
    PT 8.48 0.25703958 0
    PT 8.5 0.22266414 0
    PT 8.5005 0.25703958 0
    PT 12.4995 0.25703958 0
    PT 12.5 0.49741377 0
    PT 13.756 0.04391371 0
    PT 13.7565 0.25703958 0
  >
  <FXCHAIN
    SHOW 0
    LASTSEL 0
    DOCKED 0
  >
  <ITEM
    POSITION 2
    SNAPOFFS 0
    LENGTH 9.128
    LOOP 1
    ALLTAKES 0
    FADEIN 1 0.01 0 1 0 0 0
    FADEOUT 1 0.01 0 1 0 0 0
    MUTE 0 0
    SEL 1
    IGUID {BFDF0A90-C87D-258E-6687-8349315CE898}
    IID 11
    NAME 01-250919_0557.wav
    VOLPAN 6.16595 0 1 -1
    SOFFS 0
    PLAYRATE 1 1 0 -1 0 0.0025
    CHANMODE 0
    TAKECOLOR 33520034 B
    GUID {505F73DE-A7B2-4688-2CDE-B4EB4130FF9E}
    RECPASS 5
    <SOURCE WAVE
      FILE "/home/cody/Documents/REAPER Media/01-250919_0557.wav"
    >
    <VOLENV
      EGUID {019A2088-083D-587E-388C-12C5C853AF55}
      ACT 0 -1
      VIS 1 1 1
      LANEHEIGHT 0 0
      ARM 0
      DEFSHAPE 0 -1 -1
      VOLTYPE 1
      PT 0 1 0
      PT 0.0001 7.94328235 0
      PT 1.63929583 7.94328235 0
      PT 1.6393375 7.92253126 0
      PT 1.67144167 1.7197714 0
      PT 1.70354583 0.8809873 0
      PT 1.73565 0.60931064 0
      PT 1.76775417 0.48474096 0
      PT 1.79985833 0.43177243 0
      PT 1.8319625 0.41595908 0
      PT 1.86406667 0.41612958 0
      PT 1.89617083 0.41947573 0
      PT 1.928275 0.42481026 0
      PT 1.96037917 0.43250362 0
      PT 1.99248333 0.44196018 0
      PT 2.0245875 0.45310002 0
      PT 2.05669167 0.46597865 0
      PT 2.08879583 0.48045697 0
      PT 2.1209 0.49653329 0
      PT 2.15300417 0.51426791 0
      PT 2.18510833 0.53336754 0
      PT 2.2172125 0.55409804 0
      PT 2.24931667 0.57640986 0
      PT 2.28142083 0.60039745 0
      PT 2.313525 0.62597704 0
      PT 2.34562917 0.65352901 0
      PT 2.37773333 0.68326556 0
      PT 2.4098375 0.71492433 0
      PT 2.44194167 0.74874894 0
      PT 2.47404583 0.78411261 0
      PT 2.50615 0.82028796 0
      PT 2.53825417 0.85551761 0
      PT 2.57035833 0.88942616 0
      PT 2.6024625 0.92124165 0
      PT 2.63456667 0.95058691 0
      PT 2.66667083 0.97790566 0
      PT 2.698775 1.00354817 0
      PT 2.73087917 1.02723192 0
      PT 2.76298333 1.05000823 0
      PT 2.7950875 1.07161847 0
      PT 2.82719167 1.09135583 0
      PT 2.85929583 1.10821836 0
      PT 2.8914 1.12187709 0
      PT 2.92350417 1.13308721 0
      PT 2.95560833 1.1416902 0
      PT 2.9877125 1.14863418 0
      PT 3.01981667 1.15514475 0
      PT 3.05192083 1.16106898 0
      PT 3.084025 1.16657774 0
      PT 3.11612917 1.17227135 0
      PT 3.14823333 1.17863664 0
      PT 3.1803375 0.9341125 0
      PT 3.21244167 0.57126391 0
      PT 3.24454583 0.40911852 0
      PT 3.27665 0.32925054 0
      PT 3.30875417 0.28879609 0
      PT 3.34085833 0.27925609 0
      PT 3.3729625 0.28041615 0
      PT 3.40506667 0.28359263 0
      PT 3.43717083 0.28846531 0
      PT 3.469275 0.29484358 0
      PT 3.50137917 0.30262151 0
      PT 3.53348333 0.31160137 0
      PT 3.5655875 0.32169091 0
      PT 3.59769167 0.33290691 0
      PT 3.62979583 0.34520571 0
      PT 3.6619 0.35849962 0
      PT 3.69400417 0.37261255 0
      PT 3.72610833 0.38746743 0
      PT 3.7582125 0.40295908 0
      PT 3.79031667 0.41916394 0
      PT 3.82242083 0.43614681 0
      PT 3.854525 0.45403099 0
      PT 3.88662917 0.47305239 0
      PT 3.91873333 0.49319499 0
      PT 3.9508375 0.51462477 0
      PT 3.98294167 0.53739899 0
      PT 4.01504583 0.56143471 0
      PT 4.04715 0.58679691 0
      PT 4.07925417 0.6133262 0
      PT 4.11135833 0.64062047 0
      PT 4.1434625 0.66864498 0
      PT 4.17556667 0.69780214 0
      PT 4.20767083 0.72814646 0
      PT 4.239775 0.7598566 0
      PT 4.27187917 0.79303325 0
      PT 4.30398333 0.8270135 0
      PT 4.3360875 0.86224793 0
      PT 4.36819167 0.89882047 0
      PT 4.40029583 0.93675078 0
      PT 4.4324 0.97573009 0
      PT 4.46450417 1.01549249 0
      PT 4.49660833 1.05596964 0
      PT 4.5287125 1.09702562 0
      PT 4.56081667 1.13827788 0
      PT 4.59292083 1.17874943 0
      PT 4.625025 1.21779262 0
      PT 4.65712917 1.22740923 0
      PT 4.68923333 0.79039846 0
      PT 4.7213375 0.51379173 0
      PT 4.75344167 0.38613169 0
      PT 4.78554583 0.31993305 0
      PT 4.81765 0.29109206 0
      PT 4.84975417 0.28508615 0
      PT 4.88185833 0.28617918 0
      PT 4.9139625 0.2885729 0
      PT 4.94606667 0.29243612 0
      PT 4.97817083 0.29745803 0
      PT 5.010275 0.30327553 0
      PT 5.04237917 0.31016989 0
      PT 5.07448333 0.31794455 0
      PT 5.1065875 0.32670659 0
      PT 5.13869167 0.33672207 0
      PT 5.17079583 0.3472486 0
      PT 5.2029 0.35807152 0
      PT 5.23500417 0.36912983 0
      PT 5.26710833 0.38017299 0
      PT 5.2992125 0.39161005 0
      PT 5.33131667 0.40368283 0
      PT 5.36342083 0.4160953 0
      PT 5.395525 0.42872554 0
      PT 5.42762917 0.44147044 0
      PT 5.45973333 0.45427861 0
      PT 5.4918375 0.46729103 0
      PT 5.52394167 0.48073791 0
      PT 5.55604583 0.49403527 0
      PT 5.58815 0.50682107 0
      PT 5.62025417 0.51885137 0
      PT 5.65235833 0.53004166 0
      PT 5.6844625 0.54103529 0
      PT 5.71656667 0.55147665 0
      PT 5.74867083 0.56151965 0
      PT 5.780775 0.57167525 0
      PT 5.81287917 0.58143558 0
      PT 5.84498333 0.59143048 0
      PT 5.8770875 0.60140584 0
      PT 5.90919167 0.61074078 0
      PT 5.94129583 0.61986112 0
      PT 5.9734 0.62899107 0
      PT 6.00550417 0.63837577 0
      PT 6.03760833 0.64841313 0
      PT 6.0697125 0.65855083 0
      PT 6.10181667 0.66788749 0
      PT 6.13392083 0.67663248 0
      PT 6.166025 0.68525605 0
      PT 6.19812917 0.67885983 0
      PT 6.23023333 0.59830135 0
      PT 6.2623375 0.48844759 0
      PT 6.29444167 0.40579987 0
      PT 6.32654583 0.35443996 0
      PT 6.35865 0.32736044 0
      PT 6.39075417 0.31876439 0
      PT 6.42285833 0.31871897 0
      PT 6.4549625 0.32039742 0
      PT 6.48706667 0.32325501 0
      PT 6.51917083 0.32683191 0
      PT 6.551275 0.33117235 0
      PT 6.58337917 0.33585956 0
      PT 6.61548333 0.34091951 0
      PT 6.6475875 0.34685565 0
      PT 6.67969167 0.35389378 0
      PT 6.71179583 0.36148131 0
      PT 6.7439 0.36913772 0
      PT 6.77600417 0.37705596 0
      PT 6.80810833 0.38464972 0
      PT 6.8402125 0.39235764 0
      PT 6.87231667 0.4010667 0
      PT 6.90442083 0.4104314 0
      PT 6.936525 0.42050816 0
      PT 6.96862917 0.43102467 0
      PT 7.00073333 0.4411948 0
      PT 7.0328375 0.45094574 0
      PT 7.06494167 0.46063878 0
      PT 7.09704583 0.47033891 0
      PT 7.12915 0.48104692 0
      PT 7.16125417 0.49290571 0
      PT 7.19335833 0.50483988 0
      PT 7.2254625 0.51699222 0
      PT 7.25756667 0.52892642 0
      PT 7.28967083 0.54077408 0
      PT 7.321775 0.55347531 0
      PT 7.35387917 0.56762862 0
      PT 7.38598333 0.58294665 0
      PT 7.4180875 0.59929941 0
      PT 7.45019167 0.61569933 0
      PT 7.48229583 0.63121979 0
      PT 7.5144 0.64644911 0
      PT 7.54650417 0.66154301 0
      PT 7.57860833 0.67749026 0
      PT 7.6107125 0.69448648 0
      PT 7.64281667 0.71209888 0
      PT 7.67492083 0.72915642 0
      PT 7.707025 0.74507776 0
      PT 7.73912917 0.75997366 0
      PT 7.77123333 0.77403424 0
      PT 7.8033375 0.78831073 0
      PT 7.83544167 0.803012 0
      PT 7.86754583 0.81743833 0
      PT 7.89965 0.83099142 0
      PT 7.93175417 0.84349872 0
      PT 7.96385833 0.85376524 0
      PT 7.9959625 0.86312423 0
      PT 8.02806667 0.87300301 0
      PT 8.06017083 0.88320589 0
      PT 8.092275 0.89425401 0
      PT 8.12437917 0.90574198 0
      PT 8.15648333 0.91644042 0
      PT 8.1885875 0.92589812 0
      PT 8.22069167 0.93557536 0
      PT 8.25279583 0.94543711 0
      PT 8.2849 0.95566163 0
      PT 8.31700417 0.9672256 0
      PT 8.34910833 0.97931625 0
      PT 8.3812125 0.99121002 0
      PT 8.41331667 1.00312389 0
      PT 8.44542083 1.01511627 0
      PT 8.477525 1.02723588 0
      PT 8.50962917 1.04139628 0
      PT 8.54173333 1.0572048 0
      PT 8.5738375 1.07443933 0
      PT 8.60594167 1.0919392 0
      PT 8.63804583 1.10812544 0
      PT 8.67015 1.12434388 0
      PT 8.70225417 1.14124903 0
      PT 8.73435833 1.16057951 0
      PT 8.7664625 1.18303774 0
      PT 8.79856667 1.20753213 0
      PT 8.83067083 1.23247374 0
      PT 8.862775 1.25710058 0
      PT 8.89487917 1.28665306 0
      PT 8.92698333 1.32886338 0
      PT 8.9590875 1.39129392 0
      PT 8.99119167 1.48646018 0
      PT 9.02329583 1.62390395 0
      PT 9.0554 1.79127853 0
      PT 9.08750417 1.9778842 0
      PT 9.11960833 2.18518659 0
      PT 9.1279 7.94328235 0
      PT 9.128 1 0
    >
    <PANENV
      EGUID {CADF2A0A-DC76-333A-63CE-41260BCCDC6D}
      ACT 0 -1
      VIS 1 1 1
      LANEHEIGHT 0 0
      ARM 0
      DEFSHAPE 0 -1 -1
      PT 0 0 0
    >
  >
  <ITEM
    POSITION 12.5
    SNAPOFFS 0
    LENGTH 1.256
    LOOP 1
    ALLTAKES 0
    FADEIN 1 0.01 0 1 0 0 0
    FADEOUT 1 0.01 0 1 0 0 0
    MUTE 0 0
    SEL 0
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
    TAKE
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
    TAKE SEL
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
  >
>"#;

        let track = Track::from_rpp_block(track_content).expect("Failed to parse track");

        assert_eq!(track.name, "");
        assert!(track.peak_color.is_some());
        assert!(track.beat.is_some());
        assert_eq!(track.automation_mode, AutomationMode::TrimRead);
        assert!(track.volpan.is_some());
        assert!(track.mutesolo.is_some());
        assert!(track.folder.is_some());
        assert!(track.bus_compact.is_some());
        assert!(track.show_in_mixer.is_some());
        assert!(track.fixed_lanes.is_some());
        assert!(track.record.is_some());
        assert!(track.track_height.is_some());
        assert!(track.input_quantize.is_some());
        assert_eq!(track.channel_count, 2);
        assert!(track.fx_enabled);
        assert!(track.track_id.is_some());
        assert!(track.perf.is_some());
        assert!(track.midi_output.is_some());
        assert!(track.master_send.is_some());
        assert!(track.fx_chain.is_some());

        println!("🎵 Parsed Complex Track:");
        println!("==========================================");
        println!("Track name: '{}'", track.name);
        println!("Track has {} items", track.items.len());
        println!("Track has {} envelopes", track.envelopes.len());
        println!("Track has FX chain: {}", track.fx_chain.is_some());

        // Print details about items
        for (i, item) in track.items.iter().enumerate() {
            println!(
                "  Item {}: {} ({} takes)",
                i + 1,
                item.name,
                item.takes.len()
            );
        }

        // Print details about envelopes
        for (i, envelope) in track.envelopes.iter().enumerate() {
            println!("  Envelope {}: {} points", i + 1, envelope.points.len());
        }

        println!("{}", track);
    }

    #[test]
    fn test_automation_mode_parsing() {
        assert_eq!(AutomationMode::from(0), AutomationMode::TrimRead);
        assert_eq!(AutomationMode::from(1), AutomationMode::Read);
        assert_eq!(AutomationMode::from(2), AutomationMode::Touch);
        assert_eq!(AutomationMode::from(3), AutomationMode::Write);
        assert_eq!(AutomationMode::from(4), AutomationMode::Latch);
        assert_eq!(AutomationMode::from(99), AutomationMode::Unknown(99));
    }

    #[test]
    fn test_track_solo_state_parsing() {
        assert_eq!(TrackSoloState::from(0), TrackSoloState::NoSolo);
        assert_eq!(TrackSoloState::from(1), TrackSoloState::Solo);
        assert_eq!(TrackSoloState::from(2), TrackSoloState::SoloInPlace);
        assert_eq!(TrackSoloState::from(99), TrackSoloState::Unknown(99));
    }

    #[test]
    fn test_folder_state_parsing() {
        assert_eq!(FolderState::from(0), FolderState::Regular);
        assert_eq!(FolderState::from(1), FolderState::FolderParent);
        assert_eq!(FolderState::from(2), FolderState::LastInFolder);
        assert_eq!(FolderState::from(99), FolderState::Unknown(99));
    }

    #[test]
    fn test_record_mode_parsing() {
        assert_eq!(RecordMode::from(0), RecordMode::Input);
        assert_eq!(RecordMode::from(1), RecordMode::OutputStereo);
        assert_eq!(RecordMode::from(2), RecordMode::DisableMonitor);
        assert_eq!(RecordMode::from(4), RecordMode::OutputMidi);
        assert_eq!(RecordMode::from(7), RecordMode::MidiOverdub);
        assert_eq!(
            RecordMode::from(11),
            RecordMode::OutputMultichannelLatencyComp
        );
        assert_eq!(RecordMode::from(99), RecordMode::Unknown(99));
    }

    #[test]
    fn test_monitor_mode_parsing() {
        assert_eq!(MonitorMode::from(0), MonitorMode::Off);
        assert_eq!(MonitorMode::from(1), MonitorMode::On);
        assert_eq!(MonitorMode::from(2), MonitorMode::Auto);
        assert_eq!(MonitorMode::from(99), MonitorMode::Unknown(99));
    }

    #[test]
    fn test_free_mode_parsing() {
        assert_eq!(FreeMode::from(0), FreeMode::Disabled);
        assert_eq!(FreeMode::from(1), FreeMode::FreeItemPositioning);
        assert_eq!(FreeMode::from(2), FreeMode::FixedItemLanes);
        assert_eq!(FreeMode::from(99), FreeMode::Unknown(99));
    }
}

// Forward declaration for Item and Envelope (will be defined in their own modules)
