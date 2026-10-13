//! Default project settings saved to `reaper.ini` `[REAPER]` section.
//!
//! These are the values written when the user clicks **"Save as Default Project
//! Settings"** in REAPER's Project Settings dialog.  They live in `reaper.ini`
//! (not the `.rpp` file) and apply to every new project until changed.
//!
//! Reference: Mespotine's REAPER config-variable documentation at
//! <https://mespotin.uber.space/Ultraschall/Reaper_Config_Variables.html>

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::parse::*;
use crate::types::common::{PanLawTaper, PanMode, RippleMode, TimeDisplayFormat, TrackMixBitDepth};

const S: &str = "REAPER";

// ────────────────────────────────────────────────────────────────────────────
// Tempo / time-signature
// ────────────────────────────────────────────────────────────────────────────

/// Default project tempo and time-signature settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct ProjectTempoDefaults {
    /// Default project BPM.
    /// Key: `projbpm`
    pub bpm: Option<f64>,

    /// Default project sample rate in Hz (e.g. 44100, 48000).
    /// Key: `projsrate`
    pub sample_rate: Option<i32>,

    /// Use project sample rate instead of audio device rate.
    /// Key: `projsrateuse`
    pub sample_rate_use: Option<bool>,

    /// Default master-track channel count.
    /// Key: `projmasternch`
    pub master_nch: Option<i32>,

    /// Default measure length in beats.
    /// Key: `projmeaslen`
    pub measure_len: Option<i32>,

    /// Default measure offset (ruler start offset in beats).
    /// Key: `projmeasoffs`
    pub measure_offs: Option<i32>,

    /// Default time-signature denominator (4 = quarter note, 8 = eighth note, …).
    /// Key: `projtsdenom`
    pub ts_denom: Option<i32>,

    /// Default beat base (0 = quarter note, 1 = eighth note, …).
    /// Key: `projbeatbase`
    pub beat_base: Option<i32>,
}

impl ProjectTempoDefaults {
    /// Returns a new [`ProjectTempoDefaults`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            bpm: Some(120.0),             // projbpm=120.00000000000000
            sample_rate: Some(44100),     // projsrate=44100
            sample_rate_use: Some(false), // projsrateuse=0
            master_nch: Some(2),          // projmasternch=2
            measure_len: Some(4),         // projmeaslen=4
            measure_offs: Some(0),        // projmeasoffs=0
            ts_denom: Some(4),            // projtsdenom=4
            beat_base: Some(0),           // projbeatbase=0
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            bpm: get_f64(ini, S, "projbpm"),
            sample_rate: get_i32(ini, S, "projsrate"),
            sample_rate_use: get_bool_from_i32(ini, S, "projsrateuse"),
            master_nch: get_i32(ini, S, "projmasternch"),
            measure_len: get_i32(ini, S, "projmeaslen"),
            measure_offs: get_i32(ini, S, "projmeasoffs"),
            ts_denom: get_i32(ini, S, "projtsdenom"),
            beat_base: get_i32(ini, S, "projbeatbase"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_f64(ini, S, "projbpm", self.bpm);
        set_opt_i32(ini, S, "projsrate", self.sample_rate);
        set_opt_bool_as_i32(ini, S, "projsrateuse", self.sample_rate_use);
        set_opt_i32(ini, S, "projmasternch", self.master_nch);
        set_opt_i32(ini, S, "projmeaslen", self.measure_len);
        set_opt_i32(ini, S, "projmeasoffs", self.measure_offs);
        set_opt_i32(ini, S, "projtsdenom", self.ts_denom);
        set_opt_i32(ini, S, "projbeatbase", self.beat_base);
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Time display
// ────────────────────────────────────────────────────────────────────────────

/// Default project time-display mode.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct ProjectTimeDisplayDefaults {
    /// Ruler time-display format.
    /// Key: `projtimemode`
    #[facet(opaque)]
    pub time_mode: Option<TimeDisplayFormat>,

    /// Transport time-display format.
    /// Key: `projtimemode2`
    #[facet(opaque)]
    pub time_mode2: Option<TimeDisplayFormat>,

    /// Project time offset in seconds.
    /// Key: `projtimeoffs`
    pub time_offs: Option<f64>,
}

impl ProjectTimeDisplayDefaults {
    /// Returns a new [`ProjectTimeDisplayDefaults`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            time_mode: Some(TimeDisplayFormat::MeasuresBeats), // projtimemode=1
            time_mode2: Some(TimeDisplayFormat::Unknown(-1)),  // projtimemode2=-1
            time_offs: Some(0.0),                              // projtimeoffs=0.00000000000000
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            time_mode: get_enum_i32(ini, S, "projtimemode"),
            time_mode2: get_enum_i32(ini, S, "projtimemode2"),
            time_offs: get_f64(ini, S, "projtimeoffs"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_enum_i32(ini, S, "projtimemode", self.time_mode);
        set_opt_enum_i32(ini, S, "projtimemode2", self.time_mode2);
        set_opt_f64(ini, S, "projtimeoffs", self.time_offs);
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Grid
// ────────────────────────────────────────────────────────────────────────────

/// Default project grid and snap settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct ProjectGridDefaults {
    /// Grid division value.
    /// Key: `projgriddiv`
    pub grid_div: Option<f64>,

    /// Snap grid division value.
    /// Key: `projgriddivsnap`
    pub grid_div_snap: Option<f64>,

    /// Minimum grid line spacing in pixels before subdivisions are hidden.
    /// Key: `projgridmin`
    pub grid_min: Option<i32>,

    /// Minimum snap grid spacing in pixels.
    /// Key: `projgridsnapmin`
    pub grid_snap_min: Option<i32>,

    /// Grid frame-rate setting (for frame-based grids).
    /// Key: `projgridframe`
    pub grid_frame: Option<i32>,

    /// Grid swing/shuffle amount (0.0 – 1.0).
    /// Key: `projgridswing`
    pub grid_swing: Option<f64>,

    /// Grid visibility bitfield (0 = hidden, non-zero = shown).
    /// Key: `projshowgrid`
    pub show_grid: Option<i32>,
}

impl ProjectGridDefaults {
    /// Returns a new [`ProjectGridDefaults`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            grid_div: Some(1.0),      // projgriddiv=1.00000000000000
            grid_div_snap: Some(1.0), // projgriddivsnap=1.00000000000000
            grid_min: Some(8),        // projgridmin=8
            grid_snap_min: Some(8),   // projgridsnapmin=8
            grid_frame: Some(0),      // projgridframe=0
            grid_swing: Some(0.0),    // projgridswing=0.00000000000000
            show_grid: Some(3327),    // projshowgrid=3327
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            grid_div: get_f64(ini, S, "projgriddiv"),
            grid_div_snap: get_f64(ini, S, "projgriddivsnap"),
            grid_min: get_i32(ini, S, "projgridmin"),
            grid_snap_min: get_i32(ini, S, "projgridsnapmin"),
            grid_frame: get_i32(ini, S, "projgridframe"),
            grid_swing: get_f64(ini, S, "projgridswing"),
            show_grid: get_i32(ini, S, "projshowgrid"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_f64(ini, S, "projgriddiv", self.grid_div);
        set_opt_f64(ini, S, "projgriddivsnap", self.grid_div_snap);
        set_opt_i32(ini, S, "projgridmin", self.grid_min);
        set_opt_i32(ini, S, "projgridsnapmin", self.grid_snap_min);
        set_opt_i32(ini, S, "projgridframe", self.grid_frame);
        set_opt_f64(ini, S, "projgridswing", self.grid_swing);
        set_opt_i32(ini, S, "projshowgrid", self.show_grid);
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Project behaviour
// ────────────────────────────────────────────────────────────────────────────

/// Default project behaviour flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct ProjectBehaviorDefaults {
    /// Default ripple-edit mode.
    /// Key: `projripedit`
    #[facet(opaque)]
    pub rip_edit: Option<RippleMode>,

    /// Track-group override bitfield.
    /// Key: `projgroupover`
    pub group_over: Option<i32>,

    /// Selecting one item also selects its group.
    /// Key: `projgroupsel`
    pub group_sel: Option<bool>,

    /// Selection lock state (prevents accidental deselection).
    /// Key: `projsellock`
    pub sel_lock: Option<i32>,

    /// Take-lane display visibility (0 = hidden, 1 = shown).
    /// Key: `projtakelane`
    pub take_lane: Option<i32>,

    /// Save project file references with relative pathnames.
    /// Key: `projrelpath`
    pub rel_path: Option<bool>,

    /// Internal audio mixing precision.
    /// Key: `projintmix`
    #[facet(opaque)]
    pub int_mix: Option<TrackMixBitDepth>,

    /// Default recording mode bitfield.
    /// Key: `projrecmode`
    pub rec_mode: Option<i32>,

    /// Relative snap state.
    /// Key: `projrelsnap`
    pub rel_snap: Option<i32>,

    /// Pooled-envelope attachment mode.
    /// Key: `pooledenvattach`
    pub pooled_env_attach: Option<i32>,

    /// Whether track groups are disabled by default.
    /// Key: `projtrackgroupdisabled`
    pub track_group_disabled: Option<i32>,

    /// Align beats to playback rate.
    /// Key: `projalignbeatsrate`
    pub align_beats_rate: Option<i32>,

    /// Whether to use a separate recording path for open/copy operations.
    /// Key: `projrecforopencopy`
    pub rec_for_open_copy: Option<i32>,

    /// Default recording path (primary).
    /// Key: `projdefrecpath`
    pub def_rec_path: Option<String>,

    /// Default recording path (secondary / overflow).
    /// Key: `projdefrecpath2`
    pub def_rec_path2: Option<String>,

    /// Maximum project length in seconds.
    /// Key: `projmaxlen`
    pub max_len: Option<f64>,

    /// Limit project length and stop playback/recording at that point.
    /// Key: `projmaxlenuse`
    pub max_len_use: Option<bool>,
}

impl ProjectBehaviorDefaults {
    /// Returns a new [`ProjectBehaviorDefaults`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            rip_edit: Some(RippleMode::Off),          // projripedit=0
            group_over: Some(0),                      // projgroupover=0
            group_sel: Some(false),                   // projgroupsel=0
            sel_lock: Some(1),                        // projsellock=1
            take_lane: Some(1),                       // projtakelane=1
            rel_path: Some(true),                     // projrelpath=1
            int_mix: Some(TrackMixBitDepth::Float64), // projintmix=0
            rec_mode: Some(1),                        // projrecmode=1
            rel_snap: None,                           // projrelsnap not in factory defaults
            pooled_env_attach: Some(0),               // pooledenvattach=0
            track_group_disabled: Some(0),            // projtrackgroupdisabled=0
            align_beats_rate: None,                   // projalignbeatsrate= (empty)
            rec_for_open_copy: Some(0),               // projrecforopencopy=0
            def_rec_path: Some("Media".to_string()),  // projdefrecpath=Media
            def_rec_path2: None,                      // projdefrecpath2 not in factory defaults
            max_len: Some(0.0),                       // projmaxlen=0.00000000000000
            max_len_use: Some(false),                 // projmaxlenuse=0
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            rip_edit: get_enum_i32(ini, S, "projripedit"),
            group_over: get_i32(ini, S, "projgroupover"),
            group_sel: get_bool_from_i32(ini, S, "projgroupsel"),
            sel_lock: get_i32(ini, S, "projsellock"),
            take_lane: get_i32(ini, S, "projtakelane"),
            rel_path: get_bool_from_i32(ini, S, "projrelpath"),
            int_mix: get_enum_i32(ini, S, "projintmix"),
            rec_mode: get_i32(ini, S, "projrecmode"),
            rel_snap: get_i32(ini, S, "projrelsnap"),
            pooled_env_attach: get_i32(ini, S, "pooledenvattach"),
            track_group_disabled: get_i32(ini, S, "projtrackgroupdisabled"),
            align_beats_rate: get_i32(ini, S, "projalignbeatsrate"),
            rec_for_open_copy: get_i32(ini, S, "projrecforopencopy"),
            def_rec_path: get_string(ini, S, "projdefrecpath"),
            def_rec_path2: get_string(ini, S, "projdefrecpath2"),
            max_len: get_f64(ini, S, "projmaxlen"),
            max_len_use: get_bool_from_i32(ini, S, "projmaxlenuse"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_enum_i32(ini, S, "projripedit", self.rip_edit);
        set_opt_i32(ini, S, "projgroupover", self.group_over);
        set_opt_bool_as_i32(ini, S, "projgroupsel", self.group_sel);
        set_opt_i32(ini, S, "projsellock", self.sel_lock);
        set_opt_i32(ini, S, "projtakelane", self.take_lane);
        set_opt_bool_as_i32(ini, S, "projrelpath", self.rel_path);
        set_opt_enum_i32(ini, S, "projintmix", self.int_mix);
        set_opt_i32(ini, S, "projrecmode", self.rec_mode);
        set_opt_i32(ini, S, "projrelsnap", self.rel_snap);
        set_opt_i32(ini, S, "pooledenvattach", self.pooled_env_attach);
        set_opt_i32(ini, S, "projtrackgroupdisabled", self.track_group_disabled);
        set_opt_i32(ini, S, "projalignbeatsrate", self.align_beats_rate);
        set_opt_i32(ini, S, "projrecforopencopy", self.rec_for_open_copy);
        set_opt_string(ini, S, "projdefrecpath", &self.def_rec_path);
        set_opt_string(ini, S, "projdefrecpath2", &self.def_rec_path2);
        set_opt_f64(ini, S, "projmaxlen", self.max_len);
        set_opt_bool_as_i32(ini, S, "projmaxlenuse", self.max_len_use);
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Video
// ────────────────────────────────────────────────────────────────────────────

/// Default project video settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct ProjectVideoDefaults {
    /// Video-related flags bitfield.
    /// Key: `projvidflags`
    pub vid_flags: Option<i32>,

    /// Default video frame height in pixels.
    /// Key: `projvidh`
    pub vid_h: Option<i32>,

    /// Default video frame width in pixels.
    /// Key: `projvidw`
    pub vid_w: Option<i32>,

    /// Video frame-rate base (e.g. 25, 30, …).
    /// Key: `projfrbase`
    pub fr_base: Option<f64>,

    /// Drop-frame timecode flag (`false` = non-drop, `true` = drop-frame).
    /// Key: `projfrdrop`
    #[facet(opaque)]
    pub fr_drop: Option<bool>,

    /// Video frame index / frame-rate preset index.
    /// Key: `projfridx`
    pub fr_idx: Option<i32>,
}

impl ProjectVideoDefaults {
    /// Returns a new [`ProjectVideoDefaults`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            vid_flags: Some(65792), // projvidflags=65792
            vid_h: Some(0),         // projvidh=0
            vid_w: Some(0),         // projvidw=0
            fr_base: Some(30.0),    // projfrbase=30
            fr_drop: Some(false),   // projfrdrop=0
            fr_idx: None,           // projfridx not in factory defaults
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            vid_flags: get_i32(ini, S, "projvidflags"),
            vid_h: get_i32(ini, S, "projvidh"),
            vid_w: get_i32(ini, S, "projvidw"),
            fr_base: get_f64(ini, S, "projfrbase"),
            fr_drop: get_bool_from_i32(ini, S, "projfrdrop"),
            fr_idx: get_i32(ini, S, "projfridx"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_i32(ini, S, "projvidflags", self.vid_flags);
        set_opt_i32(ini, S, "projvidh", self.vid_h);
        set_opt_i32(ini, S, "projvidw", self.vid_w);
        set_opt_f64(ini, S, "projfrbase", self.fr_base);
        set_opt_bool_as_i32(ini, S, "projfrdrop", self.fr_drop);
        set_opt_i32(ini, S, "projfridx", self.fr_idx);
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Master track
// ────────────────────────────────────────────────────────────────────────────

/// Default master-track settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct ProjectMasterTrackDefaults {
    /// Master pan law value.
    /// Key: `projmasterpanlaw`
    pub pan_law: Option<f64>,

    /// Master pan law taper curve above −3 dB.
    /// Key: `projmasterpanlawflags`
    #[facet(opaque)]
    pub pan_law_flags: Option<PanLawTaper>,

    /// Master pan mode (stereo algorithm).
    /// Key: `projmasterpanmode`
    #[facet(opaque)]
    pub pan_mode: Option<PanMode>,

    /// Master VU meter flags bitfield.
    /// Key: `projmastervuflags`
    pub vu_flags: Option<i32>,

    /// Peak display gain adjustment.
    /// Key: `projpeaksgain`
    pub peaks_gain: Option<f64>,
}

impl ProjectMasterTrackDefaults {
    /// Returns a new [`ProjectMasterTrackDefaults`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            pan_law: Some(-1.0), // projmasterpanlaw=-1.00000000000000
            pan_law_flags: Some(PanLawTaper::HybridTaper), // projmasterpanlawflags=3
            pan_mode: Some(PanMode::StereoPan), // projmasterpanmode=3
            vu_flags: Some(2),   // projmastervuflags=2
            peaks_gain: Some(1.0), // projpeaksgain=1.00000000000000
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            pan_law: get_f64(ini, S, "projmasterpanlaw"),
            pan_law_flags: get_enum_i32(ini, S, "projmasterpanlawflags"),
            pan_mode: get_enum_i32(ini, S, "projmasterpanmode"),
            vu_flags: get_i32(ini, S, "projmastervuflags"),
            peaks_gain: get_f64(ini, S, "projpeaksgain"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_f64(ini, S, "projmasterpanlaw", self.pan_law);
        set_opt_enum_i32(ini, S, "projmasterpanlawflags", self.pan_law_flags);
        set_opt_enum_i32(ini, S, "projmasterpanmode", self.pan_mode);
        set_opt_i32(ini, S, "projmastervuflags", self.vu_flags);
        set_opt_f64(ini, S, "projpeaksgain", self.peaks_gain);
    }
}

// ────────────────────────────────────────────────────────────────────────────
// MetronomeEnableFlags
// ────────────────────────────────────────────────────────────────────────────

/// Controls when the project metronome is active.
///
/// Maps to REAPER's `projmetroen` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct MetronomeEnableFlags {
    /// Metronome active during playback. Default: `false`.
    pub during_playback: bool,
    /// Metronome active during recording. Default: `true`.
    pub during_recording: bool,
    /// Enable count-in before recording. Default: `true`.
    pub count_in: bool,
}

impl Default for MetronomeEnableFlags {
    fn default() -> Self {
        Self {
            during_playback: false,
            during_recording: true,
            count_in: true,
        }
    }
}

impl MetronomeEnableFlags {
    /// Encode as the `projmetroen` INI integer.
    pub fn to_i32(self) -> i32 {
        let mut v = 0i32;
        if self.during_playback {
            v |= 1;
        }
        if self.during_recording {
            v |= 2;
        }
        if self.count_in {
            v |= 4;
        }
        v
    }

    /// Decode from the `projmetroen` INI integer.
    pub fn from_i32(v: i32) -> Self {
        Self {
            during_playback: (v & 1) != 0,
            during_recording: (v & 2) != 0,
            count_in: (v & 4) != 0,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Metronome
// ────────────────────────────────────────────────────────────────────────────

/// Default metronome settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct ProjectMetronomeDefaults {
    /// When the metronome is active.
    /// Key: `projmetroen`
    pub enabled: Option<MetronomeEnableFlags>,

    /// Count-in length in measures before playback/record.
    /// Key: `projmetrocountin`
    pub count_in: Option<f64>,

    /// Beat duration in ms (for synthesized metronome tones).
    /// Key: `projmetrobeatlen`
    pub beat_len: Option<i32>,

    /// Strong-beat tone frequency in Hz.
    /// Key: `projmetrof1`
    pub freq1: Option<i32>,

    /// Weak-beat tone frequency in Hz.
    /// Key: `projmetrof2`
    pub freq2: Option<i32>,

    /// Optional path to strong-beat sample file.
    /// Key: `projmetrofn1`
    pub fn1: Option<String>,

    /// Optional path to weak-beat sample file.
    /// Key: `projmetrofn2`
    pub fn2: Option<String>,

    /// Strong-beat volume (linear, 1.0 = 0 dB).
    /// Key: `projmetrov1`
    pub vol1: Option<f64>,

    /// Weak-beat volume (linear, 1.0 = 0 dB).
    /// Key: `projmetrov2`
    pub vol2: Option<f64>,
}

impl ProjectMetronomeDefaults {
    /// Returns a new [`ProjectMetronomeDefaults`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            enabled: Some(MetronomeEnableFlags::default()), // projmetroen=6 (record + count-in)
            count_in: Some(2.0),                            // projmetrocountin=2.00000000000000
            beat_len: Some(4),                              // projmetrobeatlen=4
            freq1: None,                                    // projmetrof1= (empty)
            freq2: None,                                    // projmetrof2= (empty)
            fn1: None,                                      // projmetrofn1 not in factory defaults
            fn2: None,                                      // projmetrofn2 not in factory defaults
            vol1: Some(0.25),                               // projmetrov1=0.25000000000000
            vol2: Some(0.125),                              // projmetrov2=0.12500000000000
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            enabled: get_i32(ini, S, "projmetroen").map(MetronomeEnableFlags::from_i32),
            count_in: get_f64(ini, S, "projmetrocountin"),
            beat_len: get_i32(ini, S, "projmetrobeatlen"),
            freq1: get_i32(ini, S, "projmetrof1"),
            freq2: get_i32(ini, S, "projmetrof2"),
            fn1: get_string(ini, S, "projmetrofn1"),
            fn2: get_string(ini, S, "projmetrofn2"),
            vol1: get_f64(ini, S, "projmetrov1"),
            vol2: get_f64(ini, S, "projmetrov2"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        if let Some(f) = self.enabled {
            ini.set(S, "projmetroen", &f.to_i32().to_string());
        }
        set_opt_f64(ini, S, "projmetrocountin", self.count_in);
        set_opt_i32(ini, S, "projmetrobeatlen", self.beat_len);
        set_opt_i32(ini, S, "projmetrof1", self.freq1);
        set_opt_i32(ini, S, "projmetrof2", self.freq2);
        set_opt_string(ini, S, "projmetrofn1", &self.fn1);
        set_opt_string(ini, S, "projmetrofn2", &self.fn2);
        set_opt_f64(ini, S, "projmetrov1", self.vol1);
        set_opt_f64(ini, S, "projmetrov2", self.vol2);
    }
}

// ────────────────────────────────────────────────────────────────────────────
// SMPTE / timecode sync
// ────────────────────────────────────────────────────────────────────────────

/// Default SMPTE/timecode sync settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct ProjectSmpteDefaults {
    /// SMPTE ahead offset in samples.
    /// Key: `projsmpteahead`
    pub ahead: Option<i32>,

    /// SMPTE input configuration/mode.
    /// Key: `projsmpteinput`
    pub input: Option<i32>,

    /// SMPTE max free-running frames before resync.
    /// Key: `projsmptemaxfree`
    pub max_free: Option<i32>,

    /// SMPTE timecode offset in seconds.
    /// Key: `projsmpteoffs`
    pub offs: Option<f64>,

    /// SMPTE frame rate in fps (e.g. 25.0, 29.97, 30.0).
    /// Key: `projsmpterate`
    pub rate: Option<f64>,

    /// Use the project's frame rate for SMPTE timecode (vs. global rate).
    /// Key: `projsmpterateuseproj`
    pub rate_use_proj: Option<bool>,

    /// SMPTE resync mode.
    /// Key: `projsmpteresync`
    pub resync: Option<i32>,

    /// SMPTE resync mode for recording.
    /// Key: `projsmpteresync_rec`
    pub resync_rec: Option<i32>,

    /// SMPTE skip frames setting.
    /// Key: `projsmpteskip`
    pub skip: Option<i32>,

    /// SMPTE sync mode.
    /// Key: `projsmptesync`
    pub sync: Option<i32>,

    /// SMPTE forward-wind recording mode.
    /// Key: `projsmptefw_rec`
    pub fw_rec: Option<i32>,
}

impl ProjectSmpteDefaults {
    /// Returns a new [`ProjectSmpteDefaults`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            ahead: Some(1000),   // projsmpteahead=1000
            input: Some(0),      // projsmpteinput=0
            max_free: Some(300), // projsmptemaxfree=300
            offs: Some(0.0),     // projsmpteoffs=0.00000000000000
            rate: Some(30.0),    // projsmpterate=30.00000000000000
            rate_use_proj: None, // projsmpterateuseproj= (empty)
            resync: Some(100),   // projsmpteresync=100
            resync_rec: Some(0), // projsmpteresync_rec=0
            skip: Some(40),      // projsmpteskip=40
            sync: Some(0),       // projsmptesync=0
            fw_rec: Some(0),     // projsmptefw_rec=0
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            ahead: get_i32(ini, S, "projsmpteahead"),
            input: get_i32(ini, S, "projsmpteinput"),
            max_free: get_i32(ini, S, "projsmptemaxfree"),
            offs: get_f64(ini, S, "projsmpteoffs"),
            rate: get_f64(ini, S, "projsmpterate"),
            rate_use_proj: get_bool_from_i32(ini, S, "projsmpterateuseproj"),
            resync: get_i32(ini, S, "projsmpteresync"),
            resync_rec: get_i32(ini, S, "projsmpteresync_rec"),
            skip: get_i32(ini, S, "projsmpteskip"),
            sync: get_i32(ini, S, "projsmptesync"),
            fw_rec: get_i32(ini, S, "projsmptefw_rec"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_i32(ini, S, "projsmpteahead", self.ahead);
        set_opt_i32(ini, S, "projsmpteinput", self.input);
        set_opt_i32(ini, S, "projsmptemaxfree", self.max_free);
        set_opt_f64(ini, S, "projsmpteoffs", self.offs);
        set_opt_f64(ini, S, "projsmpterate", self.rate);
        set_opt_bool_as_i32(ini, S, "projsmpterateuseproj", self.rate_use_proj);
        set_opt_i32(ini, S, "projsmpteresync", self.resync);
        set_opt_i32(ini, S, "projsmpteresync_rec", self.resync_rec);
        set_opt_i32(ini, S, "projsmpteskip", self.skip);
        set_opt_i32(ini, S, "projsmptesync", self.sync);
        set_opt_i32(ini, S, "projsmptefw_rec", self.fw_rec);
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Render defaults (project-level)
// ────────────────────────────────────────────────────────────────────────────

/// Default render/bounce settings saved per project.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct ProjectRenderDefaults {
    /// Add rendered files to project after render (0 = no, 1 = yes).
    /// Key: `projrenderaddtoproj`
    pub add_to_proj: Option<i32>,

    /// Dither when rendering (0 = off, 1 = on).
    /// Key: `projrenderdither`
    pub dither: Option<i32>,

    /// Render output channel count (0 = same as project master).
    /// Key: `projrendernch`
    pub nch: Option<i32>,

    /// Render sample rate override (0 = same as project).
    /// Key: `projrendersrate`
    pub srate: Option<i32>,

    /// Render stems / multichannel mode bitfield.
    /// Key: `projrenderstems`
    pub stems: Option<i32>,

    /// Render resampling quality index.
    /// Key: `projrenderresample`
    pub resample: Option<i32>,

    /// Use project sample rate for mixing and FX/synth processing.
    /// Key: `projrenderrateinternal`
    pub rate_internal: Option<bool>,

    /// Render limiter threshold in dB (0 = disabled).
    /// Key: `projrenderlimit`
    pub limit: Option<f64>,

    /// Render queue delay in ms.
    /// Key: `projrenderqdelay`
    pub queue_delay: Option<i32>,
}

impl ProjectRenderDefaults {
    /// Returns a new [`ProjectRenderDefaults`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            add_to_proj: Some(0),      // projrenderaddtoproj=0
            dither: Some(0),           // projrenderdither=0
            nch: Some(0),              // projrendernch=0
            srate: Some(2),            // projrendersrate=2
            stems: Some(0),            // projrenderstems=0
            resample: Some(3),         // projrenderresample=3
            rate_internal: Some(true), // projrenderrateinternal=1
            limit: Some(0.0),          // projrenderlimit=0
            queue_delay: Some(0),      // projrenderqdelay=0
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            add_to_proj: get_i32(ini, S, "projrenderaddtoproj"),
            dither: get_i32(ini, S, "projrenderdither"),
            nch: get_i32(ini, S, "projrendernch"),
            srate: get_i32(ini, S, "projrendersrate"),
            stems: get_i32(ini, S, "projrenderstems"),
            resample: get_i32(ini, S, "projrenderresample"),
            rate_internal: get_bool_from_i32(ini, S, "projrenderrateinternal"),
            limit: get_f64(ini, S, "projrenderlimit"),
            queue_delay: get_i32(ini, S, "projrenderqdelay"),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_i32(ini, S, "projrenderaddtoproj", self.add_to_proj);
        set_opt_i32(ini, S, "projrenderdither", self.dither);
        set_opt_i32(ini, S, "projrendernch", self.nch);
        set_opt_i32(ini, S, "projrendersrate", self.srate);
        set_opt_i32(ini, S, "projrenderstems", self.stems);
        set_opt_i32(ini, S, "projrenderresample", self.resample);
        set_opt_bool_as_i32(ini, S, "projrenderrateinternal", self.rate_internal);
        set_opt_f64(ini, S, "projrenderlimit", self.limit);
        set_opt_i32(ini, S, "projrenderqdelay", self.queue_delay);
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Top-level aggregate
// ────────────────────────────────────────────────────────────────────────────

/// All default project settings stored in `reaper.ini` `[REAPER]` section.
///
/// These map to the values REAPER writes when the user chooses
/// **"Save as Default Project Settings"** in the Project Settings dialog.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct ProjectDefaults {
    /// Tempo and time-signature defaults.
    pub tempo: ProjectTempoDefaults,

    /// Time-display defaults.
    pub time_display: ProjectTimeDisplayDefaults,

    /// Grid and snap defaults.
    pub grid: ProjectGridDefaults,

    /// Project behaviour defaults (ripple, groups, paths, …).
    pub behavior: ProjectBehaviorDefaults,

    /// Video defaults.
    pub video: ProjectVideoDefaults,

    /// Master-track defaults.
    pub master_track: ProjectMasterTrackDefaults,

    /// Metronome defaults.
    pub metronome: ProjectMetronomeDefaults,

    /// SMPTE / timecode sync defaults.
    pub smpte: ProjectSmpteDefaults,

    /// Render defaults.
    pub render: ProjectRenderDefaults,
}

impl ProjectDefaults {
    /// Returns a new [`ProjectDefaults`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            tempo: ProjectTempoDefaults::factory_defaults(),
            time_display: ProjectTimeDisplayDefaults::factory_defaults(),
            grid: ProjectGridDefaults::factory_defaults(),
            behavior: ProjectBehaviorDefaults::factory_defaults(),
            video: ProjectVideoDefaults::factory_defaults(),
            master_track: ProjectMasterTrackDefaults::factory_defaults(),
            metronome: ProjectMetronomeDefaults::factory_defaults(),
            smpte: ProjectSmpteDefaults::factory_defaults(),
            render: ProjectRenderDefaults::factory_defaults(),
        }
    }

    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            tempo: ProjectTempoDefaults::from_ini(ini),
            time_display: ProjectTimeDisplayDefaults::from_ini(ini),
            grid: ProjectGridDefaults::from_ini(ini),
            behavior: ProjectBehaviorDefaults::from_ini(ini),
            video: ProjectVideoDefaults::from_ini(ini),
            master_track: ProjectMasterTrackDefaults::from_ini(ini),
            metronome: ProjectMetronomeDefaults::from_ini(ini),
            smpte: ProjectSmpteDefaults::from_ini(ini),
            render: ProjectRenderDefaults::from_ini(ini),
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        self.tempo.write_to(ini);
        self.time_display.write_to(ini);
        self.grid.write_to(ini);
        self.behavior.write_to(ini);
        self.video.write_to(ini);
        self.master_track.write_to(ini);
        self.metronome.write_to(ini);
        self.smpte.write_to(ini);
        self.render.write_to(ini);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ini::IniFile;

    fn ini_from(text: &str) -> IniFile {
        IniFile::parse(text)
    }

    // ── Tempo / time-signature ────────────────────────────────────────────

    #[test]
    fn parse_bpm() {
        let ini = ini_from("[REAPER]\nprojbpm=140.5\n");
        let d = ProjectTempoDefaults::from_ini(&ini);
        assert_eq!(d.bpm, Some(140.5));
    }

    #[test]
    fn parse_sample_rate() {
        let ini = ini_from("[REAPER]\nprojsrate=48000\nprojsrateuse=1\n");
        let d = ProjectTempoDefaults::from_ini(&ini);
        assert_eq!(d.sample_rate, Some(48000));
        assert_eq!(d.sample_rate_use, Some(true));
    }

    #[test]
    fn parse_master_nch() {
        let ini = ini_from("[REAPER]\nprojmasternch=2\n");
        let d = ProjectTempoDefaults::from_ini(&ini);
        assert_eq!(d.master_nch, Some(2));
    }

    #[test]
    fn parse_time_sig() {
        let ini =
            ini_from("[REAPER]\nprojmeaslen=4\nprojmeasoffs=-1\nprojtsdenom=4\nprojbeatbase=0\n");
        let d = ProjectTempoDefaults::from_ini(&ini);
        assert_eq!(d.measure_len, Some(4));
        assert_eq!(d.measure_offs, Some(-1));
        assert_eq!(d.ts_denom, Some(4));
        assert_eq!(d.beat_base, Some(0));
    }

    // ── Time display ──────────────────────────────────────────────────────

    #[test]
    fn parse_time_display() {
        use crate::types::common::TimeDisplayFormat;
        let ini = ini_from("[REAPER]\nprojtimemode=5\nprojtimemode2=0\nprojtimeoffs=3.25\n");
        let d = ProjectTimeDisplayDefaults::from_ini(&ini);
        assert_eq!(d.time_mode, Some(TimeDisplayFormat::AbsoluteFrames));
        assert_eq!(d.time_mode2, Some(TimeDisplayFormat::MinsSecs));
        assert!((d.time_offs.unwrap() - 3.25).abs() < 1e-9);
    }

    #[test]
    fn parse_time_display_unknown() {
        use crate::types::common::TimeDisplayFormat;
        let ini = ini_from("[REAPER]\nprojtimemode=99\n");
        let d = ProjectTimeDisplayDefaults::from_ini(&ini);
        assert_eq!(d.time_mode, Some(TimeDisplayFormat::Unknown(99)));
    }

    #[test]
    fn round_trip_time_display() {
        use crate::types::common::TimeDisplayFormat;
        let ini = ini_from("[REAPER]\nprojtimemode=1\nprojtimemode2=4\n");
        let d = ProjectTimeDisplayDefaults::from_ini(&ini);
        assert_eq!(d.time_mode, Some(TimeDisplayFormat::MeasuresBeats));
        assert_eq!(d.time_mode2, Some(TimeDisplayFormat::HmsFrames));

        let mut out = IniFile::parse("[REAPER]\n");
        d.write_to(&mut out);
        assert_eq!(out.get("REAPER", "projtimemode"), Some("1"));
        assert_eq!(out.get("REAPER", "projtimemode2"), Some("4"));
    }

    // ── Grid ──────────────────────────────────────────────────────────────

    #[test]
    fn parse_grid() {
        let ini = ini_from(
            "[REAPER]\nprojgriddiv=1.0\nprojgriddivsnap=1.0\nprojgridmin=8\nprojgridsnapmin=8\nprojgridframe=0\nprojgridswing=0.5\nprojshowgrid=3198\n",
        );
        let d = ProjectGridDefaults::from_ini(&ini);
        assert_eq!(d.grid_div, Some(1.0));
        assert_eq!(d.grid_div_snap, Some(1.0));
        assert_eq!(d.grid_min, Some(8));
        assert_eq!(d.grid_snap_min, Some(8));
        assert_eq!(d.grid_frame, Some(0));
        assert!((d.grid_swing.unwrap() - 0.5).abs() < 1e-9);
        assert_eq!(d.show_grid, Some(3198));
    }

    // ── Project behaviour ─────────────────────────────────────────────────

    #[test]
    fn parse_behavior() {
        use crate::types::common::{RippleMode, TrackMixBitDepth};
        let ini = ini_from(
            "[REAPER]\nprojripedit=0\nprojgroupover=0\nprojgroupsel=0\nprojsellock=1\nprojtakelane=1\nprojrelpath=1\nprojintmix=0\nprojrecmode=1\nprojrelsnap=0\n",
        );
        let d = ProjectBehaviorDefaults::from_ini(&ini);
        assert_eq!(d.rip_edit, Some(RippleMode::Off));
        assert_eq!(d.group_over, Some(0));
        assert_eq!(d.group_sel, Some(false));
        assert_eq!(d.sel_lock, Some(1));
        assert_eq!(d.take_lane, Some(1));
        assert_eq!(d.rel_path, Some(true));
        assert_eq!(d.int_mix, Some(TrackMixBitDepth::Float64));
        assert_eq!(d.rec_mode, Some(1));
        assert_eq!(d.rel_snap, Some(0));
    }

    #[test]
    fn parse_rip_edit_all_values() {
        use crate::types::common::RippleMode;
        for (raw, expected) in [
            (0, RippleMode::Off),
            (1, RippleMode::PerTrack),
            (2, RippleMode::AllTracks),
        ] {
            let ini = ini_from(&format!("[REAPER]\nprojripedit={raw}\n"));
            let d = ProjectBehaviorDefaults::from_ini(&ini);
            assert_eq!(d.rip_edit, Some(expected));
        }
    }

    #[test]
    fn round_trip_rip_edit() {
        use crate::types::common::RippleMode;
        let ini = ini_from("[REAPER]\nprojripedit=2\n");
        let d = ProjectBehaviorDefaults::from_ini(&ini);
        assert_eq!(d.rip_edit, Some(RippleMode::AllTracks));

        let mut out = IniFile::parse("[REAPER]\n");
        d.write_to(&mut out);
        assert_eq!(out.get("REAPER", "projripedit"), Some("2"));
    }

    #[test]
    fn parse_def_rec_path() {
        let ini =
            ini_from("[REAPER]\nprojdefrecpath=/home/user/audio\nprojdefrecpath2=/tmp/overflow\n");
        let d = ProjectBehaviorDefaults::from_ini(&ini);
        assert_eq!(d.def_rec_path.as_deref(), Some("/home/user/audio"));
        assert_eq!(d.def_rec_path2.as_deref(), Some("/tmp/overflow"));
    }

    #[test]
    fn parse_max_len() {
        let ini = ini_from("[REAPER]\nprojmaxlen=600.0\nprojmaxlenuse=1\n");
        let d = ProjectBehaviorDefaults::from_ini(&ini);
        assert!((d.max_len.unwrap() - 600.0).abs() < 1e-9);
        assert_eq!(d.max_len_use, Some(true));
    }

    // ── Video ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_video() {
        let ini = ini_from(
            "[REAPER]\nprojvidflags=0\nprojvidh=1080\nprojvidw=1920\nprojfrbase=25.0\nprojfrdrop=0\nprojfridx=8\n",
        );
        let d = ProjectVideoDefaults::from_ini(&ini);
        assert_eq!(d.vid_flags, Some(0));
        assert_eq!(d.vid_h, Some(1080));
        assert_eq!(d.vid_w, Some(1920));
        assert!((d.fr_base.unwrap() - 25.0).abs() < 1e-9);
        assert_eq!(d.fr_drop, Some(false));
        assert_eq!(d.fr_idx, Some(8));
    }

    #[test]
    fn parse_video_drop_frame_true() {
        let ini = ini_from("[REAPER]\nprojfrdrop=1\n");
        let d = ProjectVideoDefaults::from_ini(&ini);
        assert_eq!(d.fr_drop, Some(true));
    }

    #[test]
    fn round_trip_fr_drop() {
        let ini = ini_from("[REAPER]\nprojfrdrop=1\n");
        let d = ProjectVideoDefaults::from_ini(&ini);
        assert_eq!(d.fr_drop, Some(true));

        let mut out = IniFile::parse("[REAPER]\n");
        d.write_to(&mut out);
        assert_eq!(out.get("REAPER", "projfrdrop"), Some("1"));
    }

    // ── Master track ──────────────────────────────────────────────────────

    #[test]
    fn parse_master_track() {
        use crate::types::common::PanMode;
        let ini = ini_from(
            "[REAPER]\nprojmasterpanlaw=-3.0\nprojmasterpanlawflags=0\nprojmasterpanmode=0\nprojmastervuflags=2\nprojpeaksgain=1.1\n",
        );
        let d = ProjectMasterTrackDefaults::from_ini(&ini);
        assert!((d.pan_law.unwrap() - (-3.0)).abs() < 1e-9);
        assert_eq!(d.pan_law_flags, Some(PanLawTaper::SineTaper));
        assert_eq!(d.pan_mode, Some(PanMode::Classic));
        assert_eq!(d.vu_flags, Some(2));
        assert!((d.peaks_gain.unwrap() - 1.1).abs() < 1e-9);
    }

    #[test]
    fn parse_master_pan_mode_all_values() {
        use crate::types::common::PanMode;
        for (raw, expected) in [
            (0, PanMode::Classic),
            (1, PanMode::Balanced),
            (3, PanMode::StereoPan),
            (5, PanMode::DualPan),
            (6, PanMode::MidiPan),
        ] {
            let ini = ini_from(&format!("[REAPER]\nprojmasterpanmode={raw}\n"));
            let d = ProjectMasterTrackDefaults::from_ini(&ini);
            assert_eq!(d.pan_mode, Some(expected));
        }
    }

    #[test]
    fn round_trip_master_pan_mode() {
        use crate::types::common::PanMode;
        let ini = ini_from("[REAPER]\nprojmasterpanmode=5\n");
        let d = ProjectMasterTrackDefaults::from_ini(&ini);
        assert_eq!(d.pan_mode, Some(PanMode::DualPan));

        let mut out = IniFile::parse("[REAPER]\n");
        d.write_to(&mut out);
        assert_eq!(out.get("REAPER", "projmasterpanmode"), Some("5"));
    }

    // ── Metronome ─────────────────────────────────────────────────────────

    #[test]
    fn parse_metronome() {
        let ini = ini_from(
            "[REAPER]\nprojmetroen=6\nprojmetrocountin=2.0\nprojmetrobeatlen=4\nprojmetrof1=800\nprojmetrof2=1600\nprojmetrov1=0.25\nprojmetrov2=0.125\n",
        );
        let d = ProjectMetronomeDefaults::from_ini(&ini);
        assert_eq!(
            d.enabled,
            Some(MetronomeEnableFlags {
                during_playback: false,
                during_recording: true,
                count_in: true
            })
        );
        assert!((d.count_in.unwrap() - 2.0).abs() < 1e-9);
        assert_eq!(d.beat_len, Some(4));
        assert_eq!(d.freq1, Some(800));
        assert_eq!(d.freq2, Some(1600));
        assert!((d.vol1.unwrap() - 0.25).abs() < 1e-9);
        assert!((d.vol2.unwrap() - 0.125).abs() < 1e-9);
    }

    #[test]
    fn parse_metronome_sample_files() {
        let ini =
            ini_from("[REAPER]\nprojmetrofn1=/sounds/kick.wav\nprojmetrofn2=/sounds/hat.wav\n");
        let d = ProjectMetronomeDefaults::from_ini(&ini);
        assert_eq!(d.fn1.as_deref(), Some("/sounds/kick.wav"));
        assert_eq!(d.fn2.as_deref(), Some("/sounds/hat.wav"));
    }

    // ── SMPTE ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_smpte() {
        let ini = ini_from(
            "[REAPER]\nprojsmpteahead=1000\nprojsmpteinput=0\nprojsmptemaxfree=300\nprojsmpteoffs=0.0\nprojsmpterate=25.0\nprojsmpterateuseproj=1\nprojsmpteresync=100\nprojsmpteskip=40\nprojsmptesync=0\n",
        );
        let d = ProjectSmpteDefaults::from_ini(&ini);
        assert_eq!(d.ahead, Some(1000));
        assert_eq!(d.input, Some(0));
        assert_eq!(d.max_free, Some(300));
        assert!((d.offs.unwrap() - 0.0).abs() < 1e-9);
        assert!((d.rate.unwrap() - 25.0).abs() < 1e-9);
        assert_eq!(d.rate_use_proj, Some(true));
        assert_eq!(d.resync, Some(100));
        assert_eq!(d.skip, Some(40));
        assert_eq!(d.sync, Some(0));
    }

    // ── Render ────────────────────────────────────────────────────────────

    #[test]
    fn parse_render_defaults() {
        let ini = ini_from(
            "[REAPER]\nprojrenderaddtoproj=0\nprojrenderdither=0\nprojrendernch=0\nprojrendersrate=2\nprojrenderstems=0\nprojrenderresample=7\nprojrenderrateinternal=1\nprojrenderlimit=0.0\nprojrenderqdelay=0\n",
        );
        let d = ProjectRenderDefaults::from_ini(&ini);
        assert_eq!(d.add_to_proj, Some(0));
        assert_eq!(d.dither, Some(0));
        assert_eq!(d.nch, Some(0));
        assert_eq!(d.srate, Some(2));
        assert_eq!(d.stems, Some(0));
        assert_eq!(d.resample, Some(7));
        assert_eq!(d.rate_internal, Some(true));
        assert!((d.limit.unwrap() - 0.0).abs() < 1e-9);
        assert_eq!(d.queue_delay, Some(0));
    }

    // ── Aggregate ─────────────────────────────────────────────────────────

    #[test]
    fn parse_project_defaults_aggregate() {
        let ini = ini_from(
            "[REAPER]\nprojbpm=120.0\nprojsrate=44100\nprojmetroen=6\nprojrenderresample=7\nprojvidw=1920\nprojvidh=1080\n",
        );
        let d = ProjectDefaults::from_ini(&ini);
        assert_eq!(d.tempo.bpm, Some(120.0));
        assert_eq!(d.tempo.sample_rate, Some(44100));
        assert_eq!(
            d.metronome.enabled,
            Some(MetronomeEnableFlags {
                during_playback: false,
                during_recording: true,
                count_in: true
            })
        );
        assert_eq!(d.render.resample, Some(7));
        assert_eq!(d.video.vid_w, Some(1920));
        assert_eq!(d.video.vid_h, Some(1080));
    }

    // ── Round-trip ────────────────────────────────────────────────────────

    #[test]
    fn round_trip_project_defaults() {
        let input = "[REAPER]\nprojbpm=128.0\nprojsrate=48000\nprojsrateuse=1\nprojmasternch=2\n";
        let ini = ini_from(input);
        let d = ProjectDefaults::from_ini(&ini);

        let mut out_ini = IniFile::parse("[REAPER]\n");
        d.write_to(&mut out_ini);

        assert_eq!(out_ini.get("REAPER", "projbpm"), Some("128"));
        assert_eq!(out_ini.get("REAPER", "projsrate"), Some("48000"));
        assert_eq!(out_ini.get("REAPER", "projsrateuse"), Some("1"));
        assert_eq!(out_ini.get("REAPER", "projmasternch"), Some("2"));
    }

    #[test]
    fn absent_keys_produce_none() {
        let ini = ini_from("[REAPER]\n");
        let d = ProjectDefaults::from_ini(&ini);
        assert!(d.tempo.bpm.is_none());
        assert!(d.tempo.sample_rate.is_none());
        assert!(d.grid.grid_div.is_none());
        assert!(d.metronome.enabled.is_none());
        assert!(d.render.resample.is_none());
    }
}
