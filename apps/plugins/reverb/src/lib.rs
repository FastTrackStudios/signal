//! FTS Reverb — CLAP/VST3 reverb plugin.
//!
//! A thin nice-plug shell over the [`reverb`] engine's realtime chain
//! ([`reverb::ReverbChain`]: input filtering → pre-delay → algorithm → output
//! EQ → width → mix). The chain is stereo-coupled (one engine, two channels),
//! so exactly one [`ReverbChain`] is instantiated for the whole plugin.
//!
//! Params cover the core surface: algorithm select (all 15 engines), decay,
//! size, pre-delay, damping, tone, width, and dry/wet mix. Decay/damping
//! automation is click-free — the chain ramps those coefficients internally
//! (30 ms smoothers); we just push targets via `update_params()` every block.
//! Per-algorithm variants, the BigSky MX per-engine params, convolution IR
//! loading, ducking, and saturation are chain features not yet surfaced here.
//!
//! GUI is deliberately absent for now (headless, host-generic params), matching
//! `signal-sampler-clap`; the nice-plug-dioxus editor is a follow-up.

use nice_plug::prelude::*;
use std::sync::Arc;

use audiocore_dsp::{AudioConfig, Processor};
use reverb::{AlgorithmType, ReverbChain};

const PLUGIN_NAME: &str = "FTS Reverb";

// ── Parameters ────────────────────────────────────────────────────────────

#[derive(Params)]
pub struct ReverbParams {
    /// Algorithm select (Room, Hall, Plate, Spring, … Convolution).
    #[id = "algorithm"]
    pub algorithm: IntParam,
    /// Decay / RT60 control (0 = short, 1 = infinite).
    #[id = "decay"]
    pub decay: FloatParam,
    /// Room / space size (0 = small, 1 = massive).
    #[id = "size"]
    pub size: FloatParam,
    /// Pre-delay before the reverb onset.
    #[id = "predelay"]
    pub predelay: FloatParam,
    /// High-frequency damping (0 = bright, 1 = dark).
    #[id = "damping"]
    pub damping: FloatParam,
    /// Tone control (-1 = dark, 0 = neutral, +1 = bright).
    #[id = "tone"]
    pub tone: FloatParam,
    /// Stereo width of the wet signal (0 = mono, 1 = normal, 2 = extra wide).
    #[id = "width"]
    pub width: FloatParam,
    /// Dry/wet mix (0 = fully dry, 1 = fully wet).
    #[id = "mix"]
    pub mix: FloatParam,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            algorithm: IntParam::new(
                "Algorithm",
                0,
                IntRange::Linear {
                    min: 0,
                    max: (AlgorithmType::ALL.len() - 1) as i32,
                },
            )
            .with_value_to_string(Arc::new(|v| {
                AlgorithmType::from_index(v as usize).name().to_string()
            })),
            decay: FloatParam::new("Decay", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
            size: FloatParam::new("Size", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
            predelay: FloatParam::new(
                "Pre-Delay",
                0.0,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 500.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            damping: FloatParam::new("Damping", 0.3, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
            tone: FloatParam::new(
                "Tone",
                0.0,
                FloatRange::Linear { min: -1.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            width: FloatParam::new("Width", 1.0, FloatRange::Linear { min: 0.0, max: 2.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            mix: FloatParam::new("Mix", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsReverb {
    params: Arc<ReverbParams>,
    /// ONE stereo-coupled engine for the whole plugin (never per channel:
    /// the chain's width/mix/algorithm state spans both channels).
    chain: ReverbChain,
    /// Pre-allocated f64 scratch (the chain processes f64 planar stereo).
    /// Sized to the host's max buffer in `initialize`; never grown in
    /// `process`.
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
    sample_rate: f64,
}

impl Default for FtsReverb {
    fn default() -> Self {
        Self {
            params: Arc::new(ReverbParams::default()),
            chain: ReverbChain::new(),
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
            sample_rate: 48_000.0,
        }
    }
}

impl FtsReverb {
    /// Push the current params into the chain (no allocation on the steady
    /// path; `set_algorithm` rebuilds engine state only when the selector
    /// actually changes). Decay/damping ramp inside the chain's smoothers.
    fn sync_params(&mut self) {
        let algo = AlgorithmType::from_index(self.params.algorithm.value() as usize);
        self.chain.set_algorithm(algo);

        self.chain.params.decay = self.params.decay.value() as f64;
        self.chain.params.size = self.params.size.value() as f64;
        self.chain.params.damping = self.params.damping.value() as f64;
        self.chain.params.tone = self.params.tone.value() as f64;

        self.chain.predelay_ms = self.params.predelay.value() as f64;
        self.chain.width = self.params.width.value() as f64;
        self.chain.mix = self.params.mix.value() as f64;

        self.chain.update_params();
    }
}

impl Plugin for FtsReverb {
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
        let max = (buffer_config.max_buffer_size as usize).max(1);
        self.scratch_l = vec![0.0; max];
        self.scratch_r = vec![0.0; max];
        // Land the params before the reconfigure so `update()` snaps the
        // chain's smoothers onto the real values (no ramp from defaults).
        self.sync_params();
        self.chain.update(AudioConfig {
            sample_rate: self.sample_rate,
            max_buffer_size: max,
        });
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
        if buffer.channels() < 2 || self.scratch_l.is_empty() {
            return ProcessStatus::Normal;
        }
        self.sync_params();

        // Process in scratch-sized chunks: f32 interleave → f64 planar →
        // chain → back. No allocation — scratch was sized in `initialize`.
        let chunk = self.scratch_l.len();
        let total = buffer.samples();
        let mut offset = 0;
        while offset < total {
            let len = (total - offset).min(chunk);
            {
                let ch = buffer.as_slice();
                for i in 0..len {
                    self.scratch_l[i] = ch[0][offset + i] as f64;
                    self.scratch_r[i] = ch[1][offset + i] as f64;
                }
            }
            self.chain
                .process(&mut self.scratch_l[..len], &mut self.scratch_r[..len]);
            {
                let ch = buffer.as_slice();
                for i in 0..len {
                    ch[0][offset + i] = self.scratch_l[i] as f32;
                    ch[1][offset + i] = self.scratch_r[i] as f32;
                }
            }
            offset += len;
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsReverb {
    const CLAP_ID: &'static str = "com.fasttrackstudio.reverb";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Reverb with 15 algorithm engines: rooms, halls, plates, springs, shimmer, convolution");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Reverb,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsReverb {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsReverbPlug001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Reverb];
}

nice_export_clap!(FtsReverb);
nice_export_vst3!(FtsReverb);
