//! FTS EQ — nih-plug entry point with 24-band parametric EQ and Dioxus GUI.
//!
//! Pro-Q 4 style parametric equalizer with draggable band nodes,
//! frequency response visualization, and per-band filter type selection.

use audiocore_core::prelude::*;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use spectrum_analyzer::dsp::AudioFeed;

use eq_dsp::hardware_eq::{
    Api550aSettings, HardwareEqModel, HardwareEqSettings, PultecEqp1aSettings, SslChannelSettings,
};
use eq_dsp::neve_1073::{
    Neve1073Hpf, Neve1073LowFreq, Neve1073MidFreq, Neve1073Model, Neve1073Settings,
};
use eq_ui::params::{EqUiState, FtsEqParams, NUM_BANDS, SPECTRUM_BINS};

// ── Plugin ──────────────────────────────────────────────────────────

struct FtsEqPlugin {
    params: Arc<FtsEqParams>,
    ui_state: Arc<EqUiState>,
    editor_state: Arc<DioxusState>,
    /// The one FTS EQ engine — the same one the rig plays through. It used to
    /// be a bare `EqChain` here, which is why this plugin could not sound a
    /// dynamic or spectral band at all.
    engine: eq_dsp::engine::FtsEq,
    neve_1073: Neve1073Model,
    hardware_eq: HardwareEqModel,
    sample_rate: f64,
    // Scratch buffers for f64 block processing
    left_buf: Vec<f64>,
    right_buf: Vec<f64>,
    // Analyzer feed (taken from the shared UI state on first use) + mono scratch.
    audio_feed: Option<AudioFeed>,
    pre_mono: Vec<f32>,
    post_mono: Vec<f32>,
}

impl Default for FtsEqPlugin {
    fn default() -> Self {
        let params = Arc::new(FtsEqParams::default());
        let ui_state = Arc::new(EqUiState::new(params.clone()));
        let engine = eq_dsp::engine::FtsEq::new(48000.0);
        Self {
            params,
            ui_state,
            editor_state: DioxusState::new(|| {
                (eq_ui::control_view::EDITOR_W, eq_ui::control_view::EDITOR_H)
            })
            .with_resize_hint(eq_ui::control_view::resize_hint()),
            engine,
            neve_1073: Neve1073Model::new(48000.0, Neve1073Settings::default()),
            hardware_eq: HardwareEqModel::new(
                48000.0,
                HardwareEqSettings::Pultec(PultecEqp1aSettings::default()),
            ),
            sample_rate: 48000.0,
            left_buf: Vec::new(),
            right_buf: Vec::new(),
            audio_feed: None,
            pre_mono: Vec::new(),
            post_mono: Vec::new(),
        }
    }
}

// NOTE the `type` parameter is a **canonical** shape index — `sync_params`
// hands it straight to `BandConfig.shape`, which the engine reads through
// `FilterShape::from_canonical_index`. There used to be a `plugin_shape`
// table here claiming the persisted order swapped Low Cut and High Shelf,
// and a `shape_to_filter_type` built on it; neither was ever called, so the
// swap they described never reached the audio path. They are gone rather
// than fixed: a dead table asserting a false invariant is how a converter
// ends up translating every high shelf into a high-pass.

impl FtsEqPlugin {
    /// Sync nih-plug params → the shared engine.
    ///
    /// Everything a band can be now crosses in one shot: the static curve, the
    /// dynamics and the stereo placement. Sending the whole config each time
    /// rather than diffing field by field is what lets a preset land as one
    /// coherent band instead of a partial one — the engine does its own
    /// comparison downstream.
    fn sync_params(&mut self) {
        // Solo: while any band is soloed the rest are muted.
        let any_solo = (0..NUM_BANDS).any(|i| self.params.bands[i].solo.value() > 0.5);
        let scale = f64::from(self.params.gain_scale.value()) / 100.0;

        for i in 0..NUM_BANDS {
            let bp = &self.params.bands[i];
            let band_enabled = bp.enabled.value() > 0.5;
            let enabled = if any_solo {
                band_enabled && bp.solo.value() > 0.5
            } else {
                band_enabled
            };

            self.engine.set_band(
                i,
                eq_dsp::engine::BandConfig {
                    // The plugin has no separate "used" flag — a band it
                    // carries is a band that exists, and `enabled` says
                    // whether it sounds.
                    used: true,
                    enabled,
                    freq_hz: f64::from(bp.freq_hz.value()),
                    gain_db: f64::from(bp.gain_db.value()),
                    // Pro-Q 4 convention: display Q=1.0 = Butterworth.
                    q: f64::from(bp.q.value()) * std::f64::consts::FRAC_1_SQRT_2,
                    shape: bp.filter_type.value().max(0) as u32,
                    slope: f64::from(bp.slope.value().max(0.0)),
                    placement: eq_dsp::band::Placement::from_index(
                        bp.placement.value().max(0) as u32
                    ),
                    stream: 0,
                },
            );
            self.engine.set_band_dynamics(
                i,
                eq_dsp::engine::BandDynamics {
                    range_db: f64::from(bp.dyn_range_db.value()),
                    threshold_db: f64::from(bp.dyn_threshold_db.value()),
                    attack_pct: f64::from(bp.dyn_attack.value()),
                    release_pct: f64::from(bp.dyn_release.value()),
                    auto: bp.dyn_auto.value() > 0.5,
                    relative: false,
                    spectral: bp.spectral.value() > 0.5,
                    spectral_density: f64::from(bp.spectral_density.value()),
                    spectral_tilt: bp.spectral_tilt.value() > 0.5,
                    side_filtered: bp.side_filtered.value() > 0.5,
                    side_lo_hz: f64::from(bp.side_lo_hz.value()),
                    side_hi_hz: f64::from(bp.side_hi_hz.value()),
                },
            );
        }

        // The gain macro scales bands and dynamic ranges together, inside the
        // engine, so the two cannot drift apart.
        self.engine.set_gain_scale(scale);

        // The instance globals. Each of these is an audible part of a Pro-Q
        // preset that used to stop at the translator: Character is a squarer
        // rather than a trim, Auto Gain moves the whole output, and the pan
        // is either L/R or M/S depending on its mode.
        self.engine
            .set_auto_gain(self.params.auto_gain.value() > 0.5);
        self.engine
            .set_character(self.params.character.value().max(0) as u32);
        self.engine.set_output_pan(
            f64::from(self.params.output_pan.value()),
            self.params.output_pan_mid_side.value() > 0.5,
        );
    }

    fn sync_neve_1073(&mut self) {
        let settings = Neve1073Settings {
            eq_in: self.params.neve_eq_in.value() > 0.5,
            phase_invert: self.params.neve_phase.value() > 0.5,
            trim_db: f64::from(self.params.neve_trim_db.value()),
            drive_percent: f64::from(self.params.neve_drive.value()),
            hpf: match self.params.neve_hpf.value() {
                1 => Neve1073Hpf::Hz50,
                2 => Neve1073Hpf::Hz80,
                3 => Neve1073Hpf::Hz160,
                4 => Neve1073Hpf::Hz300,
                _ => Neve1073Hpf::Off,
            },
            low_freq: match self.params.neve_low_freq.value() {
                1 => Neve1073LowFreq::Hz35,
                2 => Neve1073LowFreq::Hz60,
                3 => Neve1073LowFreq::Hz110,
                4 => Neve1073LowFreq::Hz220,
                _ => Neve1073LowFreq::Off,
            },
            low_gain_db: f64::from(self.params.neve_low_gain_db.value()),
            mid_freq: match self.params.neve_mid_freq.value() {
                1 => Neve1073MidFreq::Hz360,
                2 => Neve1073MidFreq::Hz700,
                3 => Neve1073MidFreq::Hz1600,
                4 => Neve1073MidFreq::Hz3200,
                5 => Neve1073MidFreq::Hz4800,
                6 => Neve1073MidFreq::Hz7200,
                _ => Neve1073MidFreq::Off,
            },
            mid_gain_db: f64::from(self.params.neve_mid_gain_db.value()),
            high_gain_db: f64::from(self.params.neve_high_gain_db.value()),
        };

        if self.neve_1073.settings() != settings {
            self.neve_1073.set_settings(settings);
        }
    }

    fn sync_hardware_eq(&mut self, model: i32) {
        let settings = match model {
            1 => HardwareEqSettings::Pultec(PultecEqp1aSettings {
                eq_in: self.params.pultec_eq_in.value() > 0.5,
                low_freq_hz: pultec_low_freq(self.params.pultec_low_freq.value()),
                low_boost_db: f64::from(self.params.pultec_low_boost_db.value()),
                low_atten_db: f64::from(self.params.pultec_low_atten_db.value()),
                high_boost_freq_hz: pultec_high_boost_freq(
                    self.params.pultec_high_boost_freq.value(),
                ),
                high_boost_db: f64::from(self.params.pultec_high_boost_db.value()),
                high_bandwidth: f64::from(self.params.pultec_bandwidth.value()),
                high_atten_freq_hz: pultec_high_atten_freq(
                    self.params.pultec_high_atten_freq.value(),
                ),
                high_atten_db: f64::from(self.params.pultec_high_atten_db.value()),
                drive_percent: f64::from(self.params.pultec_drive.value()),
                trim_db: f64::from(self.params.pultec_trim_db.value()),
            }),
            3 => HardwareEqSettings::Api(Api550aSettings {
                eq_in: self.params.api_eq_in.value() > 0.5,
                low_freq_hz: api_low_freq(self.params.api_low_freq.value()),
                low_gain_db: f64::from(self.params.api_low_gain_db.value()),
                mid_freq_hz: api_mid_freq(self.params.api_mid_freq.value()),
                mid_gain_db: f64::from(self.params.api_mid_gain_db.value()),
                high_freq_hz: api_high_freq(self.params.api_high_freq.value()),
                high_gain_db: f64::from(self.params.api_high_gain_db.value()),
                drive_percent: f64::from(self.params.api_drive.value()),
                trim_db: f64::from(self.params.api_trim_db.value()),
            }),
            4 | 5 => HardwareEqSettings::Ssl(SslChannelSettings {
                eq_in: self.params.ssl_eq_in.value() > 0.5,
                e_series: model == 4,
                hpf_hz: f64::from(self.params.ssl_hpf_hz.value()),
                lpf_hz: f64::from(self.params.ssl_lpf_hz.value()),
                lf_freq_hz: f64::from(self.params.ssl_lf_freq_hz.value()),
                lf_gain_db: f64::from(self.params.ssl_lf_gain_db.value()),
                lmf_freq_hz: f64::from(self.params.ssl_lmf_freq_hz.value()),
                lmf_gain_db: f64::from(self.params.ssl_lmf_gain_db.value()),
                hmf_freq_hz: f64::from(self.params.ssl_hmf_freq_hz.value()),
                hmf_gain_db: f64::from(self.params.ssl_hmf_gain_db.value()),
                hf_freq_hz: f64::from(self.params.ssl_hf_freq_hz.value()),
                hf_gain_db: f64::from(self.params.ssl_hf_gain_db.value()),
                drive_percent: f64::from(self.params.ssl_drive.value()),
                trim_db: f64::from(self.params.ssl_trim_db.value()),
            }),
            _ => return,
        };

        self.hardware_eq.set_settings(settings);
    }

    fn publish_model_response(&self, model: i32) {
        let log_min = 20.0_f64.log10();
        let log_max = 20_000.0_f64.log10();
        let freqs: Vec<f64> = (0..SPECTRUM_BINS)
            .map(|i| {
                let t = i as f64 / (SPECTRUM_BINS - 1) as f64;
                10.0_f64.powf(log_min + t * (log_max - log_min))
            })
            .collect();

        let response = match model {
            2 => self.neve_1073.magnitude_response_db(&freqs),
            1 | 3..=5 => self.hardware_eq.magnitude_response_db(&freqs),
            _ => {
                for bin in self.ui_state.model_response_bins.iter() {
                    bin.store(0.0, Ordering::Relaxed);
                }
                return;
            }
        };

        for (bin, db) in self.ui_state.model_response_bins.iter().zip(response) {
            bin.store(db as f32, Ordering::Relaxed);
        }
    }
}

fn pultec_low_freq(value: i32) -> f64 {
    [20.0, 30.0, 60.0, 100.0]
        .get(value as usize)
        .copied()
        .unwrap_or(60.0)
}

fn pultec_high_boost_freq(value: i32) -> f64 {
    [3000.0, 4000.0, 5000.0, 8000.0, 10000.0, 12000.0, 16000.0]
        .get(value as usize)
        .copied()
        .unwrap_or(5000.0)
}

fn pultec_high_atten_freq(value: i32) -> f64 {
    [5000.0, 10000.0, 20000.0]
        .get(value as usize)
        .copied()
        .unwrap_or(10000.0)
}

fn api_low_freq(value: i32) -> f64 {
    [50.0, 100.0, 200.0, 400.0]
        .get(value as usize)
        .copied()
        .unwrap_or(100.0)
}

fn api_mid_freq(value: i32) -> f64 {
    [400.0, 800.0, 1500.0, 3000.0, 5000.0]
        .get(value as usize)
        .copied()
        .unwrap_or(1500.0)
}

fn api_high_freq(value: i32) -> f64 {
    [5000.0, 7000.0, 10000.0, 12500.0, 15000.0]
        .get(value as usize)
        .copied()
        .unwrap_or(10000.0)
}

impl Plugin for FtsEqPlugin {
    const NAME: &'static str = "FTS EQ";
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    // `Plugin::Editor` is an associated type as of the baseview 0.3 rework,
    // so the editor is named here rather than returned as a trait object.
    type Editor = audiocore_core::nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            self.ui_state.clone(),
            eq_ui::control_view::App,
        )
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = f64::from(buffer_config.sample_rate);
        self.ui_state
            .sample_rate
            .store(buffer_config.sample_rate, Ordering::Relaxed);

        self.engine
            .prepare(self.sample_rate, buffer_config.max_buffer_size);
        self.neve_1073.set_sample_rate(self.sample_rate);
        self.hardware_eq.set_sample_rate(self.sample_rate);
        self.ui_state
            .analyzer
            .set_sample_rate(buffer_config.sample_rate);

        // Pre-allocate scratch buffers
        let max_samples = buffer_config.max_buffer_size as usize;
        self.left_buf.resize(max_samples, 0.0);
        self.right_buf.resize(max_samples, 0.0);

        true
    }

    fn reset(&mut self) {
        self.engine.deactivate();
        self.neve_1073.reset();
        self.hardware_eq.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let model = self.params.model.value();
        let use_neve_1073 = model == 2;
        let use_hardware_eq = matches!(model, 1 | 3 | 4 | 5);
        if use_neve_1073 {
            self.sync_neve_1073();
        } else if use_hardware_eq {
            self.sync_hardware_eq(model);
        } else {
            self.sync_params();
        }
        self.publish_model_response(model);

        let output_gain =
            audiocore_dsp::db::db_to_linear(f64::from(self.params.output_gain_db.value()));
        let n = buffer.samples();

        // Ensure scratch buffers are large enough
        self.left_buf.resize(n, 0.0);
        self.right_buf.resize(n, 0.0);
        self.pre_mono.resize(n, 0.0);
        self.post_mono.resize(n, 0.0);

        // Take the analyzer feed out of the shared state on first process.
        if self.audio_feed.is_none() {
            self.audio_feed = self.ui_state.take_audio_feed();
        }

        // Convert f32 buffer → f64 scratch, track input peak, capture pre-EQ mono
        let mut input_peak: f32 = 0.0;
        for (i, mut frame) in buffer.iter_samples().enumerate() {
            let l = f64::from(*frame.get_mut(0).unwrap());
            let r = f64::from(*frame.get_mut(1).unwrap());
            input_peak = input_peak.max(l.abs().max(r.abs()) as f32);
            self.left_buf[i] = l;
            self.right_buf[i] = r;
            self.pre_mono[i] = ((l + r) * 0.5) as f32;
        }

        // Process selected model at native rate.
        if use_neve_1073 {
            self.neve_1073
                .process(&mut self.left_buf[..n], &mut self.right_buf[..n]);
        } else if use_hardware_eq {
            self.hardware_eq
                .process(&mut self.left_buf[..n], &mut self.right_buf[..n]);
        } else {
            self.engine
                .process(&mut self.left_buf[..n], &mut self.right_buf[..n]);
        }

        // Write back to f32 buffer, apply output gain, metering + FFT
        let mut output_peak: f32 = 0.0;
        for (i, mut frame) in buffer.iter_samples().enumerate() {
            let l = self.left_buf[i] * output_gain;
            let r = self.right_buf[i] * output_gain;

            *frame.get_mut(0).unwrap() = l as f32;
            *frame.get_mut(1).unwrap() = r as f32;

            output_peak = output_peak.max(l.abs().max(r.abs()) as f32);

            // Capture post-EQ mono for the analyzer.
            self.post_mono[i] = ((l + r) * 0.5) as f32;
        }

        // Feed the analyzer (audio-thread side: just lock-free ring pushes).
        if let Some(feed) = self.audio_feed.as_mut() {
            feed.push_pre(&self.pre_mono[..n]);
            feed.push_post(&self.post_mono[..n]);
        }

        // Update peak meters
        let prev_in = self.ui_state.input_peak_db.load(Ordering::Relaxed);
        let in_db = if input_peak > 0.0 {
            20.0 * input_peak.log10()
        } else {
            -100.0
        };
        self.ui_state.input_peak_db.store(
            if in_db > prev_in {
                in_db
            } else {
                prev_in - 0.3
            },
            Ordering::Relaxed,
        );

        let prev_out = self.ui_state.output_peak_db.load(Ordering::Relaxed);
        let out_db = if output_peak > 0.0 {
            20.0 * output_peak.log10()
        } else {
            -100.0
        };
        self.ui_state.output_peak_db.store(
            if out_db > prev_out {
                out_db
            } else {
                prev_out - 0.3
            },
            Ordering::Relaxed,
        );

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsEqPlugin {
    const CLAP_ID: &'static str = "com.fasttrackstudio.eq";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("24-band parametric EQ");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Equalizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsEqPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsEqPlugin00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Eq];
}

nice_export_clap!(FtsEqPlugin);
nice_export_vst3!(FtsEqPlugin);
