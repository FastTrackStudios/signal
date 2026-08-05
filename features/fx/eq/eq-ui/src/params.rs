//! nice_plug parameter definitions and shared UI state.
//!
//! Lives in `eq-ui` (not `eq-plugin`) so the [`crate::control_view`] component
//! can render against the param tree without forcing a circular dep.

use atomic_float::AtomicF32;
use audiocore_core::prelude::*;
use parking_lot::Mutex;
use spectrum_analyzer::dsp::{Analyzer, AudioFeed};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Process-global source of unique analyzer instance ids (used for cross-instance
/// spectrum sharing).
static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

/// Number of parametric bands (Pro-Q style).
pub const NUM_BANDS: usize = 24;

/// Number of logarithmically-spaced spectrum bins fed to the UI.
pub const SPECTRUM_BINS: usize = 256;

/// Audio-thread → UI metering data.
pub struct EqUiState {
    pub params: Arc<FtsEqParams>,
    pub input_peak_db: AtomicF32,
    pub output_peak_db: AtomicF32,
    pub sample_rate: AtomicF32,
    pub spectrum_bins: Box<[AtomicF32; SPECTRUM_BINS]>,
    pub model_response_bins: Box<[AtomicF32; SPECTRUM_BINS]>,
    /// Full Pro-Q 4-style analyzer engine. The UI ticks it and reads snapshots;
    /// the audio thread feeds it through [`Self::take_audio_feed`].
    pub analyzer: Arc<Analyzer>,
    /// Audio-thread feed for the analyzer. The processor takes this out once on
    /// the first `process` call (or `initialize`); `None` afterwards.
    pub audio_feed: Mutex<Option<AudioFeed>>,
}

impl EqUiState {
    pub fn new(params: Arc<FtsEqParams>) -> Self {
        let id = NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed);
        let (analyzer, feed) = Analyzer::new(id, 48000.0);
        Self {
            params,
            input_peak_db: AtomicF32::new(-100.0),
            output_peak_db: AtomicF32::new(-100.0),
            sample_rate: AtomicF32::new(48000.0),
            spectrum_bins: Box::new(std::array::from_fn(|_| AtomicF32::new(-100.0))),
            model_response_bins: Box::new(std::array::from_fn(|_| AtomicF32::new(0.0))),
            analyzer,
            audio_feed: Mutex::new(Some(feed)),
        }
    }

    /// Take the analyzer's audio feed (the realtime processor's handle). Returns
    /// `None` if already taken.
    pub fn take_audio_feed(&self) -> Option<AudioFeed> {
        self.audio_feed.lock().take()
    }
}

#[derive(Params)]
pub struct BandParams {
    #[id = "on"]
    pub enabled: FloatParam,

    #[id = "type"]
    pub filter_type: IntParam,

    #[id = "freq"]
    pub freq_hz: FloatParam,

    #[id = "gain"]
    pub gain_db: FloatParam,

    #[id = "q"]
    pub q: FloatParam,

    /// Filter slope: 0=6dB/oct, 1=12, 2=18, 3=24, 4=30, 5=36, 6=42, 7=48, 8=72, 9=96, 10=Brickwall.
    #[id = "slope"]
    pub slope: IntParam,

    /// Solo this band (mutes all other bands when active).
    #[id = "solo"]
    pub solo: FloatParam,

    /// User-assigned short name for this band (e.g. "Honk", "Air"). Persisted
    /// (not automatable) — serialized per band as `name_1`..`name_24`.
    #[persist = "name"]
    pub name: parking_lot::RwLock<String>,

    /// User notes annotating why this band exists / what it's doing. Persisted
    /// per band as `notes_1`..`notes_24`.
    #[persist = "notes"]
    pub notes: parking_lot::RwLock<String>,
}

impl BandParams {
    pub fn new(idx: usize) -> Self {
        // Gregory Scott baseline, default on every instance: band 0 is a low
        // shelf cornered ~400 Hz and band 1 a high shelf ~2.5 kHz — both enabled
        // and flat (0 dB) so they sit ready to pull "as needed", pre-named so the
        // intent is obvious. All other bands start off/empty (Bell @ 1k).
        // Instrument profiles reposition these at runtime per track name.
        let (default_enabled, default_type, default_freq, default_name): (f32, i32, f32, &str) =
            match idx {
                0 => (1.0, 1, 400.0, "Low Shelf"),   // 1 = Low Shelf
                1 => (1.0, 3, 2500.0, "High Shelf"), // 3 = High Shelf
                _ => (0.0, 0, 1000.0, ""),           // 0 = Bell, off
            };
        Self {
            enabled: FloatParam::new(
                format!("B{} On", idx + 1),
                default_enabled,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                if v > 0.5 {
                    "On".to_string()
                } else {
                    "Off".to_string()
                }
            }))
            .with_string_to_value(Arc::new(|s| {
                match s.trim().to_lowercase().as_str() {
                    "on" | "1" | "true" => Some(1.0),
                    "off" | "0" | "false" => Some(0.0),
                    _ => s.parse().ok(),
                }
            })),

            filter_type: IntParam::new(
                format!("B{} Type", idx + 1),
                default_type,
                IntRange::Linear { min: 0, max: 9 },
            )
            .with_value_to_string(Arc::new(|v| match v {
                0 => "Bell".to_string(),
                1 => "Low Shelf".to_string(),
                2 => "Low Cut".to_string(),
                3 => "High Shelf".to_string(),
                4 => "High Cut".to_string(),
                5 => "Notch".to_string(),
                6 => "Bandpass".to_string(),
                7 => "Tilt Shelf".to_string(),
                8 => "Flat Tilt".to_string(),
                9 => "All Pass".to_string(),
                _ => "Bell".to_string(),
            })),

            freq_hz: FloatParam::new(
                format!("B{} Freq", idx + 1),
                default_freq,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 30000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(Arc::new(|v| {
                if v >= 1000.0 {
                    format!("{:.1}k", v / 1000.0)
                } else {
                    format!("{:.0}", v)
                }
            })),

            gain_db: FloatParam::new(
                format!("B{} Gain", idx + 1),
                0.0,
                FloatRange::Linear {
                    min: -30.0,
                    max: 30.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            q: FloatParam::new(
                format!("B{} Q", idx + 1),
                1.0,
                FloatRange::Skewed {
                    min: 0.1,
                    max: 18.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            slope: IntParam::new(
                format!("B{} Slope", idx + 1),
                2,
                IntRange::Linear { min: 0, max: 10 },
            )
            .with_value_to_string(Arc::new(|v| match v {
                0 => "0 dB/oct".to_string(),
                1 => "6 dB/oct".to_string(),
                2 => "12 dB/oct".to_string(),
                3 => "18 dB/oct".to_string(),
                4 => "24 dB/oct".to_string(),
                5 => "30 dB/oct".to_string(),
                6 => "36 dB/oct".to_string(),
                7 => "48 dB/oct".to_string(),
                8 => "72 dB/oct".to_string(),
                9 => "96 dB/oct".to_string(),
                10 => "Brickwall".to_string(),
                _ => format!("{}", v),
            })),

            solo: FloatParam::new(
                format!("B{} Solo", idx + 1),
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                if v > 0.5 {
                    "On".to_string()
                } else {
                    "Off".to_string()
                }
            }))
            .with_string_to_value(Arc::new(|s| {
                match s.trim().to_lowercase().as_str() {
                    "on" | "1" | "true" => Some(1.0),
                    "off" | "0" | "false" => Some(0.0),
                    _ => s.parse().ok(),
                }
            })),

            name: parking_lot::RwLock::new(default_name.to_string()),
            notes: parking_lot::RwLock::new(String::new()),
        }
    }
}

#[derive(Params)]
pub struct FtsEqParams {
    /// Processing model:
    /// 0=Default parametric, 1=Pultec, 2=Neve 1073, 3=API, 4=SSL E, 5=SSL G.
    ///
    /// An *index*, because a host parameter has to be a number, and the
    /// automatable model switch. Not how the choice is persisted — see
    /// [`FtsEqParams::model_id`].
    #[id = "model"]
    pub model: IntParam,

    /// The selected model's id, persisted alongside the index.
    ///
    /// A parameter's stored value is normalized, so an index-valued parameter
    /// is only stable while the list length is: add a seventh model and a
    /// session that saved the sixth reloads pointing at something else. Names
    /// do not have that problem, so the id is what a session restores from and
    /// the index is reconciled to it on load. Empty means "saved before ids
    /// existed", and then the index stands.
    #[persist = "model_id"]
    pub model_id: parking_lot::RwLock<String>,

    #[id = "output_gain"]
    pub output_gain_db: FloatParam,

    /// Display dB range for the EQ graph (0=6dB, 1=12dB, 2=18dB, 3=24dB, 4=30dB).
    #[id = "db_range"]
    pub db_range: IntParam,

    /// Global gain scale (0-200%). Multiplies all band gains proportionally.
    #[id = "gain_scale"]
    pub gain_scale: FloatParam,

    /// Hidden tuning param for coefficient optimization (set via CLAP API).
    #[id = "tune_peak_q_comp"]
    pub tune_peak_q_comp: FloatParam,

    #[id = "neve_eq_in"]
    pub neve_eq_in: FloatParam,

    #[id = "neve_phase"]
    pub neve_phase: FloatParam,

    #[id = "neve_trim"]
    pub neve_trim_db: FloatParam,

    #[id = "neve_drive"]
    pub neve_drive: FloatParam,

    #[id = "neve_hpf"]
    pub neve_hpf: IntParam,

    #[id = "neve_low_freq"]
    pub neve_low_freq: IntParam,

    #[id = "neve_low_gain"]
    pub neve_low_gain_db: FloatParam,

    #[id = "neve_mid_freq"]
    pub neve_mid_freq: IntParam,

    #[id = "neve_mid_gain"]
    pub neve_mid_gain_db: FloatParam,

    #[id = "neve_high_gain"]
    pub neve_high_gain_db: FloatParam,

    #[id = "pultec_eq_in"]
    pub pultec_eq_in: FloatParam,
    #[id = "pultec_low_freq"]
    pub pultec_low_freq: IntParam,
    #[id = "pultec_low_boost"]
    pub pultec_low_boost_db: FloatParam,
    #[id = "pultec_low_atten"]
    pub pultec_low_atten_db: FloatParam,
    #[id = "pultec_high_boost_freq"]
    pub pultec_high_boost_freq: IntParam,
    #[id = "pultec_high_boost"]
    pub pultec_high_boost_db: FloatParam,
    #[id = "pultec_bandwidth"]
    pub pultec_bandwidth: FloatParam,
    #[id = "pultec_high_atten_freq"]
    pub pultec_high_atten_freq: IntParam,
    #[id = "pultec_high_atten"]
    pub pultec_high_atten_db: FloatParam,
    #[id = "pultec_drive"]
    pub pultec_drive: FloatParam,
    #[id = "pultec_trim"]
    pub pultec_trim_db: FloatParam,

    #[id = "api_eq_in"]
    pub api_eq_in: FloatParam,
    #[id = "api_low_freq"]
    pub api_low_freq: IntParam,
    #[id = "api_low_gain"]
    pub api_low_gain_db: FloatParam,
    #[id = "api_mid_freq"]
    pub api_mid_freq: IntParam,
    #[id = "api_mid_gain"]
    pub api_mid_gain_db: FloatParam,
    #[id = "api_high_freq"]
    pub api_high_freq: IntParam,
    #[id = "api_high_gain"]
    pub api_high_gain_db: FloatParam,
    #[id = "api_drive"]
    pub api_drive: FloatParam,
    #[id = "api_trim"]
    pub api_trim_db: FloatParam,

    #[id = "ssl_eq_in"]
    pub ssl_eq_in: FloatParam,
    #[id = "ssl_hpf"]
    pub ssl_hpf_hz: FloatParam,
    #[id = "ssl_lpf"]
    pub ssl_lpf_hz: FloatParam,
    #[id = "ssl_lf_freq"]
    pub ssl_lf_freq_hz: FloatParam,
    #[id = "ssl_lf_gain"]
    pub ssl_lf_gain_db: FloatParam,
    #[id = "ssl_lmf_freq"]
    pub ssl_lmf_freq_hz: FloatParam,
    #[id = "ssl_lmf_gain"]
    pub ssl_lmf_gain_db: FloatParam,
    #[id = "ssl_hmf_freq"]
    pub ssl_hmf_freq_hz: FloatParam,
    #[id = "ssl_hmf_gain"]
    pub ssl_hmf_gain_db: FloatParam,
    #[id = "ssl_hf_freq"]
    pub ssl_hf_freq_hz: FloatParam,
    #[id = "ssl_hf_gain"]
    pub ssl_hf_gain_db: FloatParam,
    #[id = "ssl_drive"]
    pub ssl_drive: FloatParam,
    #[id = "ssl_trim"]
    pub ssl_trim_db: FloatParam,

    #[nested(array, group = "Band {}")]
    pub bands: [BandParams; NUM_BANDS],
}

impl Default for FtsEqParams {
    fn default() -> Self {
        Self {
            model_id: parking_lot::RwLock::new(String::new()),

            model: IntParam::new("Model", 0, IntRange::Linear { min: 0, max: 5 })
                .with_value_to_string(Arc::new(|v| match v {
                    1 => "Pultec EQP-1A".to_string(),
                    2 => "Neve 1073".to_string(),
                    3 => "API 550A".to_string(),
                    4 => "SSL E".to_string(),
                    5 => "SSL G".to_string(),
                    _ => "Default".to_string(),
                })),

            output_gain_db: FloatParam::new(
                "Output",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            // 3 dB (index 0) is the default — a tight range suits the subtle,
            // shelf-first baseline moves. Ascending: 3/6/12/18/24/30.
            db_range: IntParam::new("dB Range", 0, IntRange::Linear { min: 0, max: 5 })
                .with_value_to_string(Arc::new(|v| match v {
                    0 => "3 dB".to_string(),
                    1 => "6 dB".to_string(),
                    2 => "12 dB".to_string(),
                    3 => "18 dB".to_string(),
                    4 => "24 dB".to_string(),
                    5 => "30 dB".to_string(),
                    _ => format!("{v}"),
                })),

            gain_scale: FloatParam::new(
                "Scale",
                100.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 200.0,
                },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            tune_peak_q_comp: FloatParam::new(
                "Tune: Peak Q Comp",
                0.105,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(4)),

            neve_eq_in: FloatParam::new(
                "Neve EQ In",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                if v > 0.5 {
                    "In".to_string()
                } else {
                    "Out".to_string()
                }
            })),

            neve_phase: FloatParam::new(
                "Neve Phase",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                if v > 0.5 {
                    "Invert".to_string()
                } else {
                    "Normal".to_string()
                }
            })),

            neve_trim_db: FloatParam::new(
                "Neve Trim",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            neve_drive: FloatParam::new(
                "Neve Drive",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 100.0,
                },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            neve_hpf: IntParam::new("Neve HPF", 0, IntRange::Linear { min: 0, max: 4 })
                .with_value_to_string(Arc::new(|v| match v {
                    1 => "50 Hz".to_string(),
                    2 => "80 Hz".to_string(),
                    3 => "160 Hz".to_string(),
                    4 => "300 Hz".to_string(),
                    _ => "Off".to_string(),
                })),

            neve_low_freq: IntParam::new("Neve Low Freq", 0, IntRange::Linear { min: 0, max: 4 })
                .with_value_to_string(Arc::new(|v| match v {
                    1 => "35 Hz".to_string(),
                    2 => "60 Hz".to_string(),
                    3 => "110 Hz".to_string(),
                    4 => "220 Hz".to_string(),
                    _ => "Off".to_string(),
                })),

            neve_low_gain_db: FloatParam::new(
                "Neve Low Gain",
                0.0,
                FloatRange::Linear {
                    min: -16.0,
                    max: 16.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            neve_mid_freq: IntParam::new("Neve Mid Freq", 0, IntRange::Linear { min: 0, max: 6 })
                .with_value_to_string(Arc::new(|v| match v {
                    1 => "360 Hz".to_string(),
                    2 => "700 Hz".to_string(),
                    3 => "1.6 kHz".to_string(),
                    4 => "3.2 kHz".to_string(),
                    5 => "4.8 kHz".to_string(),
                    6 => "7.2 kHz".to_string(),
                    _ => "Off".to_string(),
                })),

            neve_mid_gain_db: FloatParam::new(
                "Neve Mid Gain",
                0.0,
                FloatRange::Linear {
                    min: -18.0,
                    max: 18.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            neve_high_gain_db: FloatParam::new(
                "Neve High Gain",
                0.0,
                FloatRange::Linear {
                    min: -16.0,
                    max: 16.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            pultec_eq_in: FloatParam::new(
                "Pultec EQ In",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(on_off_string("In", "Out")),
            pultec_low_freq: IntParam::new(
                "Pultec Low Freq",
                1,
                IntRange::Linear { min: 0, max: 3 },
            )
            .with_value_to_string(index_string(&["20 Hz", "30 Hz", "60 Hz", "100 Hz"])),
            pultec_low_boost_db: db_param("Pultec Low Boost", 0.0, 0.0, 13.0),
            pultec_low_atten_db: db_param("Pultec Low Atten", 0.0, 0.0, 13.0),
            pultec_high_boost_freq: IntParam::new(
                "Pultec High Boost Freq",
                3,
                IntRange::Linear { min: 0, max: 6 },
            )
            .with_value_to_string(index_string(&[
                "3 kHz", "4 kHz", "5 kHz", "8 kHz", "10 kHz", "12 kHz", "16 kHz",
            ])),
            pultec_high_boost_db: db_param("Pultec High Boost", 0.0, 0.0, 16.0),
            pultec_bandwidth: FloatParam::new(
                "Pultec Bandwidth",
                5.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 10.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            pultec_high_atten_freq: IntParam::new(
                "Pultec High Atten Freq",
                1,
                IntRange::Linear { min: 0, max: 2 },
            )
            .with_value_to_string(index_string(&["5 kHz", "10 kHz", "20 kHz"])),
            pultec_high_atten_db: db_param("Pultec High Atten", 0.0, 0.0, 16.0),
            pultec_drive: percent_param("Pultec Drive"),
            pultec_trim_db: db_param("Pultec Trim", 0.0, -24.0, 24.0),

            api_eq_in: FloatParam::new("API EQ In", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(on_off_string("In", "Out")),
            api_low_freq: IntParam::new("API Low Freq", 1, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(index_string(&["50 Hz", "100 Hz", "200 Hz", "400 Hz"])),
            api_low_gain_db: db_param("API Low Gain", 0.0, -12.0, 12.0),
            api_mid_freq: IntParam::new("API Mid Freq", 2, IntRange::Linear { min: 0, max: 4 })
                .with_value_to_string(index_string(&[
                    "400 Hz", "800 Hz", "1.5 kHz", "3 kHz", "5 kHz",
                ])),
            api_mid_gain_db: db_param("API Mid Gain", 0.0, -12.0, 12.0),
            api_high_freq: IntParam::new("API High Freq", 2, IntRange::Linear { min: 0, max: 4 })
                .with_value_to_string(index_string(&[
                    "5 kHz", "7 kHz", "10 kHz", "12.5 kHz", "15 kHz",
                ])),
            api_high_gain_db: db_param("API High Gain", 0.0, -12.0, 12.0),
            api_drive: percent_param("API Drive"),
            api_trim_db: db_param("API Trim", 0.0, -24.0, 24.0),

            ssl_eq_in: FloatParam::new("SSL EQ In", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(on_off_string("In", "Out")),
            ssl_hpf_hz: freq_param("SSL HPF", 20.0, 16.0, 350.0),
            ssl_lpf_hz: freq_param("SSL LPF", 22_000.0, 3000.0, 22_000.0),
            ssl_lf_freq_hz: freq_param("SSL LF Freq", 100.0, 30.0, 450.0),
            ssl_lf_gain_db: db_param("SSL LF Gain", 0.0, -15.0, 15.0),
            ssl_lmf_freq_hz: freq_param("SSL LMF Freq", 800.0, 200.0, 2500.0),
            ssl_lmf_gain_db: db_param("SSL LMF Gain", 0.0, -15.0, 15.0),
            ssl_hmf_freq_hz: freq_param("SSL HMF Freq", 3000.0, 600.0, 7000.0),
            ssl_hmf_gain_db: db_param("SSL HMF Gain", 0.0, -15.0, 15.0),
            ssl_hf_freq_hz: freq_param("SSL HF Freq", 8000.0, 1500.0, 16000.0),
            ssl_hf_gain_db: db_param("SSL HF Gain", 0.0, -15.0, 15.0),
            ssl_drive: percent_param("SSL Drive"),
            ssl_trim_db: db_param("SSL Trim", 0.0, -24.0, 24.0),

            bands: std::array::from_fn(BandParams::new),
        }
    }
}

fn db_param(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(" dB")
        .with_value_to_string(formatters::v2s_f32_rounded(1))
}

fn freq_param(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(" Hz")
        .with_value_to_string(formatters::v2s_f32_rounded(0))
}

fn percent_param(name: &'static str) -> FloatParam {
    FloatParam::new(
        name,
        0.0,
        FloatRange::Linear {
            min: 0.0,
            max: 100.0,
        },
    )
    .with_unit("%")
    .with_value_to_string(formatters::v2s_f32_rounded(0))
}

fn on_off_string(on: &'static str, off: &'static str) -> Arc<dyn Fn(f32) -> String + Send + Sync> {
    Arc::new(move |v| {
        if v > 0.5 {
            on.to_string()
        } else {
            off.to_string()
        }
    })
}

fn index_string(labels: &'static [&'static str]) -> Arc<dyn Fn(i32) -> String + Send + Sync> {
    Arc::new(move |v| {
        labels
            .get(v as usize)
            .copied()
            .unwrap_or("Unknown")
            .to_string()
    })
}
