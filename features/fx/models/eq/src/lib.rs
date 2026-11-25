//! FTS EQ — nih-plug entry point with 24-band parametric EQ and Dioxus GUI.
//!
//! Pro-Q 4 style parametric equalizer with draggable band nodes,
//! frequency response visualization, and per-band filter type selection.
//!
//! The plugin is a stack (`docs/spec/fx/stack.md`): up to
//! [`MAX_STAGES`] complete EQs — each its own model, bands and output — run
//! through [`dsp::stack::StagePool`]. The default is one stage on one lane,
//! which is the plain EQ, sample for sample.

use plugin_core::prelude::*;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use dsp::eq::analyzer::AudioFeed;

use dsp::eq::hardware::hardware_eq::{
    Api550aSettings, HardwareEqModel, HardwareEqSettings, PultecEqp1aSettings, SslChannelSettings,
};
use dsp::eq::hardware::neve_1073::{
    Neve1073Hpf, Neve1073LowFreq, Neve1073MidFreq, Neve1073Model, Neve1073Settings,
};
use dsp::eq::ui::params::{
    EqStageParams, EqUiState, FtsEqParams, MAX_STAGES, NUM_BANDS, SPECTRUM_BINS,
};
use dsp::stack::{Stage, StagePool};

/// The most latency one stage can add, in samples: the spectral engine's
/// 4096-point STFT (`FtsEq::prepare`), the only latency an EQ stage has.
/// Bounds the pool's lane-alignment buffers (`fx.stack.latency`).
const MAX_STAGE_LATENCY: usize = 4096;

// NOTE the `type` parameter is a **canonical** shape index — `sync_params`
// hands it straight to `BandConfig.shape`, which the engine reads through
// `FilterShape::from_canonical_index`. There used to be a `plugin_shape`
// table here claiming the persisted order swapped Low Cut and High Shelf,
// and a `shape_to_filter_type` built on it; neither was ever called, so the
// swap they described never reached the audio path. They are gone rather
// than fixed: a dead table asserting a false invariant is how a converter
// ends up translating every high shelf into a high-pass.

// ── Stage adapter ─────────────────────────────────────────────────────────

/// One complete EQ as a stack [`Stage`]: the parametric engine and the
/// hardware models, whichever the stage's model selects, then its output
/// gain — exactly what the single-stage plugin ran per block.
struct EqStage {
    /// The one FTS EQ engine — the same one the rig plays through. It used to
    /// be a bare `EqChain` here, which is why this plugin could not sound a
    /// dynamic or spectral band at all.
    engine: dsp::eq::host::CanonicalEq,
    neve_1073: Neve1073Model,
    hardware_eq: HardwareEqModel,
    /// The model this block runs, read from the stage's params at sync.
    model: i32,
    output_gain: f64,
    /// This block's side-chain, copied in by the shell (a [`Stage`] only
    /// sees its own audio). Sized in `activate`.
    key_left: Vec<f64>,
    key_right: Vec<f64>,
    keyed: bool,
}

impl EqStage {
    fn new() -> Self {
        Self {
            engine: dsp::eq::host::CanonicalEq::new(48000.0),
            neve_1073: Neve1073Model::new(48000.0, Neve1073Settings::default()),
            hardware_eq: HardwareEqModel::new(
                48000.0,
                HardwareEqSettings::Pultec(PultecEqp1aSettings::default()),
            ),
            model: 0,
            output_gain: 1.0,
            key_left: Vec::new(),
            key_right: Vec::new(),
            keyed: false,
        }
    }

    fn prepare(&mut self, sample_rate: f64, max_block: u32) {
        self.engine.prepare(sample_rate, max_block);
        self.neve_1073.set_sample_rate(sample_rate);
        self.hardware_eq.set_sample_rate(sample_rate);
        self.key_left = vec![0.0; max_block as usize];
        self.key_right = vec![0.0; max_block as usize];
    }

    const fn uses_neve_1073(&self) -> bool {
        self.model == 2
    }

    const fn uses_hardware_eq(&self) -> bool {
        matches!(self.model, 1 | 3 | 4 | 5)
    }

    /// Pull this stage's params into whichever processor its model runs.
    fn sync(&mut self, p: &EqStageParams) {
        self.model = p.model.value();
        if self.uses_neve_1073() {
            self.sync_neve_1073(p);
        } else if self.uses_hardware_eq() {
            self.sync_hardware_eq(p, self.model);
        } else {
            self.sync_params(p);
        }
        self.output_gain = dsp_core::audio::db::db_to_linear(f64::from(p.output_gain_db.value()));
    }

    /// Sync nih-plug params → the shared engine.
    ///
    /// Everything a band can be now crosses in one shot: the static curve, the
    /// dynamics and the stereo placement. Sending the whole config each time
    /// rather than diffing field by field is what lets a preset land as one
    /// coherent band instead of a partial one — the engine does its own
    /// comparison downstream.
    fn sync_params(&mut self, p: &EqStageParams) {
        // Focus and delta, the engine's two listen modes — neither of which
        // the plugin used to reach at all.
        //
        // Focus auditions a *region*: a bandpass at the focused band's
        // frequency and Q, or the whole reach of a shelf or cut. It was
        // previously wired as a focus — every other band muted, so what you
        // heard was this band's processing rather than the signal it sits in —
        // which is a different thing and not the one that helps you find what
        // is ringing.
        //
        // Delta is the other question: the EQ's input subtracted from its
        // output, so you hear what it is doing rather than what it makes. It
        // wins when both are asked for, because it is a statement about the
        // whole EQ.
        let focused = (0..NUM_BANDS).find(|i| p.bands[*i].focus.value() > 0.5);
        self.engine.set_listen(if p.delta.value() > 0.5 {
            Some((0, 2))
        } else {
            focused.map(|i| (i, 1))
        });
        let scale = f64::from(p.gain_scale.value()) / 100.0;

        for i in 0..NUM_BANDS {
            let bp = &p.bands[i];
            let enabled = bp.enabled.value() > 0.5;

            self.engine.set_canonical_band(
                i,
                dsp::eq::host::CanonicalBandConfig {
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
                    placement: dsp::eq::Placement::from_index(bp.placement.value().max(0) as u32),
                    stream: 0,
                },
                dsp::eq::host::CanonicalBandDynamics {
                    range_db: f64::from(bp.dyn_range_db.value()),
                    threshold_db: f64::from(bp.dyn_threshold_db.value()),
                    attack_pct: f64::from(bp.dyn_attack.value()),
                    release_pct: f64::from(bp.dyn_release.value()),
                    auto: bp.dyn_auto.value() > 0.5,
                    relative: bp.dyn_relative.value() > 0.5,
                    spectral: bp.spectral.value() > 0.5,
                    spectral_density: f64::from(bp.spectral_density.value()),
                    spectral_tilt: bp.spectral_tilt.value() > 0.5,
                    side_filtered: bp.side_filtered.value() > 0.5,
                    side_lo_hz: f64::from(bp.side_lo_hz.value()),
                    side_hi_hz: f64::from(bp.side_hi_hz.value()),
                    external_key: bp.dyn_key.value() > 0.5,
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
        self.engine.set_auto_gain(p.auto_gain.value() > 0.5);
        self.engine.set_character(p.character.value().max(0) as u32);
        self.engine.set_output_pan(
            f64::from(p.output_pan.value()),
            p.output_pan_mid_side.value() > 0.5,
        );
    }

    fn sync_neve_1073(&mut self, p: &EqStageParams) {
        let settings = Neve1073Settings {
            eq_in: p.neve_eq_in.value() > 0.5,
            phase_invert: p.neve_phase.value() > 0.5,
            trim_db: f64::from(p.neve_trim_db.value()),
            drive_percent: f64::from(p.neve_drive.value()),
            hpf: match p.neve_hpf.value() {
                1 => Neve1073Hpf::Hz50,
                2 => Neve1073Hpf::Hz80,
                3 => Neve1073Hpf::Hz160,
                4 => Neve1073Hpf::Hz300,
                _ => Neve1073Hpf::Off,
            },
            low_freq: match p.neve_low_freq.value() {
                1 => Neve1073LowFreq::Hz35,
                2 => Neve1073LowFreq::Hz60,
                3 => Neve1073LowFreq::Hz110,
                4 => Neve1073LowFreq::Hz220,
                _ => Neve1073LowFreq::Off,
            },
            low_gain_db: f64::from(p.neve_low_gain_db.value()),
            mid_freq: match p.neve_mid_freq.value() {
                1 => Neve1073MidFreq::Hz360,
                2 => Neve1073MidFreq::Hz700,
                3 => Neve1073MidFreq::Hz1600,
                4 => Neve1073MidFreq::Hz3200,
                5 => Neve1073MidFreq::Hz4800,
                6 => Neve1073MidFreq::Hz7200,
                _ => Neve1073MidFreq::Off,
            },
            mid_gain_db: f64::from(p.neve_mid_gain_db.value()),
            high_gain_db: f64::from(p.neve_high_gain_db.value()),
        };

        if self.neve_1073.settings() != settings {
            if self.neve_1073.settings() != settings {
                self.neve_1073.set_settings(settings);
            }
        }
    }

    fn sync_hardware_eq(&mut self, p: &EqStageParams, model: i32) {
        let settings = match model {
            1 => HardwareEqSettings::Pultec(PultecEqp1aSettings {
                eq_in: p.pultec_eq_in.value() > 0.5,
                low_freq_hz: pultec_low_freq(p.pultec_low_freq.value()),
                low_boost_db: f64::from(p.pultec_low_boost_db.value()),
                low_atten_db: f64::from(p.pultec_low_atten_db.value()),
                high_boost_freq_hz: pultec_high_boost_freq(p.pultec_high_boost_freq.value()),
                high_boost_db: f64::from(p.pultec_high_boost_db.value()),
                high_bandwidth: f64::from(p.pultec_bandwidth.value()),
                high_atten_freq_hz: pultec_high_atten_freq(p.pultec_high_atten_freq.value()),
                high_atten_db: f64::from(p.pultec_high_atten_db.value()),
                drive_percent: f64::from(p.pultec_drive.value()),
                trim_db: f64::from(p.pultec_trim_db.value()),
            }),
            3 => HardwareEqSettings::Api(Api550aSettings {
                eq_in: p.api_eq_in.value() > 0.5,
                low_freq_hz: api_low_freq(p.api_low_freq.value()),
                low_gain_db: f64::from(p.api_low_gain_db.value()),
                mid_freq_hz: api_mid_freq(p.api_mid_freq.value()),
                mid_gain_db: f64::from(p.api_mid_gain_db.value()),
                high_freq_hz: api_high_freq(p.api_high_freq.value()),
                high_gain_db: f64::from(p.api_high_gain_db.value()),
                drive_percent: f64::from(p.api_drive.value()),
                trim_db: f64::from(p.api_trim_db.value()),
            }),
            4 | 5 => HardwareEqSettings::Ssl(SslChannelSettings {
                eq_in: p.ssl_eq_in.value() > 0.5,
                e_series: model == 4,
                hpf_hz: f64::from(p.ssl_hpf_hz.value()),
                lpf_hz: f64::from(p.ssl_lpf_hz.value()),
                lf_freq_hz: f64::from(p.ssl_lf_freq_hz.value()),
                lf_gain_db: f64::from(p.ssl_lf_gain_db.value()),
                lmf_freq_hz: f64::from(p.ssl_lmf_freq_hz.value()),
                lmf_gain_db: f64::from(p.ssl_lmf_gain_db.value()),
                hmf_freq_hz: f64::from(p.ssl_hmf_freq_hz.value()),
                hmf_gain_db: f64::from(p.ssl_hmf_gain_db.value()),
                hf_freq_hz: f64::from(p.ssl_hf_freq_hz.value()),
                hf_gain_db: f64::from(p.ssl_hf_gain_db.value()),
                drive_percent: f64::from(p.ssl_drive.value()),
                trim_db: f64::from(p.ssl_trim_db.value()),
            }),
            _ => return,
        };

        self.hardware_eq.set_settings(settings);
    }

    /// Publish this stage's model response for the graph (the hardware
    /// models' curve; flat for the parametric engine, which the graph draws
    /// from its own bands).
    fn publish_model_response(&self, ui: &EqUiState) {
        let filter = match self.model {
            2 => self.neve_1073.prepared_filter(),
            1 | 3..=5 => self.hardware_eq.prepared_filter(),
            _ => {
                for bin in ui.model_response_bins.iter() {
                    bin.store(0.0, Ordering::Relaxed);
                }
                return;
            }
        };
        let log_min = 20.0_f64.log10();
        let log_max = 20_000.0_f64.log10();
        for (i, bin) in ui.model_response_bins.iter().enumerate() {
            let t = i as f64 / (SPECTRUM_BINS - 1) as f64;
            let hz = 10.0_f64.powf(log_min + t * (log_max - log_min));
            let db = filter
                .as_ref()
                .map_or(f64::NAN, |f| f.magnitude_db(hz).unwrap_or(f64::NAN));
            bin.store(db as f32, Ordering::Relaxed);
        }
    }
}

impl Stage for EqStage {
    fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
        let n = l.len();
        // Process selected model at native rate.
        if self.uses_neve_1073() {
            self.neve_1073.process(l, r);
        } else if self.uses_hardware_eq() {
            self.hardware_eq.process(l, r);
        } else {
            let key = self
                .keyed
                .then(|| (&self.key_left[..n], &self.key_right[..n]));
            self.engine.process_keyed(l, r, key);
        }
        // The output gain, in f64 as it always was — at 0 dB a multiply by
        // exactly 1.0, so the plain EQ is untouched.
        let g = self.output_gain;
        for (a, b) in l.iter_mut().zip(r.iter_mut()) {
            *a *= g;
            *b *= g;
        }
    }

    fn latency(&self) -> usize {
        // Only the parametric engine has latency (a spectral band's STFT);
        // a stage on a hardware model does not run it.
        if self.uses_neve_1073() || self.uses_hardware_eq() {
            0
        } else {
            self.engine.latency() as usize
        }
    }

    fn reset(&mut self) {
        self.engine.reset();
        self.neve_1073.reset();
        self.hardware_eq.reset();
    }
}

// ── Plugin ──────────────────────────────────────────────────────────

pub struct FtsEqPlugin {
    params: Arc<FtsEqParams>,
    ui_state: Arc<EqUiState>,
    editor_state: Arc<DioxusState>,
    /// The stack: a fixed pool of complete EQs, topology from params.
    pool: StagePool<EqStage>,
    /// What the host was last told the latency is.
    last_latency: usize,
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
        Self {
            params,
            ui_state,
            editor_state: DioxusState::new(|| {
                (
                    dsp::eq::ui::control_view::EDITOR_W,
                    dsp::eq::ui::control_view::EDITOR_H,
                )
            })
            .with_resize_hint(dsp::eq::ui::control_view::resize_hint()),
            pool: StagePool::new((0..MAX_STAGES).map(|_| EqStage::new()).collect()),
            last_latency: 0,
            sample_rate: 48000.0,
            left_buf: Vec::new(),
            right_buf: Vec::new(),
            audio_feed: None,
            pre_mono: Vec::new(),
            post_mono: Vec::new(),
        }
    }
}

impl FtsEqPlugin {
    /// Push params into every in-use stage, then the pool topology (no
    /// allocation).
    fn sync_stack(&mut self) {
        for i in 0..MAX_STAGES {
            let p = self.params.stage(i);
            if p.place.in_use.value()
                && let Some(stage) = self.pool.stage_mut(i)
            {
                stage.sync(p);
            }
        }
        dsp::stack::host::sync_pool(&mut self.pool, &*self.params);
    }

    /// The stage the editor is looking at — what the per-stage readouts
    /// describe.
    fn focused_stage(&self) -> usize {
        self.ui_state
            .focused_stage
            .load(Ordering::Relaxed)
            .min(MAX_STAGES - 1)
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

    /// Stereo in and out, with a stereo side-chain a dynamic band can key
    /// from; the plain stereo layout for hosts that will not offer one.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2)],
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];

    // `Plugin::Editor` is an associated type as of the baseview 0.3 rework,
    // so the editor is named here rather than returned as a trait object.
    type Editor = plugin_core::nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            self.ui_state.clone(),
            dsp::eq::ui::control_view::App,
        )
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = f64::from(buffer_config.sample_rate);
        self.ui_state
            .sample_rate
            .store(buffer_config.sample_rate, Ordering::Relaxed);

        // Every stage is prepared here, not when it joins the stack: the
        // audio thread must not allocate, and four EQs are small enough to
        // hold ready.
        for i in 0..MAX_STAGES {
            if let Some(stage) = self.pool.stage_mut(i) {
                stage.prepare(self.sample_rate, buffer_config.max_buffer_size);
            }
        }
        // Worst case a lane could need compensating by: every stage serial
        // on one lane, each with a spectral band in.
        self.pool.prepare(
            buffer_config.max_buffer_size as usize,
            MAX_STAGE_LATENCY * MAX_STAGES,
        );
        self.ui_state
            .analyzer
            .set_sample_rate(buffer_config.sample_rate);

        // Pre-allocate scratch buffers
        let max_samples = buffer_config.max_buffer_size as usize;
        self.left_buf.resize(max_samples, 0.0);
        self.right_buf.resize(max_samples, 0.0);
        self.pre_mono.resize(max_samples, 0.0);
        self.post_mono.resize(max_samples, 0.0);
        if self.audio_feed.is_none() {
            self.audio_feed = self.ui_state.take_audio_feed();
        }

        self.sync_stack();
        self.last_latency = self.pool.latency();
        context.set_latency_samples(self.last_latency as u32);
        true
    }

    fn reset(&mut self) {
        self.pool.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync_stack();
        let focused = self.focused_stage();
        if let Some(stage) = self.pool.stage(focused) {
            stage.publish_model_response(&self.ui_state);
        }

        let n = buffer.samples();

        // Ensure scratch buffers are large enough
        self.left_buf.resize(n, 0.0);
        self.right_buf.resize(n, 0.0);
        self.pre_mono.resize(n, 0.0);
        self.post_mono.resize(n, 0.0);

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

        // The side-chain, when the host routes one: a stereo aux bus, mono
        // or wider folded to its first two channels. Every in-use stage
        // keys from the same input, so each gets a copy — a stage only sees
        // its own audio.
        let key = match aux.inputs.first().map(Buffer::as_slice_immutable) {
            Some(channels) if !channels.is_empty() && channels[0].len() >= n => Some((
                &*channels[0],
                channels.get(1).map_or(&*channels[0], |c| &**c),
            )),
            _ => None,
        };
        for si in 0..MAX_STAGES {
            let in_use = self.params.stage(si).place.in_use.value();
            let Some(stage) = self.pool.stage_mut(si) else {
                continue;
            };
            stage.keyed = false;
            if let Some((left, right)) = key
                && in_use
                && stage.key_left.len() >= n
            {
                for i in 0..n {
                    stage.key_left[i] = f64::from(left[i]);
                    stage.key_right[i] = f64::from(right.get(i).copied().unwrap_or(0.0));
                }
                stage.keyed = true;
            }
        }

        // The stack: each stage runs its model and its output gain.
        self.pool
            .process(&mut self.left_buf[..n], &mut self.right_buf[..n]);

        // Write back to f32 buffer, metering + FFT
        let mut output_peak: f32 = 0.0;
        for (i, mut frame) in buffer.iter_samples().enumerate() {
            let l = self.left_buf[i];
            let r = self.right_buf[i];

            *frame.get_mut(0).unwrap() = l as f32;
            *frame.get_mut(1).unwrap() = r as f32;

            output_peak = output_peak.max(l.abs().max(r.abs()) as f32);

            // Capture post-EQ mono for the analyzer.
            self.post_mono[i] = ((l + r) * 0.5) as f32;
        }

        // Publish each band's live dynamic gain so the inspector's ring can
        // show what the band is actually doing — the focused stage's bands,
        // since those are the ones on screen. `live_dyn_gain_db` returns
        // `None` for a band that is not dynamic, which reads as 0.0 — the
        // ring draws no live arc at that value.
        if let Some(stage) = self.pool.stage(focused)
            && !stage.uses_neve_1073()
            && !stage.uses_hardware_eq()
        {
            for i in 0..NUM_BANDS {
                let g = stage.engine.live_dyn_gain_db(i).unwrap_or(0.0);
                self.ui_state.band_dyn_gain_db[i].store(g as f32, Ordering::Relaxed);
            }
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

        // Latency follows the slowest lane (`fx.stack.latency`) — non-zero
        // only while a stage has a spectral band in.
        let latency = self.pool.latency();
        if latency != self.last_latency {
            self.last_latency = latency;
            context.set_latency_samples(latency as u32);
        }

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

#[cfg(feature = "export")]
nice_export_clap!(FtsEqPlugin);
#[cfg(feature = "export")]
nice_export_vst3!(FtsEqPlugin);
