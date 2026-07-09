//! FTS Compressor — nih-plug entry point with full DSP bridge and Dioxus GUI.

use atomic_float::AtomicF32;
use fts_plugin_core::prelude::*;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use comp_dsp::{chain::CompChain, MultiBandCompressor};
use effective_params::{profile_for_index, profile_name, EffectiveCompParams};
use audiocore_dsp::AudioConfig;

pub mod editor;
mod effective_params;

// ── Shared UI State ──────────────────────────────────────────────────

/// Audio-thread → UI metering data.
pub struct CompUiState {
    pub params: Arc<FtsCompParams>,
    /// Current gain reduction in dB (positive = reducing).
    pub gain_reduction_db: AtomicF32,
    /// Peak input level in dB.
    pub input_peak_db: AtomicF32,
    /// Peak output level in dB.
    pub output_peak_db: AtomicF32,
    /// Waveform history: input peaks (0.0–1.0 normalized), ring buffer.
    pub waveform_input: Box<[AtomicF32]>,
    /// Waveform history: GR (0.0–1.0 normalized), ring buffer.
    pub waveform_gr: Box<[AtomicF32]>,
    /// Integer write position into waveform ring buffers.
    pub waveform_pos: AtomicF32,
    /// Fractional scroll phase: counter / interval (0.0–1.0).
    /// The renderer uses this to smoothly interpolate x-positions between
    /// data updates, giving sub-pixel accurate scrolling at any refresh rate.
    pub waveform_phase: AtomicF32,
}

/// Number of waveform history entries.
/// At 240 Hz updates: 960 entries ≈ 4 seconds of history.
pub const WAVEFORM_LEN: usize = 960;

impl CompUiState {
    pub fn new(params: Arc<FtsCompParams>) -> Self {
        let waveform_input: Box<[AtomicF32]> = (0..WAVEFORM_LEN)
            .map(|_| AtomicF32::new(0.0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let waveform_gr: Box<[AtomicF32]> = (0..WAVEFORM_LEN)
            .map(|_| AtomicF32::new(0.0))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self {
            params,
            gain_reduction_db: AtomicF32::new(0.0),
            input_peak_db: AtomicF32::new(-100.0),
            output_peak_db: AtomicF32::new(-100.0),
            waveform_input,
            waveform_gr,
            waveform_pos: AtomicF32::new(0.0),
            waveform_phase: AtomicF32::new(0.0),
        }
    }
}

// ── Parameters ───────────────────────────────────────────────────────

#[derive(Params)]
pub struct FtsCompParams {
    #[id = "threshold"]
    pub threshold_db: FloatParam,

    #[id = "ratio"]
    pub ratio: FloatParam,

    #[id = "attack"]
    pub attack_ms: FloatParam,

    #[id = "release"]
    pub release_ms: FloatParam,

    #[id = "knee"]
    pub knee_db: FloatParam,

    #[id = "auto_makeup"]
    pub auto_makeup: FloatParam,

    #[id = "feedback"]
    pub feedback: FloatParam,

    #[id = "link"]
    pub channel_link: FloatParam,

    #[id = "detector_rms_mix"]
    pub detector_rms_mix: FloatParam,

    #[id = "inertia"]
    pub inertia: FloatParam,

    #[id = "inertia_decay"]
    pub inertia_decay: FloatParam,

    #[id = "ceiling"]
    pub ceiling: FloatParam,

    #[id = "drive"]
    pub drive: FloatParam,

    #[id = "character"]
    pub character_mode: IntParam,

    #[id = "mix"]
    pub fold: FloatParam,

    #[id = "multiband_amount"]
    pub multiband_amount: FloatParam,

    #[id = "input_gain"]
    pub input_gain_db: FloatParam,

    #[id = "output_gain"]
    pub output_gain_db: FloatParam,

    #[id = "sc_freq"]
    pub sidechain_freq: FloatParam,

    #[id = "sc_lpf"]
    pub sidechain_lowpass_freq: FloatParam,

    #[id = "range"]
    pub range_db: FloatParam,

    #[id = "expander_threshold"]
    pub expander_threshold_db: FloatParam,

    #[id = "expander_ratio"]
    pub expander_ratio: FloatParam,

    #[id = "upward_threshold"]
    pub upward_threshold_db: FloatParam,

    #[id = "upward_ratio"]
    pub upward_ratio: FloatParam,

    #[id = "hold"]
    pub hold_ms: FloatParam,

    #[id = "lookahead"]
    pub lookahead_ms: FloatParam,

    #[id = "style"]
    pub style: IntParam,

    #[id = "profile"]
    pub profile: IntParam,

    #[id = "profile_drive"]
    pub profile_drive: FloatParam,

    #[id = "profile_output"]
    pub profile_output: FloatParam,

    /// Read-only gain reduction output for host metering.
    #[id = "gr_out"]
    pub gr_output_db: FloatParam,
}

impl Default for FtsCompParams {
    fn default() -> Self {
        Self {
            threshold_db: FloatParam::new(
                "Threshold",
                -20.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            ratio: FloatParam::new(
                "Ratio",
                4.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(":1")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            attack_ms: FloatParam::new(
                "Attack",
                3.0,
                FloatRange::Skewed {
                    min: 0.005,
                    max: 300.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            release_ms: FloatParam::new(
                "Release",
                100.0,
                FloatRange::Skewed {
                    min: 10.0,
                    max: 3000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            knee_db: FloatParam::new(
                "Knee",
                6.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 72.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            auto_makeup: FloatParam::new(
                "Auto Gain",
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

            feedback: FloatParam::new("Feedback", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            channel_link: FloatParam::new(
                "Stereo Link",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0)),

            detector_rms_mix: FloatParam::new(
                "Detector RMS",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0)),

            inertia: FloatParam::new(
                "Inertia",
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 0.3,
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            inertia_decay: FloatParam::new(
                "Inertia Decay",
                0.94,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            ceiling: FloatParam::new(
                "Ceiling",
                1.0,
                FloatRange::Linear {
                    min: 0.01,
                    max: 4.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            drive: FloatParam::new("Drive", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            character_mode: IntParam::new("Character", 0, IntRange::Linear { min: 0, max: 6 })
                .with_value_to_string(Arc::new(|v| {
                    match v {
                        1 => "Tube",
                        2 => "Diode",
                        3 => "Bright",
                        4 => "Cubic",
                        5 => "Clip",
                        6 => "Asym",
                        _ => "Tanh",
                    }
                    .to_string()
                })),

            fold: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            multiband_amount: FloatParam::new(
                "Multiband",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0)),

            input_gain_db: FloatParam::new(
                "Input",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

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

            sidechain_freq: FloatParam::new(
                "SC HPF",
                85.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 300.0,
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            sidechain_lowpass_freq: FloatParam::new(
                "SC LPF",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 20_000.0,
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(Arc::new(|v| {
                if v <= 20.0 {
                    "Off".to_string()
                } else {
                    format!("{v:.0}")
                }
            })),

            range_db: FloatParam::new(
                "Range",
                60.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 60.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            expander_threshold_db: FloatParam::new(
                "Gate Threshold",
                -80.0,
                FloatRange::Linear {
                    min: -100.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            expander_ratio: FloatParam::new(
                "Gate Ratio",
                1.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(":1")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            upward_threshold_db: FloatParam::new(
                "Up Threshold",
                -60.0,
                FloatRange::Linear {
                    min: -100.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            upward_ratio: FloatParam::new(
                "Up Ratio",
                1.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(":1")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            hold_ms: FloatParam::new(
                "Hold",
                0.0,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 500.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            lookahead_ms: FloatParam::new(
                "Lookahead",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 20.0,
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            style: IntParam::new("Style", 0, IntRange::Linear { min: 0, max: 4 })
                .with_value_to_string(Arc::new(|v| {
                    match v {
                        0 => "Clean",
                        1 => "Classic",
                        2 => "Opto",
                        3 => "FET",
                        4 => "Punch",
                        _ => "Smooth",
                    }
                    .to_string()
                })),

            profile: IntParam::new("Profile", 0, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(Arc::new(|v| profile_name(v).to_string())),

            profile_drive: FloatParam::new(
                "Profile Drive",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0)),

            profile_output: FloatParam::new(
                "Profile Output",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0)),

            gr_output_db: FloatParam::new(
                "GR",
                0.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .non_automatable()
            .hide(),
        }
    }
}

// ── Plugin ───────────────────────────────────────────────────────────

struct FtsComp {
    params: Arc<FtsCompParams>,
    ui_state: Arc<CompUiState>,
    editor_state: Arc<DioxusState>,
    chain: CompChain,
    multiband_l: MultiBandCompressor,
    multiband_r: MultiBandCompressor,
    sample_rate: f64,
    /// Counter for waveform decimation.
    waveform_counter: usize,
    /// Samples between waveform writes (~50 updates/sec at 48kHz).
    waveform_interval: usize,
    /// Accumulated peak for current waveform interval.
    waveform_peak: f32,
    waveform_gr_peak: f32,
}

impl Default for FtsComp {
    fn default() -> Self {
        let params = Arc::new(FtsCompParams::default());
        let ui_state = Arc::new(CompUiState::new(params.clone()));
        Self {
            params,
            ui_state,
            editor_state: DioxusState::new(|| (900, 620)),
            chain: CompChain::new(),
            multiband_l: MultiBandCompressor::new(48000.0),
            multiband_r: MultiBandCompressor::new(48000.0),
            sample_rate: 48000.0,
            waveform_counter: 0,
            waveform_interval: 200, // ~240 Hz at 48kHz
            waveform_peak: 0.0,
            waveform_gr_peak: 0.0,
        }
    }
}

impl FtsComp {
    /// Sync nih-plug params → comp-dsp parameters.
    fn sync_params(&mut self) {
        let profile_index = self.params.profile.value();
        let profile = profile_for_index(profile_index);

        let mut effective = EffectiveCompParams::from_params(&self.params);
        effective.apply_profile_macros(
            profile_index,
            self.params.profile_drive.value() as f64,
            self.params.profile_output.value() as f64,
        );
        effective.apply_constraints(profile.constraints());

        let c = &mut self.chain.comp;
        c.set_threshold(effective.threshold_db);
        c.set_ratio(effective.ratio);
        c.set_attack_ms(effective.attack_ms);
        c.set_release_ms(effective.release_ms);
        c.set_knee(effective.knee_db);
        c.set_style(effective.style);
        c.style = effective.style;
        c.auto_makeup = effective.auto_makeup;
        c.feedback = effective.feedback;
        c.channel_link = effective.channel_link;
        c.detector_rms_mix = effective.detector_rms_mix;
        c.inertia = self.params.inertia.value() as f64;
        c.inertia_decay = self.params.inertia_decay.value() as f64;
        c.ceiling = self.params.ceiling.value() as f64;
        c.drive = effective.drive;
        c.character_mode = effective.character_mode;
        c.set_fold(self.params.fold.value() as f64);
        c.input_gain_db = effective.input_gain_db;
        c.output_gain_db = effective.output_gain_db;

        c.set_range_db(effective.range_db);
        c.expander_threshold_db = effective.expander_threshold_db;
        c.expander_ratio = effective.expander_ratio;
        c.upward_threshold_db = effective.upward_threshold_db;
        c.upward_ratio = effective.upward_ratio;
        c.hold_ms = self.params.hold_ms.value() as f64;

        let sc_freq = self.params.sidechain_freq.value() as f64;
        self.chain.set_sidechain_freq(sc_freq);

        let sc_lpf = self.params.sidechain_lowpass_freq.value() as f64;
        self.chain.set_sidechain_lowpass_freq(sc_lpf);

        let la_ms = self.params.lookahead_ms.value() as f64;
        self.chain.set_lookahead(la_ms);

        self.chain.comp.update(self.sample_rate);

        for multiband in [&mut self.multiband_l, &mut self.multiband_r] {
            multiband.update(self.sample_rate);
            multiband.set_threshold(effective.threshold_db);
            multiband.set_ratio(effective.ratio);
            multiband.set_attack_ms(effective.attack_ms);
            multiband.set_release_ms(effective.release_ms);
            multiband.set_knee(effective.knee_db);
            multiband.set_style(effective.style);
        }
    }

    fn process_frame(
        &mut self,
        left_ref: &mut f32,
        right_ref: &mut f32,
        sidechain_l: f64,
        sidechain_r: f64,
    ) {
        let mut left = *left_ref as f64;
        let mut right = *right_ref as f64;

        let input_peak = left.abs().max(right.abs()) as f32;

        self.chain
            .process_sample_with_sidechain(&mut left, &mut right, sidechain_l, sidechain_r);

        let multiband_amount = self.params.multiband_amount.value() as f64;
        if multiband_amount > 1e-6 {
            let mb_l = self.multiband_l.process(left, 0);
            let mb_r = self.multiband_r.process(right, 1);
            let amount = multiband_amount.clamp(0.0, 1.0);
            left = left * (1.0 - amount) + mb_l * amount;
            right = right * (1.0 - amount) + mb_r * amount;
        }

        *left_ref = left as f32;
        *right_ref = right as f32;

        let output_peak = left.abs().max(right.abs()) as f32;
        let gr = self
            .chain
            .comp
            .gain_reduction_db()
            .max(self.multiband_l.gain_reduction_db())
            .max(self.multiband_r.gain_reduction_db()) as f32;
        self.ui_state.gain_reduction_db.store(gr, Ordering::Relaxed);

        let prev_in = self.ui_state.input_peak_db.load(Ordering::Relaxed);
        let in_db = if input_peak > 0.0 {
            20.0 * input_peak.log10()
        } else {
            -100.0
        };
        let new_in = if in_db > prev_in {
            in_db
        } else {
            prev_in - 0.3
        };
        self.ui_state.input_peak_db.store(new_in, Ordering::Relaxed);

        let prev_out = self.ui_state.output_peak_db.load(Ordering::Relaxed);
        let out_db = if output_peak > 0.0 {
            20.0 * output_peak.log10()
        } else {
            -100.0
        };
        let new_out = if out_db > prev_out {
            out_db
        } else {
            prev_out - 0.3
        };
        self.ui_state
            .output_peak_db
            .store(new_out, Ordering::Relaxed);

        self.waveform_peak = self.waveform_peak.max(input_peak);
        self.waveform_gr_peak = self.waveform_gr_peak.max(gr / 30.0);
        self.waveform_counter += 1;

        let phase = self.waveform_counter as f32 / self.waveform_interval as f32;
        self.ui_state.waveform_phase.store(phase, Ordering::Relaxed);

        if self.waveform_counter >= self.waveform_interval {
            let pos = self.ui_state.waveform_pos.load(Ordering::Relaxed) as usize % WAVEFORM_LEN;
            self.ui_state.waveform_input[pos].store(self.waveform_peak.min(1.0), Ordering::Relaxed);
            self.ui_state.waveform_gr[pos].store(self.waveform_gr_peak.min(1.0), Ordering::Relaxed);
            self.ui_state
                .waveform_pos
                .store((pos + 1) as f32, Ordering::Relaxed);
            self.ui_state.waveform_phase.store(0.0, Ordering::Relaxed);

            self.waveform_counter = 0;
            self.waveform_peak = 0.0;
            self.waveform_gr_peak = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effective_params::{constrained_f64, profile_macro_controls};

    fn set_param(param: ParamPtr, value: &str) {
        unsafe {
            let normalized = param
                .string_to_normalized_value(value)
                .unwrap_or_else(|| panic!("could not parse parameter value {value:?}"));
            param.set_normalized_value(normalized);
        }
    }

    fn assert_approx_eq(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-5,
            "expected {actual} to approximately equal {expected}"
        );
    }

    #[test]
    fn profile_index_maps_to_named_profiles() {
        assert_eq!(profile_name(0), "Control");
        assert_eq!(profile_name(1), "LA-2A");
        assert_eq!(profile_name(2), "SSL Bus");
        assert_eq!(profile_name(3), "UREI 1176");
        assert_eq!(profile_name(99), "UREI 1176");
    }

    #[test]
    fn profile_constraints_override_or_clamp_effective_params() {
        let la2a = profile_for_index(1);
        assert_eq!(constrained_f64("style", 0.0, la2a.constraints()), 2.0);
        assert_eq!(
            constrained_f64("attack_ms", 0.005, la2a.constraints()),
            10.0
        );

        let ssl = profile_for_index(2);
        assert_eq!(constrained_f64("range_db", 60.0, ssl.constraints()), 18.0);

        let urei = profile_for_index(3);
        assert_eq!(constrained_f64("feedback", 0.0, urei.constraints()), 0.35);
    }

    #[test]
    fn profile_macros_expand_to_profile_specific_controls() {
        assert_eq!(profile_macro_controls(0, 1.0, 1.0), Vec::new());
        assert_eq!(
            profile_macro_controls(1, 0.25, 0.75),
            vec![("peak_reduction", 0.25), ("gain", 0.75)]
        );
        assert_eq!(
            profile_macro_controls(2, 0.25, 0.75),
            vec![("threshold", 0.25), ("makeup", 0.75)]
        );
        assert_eq!(
            profile_macro_controls(3, 0.25, 0.75),
            vec![("input", 0.25), ("output", 0.75)]
        );
    }

    #[test]
    fn profile_macro_writes_affect_effective_params_before_constraints() {
        let params = FtsCompParams::default();
        let mut effective = EffectiveCompParams::from_params(&params);
        effective.apply_profile_macros(3, 1.0, 1.0);
        effective.apply_constraints(profile_for_index(3).constraints());

        assert_eq!(effective.input_gain_db, 24.0);
        assert_eq!(effective.threshold_db, -44.0);
        assert_eq!(effective.output_gain_db, 24.0);
        assert_eq!(effective.drive, 0.8);
        assert_eq!(effective.character_mode, 2);
        assert_eq!(effective.style, 3);
        assert_eq!(effective.feedback, 0.35);
    }

    #[test]
    fn hardware_profiles_apply_character_defaults() {
        let params = FtsCompParams::default();

        let mut la2a = EffectiveCompParams::from_params(&params);
        la2a.apply_profile_macros(1, 1.0, 0.5);
        la2a.apply_constraints(profile_for_index(1).constraints());
        assert_eq!(la2a.detector_rms_mix, 1.0);
        assert_eq!(la2a.drive, 0.25);
        assert_eq!(la2a.character_mode, 1);

        let mut ssl = EffectiveCompParams::from_params(&params);
        ssl.apply_constraints(profile_for_index(2).constraints());
        assert_eq!(ssl.detector_rms_mix, 0.35);
        assert_eq!(ssl.character_mode, 0);

        let mut urei = EffectiveCompParams::from_params(&params);
        urei.apply_profile_macros(3, 1.0, 0.5);
        urei.apply_constraints(profile_for_index(3).constraints());
        assert_eq!(urei.detector_rms_mix, 0.0);
        assert_eq!(urei.drive, 0.8);
        assert_eq!(urei.character_mode, 2);
    }

    #[test]
    fn sync_params_routes_expanded_controls_to_dsp() {
        let mut plugin = FtsComp::default();
        let params = &plugin.params;

        set_param(params.detector_rms_mix.as_ptr(), "0.75");
        set_param(params.drive.as_ptr(), "0.5");
        set_param(params.character_mode.as_ptr(), "2");
        set_param(params.sidechain_lowpass_freq.as_ptr(), "2500");
        set_param(params.expander_threshold_db.as_ptr(), "-45");
        set_param(params.expander_ratio.as_ptr(), "3");
        set_param(params.upward_threshold_db.as_ptr(), "-50");
        set_param(params.upward_ratio.as_ptr(), "2.5");

        plugin.sync_params();

        assert_approx_eq(plugin.chain.comp.detector_rms_mix, 0.75);
        assert_approx_eq(plugin.chain.comp.drive, 0.5);
        assert_eq!(plugin.chain.comp.character_mode, 2);
        assert_approx_eq(plugin.chain.sidechain_lowpass_freq, 2_500.0);
        assert_approx_eq(plugin.chain.comp.expander_threshold_db, -45.0);
        assert_approx_eq(plugin.chain.comp.expander_ratio, 3.0);
        assert_approx_eq(plugin.chain.comp.upward_threshold_db, -50.0);
        assert_approx_eq(plugin.chain.comp.upward_ratio, 2.5);
    }

    #[test]
    fn process_frame_blends_multiband_path_when_enabled() {
        let mut broadband = FtsComp::default();
        let mut multiband = FtsComp::default();

        for plugin in [&mut broadband, &mut multiband] {
            let params = &plugin.params;
            set_param(params.threshold_db.as_ptr(), "-48");
            set_param(params.ratio.as_ptr(), "12");
            set_param(params.attack_ms.as_ptr(), "0.005");
            set_param(params.release_ms.as_ptr(), "40");
            set_param(params.knee_db.as_ptr(), "0");
            set_param(params.sidechain_freq.as_ptr(), "0");
            set_param(params.fold.as_ptr(), "1");
            plugin.sync_params();
        }

        set_param(multiband.params.multiband_amount.as_ptr(), "1");
        multiband.sync_params();

        let mut broadband_l = 0.0_f32;
        let mut broadband_r = 0.0_f32;
        let mut multiband_l = 0.0_f32;
        let mut multiband_r = 0.0_f32;

        for idx in 0..2_000 {
            let sample = if idx % 2 == 0 { 0.7 } else { -0.7 };
            broadband_l = sample;
            broadband_r = -sample;
            multiband_l = sample;
            multiband_r = -sample;

            broadband.process_frame(
                &mut broadband_l,
                &mut broadband_r,
                sample as f64,
                sample as f64,
            );
            multiband.process_frame(
                &mut multiband_l,
                &mut multiband_r,
                sample as f64,
                sample as f64,
            );
        }

        let output_delta = (multiband_l - broadband_l).abs() + (multiband_r - broadband_r).abs();
        assert!(
            output_delta > 1e-4,
            "multiband blend should alter the plugin output when enabled"
        );
        assert!(
            multiband.ui_state.gain_reduction_db.load(Ordering::Relaxed) > 0.0,
            "multiband gain reduction should contribute to the public meter"
        );
    }
}

impl Plugin for FtsComp {
    const NAME: &'static str = "FTS Compressor";
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[new_nonzero_u32(2)],
        names: PortNames {
            layout: Some("Stereo with sidechain"),
            main_input: Some("Input"),
            main_output: Some("Output"),
            aux_inputs: &["Sidechain"],
            aux_outputs: &[],
        },
        ..AudioIOLayout::const_default()
    }];

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            self.ui_state.clone(),
            editor::App,
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        self.waveform_interval = (buffer_config.sample_rate as usize / 240).max(1);
        self.chain.update(AudioConfig {
            sample_rate: self.sample_rate,
            max_buffer_size: buffer_config.max_buffer_size as usize,
        });
        true
    }

    fn reset(&mut self) {
        self.chain.reset();
        self.multiband_l.reset();
        self.multiband_r.reset();
        self.waveform_counter = 0;
        self.waveform_peak = 0.0;
        self.waveform_gr_peak = 0.0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync_params();

        // Report lookahead latency so the DAW can compensate
        context.set_latency_samples(self.chain.lookahead_samples as u32);

        if let Some(sidechain) = aux.inputs.get_mut(0) {
            for (mut frame, mut sc_frame) in buffer.iter_samples().zip(sidechain.iter_samples()) {
                let mut channels = frame.iter_mut();
                let Some(left_ref) = channels.next() else {
                    continue;
                };

                let mut sc_channels = sc_frame.iter_mut();
                let sc_l = sc_channels
                    .next()
                    .map(|s| *s as f64)
                    .unwrap_or(*left_ref as f64);
                let sc_r = sc_channels.next().map(|s| *s as f64).unwrap_or(sc_l);

                if let Some(right_ref) = channels.next() {
                    self.process_frame(left_ref, right_ref, sc_l, sc_r);
                } else {
                    let mut right = *left_ref;
                    self.process_frame(left_ref, &mut right, sc_l, sc_r);
                }
            }
        } else {
            for mut frame in buffer.iter_samples() {
                let mut channels = frame.iter_mut();
                let Some(left_ref) = channels.next() else {
                    continue;
                };

                if let Some(right_ref) = channels.next() {
                    self.process_frame(left_ref, right_ref, *left_ref as f64, *right_ref as f64);
                } else {
                    let mut right = *left_ref;
                    let sc_l = *left_ref as f64;
                    let sc_r = right as f64;
                    self.process_frame(left_ref, &mut right, sc_l, sc_r);
                }
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsComp {
    const CLAP_ID: &'static str = "com.fasttrackstudio.comp";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Compressor with hardware profiles");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Compressor,
        ClapFeature::Stereo,
    ];

    fn gain_adjustment_db(&self) -> f64 {
        // Return negative dB for gain reduction (compressor convention)
        -(self.ui_state.gain_reduction_db.load(Ordering::Relaxed) as f64)
    }
}

impl Vst3Plugin for FtsComp {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsCompPlugin001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_clap!(FtsComp);
nih_export_vst3!(FtsComp);
