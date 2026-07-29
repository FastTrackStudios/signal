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
//! GUI is deliberately absent for now (headless, host-generic params),
//! matching `signal-sampler-clap`; the nice-plug-dioxus editor is a follow-up.

use nice_plug::prelude::*;
use std::sync::Arc;

use comp::limiter::{sin_clip, GoldenClip};

const PLUGIN_NAME: &str = "FTS Limiter";

// ── Parameters ────────────────────────────────────────────────────────────

#[derive(Params)]
pub struct LimiterParams {
    /// Gain into the limiter — drives the signal against the ceiling.
    #[id = "in_gain"]
    pub input_gain: FloatParam,
    /// Output ceiling; no sample exceeds this level.
    #[id = "ceiling"]
    pub ceiling: FloatParam,
    /// Gain-reduction recovery time (attack is instantaneous / brickwall).
    #[id = "release"]
    pub release_ms: FloatParam,
    /// Ceiling-stage morph: 0 = transparent hard clip (ADClip-style golden
    /// ratio), 1 = full ClipSoftly sine shaping (rounder, adds harmonics).
    #[id = "character"]
    pub character: FloatParam,
    /// True-peak mode: the gain computer detects INTER-SAMPLE peaks
    /// (4x oversampled estimate), so the ceiling holds in dBTP terms —
    /// what streaming loudness normalization actually measures.
    #[id = "true_peak"]
    pub true_peak: BoolParam,
}

impl Default for LimiterParams {
    fn default() -> Self {
        Self {
            input_gain: FloatParam::new(
                "Input",
                0.0,
                FloatRange::Linear { min: -12.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            ceiling: FloatParam::new(
                "Ceiling",
                -0.3,
                FloatRange::Linear { min: -20.0, max: 0.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            release_ms: FloatParam::new(
                "Release",
                100.0,
                FloatRange::Skewed { min: 5.0, max: 500.0, factor: 0.5 },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            character: FloatParam::new(
                "Character",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_unit("%"),
            true_peak: BoolParam::new("True Peak", true),
        }
    }
}

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
    channels: Vec<Channel>,
    sample_rate: f64,
}

impl Default for FtsLimiter {
    fn default() -> Self {
        Self {
            params: Arc::new(LimiterParams::default()),
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

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        let ch = audio_io_layout
            .main_output_channels
            .map(|n| n.get() as usize)
            .unwrap_or(2)
            .max(1);
        self.channels = (0..ch).map(|_| Channel::new()).collect();
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

        for mut frame in buffer.iter_samples() {
            for (c, sample) in frame.iter_mut().enumerate() {
                if let Some(ch) = self.channels.get_mut(c) {
                    let normalized = *sample as f64 * in_gain * inv_ceiling;
                    *sample =
                        (ch.tick(normalized, release_coeff, character, true_peak) * ceiling)
                            as f32;
                }
            }
        }
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
