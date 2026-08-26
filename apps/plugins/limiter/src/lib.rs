//! FTS Limiter — CLAP/VST3 brickwall limiter plugin.
//!
//! A thin nice-plug shell over the [`comp::limiter`] engine surface: input
//! gain drives the signal into a brickwall gain computer (instant attack,
//! program-release), and a Character-blended ceiling stage — golden-ratio hard
//! clip ([`comp::limiter::GoldenClip`], ClipOnly2/ADClip8 lineage) morphing
//! into the ClipSoftly sine waveshaper ([`comp::limiter::sin_clip`]) —
//! guarantees no sample ever exceeds the ceiling.
//!
//! Once `limiter-dsp`'s `LimiterChain` (AdClip → ClipSoftly → BlockParty →
//! Loud) is implemented and re-exported by the `comp` facade, the per-channel
//! engine below moves behind it; the param surface here is designed to map
//! onto that chain (Input → drive, Ceiling → output ceiling, Release →
//! BlockParty release, Character → clip-stage morph).
//!
//! Params + shared UI state live in [`limiter_ui::params`] (like `comp-ui`),
//! so the Dioxus editor ([`limiter_ui::control_view::App`]) renders against
//! them without a circular dep.

use nice_plug::prelude::*;
use nice_plug_dioxus::{create_dioxus_editor_with_state, DioxusState};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use comp::limiter::{sin_clip, GoldenClip};
use limiter_ui::params::{LimiterParams, LimiterUiState};

const PLUGIN_NAME: &str = "FTS Limiter";

/// 4-point Catmull-Rom inter-sample peak estimate over the last four
/// input samples — a cheap, allocation-free stand-in for full 4x
/// polyphase upsampling that catches the overwhelming majority of ISPs
/// (the same estimator family meter-dsp's TruePeakDetector uses).
#[derive(Default)]
struct IspEstimator {
    h: [f64; 4],
}

impl IspEstimator {
    #[inline]
    fn push(&mut self, x: f64) -> f64 {
        self.h.rotate_left(1);
        self.h[3] = x;
        let [a, b, c, d] = self.h;
        // Peak of |interpolated curve| between b and c at t = ¼, ½, ¾.
        let mut peak = b.abs().max(c.abs());
        for &t in &[0.25f64, 0.5, 0.75] {
            let t2 = t * t;
            let t3 = t2 * t;
            let v = 0.5
                * ((2.0 * b)
                    + (-a + c) * t
                    + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
                    + (-a + 3.0 * b - 3.0 * c + d) * t3);
            peak = peak.max(v.abs());
        }
        peak
    }

    fn reset(&mut self) {
        self.h = [0.0; 4];
    }
}

// ── Per-channel engine ────────────────────────────────────────────────────

/// One limiter lane: gain-reduction envelope + stateful hard-clip stage.
struct Channel {
    /// Current gain-reduction multiplier (1.0 = no reduction).
    envelope: f64,
    /// Golden-ratio interpolated hard clip (per-channel instance, lane 0).
    clip: GoldenClip,
    /// Inter-sample peak estimator (true-peak mode).
    isp: IspEstimator,
}

impl Channel {
    fn new() -> Self {
        Self {
            envelope: 1.0,
            clip: GoldenClip::new(),
            isp: IspEstimator::default(),
        }
    }

    fn reset(&mut self) {
        self.envelope = 1.0;
        self.clip.reset();
        self.isp.reset();
    }

    /// Process one sample already normalized to the ceiling domain
    /// (|1.0| == ceiling). Returns a value guaranteed within ±1.0.
    #[inline]
    fn tick(
        &mut self,
        normalized: f64,
        release_coeff: f64,
        character: f64,
        true_peak: bool,
    ) -> f64 {
        // Brickwall gain computer: instant attack, smoothed release.
        // True-peak mode drives it with the inter-sample peak estimate
        // so the ceiling holds between samples too.
        let level = if true_peak {
            self.isp.push(normalized)
        } else {
            normalized.abs()
        };
        let target = if level > 1.0 { 1.0 / level } else { 1.0 };
        if target < self.envelope {
            self.envelope = target;
        } else {
            self.envelope = target + (self.envelope - target) * release_coeff;
        }
        let limited = normalized * self.envelope;

        // Safety/character ceiling stage in the unity domain.
        let hard = self.clip.tick(limited, 0);
        let soft = sin_clip(limited);
        hard + (soft - hard) * character
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsLimiter {
    params: Arc<LimiterParams>,
    /// One lane per channel (linked-free, mono detection each).
    ui_state: Arc<LimiterUiState>,
    editor_state: Arc<DioxusState>,
    channels: Vec<Channel>,
    sample_rate: f64,
}

impl Default for FtsLimiter {
    fn default() -> Self {
        let params = Arc::new(LimiterParams::default());
        let ui_state = Arc::new(LimiterUiState::new(params.clone()));
        Self {
            params,
            ui_state,
            editor_state: DioxusState::new(|| {
                (
                    limiter_ui::control_view::EDITOR_W,
                    limiter_ui::control_view::EDITOR_H,
                )
            })
            .with_resize_hint(limiter_ui::control_view::resize_hint()),
            channels: Vec::new(),
            sample_rate: 48_000.0,
        }
    }
}

impl Plugin for FtsLimiter {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Audio effect: stereo in, stereo out.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    type Editor = nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            self.ui_state.clone(),
            limiter_ui::control_view::App,
        )
    }

    fn activate(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        let ch = audio_io_layout
            .main_output_channels
            .map(|n| n.get() as usize)
            .unwrap_or(2)
            .max(1);
        self.channels = (0..ch).map(|_| Channel::new()).collect();
        self.ui_state
            .sample_rate
            .store(buffer_config.sample_rate as u32, Ordering::Relaxed);
        true
    }

    fn reset(&mut self) {
        for ch in &mut self.channels {
            ch.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        if self.channels.is_empty() {
            return ProcessStatus::Normal;
        }

        // Per-block param snapshot (no allocation on the hot path).
        let in_gain = util::db_to_gain(self.params.input_gain.value()) as f64;
        let ceiling = util::db_to_gain(self.params.ceiling.value()) as f64;
        let character = self.params.character.value() as f64;
        let release_s = (self.params.release_ms.value() as f64 / 1_000.0).max(1e-4);
        let true_peak = self.params.true_peak.value();
        // One-pole release: per-sample coefficient toward full recovery.
        let release_coeff = (-1.0 / (self.sample_rate * release_s)).exp();
        let inv_ceiling = 1.0 / ceiling.max(1e-6);

        let mut input_peak: f32 = 0.0;
        let mut output_peak: f32 = 0.0;
        // Deepest reduction in the block: the envelope is a gain multiplier
        // (1.0 = untouched), so the *smallest* envelope is the most reduction.
        let mut min_envelope: f64 = 1.0;

        for mut frame in buffer.iter_samples() {
            for (c, sample) in frame.iter_mut().enumerate() {
                if let Some(ch) = self.channels.get_mut(c) {
                    input_peak = input_peak.max(sample.abs());
                    let normalized = *sample as f64 * in_gain * inv_ceiling;
                    *sample =
                        (ch.tick(normalized, release_coeff, character, true_peak) * ceiling) as f32;
                    output_peak = output_peak.max(sample.abs());
                    min_envelope = min_envelope.min(ch.envelope);
                }
            }
        }

        // ── UI feeds (lock-free atomics; no allocation) ──────────────────
        // GR is reported positive-going, like every other dynamics meter in
        // the suite: 0 dB = not limiting.
        let gr_db = if min_envelope > 0.0 {
            (-20.0 * min_envelope.log10()) as f32
        } else {
            0.0
        };
        self.ui_state
            .gain_reduction_db
            .store(gr_db, Ordering::Relaxed);
        self.ui_state.gr_wave.push(gr_db);
        self.ui_state.output_wave.push(output_peak);
        self.ui_state.input.push_peak(input_peak);
        self.ui_state.output.push_peak(output_peak);

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsLimiter {
    const CLAP_ID: &'static str = "com.fasttrackstudio.limiter";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Brickwall limiter: instant-attack peak limiting with a hard/soft ceiling character");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Limiter,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsLimiter {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsLimiterPlg001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nice_export_clap!(FtsLimiter);
nice_export_vst3!(FtsLimiter);
