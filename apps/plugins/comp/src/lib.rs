//! FTS Comp — CLAP/VST3 compressor plugin.
//!
//! A thin nice-plug shell over the [`comp`] engine's stereo chain
//! ([`comp::CompChain`]: linked detector → gain curve → hermite-smoothed gain
//! reduction → makeup → parallel mix). The classic parameter set only —
//! threshold / ratio / attack / release / knee / makeup / mix, plus the
//! chain's stereo-link amount. The engine's extended surface (styles,
//! character/drive, expander, upward comp, sidechain EQ, lookahead,
//! multiband) is deliberately not exposed here.
//!
//! Detection is stereo linked by default: `CompChain` feeds both channels a
//! max-linked key (blended by `channel_link`), while gain smoothing and
//! metering stay per channel inside the shared `ProC3Compressor` core.
//!
//! GUI is deliberately absent for now (headless, host-generic params),
//! matching `level-plugin`; the nice-plug-dioxus editor is a follow-up.

use nice_plug::prelude::*;
use std::sync::Arc;

use comp::CompChain;

const PLUGIN_NAME: &str = "FTS Comp";

// ── Parameters ────────────────────────────────────────────────────────────

#[derive(Params)]
pub struct CompParams {
    /// Level above which compression starts.
    #[id = "threshold"]
    pub threshold_db: FloatParam,
    /// Compression ratio (1:1 = off).
    #[id = "ratio"]
    pub ratio: FloatParam,
    /// Attack time.
    #[id = "attack"]
    pub attack_ms: FloatParam,
    /// Release time.
    #[id = "release"]
    pub release_ms: FloatParam,
    /// Soft-knee width around the threshold (0 = hard knee).
    #[id = "knee"]
    pub knee_db: FloatParam,
    /// Makeup (output) gain applied after compression.
    #[id = "makeup"]
    pub makeup_db: FloatParam,
    /// Parallel (dry/wet) mix — the engine's `fold` parameter.
    #[id = "mix"]
    pub mix: FloatParam,
    /// Stereo detector link (1 = fully linked max of both channels).
    #[id = "link"]
    pub stereo_link: FloatParam,
}

impl Default for CompParams {
    fn default() -> Self {
        Self {
            threshold_db: FloatParam::new(
                "Threshold",
                -20.0,
                FloatRange::Linear { min: -60.0, max: 0.0 },
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
                FloatRange::Linear { min: 0.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            makeup_db: FloatParam::new(
                "Makeup",
                0.0,
                FloatRange::Linear { min: -24.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
            stereo_link: FloatParam::new(
                "Stereo Link",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0)),
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsComp {
    params: Arc<CompParams>,
    /// One stereo chain: linked detection, per-channel gain state inside.
    chain: CompChain,
    sample_rate: f64,
}

impl Default for FtsComp {
    fn default() -> Self {
        Self {
            params: Arc::new(CompParams::default()),
            chain: CompChain::new(),
            sample_rate: 48_000.0,
        }
    }
}

impl FtsComp {
    /// Push the current params into the chain (no allocation).
    ///
    /// The core setters propagate into the gain curve and only reset the
    /// hermite smoother when a value actually changed, so this is safe to
    /// call once per block.
    fn sync_params(&mut self) {
        let c = &mut self.chain.comp;
        c.set_threshold(self.params.threshold_db.value() as f64);
        c.set_ratio(self.params.ratio.value() as f64);
        c.set_attack_ms(self.params.attack_ms.value() as f64);
        c.set_release_ms(self.params.release_ms.value() as f64);
        c.set_knee(self.params.knee_db.value() as f64);
        c.set_fold(self.params.mix.value() as f64);
        c.output_gain_db = self.params.makeup_db.value() as f64;
        c.channel_link = self.params.stereo_link.value() as f64;
        c.update(self.sample_rate);
    }
}

impl Plugin for FtsComp {
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
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        self.chain.update_sample_rate(self.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.chain.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync_params();

        for mut frame in buffer.iter_samples() {
            let mut it = frame.iter_mut();
            let (Some(l), r) = (it.next(), it.next()) else {
                continue;
            };
            let mut left = *l as f64;
            // Mono buses: feed the single channel to both sides of the chain
            // (the linked detector then behaves as plain mono detection).
            let mut right = r.as_ref().map(|s| **s as f64).unwrap_or(left);
            self.chain.process_sample(&mut left, &mut right);
            *l = left as f32;
            if let Some(r) = r {
                *r = right as f32;
            }
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsComp {
    const CLAP_ID: &'static str = "com.fasttrackstudio.comp";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Compressor: threshold, ratio, attack, release, knee, makeup, and parallel mix");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Compressor,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsComp {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsCompPlugin001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nice_export_clap!(FtsComp);
nice_export_vst3!(FtsComp);
