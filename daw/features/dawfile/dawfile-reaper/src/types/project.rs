//! REAPER project data structures and parsing

#[cfg(feature = "parallel")]
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use super::marker_region::MarkerRegionCollection;
use super::time_tempo::TempoTimeEnvelope;
use super::track::TrackParseOptions;
use crate::primitives::{BlockType, RppBlockContent, RppProject, Token};

/// Automation mode for tracks and envelopes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AutomationMode {
    TrimRead,
    Read,
    Touch,
    Write,
    Latch,
}

impl AutomationMode {
    pub fn from_value(value: i32) -> Self {
        match value {
            0 => AutomationMode::TrimRead,
            1 => AutomationMode::Read,
            2 => AutomationMode::Touch,
            3 => AutomationMode::Write,
            4 => AutomationMode::Latch,
            _ => AutomationMode::TrimRead, // Default fallback
        }
    }

    pub fn to_value(&self) -> i32 {
        match self {
            AutomationMode::TrimRead => 0,
            AutomationMode::Read => 1,
            AutomationMode::Touch => 2,
            AutomationMode::Write => 3,
            AutomationMode::Latch => 4,
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
        }
    }
}

/// Envelope point shape
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EnvelopeShape {
    Linear,
    Square,
    SlowStartEnd,
    FastStart,
    FastEnd,
    Bezier,
}

impl EnvelopeShape {
    pub fn from_value(value: i32) -> Self {
        match value {
            0 => EnvelopeShape::Linear,
            1 => EnvelopeShape::Square,
            2 => EnvelopeShape::SlowStartEnd,
            3 => EnvelopeShape::FastStart,
            4 => EnvelopeShape::FastEnd,
            5 => EnvelopeShape::Bezier,
            _ => EnvelopeShape::Linear, // Default fallback
        }
    }

    pub fn to_value(&self) -> i32 {
        match self {
            EnvelopeShape::Linear => 0,
            EnvelopeShape::Square => 1,
            EnvelopeShape::SlowStartEnd => 2,
            EnvelopeShape::FastStart => 3,
            EnvelopeShape::FastEnd => 4,
            EnvelopeShape::Bezier => 5,
        }
    }
}

impl fmt::Display for EnvelopeShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvelopeShape::Linear => write!(f, "Linear"),
            EnvelopeShape::Square => write!(f, "Square"),
            EnvelopeShape::SlowStartEnd => write!(f, "Slow Start/End"),
            EnvelopeShape::FastStart => write!(f, "Fast Start"),
            EnvelopeShape::FastEnd => write!(f, "Fast End"),
            EnvelopeShape::Bezier => write!(f, "Bezier"),
        }
    }
}

/// Metronome settings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metronome {
    pub volume: (f64, f64), // (accent, normal)
    pub beat_length: i32,
    pub frequency: (i32, i32, i32), // (accent, normal, ?)
    pub samples: (String, String, String, String), // Sample file paths
    pub split_ignore: (i32, i32),
    pub split_def: Vec<SplitDefinition>,
    pub pattern: (i32, i32),
    pub pattern_string: String,
    pub mult: i32,
}

/// Split definition for metronome
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitDefinition {
    pub index: i32,
    pub frequency: i32,
    pub sample_path: String,
    pub flags: i32,
    pub name: String,
}

/// Envelope settings (common to all envelope types)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeSettings {
    pub guid: Option<String>,
    pub active: (i32, i32),       // (active, ?)
    pub visible: (i32, i32, f64), // (visible, show_in_lane, deprecated_value)
    pub lane_height: (i32, i32),
    pub armed: i32,
    pub default_shape: (EnvelopeShape, i32, i32), // (shape, pitch_range, pitch_snap)
}

impl fmt::Display for EnvelopeSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  Active: {}", self.active.0 != 0)?;
        writeln!(
            f,
            "  Visible: {}, Show in Lane: {}",
            self.visible.0 != 0,
            self.visible.1 != 0
        )?;
        writeln!(f, "  Lane Height: {}px", self.lane_height.0)?;
        writeln!(f, "  Armed: {}", self.armed != 0)?;
        writeln!(f, "  Default Shape: {}", self.default_shape.0)?;
        if let Some(guid) = &self.guid {
            writeln!(f, "  GUID: {}", guid)?;
        }
        Ok(())
    }
}

/// Master track view settings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MasterTrackView {
    pub field1: i32,
    pub field2: f64,
    pub field3: f64,
    pub field4: f64,
    pub field5: i32,
    pub field6: i32,
    pub field7: i32,
    pub field8: i32,
    pub field9: i32,
    pub field10: i32,
    pub field11: i32,
    pub field12: i32,
    pub field13: i32,
    pub field14: i32,
}

/// A complete REAPER project with typed data structures
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReaperProject {
    pub version: f64,
    pub version_string: String,
    pub timestamp: i64,
    pub properties: ProjectProperties,
    pub tracks: Vec<Track>,
    pub items: Vec<Item>,
    pub envelopes: Vec<Envelope>,
    pub fx_chains: Vec<FxChain>,
    pub markers_regions: MarkerRegionCollection,
    pub tempo_envelope: Option<TempoTimeEnvelope>,
    /// Ruler lanes (v7.62+)
    pub ruler_lanes: Vec<RulerLane>,
    /// Ruler height (v7.62+): (total_height, secondary)
    pub ruler_height: Option<(i32, i32)>,
}

/// A ruler lane definition (v7.62+)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RulerLane {
    /// Lane index (1-based)
    pub index: i32,
    /// Flags: 8 = default region lane, 4 = default marker lane, 0 = normal
    pub flags: i32,
    /// Lane name
    pub name: String,
    /// Color (0 = default)
    pub color: i32,
    /// Extra field (purpose unknown, typically -1)
    pub extra: i32,
}

/// Controls which typed domains to decode from an `RppProject`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecodeOptions {
    pub parse_tracks: bool,
    pub parse_project_items: bool,
    pub parse_project_envelopes: bool,
    pub parse_project_fxchains: bool,
    pub parse_markers_regions: bool,
    pub parse_tempo_envelope: bool,
    pub track_options: TrackParseOptions,
}

impl DecodeOptions {
    pub const fn full() -> Self {
        Self {
            parse_tracks: true,
            parse_project_items: true,
            parse_project_envelopes: true,
            parse_project_fxchains: true,
            parse_markers_regions: true,
            parse_tempo_envelope: true,
            track_options: TrackParseOptions::full(),
        }
    }

    /// Summary-oriented decoding: keep project metadata and lightweight tracks,
    /// skip heavy nested domains by default.
    pub const fn summary() -> Self {
        Self {
            parse_tracks: true,
            parse_project_items: false,
            parse_project_envelopes: false,
            parse_project_fxchains: false,
            parse_markers_regions: true,
            parse_tempo_envelope: false,
            track_options: TrackParseOptions::summary(),
        }
    }
}

type GridSettings = Option<(i32, i32, i32, i32, i32, i32, i32, i32)>;
type SmpteSyncSettings = Option<(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32)>;
type MasterHwOut = Option<(i32, i32, i32, i32, i32, i32, i32, i32)>;

/// Project-level properties and settings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectProperties {
    // Editing behavior
    pub ripple: Option<(i32, i32)>,              // RIPPLE 0 0
    pub group_override: Option<(i32, i32, i32)>, // GROUPOVERRIDE 0 0 0
    pub auto_xfade: Option<i32>,                 // AUTOXFADE 129
    pub env_attach: Option<i32>,                 // ENVATTACH 3
    pub pooled_env_attach: Option<i32>,          // POOLEDENVATTACH 0

    // UI settings
    pub mixer_ui_flags: Option<(i32, i32)>, // MIXERUIFLAGS 11 48
    pub env_fade_sz10: Option<i32>,         // ENVFADESZ10 40
    pub peak_gain: Option<i32>,             // PEAKGAIN 1
    pub feedback: Option<i32>,              // FEEDBACK 0
    pub pan_law: Option<i32>,               // PANLAW 1

    // Project settings
    pub proj_offs: Option<(i32, i32, i32)>, // PROJOFFS 0 0 0
    pub max_proj_len: Option<(i32, i32)>,   // MAXPROJLEN 0 0
    pub grid: GridSettings,                 // GRID 3199 8 1 8 1 0 0 0
    pub time_mode: Option<(i32, i32, i32, i32, i32, i32, i32)>, // TIMEMODE 1 5 -1 30 0 0 -1
    pub video_config: Option<(i32, i32, i32)>, // VIDEO_CONFIG 0 0 65792
    pub pan_mode: Option<i32>,              // PANMODE 3
    pub pan_law_flags: Option<i32>,         // PANLAWFLAGS 3

    // View settings
    pub cursor: Option<i32>,           // CURSOR 11
    pub zoom: Option<(i32, i32, i32)>, // ZOOM 100 0 0
    pub v_zoom_ex: Option<(i32, i32)>, // VZOOMEX 6 0

    // Recording settings
    pub use_rec_cfg: Option<i32>,              // USE_REC_CFG 0
    pub rec_mode: Option<i32>,                 // RECMODE 1
    pub smpte_sync: SmpteSyncSettings,         // SMPTESYNC 0 30 100 40 1000 300 0 0 1 0 0
    pub r#loop: Option<i32>,                   // LOOP 0
    pub loop_gran: Option<(i32, i32)>,         // LOOPGRAN 0 4
    pub record_path: Option<(String, String)>, // RECORD_PATH "Media" ""

    // Render settings
    pub render_file: Option<String>,         // RENDER_FILE ""
    pub render_pattern: Option<String>,      // RENDER_PATTERN ""
    pub render_fmt: Option<(i32, i32, i32)>, // RENDER_FMT 0 2 0
    pub render_1x: Option<i32>,              // RENDER_1X 0
    pub render_range: Option<(i32, i32, i32, i32, i32)>, // RENDER_RANGE 1 0 0 0 1000
    pub render_resample: Option<(i32, i32, i32)>, // RENDER_RESAMPLE 3 0 1
    pub render_add_to_proj: Option<i32>,     // RENDER_ADDTOPROJ 0
    pub render_stems: Option<i32>,           // RENDER_STEMS 0
    pub render_dither: Option<i32>,          // RENDER_DITHER 0
    pub render_trim: Option<(i32, i32, i32, i32)>, // RENDER_TRIM 0 0 0 0

    // Time settings
    pub time_lock_mode: Option<i32>,          // TIMELOCKMODE 1
    pub tempo_env_lock_mode: Option<i32>,     // TEMPOENVLOCKMODE 1
    pub item_mix: Option<i32>,                // ITEMMIX 1
    pub def_pitch_mode: Option<(i32, i32)>,   // DEFPITCHMODE 589824 0
    pub take_lane: Option<i32>,               // TAKELANE 1
    pub sample_rate: Option<(i32, i32, i32)>, // SAMPLERATE 44100 0 0
    pub lock: Option<i32>,                    // LOCK 1

    // Tempo and playback
    /// `TEMPO 120 4 4 0` — BPM, time-signature numerator, denominator,
    /// flags.
    ///
    /// BPM is `f64` because REAPER writes it fractionally and songs use
    /// it: `TEMPO 86.28` truncated to 86 is 9 ms of error per bar, which
    /// is two thirds of a second by the end of a three-minute song — far
    /// past where a lighting cue or a click track stops landing.
    pub tempo: Option<(f64, i32, i32, i32)>,
    pub play_rate: Option<(i32, i32, i32, i32)>, // PLAYRATE 1 0 0.25 4
    pub selection: Option<(i32, i32)>,           // SELECTION 0 0
    pub selection2: Option<(i32, i32)>,          // SELECTION2 0 0

    // Master track settings
    pub master_auto_mode: Option<i32>,           // MASTERAUTOMODE 0
    pub master_track_height: Option<(i32, i32)>, // MASTERTRACKHEIGHT 0 0
    pub master_peak_col: Option<i32>,            // MASTERPEAKCOL 16576
    pub master_mute_solo: Option<i32>,           // MASTERMUTESOLO 0
    pub master_track_view: Option<MasterTrackView>, // MASTERTRACKVIEW 0 0.6667 0.5 0.5 0 0 0 0 0 0 0 0 0 0
    pub master_hw_out: MasterHwOut,                 // MASTERHWOUT 0 0 1 0 0 0 0 -1
    pub master_nch: Option<(i32, i32)>,             // MASTER_NCH 2 2
    /// `MASTER_VOLUME vol pan ? ? ?` — vol is linear gain, pan −1…1.
    pub master_volume: Option<(f64, f64, f64, f64, f64)>,
    pub master_pan_mode: Option<i32>,      // MASTER_PANMODE 3
    pub master_pan_law_flags: Option<i32>, // MASTER_PANLAWFLAGS 3
    pub master_fx: Option<i32>,            // MASTER_FX 1
    pub master_sel: Option<i32>,           // MASTER_SEL 0

    // Global automation
    pub global_auto: Option<i32>, // GLOBAL_AUTO -1

    // Metronome settings
    pub metronome: Option<Metronome>, // METRONOME block

    // Envelope settings (for project-level envelopes)
    pub master_play_speed_env: Option<EnvelopeSettings>, // MASTERPLAYSPEEDENV
    pub tempo_env: Option<EnvelopeSettings>,             // TEMPOENVEX

    // Ruler lanes (v7.62+)
    pub ruler_lanes: Vec<RulerLane>,
    pub ruler_height: Option<(i32, i32)>,

    // Custom properties (for any we haven't defined yet)
    pub custom_properties: HashMap<String, Vec<Token>>,
}

impl Default for ProjectProperties {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectProperties {
    /// Create empty project properties
    pub fn new() -> Self {
        Self {
            ripple: None,
            group_override: None,
            auto_xfade: None,
            env_attach: None,
            pooled_env_attach: None,
            mixer_ui_flags: None,
            env_fade_sz10: None,
            peak_gain: None,
            feedback: None,
            pan_law: None,
            proj_offs: None,
            max_proj_len: None,
            grid: None,
            time_mode: None,
            video_config: None,
            pan_mode: None,
            pan_law_flags: None,
            cursor: None,
            zoom: None,
            v_zoom_ex: None,
            use_rec_cfg: None,
            rec_mode: None,
            smpte_sync: None,
            r#loop: None,
            loop_gran: None,
            record_path: None,
            render_file: None,
            render_pattern: None,
            render_fmt: None,
            render_1x: None,
            render_range: None,
            render_resample: None,
            render_add_to_proj: None,
            render_stems: None,
            render_dither: None,
            render_trim: None,
            time_lock_mode: None,
            tempo_env_lock_mode: None,
            item_mix: None,
            def_pitch_mode: None,
            take_lane: None,
            sample_rate: None,
            lock: None,
            tempo: None,
            play_rate: None,
            selection: None,
            selection2: None,
            master_auto_mode: None,
            master_track_height: None,
            master_peak_col: None,
            master_mute_solo: None,
            master_track_view: None,
            master_hw_out: None,
            master_nch: None,
            master_volume: None,
            master_pan_mode: None,
            master_pan_law_flags: None,
            master_fx: None,
            master_sel: None,
            global_auto: None,
            metronome: None,
            master_play_speed_env: None,
            tempo_env: None,
            ruler_lanes: Vec::new(),
            ruler_height: None,
            custom_properties: HashMap::new(),
        }
    }

    /// Parse project properties from RPP blocks
    pub fn from_blocks(blocks: &[crate::primitives::RppBlock]) -> Self {
        let mut properties = Self::new();

        for block in blocks {
            // Handle special blocks first
            match block.name.as_str() {
                "METRONOME" => {
                    properties.metronome = Self::parse_metronome_block(block);
                }
                "MASTERPLAYSPEEDENV" => {
                    properties.master_play_speed_env = Self::parse_envelope_block(block);
                }
                "TEMPOENVEX" => {
                    properties.tempo_env = Self::parse_envelope_block(block);
                }
                _ => {
                    // Look for content lines in blocks that represent project properties
                    for content in &block.children {
                        if let RppBlockContent::Content(tokens) = content
                            && let Some(property_name) = tokens.first()
                            && let Some(name) = property_name.as_string()
                        {
                            properties.parse_property(name, &tokens[1..]);
                        }
                    }
                }
            }
        }

        properties
    }

    /// Parse metronome block
    fn parse_metronome_block(block: &crate::primitives::RppBlock) -> Option<Metronome> {
        let mut metronome = Metronome {
            volume: (0.25, 0.125), // Default values
            beat_length: 4,
            frequency: (1760, 880, 1),
            samples: (
                "".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
            ),
            split_ignore: (0, 0),
            split_def: Vec::new(),
            pattern: (0, 169),
            pattern_string: "ABBB".to_string(),
            mult: 1,
        };

        for content in &block.children {
            if let RppBlockContent::Content(tokens) = content
                && let Some(property_name) = tokens.first()
                && let Some(name) = property_name.as_string()
            {
                match name {
                    "VOL" if tokens.len() >= 3 => {
                        if let (Some(a), Some(b)) = (tokens[1].as_number(), tokens[2].as_number()) {
                            metronome.volume = (a, b);
                        }
                    }
                    "BEATLEN" => {
                        if let Some(val) = tokens.get(1).and_then(|t| t.as_number()) {
                            metronome.beat_length = val as i32;
                        }
                    }
                    "FREQ" if tokens.len() >= 4 => {
                        if let (Some(a), Some(b), Some(c)) = (
                            tokens[1].as_number(),
                            tokens[2].as_number(),
                            tokens[3].as_number(),
                        ) {
                            metronome.frequency = (a as i32, b as i32, c as i32);
                        }
                    }
                    "SAMPLES" if tokens.len() >= 5 => {
                        if let (Some(a), Some(b), Some(c), Some(d)) = (
                            tokens[1].as_string(),
                            tokens[2].as_string(),
                            tokens[3].as_string(),
                            tokens[4].as_string(),
                        ) {
                            metronome.samples =
                                (a.to_string(), b.to_string(), c.to_string(), d.to_string());
                        }
                    }
                    "SPLIGNORE" if tokens.len() >= 3 => {
                        if let (Some(a), Some(b)) = (tokens[1].as_number(), tokens[2].as_number()) {
                            metronome.split_ignore = (a as i32, b as i32);
                        }
                    }
                    "SPLDEF" if tokens.len() >= 6 => {
                        if let (Some(index), Some(freq), Some(sample), Some(flags), Some(name)) = (
                            tokens[1].as_number(),
                            tokens[2].as_number(),
                            tokens[3].as_string(),
                            tokens[4].as_number(),
                            tokens[5].as_string(),
                        ) {
                            metronome.split_def.push(SplitDefinition {
                                index: index as i32,
                                frequency: freq as i32,
                                sample_path: sample.to_string(),
                                flags: flags as i32,
                                name: name.to_string(),
                            });
                        }
                    }
                    "PATTERN" if tokens.len() >= 3 => {
                        if let (Some(a), Some(b)) = (tokens[1].as_number(), tokens[2].as_number()) {
                            metronome.pattern = (a as i32, b as i32);
                        }
                    }
                    "PATTERNSTR" => {
                        if let Some(val) = tokens.get(1).and_then(|t| t.as_string()) {
                            metronome.pattern_string = val.to_string();
                        }
                    }
                    "MULT" => {
                        if let Some(val) = tokens.get(1).and_then(|t| t.as_number()) {
                            metronome.mult = val as i32;
                        }
                    }
                    _ => {}
                }
            }
        }

        Some(metronome)
    }

    /// Parse envelope block
    fn parse_envelope_block(block: &crate::primitives::RppBlock) -> Option<EnvelopeSettings> {
        let mut envelope = EnvelopeSettings {
            guid: None,
            active: (0, -1),
            visible: (0, 1, 1.0),
            lane_height: (0, 0),
            armed: 0,
            default_shape: (EnvelopeShape::Linear, -1, -1),
        };

        for content in &block.children {
            if let RppBlockContent::Content(tokens) = content
                && let Some(property_name) = tokens.first()
                && let Some(name) = property_name.as_string()
            {
                match name {
                    "EGUID" => {
                        if let Some(val) = tokens.get(1).and_then(|t| t.as_string()) {
                            envelope.guid = Some(val.to_string());
                        }
                    }
                    "ACT" if tokens.len() >= 3 => {
                        if let (Some(a), Some(b)) = (tokens[1].as_number(), tokens[2].as_number()) {
                            envelope.active = (a as i32, b as i32);
                        }
                    }
                    "VIS" if tokens.len() >= 4 => {
                        if let (Some(a), Some(b), Some(c)) = (
                            tokens[1].as_number(),
                            tokens[2].as_number(),
                            tokens[3].as_number(),
                        ) {
                            envelope.visible = (a as i32, b as i32, c);
                        }
                    }
                    "LANEHEIGHT" if tokens.len() >= 3 => {
                        if let (Some(a), Some(b)) = (tokens[1].as_number(), tokens[2].as_number()) {
                            envelope.lane_height = (a as i32, b as i32);
                        }
                    }
                    "ARM" => {
                        if let Some(val) = tokens.get(1).and_then(|t| t.as_number()) {
                            envelope.armed = val as i32;
                        }
                    }
                    "DEFSHAPE" if tokens.len() >= 4 => {
                        if let (Some(shape), Some(range), Some(snap)) = (
                            tokens[1].as_number(),
                            tokens[2].as_number(),
                            tokens[3].as_number(),
                        ) {
                            envelope.default_shape = (
                                EnvelopeShape::from_value(shape as i32),
                                range as i32,
                                snap as i32,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        Some(envelope)
    }

    /// Parse a single property from tokens
    fn parse_property(&mut self, name: &str, tokens: &[Token]) {
        match name {
            "RIPPLE" => {
                if tokens.len() >= 2
                    && let (Some(a), Some(b)) = (tokens[0].as_number(), tokens[1].as_number())
                {
                    self.ripple = Some((a as i32, b as i32));
                }
            }
            "GROUPOVERRIDE" => {
                if tokens.len() >= 3
                    && let (Some(a), Some(b), Some(c)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                    )
                {
                    self.group_override = Some((a as i32, b as i32, c as i32));
                }
            }
            "AUTOXFADE" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.auto_xfade = Some(val as i32);
                }
            }
            "ENVATTACH" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.env_attach = Some(val as i32);
                }
            }
            "POOLEDENVATTACH" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.pooled_env_attach = Some(val as i32);
                }
            }
            "MIXERUIFLAGS" => {
                if tokens.len() >= 2
                    && let (Some(a), Some(b)) = (tokens[0].as_number(), tokens[1].as_number())
                {
                    self.mixer_ui_flags = Some((a as i32, b as i32));
                }
            }
            "ENVFADESZ10" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.env_fade_sz10 = Some(val as i32);
                }
            }
            "PEAKGAIN" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.peak_gain = Some(val as i32);
                }
            }
            "FEEDBACK" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.feedback = Some(val as i32);
                }
            }
            "PANLAW" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.pan_law = Some(val as i32);
                }
            }
            "PROJOFFS" => {
                if tokens.len() >= 3
                    && let (Some(a), Some(b), Some(c)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                    )
                {
                    self.proj_offs = Some((a as i32, b as i32, c as i32));
                }
            }
            "MAXPROJLEN" => {
                if tokens.len() >= 2
                    && let (Some(a), Some(b)) = (tokens[0].as_number(), tokens[1].as_number())
                {
                    self.max_proj_len = Some((a as i32, b as i32));
                }
            }
            "GRID" => {
                if tokens.len() >= 8
                    && let (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f), Some(g), Some(h)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                        tokens[3].as_number(),
                        tokens[4].as_number(),
                        tokens[5].as_number(),
                        tokens[6].as_number(),
                        tokens[7].as_number(),
                    )
                {
                    self.grid = Some((
                        a as i32, b as i32, c as i32, d as i32, e as i32, f as i32, g as i32,
                        h as i32,
                    ));
                }
            }
            "TIMEMODE" => {
                if tokens.len() >= 7
                    && let (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f), Some(g)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                        tokens[3].as_number(),
                        tokens[4].as_number(),
                        tokens[5].as_number(),
                        tokens[6].as_number(),
                    )
                {
                    self.time_mode = Some((
                        a as i32, b as i32, c as i32, d as i32, e as i32, f as i32, g as i32,
                    ));
                }
            }
            "VIDEO_CONFIG" => {
                if tokens.len() >= 3
                    && let (Some(a), Some(b), Some(c)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                    )
                {
                    self.video_config = Some((a as i32, b as i32, c as i32));
                }
            }
            "PANMODE" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.pan_mode = Some(val as i32);
                }
            }
            "PANLAWFLAGS" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.pan_law_flags = Some(val as i32);
                }
            }
            "CURSOR" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.cursor = Some(val as i32);
                }
            }
            "ZOOM" => {
                if tokens.len() >= 3
                    && let (Some(a), Some(b), Some(c)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                    )
                {
                    self.zoom = Some((a as i32, b as i32, c as i32));
                }
            }
            "VZOOMEX" => {
                if tokens.len() >= 2
                    && let (Some(a), Some(b)) = (tokens[0].as_number(), tokens[1].as_number())
                {
                    self.v_zoom_ex = Some((a as i32, b as i32));
                }
            }
            "USE_REC_CFG" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.use_rec_cfg = Some(val as i32);
                }
            }
            "RECMODE" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.rec_mode = Some(val as i32);
                }
            }
            "SMPTESYNC" => {
                if tokens.len() >= 11
                    && let (
                        Some(a),
                        Some(b),
                        Some(c),
                        Some(d),
                        Some(e),
                        Some(f),
                        Some(g),
                        Some(h),
                        Some(i),
                        Some(j),
                        Some(k),
                    ) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                        tokens[3].as_number(),
                        tokens[4].as_number(),
                        tokens[5].as_number(),
                        tokens[6].as_number(),
                        tokens[7].as_number(),
                        tokens[8].as_number(),
                        tokens[9].as_number(),
                        tokens[10].as_number(),
                    )
                {
                    self.smpte_sync = Some((
                        a as i32, b as i32, c as i32, d as i32, e as i32, f as i32, g as i32,
                        h as i32, i as i32, j as i32, k as i32,
                    ));
                }
            }
            "LOOP" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.r#loop = Some(val as i32);
                }
            }
            "LOOPGRAN" => {
                if tokens.len() >= 2
                    && let (Some(a), Some(b)) = (tokens[0].as_number(), tokens[1].as_number())
                {
                    self.loop_gran = Some((a as i32, b as i32));
                }
            }
            "RECORD_PATH" => {
                if tokens.len() >= 2
                    && let (Some(a), Some(b)) = (tokens[0].as_string(), tokens[1].as_string())
                {
                    self.record_path = Some((a.to_string(), b.to_string()));
                }
            }
            "RENDER_FILE" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_string()) {
                    self.render_file = Some(val.to_string());
                }
            }
            "RENDER_PATTERN" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_string()) {
                    self.render_pattern = Some(val.to_string());
                }
            }
            "RENDER_FMT" => {
                if tokens.len() >= 3
                    && let (Some(a), Some(b), Some(c)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                    )
                {
                    self.render_fmt = Some((a as i32, b as i32, c as i32));
                }
            }
            "RENDER_1X" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.render_1x = Some(val as i32);
                }
            }
            "RENDER_RANGE" => {
                if tokens.len() >= 5
                    && let (Some(a), Some(b), Some(c), Some(d), Some(e)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                        tokens[3].as_number(),
                        tokens[4].as_number(),
                    )
                {
                    self.render_range = Some((a as i32, b as i32, c as i32, d as i32, e as i32));
                }
            }
            "RENDER_RESAMPLE" => {
                if tokens.len() >= 3
                    && let (Some(a), Some(b), Some(c)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                    )
                {
                    self.render_resample = Some((a as i32, b as i32, c as i32));
                }
            }
            "RENDER_ADDTOPROJ" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.render_add_to_proj = Some(val as i32);
                }
            }
            "RENDER_STEMS" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.render_stems = Some(val as i32);
                }
            }
            "RENDER_DITHER" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.render_dither = Some(val as i32);
                }
            }
            "RENDER_TRIM" => {
                if tokens.len() >= 4
                    && let (Some(a), Some(b), Some(c), Some(d)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                        tokens[3].as_number(),
                    )
                {
                    self.render_trim = Some((a as i32, b as i32, c as i32, d as i32));
                }
            }
            "TIMELOCKMODE" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.time_lock_mode = Some(val as i32);
                }
            }
            "TEMPOENVLOCKMODE" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.tempo_env_lock_mode = Some(val as i32);
                }
            }
            "ITEMMIX" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.item_mix = Some(val as i32);
                }
            }
            "DEFPITCHMODE" => {
                if tokens.len() >= 2
                    && let (Some(a), Some(b)) = (tokens[0].as_number(), tokens[1].as_number())
                {
                    self.def_pitch_mode = Some((a as i32, b as i32));
                }
            }
            "TAKELANE" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.take_lane = Some(val as i32);
                }
            }
            "SAMPLERATE" => {
                if tokens.len() >= 3
                    && let (Some(a), Some(b), Some(c)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                    )
                {
                    self.sample_rate = Some((a as i32, b as i32, c as i32));
                }
            }
            "LOCK" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.lock = Some(val as i32);
                }
            }
            "TEMPO" => {
                if tokens.len() >= 4
                    && let (Some(a), Some(b), Some(c), Some(d)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                        tokens[3].as_number(),
                    )
                {
                    self.tempo = Some((a, b as i32, c as i32, d as i32));
                }
            }
            "PLAYRATE" => {
                if tokens.len() >= 4
                    && let (Some(a), Some(b), Some(c), Some(d)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                        tokens[3].as_number(),
                    )
                {
                    self.play_rate = Some((a as i32, b as i32, c as i32, d as i32));
                }
            }
            "SELECTION" => {
                if tokens.len() >= 2
                    && let (Some(a), Some(b)) = (tokens[0].as_number(), tokens[1].as_number())
                {
                    self.selection = Some((a as i32, b as i32));
                }
            }
            "SELECTION2" => {
                if tokens.len() >= 2
                    && let (Some(a), Some(b)) = (tokens[0].as_number(), tokens[1].as_number())
                {
                    self.selection2 = Some((a as i32, b as i32));
                }
            }
            "MASTERAUTOMODE" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.master_auto_mode = Some(val as i32);
                }
            }
            "MASTERTRACKHEIGHT" => {
                if tokens.len() >= 2
                    && let (Some(a), Some(b)) = (tokens[0].as_number(), tokens[1].as_number())
                {
                    self.master_track_height = Some((a as i32, b as i32));
                }
            }
            "MASTERPEAKCOL" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.master_peak_col = Some(val as i32);
                }
            }
            "MASTERMUTESOLO" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.master_mute_solo = Some(val as i32);
                }
            }
            "MASTERTRACKVIEW" => {
                if tokens.len() >= 14
                    && let (
                        Some(a),
                        Some(b),
                        Some(c),
                        Some(d),
                        Some(e),
                        Some(f),
                        Some(g),
                        Some(h),
                        Some(i),
                        Some(j),
                        Some(k),
                        Some(l),
                        Some(m),
                        Some(n),
                    ) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                        tokens[3].as_number(),
                        tokens[4].as_number(),
                        tokens[5].as_number(),
                        tokens[6].as_number(),
                        tokens[7].as_number(),
                        tokens[8].as_number(),
                        tokens[9].as_number(),
                        tokens[10].as_number(),
                        tokens[11].as_number(),
                        tokens[12].as_number(),
                        tokens[13].as_number(),
                    )
                {
                    self.master_track_view = Some(MasterTrackView {
                        field1: a as i32,
                        field2: b,
                        field3: c,
                        field4: d,
                        field5: e as i32,
                        field6: f as i32,
                        field7: g as i32,
                        field8: h as i32,
                        field9: i as i32,
                        field10: j as i32,
                        field11: k as i32,
                        field12: l as i32,
                        field13: m as i32,
                        field14: n as i32,
                    });
                }
            }
            "MASTERHWOUT" => {
                if tokens.len() >= 8
                    && let (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f), Some(g), Some(h)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                        tokens[3].as_number(),
                        tokens[4].as_number(),
                        tokens[5].as_number(),
                        tokens[6].as_number(),
                        tokens[7].as_number(),
                    )
                {
                    self.master_hw_out = Some((
                        a as i32, b as i32, c as i32, d as i32, e as i32, f as i32, g as i32,
                        h as i32,
                    ));
                }
            }
            "MASTER_NCH" => {
                if tokens.len() >= 2
                    && let (Some(a), Some(b)) = (tokens[0].as_number(), tokens[1].as_number())
                {
                    self.master_nch = Some((a as i32, b as i32));
                }
            }
            "MASTER_VOLUME" => {
                if tokens.len() >= 5
                    && let (Some(a), Some(b), Some(c), Some(d), Some(e)) = (
                        tokens[0].as_number(),
                        tokens[1].as_number(),
                        tokens[2].as_number(),
                        tokens[3].as_number(),
                        tokens[4].as_number(),
                    )
                {
                    self.master_volume = Some((a, b, c, d, e));
                }
            }
            "MASTER_PANMODE" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.master_pan_mode = Some(val as i32);
                }
            }
            "MASTER_PANLAWFLAGS" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.master_pan_law_flags = Some(val as i32);
                }
            }
            "MASTER_FX" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.master_fx = Some(val as i32);
                }
            }
            "MASTER_SEL" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.master_sel = Some(val as i32);
                }
            }
            "GLOBAL_AUTO" => {
                if let Some(val) = tokens.first().and_then(|t| t.as_number()) {
                    self.global_auto = Some(val as i32);
                }
            }
            "RULERLANE" => {
                // RULERLANE index flags name color extra
                if tokens.len() >= 5 {
                    let index = tokens[0].as_number().unwrap_or(0.0) as i32;
                    let flags = tokens[1].as_number().unwrap_or(0.0) as i32;
                    let name_str = match &tokens[2] {
                        Token::String(s, _) => s.clone(),
                        Token::Identifier(s) => s.clone(),
                        _ => String::new(),
                    };
                    let color = tokens[3].as_number().unwrap_or(0.0) as i32;
                    let extra = tokens[4].as_number().unwrap_or(-1.0) as i32;
                    self.ruler_lanes.push(RulerLane {
                        index,
                        flags,
                        name: name_str,
                        color,
                        extra,
                    });
                }
            }
            "RULERHEIGHT" => {
                if tokens.len() >= 2
                    && let (Some(a), Some(b)) = (tokens[0].as_number(), tokens[1].as_number())
                {
                    self.ruler_height = Some((a as i32, b as i32));
                }
            }
            _ => {
                // Store unknown properties in custom_properties
                self.custom_properties
                    .insert(name.to_string(), tokens.to_vec());
            }
        }
    }
}

impl Default for ReaperProject {
    fn default() -> Self {
        Self {
            version: 0.1,
            version_string: String::new(),
            timestamp: 0,
            properties: ProjectProperties::default(),
            tracks: vec![],
            items: vec![],
            envelopes: vec![],
            fx_chains: vec![],
            markers_regions: MarkerRegionCollection::default(),
            tempo_envelope: None,
            ruler_lanes: vec![],
            ruler_height: None,
        }
    }
}

impl ReaperProject {
    /// Create a ReaperProject from a parsed RPP project
    pub fn from_rpp_project(rpp_project: &RppProject) -> Result<Self, String> {
        Self::from_rpp_project_with_options(rpp_project, DecodeOptions::full())
    }

    /// Create a ReaperProject from a parsed RPP project with selective decoding.
    pub fn from_rpp_project_with_options(
        rpp_project: &RppProject,
        options: DecodeOptions,
    ) -> Result<Self, String> {
        let mut project = ReaperProject {
            version: rpp_project.version,
            version_string: rpp_project.version_string.clone(),
            timestamp: rpp_project.timestamp,
            properties: ProjectProperties::from_blocks(&rpp_project.blocks),
            tracks: Vec::new(),
            items: Vec::new(),
            envelopes: Vec::new(),
            fx_chains: Vec::new(),
            markers_regions: MarkerRegionCollection::new(),
            tempo_envelope: None,
            ruler_lanes: Vec::new(),
            ruler_height: None,
        };

        // Copy ruler lanes from parsed properties
        project.ruler_lanes = project.properties.ruler_lanes.clone();
        project.ruler_height = project.properties.ruler_height;

        // Parse project-global domains in-order.
        for block in &rpp_project.blocks {
            if block.block_type == BlockType::Project && options.parse_markers_regions {
                for child in &block.children {
                    if let RppBlockContent::Content(tokens) = child
                        && let Some(Token::Identifier(kind)) = tokens.first()
                        && kind == "MARKER"
                    {
                        match super::marker_region::MarkerRegion::from_marker_tokens(tokens) {
                            Ok(marker_region) => project.markers_regions.add(marker_region),
                            Err(e) => eprintln!("Warning: Failed to parse marker: {}", e),
                        }
                    }
                }
            }

            if block.block_type == BlockType::TempoEnvEx
                && options.parse_tempo_envelope
                && let Some(tempo_envelope) = Self::parse_tempo_envelope_block(block)
            {
                project.tempo_envelope = Some(tempo_envelope);
            }
        }

        // Parallel parsing requires the `parallel` feature (rayon).
        // WASM builds always take the serial branch.
        #[cfg(feature = "parallel")]
        let can_parallelize = rpp_project.blocks.len() >= 16;
        #[cfg(not(feature = "parallel"))]
        let can_parallelize = false;
        if can_parallelize {
            #[cfg(not(feature = "parallel"))]
            unreachable!("parallel branch entered without rayon feature");
            #[cfg(feature = "parallel")]
            {
                if options.parse_tracks {
                    let mut track_results: Vec<(usize, Result<Track, String>)> = rpp_project
                        .blocks
                        .par_iter()
                        .enumerate()
                        .filter_map(|(idx, block)| {
                            if block.block_type != BlockType::Track {
                                return None;
                            }
                            Some((
                                idx,
                                Track::from_block_with_options(block, options.track_options),
                            ))
                        })
                        .collect();
                    track_results.sort_by_key(|(idx, _)| *idx);
                    for (idx, result) in track_results {
                        match result {
                            Ok(track) => project.tracks.push(track),
                            Err(e) => {
                                eprintln!("Warning: Failed to parse track at block {idx}: {e}")
                            }
                        }
                    }
                }

                if options.parse_project_items {
                    let mut item_results: Vec<(usize, Result<Item, String>)> = rpp_project
                        .blocks
                        .par_iter()
                        .enumerate()
                        .filter_map(|(idx, block)| {
                            if block.block_type != BlockType::Item {
                                return None;
                            }
                            Some((idx, Item::from_block(block)))
                        })
                        .collect();
                    item_results.sort_by_key(|(idx, _)| *idx);
                    for (idx, result) in item_results {
                        match result {
                            Ok(item) => project.items.push(item),
                            Err(e) => {
                                eprintln!("Warning: Failed to parse item at block {idx}: {e}")
                            }
                        }
                    }
                }

                if options.parse_project_envelopes {
                    let mut envelope_results: Vec<(usize, Result<Envelope, String>)> = rpp_project
                        .blocks
                        .par_iter()
                        .enumerate()
                        .filter_map(|(idx, block)| {
                            if block.block_type != BlockType::Envelope {
                                return None;
                            }
                            Some((idx, Envelope::from_block(block)))
                        })
                        .collect();
                    envelope_results.sort_by_key(|(idx, _)| *idx);
                    for (idx, result) in envelope_results {
                        match result {
                            Ok(envelope) => project.envelopes.push(envelope),
                            Err(e) => {
                                eprintln!("Warning: Failed to parse envelope at block {idx}: {e}")
                            }
                        }
                    }
                }

                if options.parse_project_fxchains {
                    let mut fx_results: Vec<(usize, Result<FxChain, String>)> = rpp_project
                        .blocks
                        .par_iter()
                        .enumerate()
                        .filter_map(|(idx, block)| {
                            if block.block_type != BlockType::FxChain {
                                return None;
                            }
                            Some((idx, FxChain::from_block(block)))
                        })
                        .collect();
                    fx_results.sort_by_key(|(idx, _)| *idx);
                    for (idx, result) in fx_results {
                        match result {
                            Ok(fx) => project.fx_chains.push(fx),
                            Err(e) => {
                                eprintln!("Warning: Failed to parse FX chain at block {idx}: {e}")
                            }
                        }
                    }
                }
            } // end #[cfg(feature = "parallel")] block
        } else {
            for block in &rpp_project.blocks {
                match block.block_type {
                    BlockType::Track if options.parse_tracks => {
                        match Track::from_block_with_options(block, options.track_options) {
                            Ok(track) => project.tracks.push(track),
                            Err(e) => eprintln!("Warning: Failed to parse track: {}", e),
                        }
                    }
                    BlockType::Item if options.parse_project_items => match Item::from_block(block)
                    {
                        Ok(item) => project.items.push(item),
                        Err(e) => eprintln!("Warning: Failed to parse item: {}", e),
                    },
                    BlockType::Envelope if options.parse_project_envelopes => {
                        match Envelope::from_block(block) {
                            Ok(envelope) => project.envelopes.push(envelope),
                            Err(e) => eprintln!("Warning: Failed to parse envelope: {}", e),
                        }
                    }
                    BlockType::FxChain if options.parse_project_fxchains => {
                        match FxChain::from_block(block) {
                            Ok(fx_chain) => project.fx_chains.push(fx_chain),
                            Err(e) => eprintln!("Warning: Failed to parse FX chain: {}", e),
                        }
                    }
                    _ => {}
                }
            }
        }

        // Process markers to create regions from START/END marker pairs
        if options.parse_markers_regions {
            project.markers_regions.process_regions();
        }

        Ok(project)
    }

    /// Parse a TEMPOENVEX block into a TempoTimeEnvelope
    fn parse_tempo_envelope_block(
        block: &crate::primitives::RppBlock,
    ) -> Option<TempoTimeEnvelope> {
        use super::time_tempo::TempoTimePoint;

        let mut points = Vec::new();
        let mut default_tempo = 120.0;
        let mut default_time_signature = (4, 4);

        // Parse the block content to extract tempo points
        for child in &block.children {
            if let crate::primitives::RppBlockContent::Content(tokens) = child
                && let Some(Token::Identifier(kind)) = tokens.first()
                && kind == "PT"
                && let Ok(point) = TempoTimePoint::from_tokens(tokens)
            {
                points.push(point);
            }
        }

        // Create tempo envelope with the parsed points
        if !points.is_empty() {
            // Use the tempo from the first point as the default tempo
            // Sort points by position to ensure we get the earliest one
            points.sort_by(|a, b| {
                a.position
                    .partial_cmp(&b.position)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            default_tempo = points[0].tempo;

            // Try to get time signature from the first point
            if let Some(time_sig) = points[0].time_signature() {
                default_time_signature = time_sig;
            }

            let mut envelope = TempoTimeEnvelope::new(default_tempo, default_time_signature);
            for point in points {
                envelope.add_point(point);
            }
            Some(envelope)
        } else {
            None
        }
    }
}

impl fmt::Display for ReaperProject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "REAPER Project v{} ({})",
            self.version, self.version_string
        )?;
        writeln!(f, "Timestamp: {}", self.timestamp)?;
        writeln!(f, "Tracks: {}", self.tracks.len())?;
        writeln!(f, "Items: {}", self.items.len())?;
        writeln!(f, "Envelopes: {}", self.envelopes.len())?;
        writeln!(f, "FX Chains: {}", self.fx_chains.len())?;
        writeln!(
            f,
            "Markers: {}, Regions: {}",
            self.markers_regions.markers.len(),
            self.markers_regions.regions.len()
        )?;

        // Display key project properties
        if let Some(tempo) = self.properties.tempo {
            writeln!(
                f,
                "Tempo: {} BPM, Time Signature: {}/{}",
                tempo.0, tempo.1, tempo.2
            )?;
        }
        if let Some(sample_rate) = self.properties.sample_rate {
            writeln!(f, "Sample Rate: {} Hz", sample_rate.0)?;
        }
        if let Some(selection) = self.properties.selection {
            writeln!(f, "Selection: {} - {}", selection.0, selection.1)?;
        }
        if let Some(zoom) = self.properties.zoom {
            writeln!(f, "Zoom: {}", zoom.0)?;
        }
        if let Some(cursor) = self.properties.cursor {
            writeln!(f, "Cursor: {}", cursor)?;
        }

        // Display metronome settings
        if let Some(metronome) = &self.properties.metronome {
            writeln!(f, "\nMetronome:")?;
            writeln!(
                f,
                "  Volume: {:.3} / {:.3}",
                metronome.volume.0, metronome.volume.1
            )?;
            writeln!(f, "  Beat Length: {}", metronome.beat_length)?;
            writeln!(
                f,
                "  Frequency: {} / {} Hz",
                metronome.frequency.0, metronome.frequency.1
            )?;
            writeln!(f, "  Pattern: {}", metronome.pattern_string)?;
            writeln!(f, "  Multiplier: {}", metronome.mult)?;
        }

        // Display envelope settings
        if let Some(master_play_speed_env) = &self.properties.master_play_speed_env {
            writeln!(f, "\nMaster Play Speed Envelope:")?;
            write!(f, "{}", master_play_speed_env)?;
        }

        if let Some(tempo_env) = &self.properties.tempo_env {
            writeln!(f, "\nTempo Envelope:")?;
            write!(f, "{}", tempo_env)?;
        }

        for (i, track) in self.tracks.iter().enumerate() {
            writeln!(f, "Track {}: {}", i + 1, track.name)?;
        }

        Ok(())
    }
}

// Forward declarations for the types we use
pub use crate::types::envelope::Envelope;
pub use crate::types::fx_chain::FxChain;
pub use crate::types::item::Item;
pub use crate::types::track::Track;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::project::parse_rpp;

    #[test]
    fn test_simple_project_parsing() {
        let input = r#"<REAPER_PROJECT 0.1 "7.42/linux-x86_64" 1758257333
  RIPPLE 0 0
  TEMPO 120 4 4 0
>"#;

        let result = parse_rpp(input);
        assert!(result.is_ok());

        let (remaining, rpp_project) = result.unwrap();
        println!("Remaining input: '{}'", remaining);
        println!("Number of blocks: {}", rpp_project.blocks.len());

        for (i, block) in rpp_project.blocks.iter().enumerate() {
            println!(
                "Block {}: {} with {} children",
                i,
                block.name,
                block.children.len()
            );
            for (j, child) in block.children.iter().enumerate() {
                match child {
                    RppBlockContent::Content(tokens) => {
                        println!("  Content {}: {:?}", j, tokens);
                    }
                    RppBlockContent::RawLine(raw) => {
                        println!("  RawLine {}: {}", j, raw);
                    }
                    RppBlockContent::Block(_) => {
                        println!("  Nested block {}", j);
                    }
                }
            }
        }

        assert_eq!(remaining, "");
    }

    #[test]
    fn test_project_properties_parsing() {
        let input = r#"<REAPER_PROJECT 0.1 "7.42/linux-x86_64" 1758257333
  RIPPLE 0 0
  GROUPOVERRIDE 0 0 0
  AUTOXFADE 129
  ENVATTACH 3
  POOLEDENVATTACH 0
  MIXERUIFLAGS 11 48
  ENVFADESZ10 40
  PEAKGAIN 1
  FEEDBACK 0
  PANLAW 1
  PROJOFFS 0 0 0
  MAXPROJLEN 0 0
  GRID 3199 8 1 8 1 0 0 0
  TIMEMODE 1 5 -1 30 0 0 -1
  VIDEO_CONFIG 0 0 65792
  PANMODE 3
  PANLAWFLAGS 3
  CURSOR 11
  ZOOM 100 0 0
  VZOOMEX 6 0
  USE_REC_CFG 0
  RECMODE 1
  SMPTESYNC 0 30 100 40 1000 300 0 0 1 0 0
  LOOP 0
  LOOPGRAN 0 4
  RECORD_PATH "Media" ""
  RENDER_FILE ""
  RENDER_PATTERN ""
  RENDER_FMT 0 2 0
  RENDER_1X 0
  RENDER_RANGE 1 0 0 0 1000
  RENDER_RESAMPLE 3 0 1
  RENDER_ADDTOPROJ 0
  RENDER_STEMS 0
  RENDER_DITHER 0
  RENDER_TRIM 0 0 0 0
  TIMELOCKMODE 1
  TEMPOENVLOCKMODE 1
  ITEMMIX 1
  DEFPITCHMODE 589824 0
  TAKELANE 1
  SAMPLERATE 44100 0 0
  LOCK 1
  TEMPO 120 4 4 0
  PLAYRATE 1 0 0.25 4
  SELECTION 0 0
  SELECTION2 0 0
  MASTERAUTOMODE 0
  MASTERTRACKHEIGHT 0 0
  MASTERPEAKCOL 16576
  MASTERMUTESOLO 0
  MASTERTRACKVIEW 0 0.6667 0.5 0.5 0 0 0 0 0 0 0 0 0 0
  MASTERHWOUT 0 0 1 0 0 0 0 -1
  MASTER_NCH 2 2
  MASTER_VOLUME 1 0 -1 -1 1
  MASTER_PANMODE 3
  MASTER_PANLAWFLAGS 3
  MASTER_FX 1
  MASTER_SEL 0
  GLOBAL_AUTO -1
>"#;

        let result = parse_rpp(input);
        assert!(result.is_ok());

        let (remaining, rpp_project) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(rpp_project.version, 0.1);
        assert_eq!(rpp_project.version_string, "7.42/linux-x86_64");
        assert_eq!(rpp_project.timestamp, 1758257333);

        // Test that we can create a ReaperProject with properties
        let reaper_project = ReaperProject::from_rpp_project(&rpp_project);
        assert!(reaper_project.is_ok());

        let project = reaper_project.unwrap();
        let props = &project.properties;

        // Test some key properties
        assert_eq!(props.ripple, Some((0, 0)));
        assert_eq!(props.group_override, Some((0, 0, 0)));
        assert_eq!(props.auto_xfade, Some(129));
        assert_eq!(props.env_attach, Some(3));
        assert_eq!(props.pooled_env_attach, Some(0));
        assert_eq!(props.mixer_ui_flags, Some((11, 48)));
        assert_eq!(props.env_fade_sz10, Some(40));
        assert_eq!(props.peak_gain, Some(1));
        assert_eq!(props.feedback, Some(0));
        assert_eq!(props.pan_law, Some(1));
        assert_eq!(props.proj_offs, Some((0, 0, 0)));
        assert_eq!(props.max_proj_len, Some((0, 0)));
        assert_eq!(props.grid, Some((3199, 8, 1, 8, 1, 0, 0, 0)));
        assert_eq!(props.time_mode, Some((1, 5, -1, 30, 0, 0, -1)));
        assert_eq!(props.video_config, Some((0, 0, 65792)));
        assert_eq!(props.pan_mode, Some(3));
        assert_eq!(props.pan_law_flags, Some(3));
        assert_eq!(props.cursor, Some(11));
        assert_eq!(props.zoom, Some((100, 0, 0)));
        assert_eq!(props.v_zoom_ex, Some((6, 0)));
        assert_eq!(props.use_rec_cfg, Some(0));
        assert_eq!(props.rec_mode, Some(1));
        assert_eq!(
            props.smpte_sync,
            Some((0, 30, 100, 40, 1000, 300, 0, 0, 1, 0, 0))
        );
        assert_eq!(props.r#loop, Some(0));
        assert_eq!(props.loop_gran, Some((0, 4)));
        assert_eq!(
            props.record_path,
            Some(("Media".to_string(), "".to_string()))
        );
        assert_eq!(props.render_file, Some("".to_string()));
        assert_eq!(props.render_pattern, Some("".to_string()));
        assert_eq!(props.render_fmt, Some((0, 2, 0)));
        assert_eq!(props.render_1x, Some(0));
        assert_eq!(props.render_range, Some((1, 0, 0, 0, 1000)));
        assert_eq!(props.render_resample, Some((3, 0, 1)));
        assert_eq!(props.render_add_to_proj, Some(0));
        assert_eq!(props.render_stems, Some(0));
        assert_eq!(props.render_dither, Some(0));
        assert_eq!(props.render_trim, Some((0, 0, 0, 0)));
        assert_eq!(props.time_lock_mode, Some(1));
        assert_eq!(props.tempo_env_lock_mode, Some(1));
        assert_eq!(props.item_mix, Some(1));
        assert_eq!(props.def_pitch_mode, Some((589824, 0)));
        assert_eq!(props.take_lane, Some(1));
        assert_eq!(props.sample_rate, Some((44100, 0, 0)));
        assert_eq!(props.lock, Some(1));
        assert_eq!(props.tempo, Some((120.0, 4, 4, 0)));
        assert_eq!(props.play_rate, Some((1, 0, 0, 4)));
        assert_eq!(props.selection, Some((0, 0)));
        assert_eq!(props.selection2, Some((0, 0)));
        assert_eq!(props.master_auto_mode, Some(0));
        assert_eq!(props.master_track_height, Some((0, 0)));
        assert_eq!(props.master_peak_col, Some(16576));
        assert_eq!(props.master_mute_solo, Some(0));
        assert_eq!(props.master_hw_out, Some((0, 0, 1, 0, 0, 0, 0, -1)));
        assert_eq!(props.master_nch, Some((2, 2)));
        assert_eq!(props.master_volume, Some((1.0, 0.0, -1.0, -1.0, 1.0)));
        assert_eq!(props.master_pan_mode, Some(3));
        assert_eq!(props.master_pan_law_flags, Some(3));
        assert_eq!(props.master_fx, Some(1));
        assert_eq!(props.master_sel, Some(0));
        assert_eq!(props.global_auto, Some(-1));

        // Test master_track_view
        assert!(props.master_track_view.is_some());
        let master_view = props.master_track_view.as_ref().unwrap();
        assert_eq!(master_view.field1, 0);
        assert_eq!(master_view.field2, 0.6667);
        assert_eq!(master_view.field3, 0.5);
        assert_eq!(master_view.field4, 0.5);
        assert_eq!(master_view.field5, 0);
        assert_eq!(master_view.field6, 0);
        assert_eq!(master_view.field7, 0);
        assert_eq!(master_view.field8, 0);
        assert_eq!(master_view.field9, 0);
        assert_eq!(master_view.field10, 0);
        assert_eq!(master_view.field11, 0);
        assert_eq!(master_view.field12, 0);
        assert_eq!(master_view.field13, 0);
        assert_eq!(master_view.field14, 0);
    }

    /// REAPER writes tempo fractionally and songs use it. This was
    /// parsed through `as i32`, so `TEMPO 86.28` became 86 — 9 ms of
    /// error per bar, two thirds of a second by the end of a
    /// three-minute song, which is well past where a lighting cue or a
    /// click stops landing.
    #[test]
    fn fractional_tempo_survives_parsing() {
        let mut props = ProjectProperties::default();
        for (line, want) in [
            ("TEMPO 86.28 4 4 0", 86.28),
            // Four decimal places, which is what REAPER's tempo field
            // offers and what a tempo-mapped song actually contains.
            ("TEMPO 128.0001 4 4 0", 128.0001),
            ("TEMPO 120 4 4 0", 120.0),
        ] {
            let tokens = crate::primitives::token::parse_token_line(line).unwrap().1;
            props.parse_property("TEMPO", &tokens[1..]);
            let (bpm, num, den, _) = props.tempo.expect("TEMPO parsed");
            assert_eq!(bpm, want, "{line}");
            assert_eq!((num, den), (4, 4));
        }
    }

    #[test]
    fn test_automation_mode_enum() {
        assert_eq!(AutomationMode::from_value(0), AutomationMode::TrimRead);
        assert_eq!(AutomationMode::from_value(1), AutomationMode::Read);
        assert_eq!(AutomationMode::from_value(2), AutomationMode::Touch);
        assert_eq!(AutomationMode::from_value(3), AutomationMode::Write);
        assert_eq!(AutomationMode::from_value(4), AutomationMode::Latch);

        assert_eq!(AutomationMode::TrimRead.to_value(), 0);
        assert_eq!(AutomationMode::Read.to_value(), 1);
        assert_eq!(AutomationMode::Touch.to_value(), 2);
        assert_eq!(AutomationMode::Write.to_value(), 3);
        assert_eq!(AutomationMode::Latch.to_value(), 4);

        assert_eq!(format!("{}", AutomationMode::TrimRead), "Trim/Read");
        assert_eq!(format!("{}", AutomationMode::Read), "Read");
        assert_eq!(format!("{}", AutomationMode::Touch), "Touch");
        assert_eq!(format!("{}", AutomationMode::Write), "Write");
        assert_eq!(format!("{}", AutomationMode::Latch), "Latch");
    }

    #[test]
    fn test_envelope_shape_enum() {
        assert_eq!(EnvelopeShape::from_value(0), EnvelopeShape::Linear);
        assert_eq!(EnvelopeShape::from_value(1), EnvelopeShape::Square);
        assert_eq!(EnvelopeShape::from_value(2), EnvelopeShape::SlowStartEnd);
        assert_eq!(EnvelopeShape::from_value(3), EnvelopeShape::FastStart);
        assert_eq!(EnvelopeShape::from_value(4), EnvelopeShape::FastEnd);
        assert_eq!(EnvelopeShape::from_value(5), EnvelopeShape::Bezier);

        assert_eq!(EnvelopeShape::Linear.to_value(), 0);
        assert_eq!(EnvelopeShape::Square.to_value(), 1);
        assert_eq!(EnvelopeShape::SlowStartEnd.to_value(), 2);
        assert_eq!(EnvelopeShape::FastStart.to_value(), 3);
        assert_eq!(EnvelopeShape::FastEnd.to_value(), 4);
        assert_eq!(EnvelopeShape::Bezier.to_value(), 5);

        assert_eq!(format!("{}", EnvelopeShape::Linear), "Linear");
        assert_eq!(format!("{}", EnvelopeShape::Square), "Square");
        assert_eq!(format!("{}", EnvelopeShape::SlowStartEnd), "Slow Start/End");
        assert_eq!(format!("{}", EnvelopeShape::FastStart), "Fast Start");
        assert_eq!(format!("{}", EnvelopeShape::FastEnd), "Fast End");
        assert_eq!(format!("{}", EnvelopeShape::Bezier), "Bezier");
    }

    #[test]
    fn test_parse_template_with_takes_lanes() {
        // Read the template file
        let template_path = "resources/Template-with-takes-lanes.RPP";
        let content = std::fs::read_to_string(template_path).expect("Failed to read template file");

        // Parse the RPP file
        let (remaining, rpp_project) =
            crate::primitives::project::parse_rpp(&content).expect("Failed to parse template file");

        // Verify basic project structure
        assert_eq!(rpp_project.version, 0.1);
        assert_eq!(rpp_project.version_string, "7.42/linux-x86_64");
        assert_eq!(rpp_project.timestamp, 1758257039);

        // Verify we have tracks
        assert!(!rpp_project.blocks.is_empty(), "Should have blocks");

        // Convert to ReaperProject
        let project = ReaperProject::from_rpp_project(&rpp_project)
            .expect("Failed to convert to ReaperProject");

        // Verify basic project properties
        assert_eq!(project.properties.tempo, Some((120.0, 4, 4, 0)));
        assert_eq!(project.properties.sample_rate, Some((44100, 0, 0)));
        assert_eq!(project.properties.ripple, Some((0, 0)));
        assert_eq!(project.properties.auto_xfade, Some(393));
        assert_eq!(project.properties.pan_mode, Some(3));
        assert_eq!(project.properties.cursor, Some(2));
        assert_eq!(project.properties.zoom, Some((100, 0, 0)));
        assert_eq!(project.properties.r#loop, Some(0));

        // Verify we have tracks (the parser is now correctly parsing all blocks)
        // Note: Currently Track::from_block is not implemented, so tracks.len() will be 0
        // But we can verify that the parser is working by checking the total number of blocks
        assert_eq!(
            rpp_project.blocks.len(),
            67,
            "Should have 67 total blocks (including nested blocks)"
        );

        // Verify that we have the expected block types
        let track_blocks: Vec<_> = rpp_project
            .blocks
            .iter()
            .filter(|b| b.block_type == crate::primitives::BlockType::Track)
            .collect();
        let item_blocks: Vec<_> = rpp_project
            .blocks
            .iter()
            .filter(|b| b.block_type == crate::primitives::BlockType::Item)
            .collect();

        assert_eq!(
            track_blocks.len(),
            57,
            "Should have 57 TRACK blocks (including nested items)"
        );
        assert_eq!(
            item_blocks.len(),
            0,
            "Should have 0 top-level ITEM blocks (they're nested in tracks)"
        );

        // Verify first track block
        let first_track = &track_blocks[0];
        assert_eq!(first_track.name, "TRACK");

        // Note: ITEM blocks are now properly nested within TRACK blocks
        // so they don't appear as top-level blocks

        // Verify remaining input is minimal (just the final >)
        assert!(
            remaining.len() <= 2,
            "Remaining input should be minimal, got: '{}'",
            remaining
        );

        println!(
            "✅ Successfully parsed template with {} tracks and {} items",
            project.tracks.len(),
            project.items.len()
        );
    }

    #[test]
    fn test_decode_options_summary_skips_heavy_track_domains() {
        let input = r#"<REAPER_PROJECT 0.1 "7.42/linux-x86_64" 1758257333
  <TRACK
    NAME "Track 1"
    <ITEM
      POSITION 0
      LENGTH 1
    >
    <VOLENV
      PT 0 1 0
    >
    <FXCHAIN
      SHOW 0
    >
  >
>"#;

        let (_, parsed) = parse_rpp(input).expect("parse_rpp");
        let full = ReaperProject::from_rpp_project_with_options(&parsed, DecodeOptions::full())
            .expect("full decode");
        let summary =
            ReaperProject::from_rpp_project_with_options(&parsed, DecodeOptions::summary())
                .expect("summary decode");

        assert_eq!(full.tracks.len(), 1);
        assert_eq!(summary.tracks.len(), 1);
        assert_eq!(full.tracks[0].items.len(), 1);
        assert_eq!(summary.tracks[0].items.len(), 0);
    }

    #[test]
    fn test_parallel_decode_is_deterministic() {
        let mut input = String::from("<REAPER_PROJECT 0.1 \"7.42/linux-x86_64\" 1758257333\n");
        for i in 0..20 {
            input.push_str(&format!(
                "  <TRACK\n    NAME \"Track {i}\"\n    <ITEM\n      POSITION {i}\n      LENGTH 1\n    >\n  >\n"
            ));
        }
        input.push('>');

        let (_, parsed) = parse_rpp(&input).expect("parse_rpp");
        let options = DecodeOptions::summary();
        let a =
            ReaperProject::from_rpp_project_with_options(&parsed, options).expect("decode pass A");
        let b =
            ReaperProject::from_rpp_project_with_options(&parsed, options).expect("decode pass B");

        assert_eq!(a, b);
        assert_eq!(a.tracks.len(), 20);
    }
}
