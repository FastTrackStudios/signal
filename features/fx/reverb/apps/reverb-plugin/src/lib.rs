//! FTS Reverb — nih-plug entry point.
//!
//! Comprehensive reverb plugin with 12 algorithm types,
//! each with multiple sub-type variants:
//!   Room (Medium, Chamber, Studio), Hall (Concert, Cathedral, Arena),
//!   Plate (Dattorro, Lexicon, Progenitor), Spring (Classic, Vintage),
//!   Cloud, Bloom, Shimmer, Chorale, Magneto, Non-Linear, Swell, Reflections.

use fts_dsp::Processor;
use nih_plug::prelude::*;
use reverb_dsp::{AlgorithmType, ReverbChain};
use std::sync::Arc;

struct FtsReverb {
    params: Arc<FtsReverbParams>,
    chain: ReverbChain,
    sample_rate: f64,
}

#[derive(Params)]
struct FtsReverbParams {
    // Algorithm selector
    #[id = "algorithm"]
    pub algorithm: IntParam,

    // Per-algorithm variant selectors (each remembers its own sub-type)
    #[id = "room_variant"]
    pub room_variant: IntParam,

    #[id = "hall_variant"]
    pub hall_variant: IntParam,

    #[id = "plate_variant"]
    pub plate_variant: IntParam,

    #[id = "spring_variant"]
    pub spring_variant: IntParam,

    // Shared controls
    #[id = "decay"]
    pub decay: FloatParam,

    #[id = "size"]
    pub size: FloatParam,

    #[id = "predelay"]
    pub predelay: FloatParam,

    #[id = "diffusion"]
    pub diffusion: FloatParam,

    #[id = "damping"]
    pub damping: FloatParam,

    #[id = "modulation"]
    pub modulation: FloatParam,

    #[id = "tone"]
    pub tone: FloatParam,

    #[id = "mix"]
    pub mix: FloatParam,

    #[id = "width"]
    pub width: FloatParam,

    // Input conditioning
    #[id = "input_hp"]
    pub input_hp: FloatParam,

    #[id = "input_lp"]
    pub input_lp: FloatParam,

    // Algorithm-specific
    #[id = "extra_a"]
    pub extra_a: FloatParam,

    #[id = "extra_b"]
    pub extra_b: FloatParam,
}

impl Default for FtsReverbParams {
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

            room_variant: IntParam::new(
                "Room Type",
                0,
                IntRange::Linear {
                    min: 0,
                    max: (AlgorithmType::Room.variant_count() - 1) as i32,
                },
            )
            .with_value_to_string(Arc::new(|v| {
                AlgorithmType::Room.variant_name(v as usize).to_string()
            })),

            hall_variant: IntParam::new(
                "Hall Type",
                0,
                IntRange::Linear {
                    min: 0,
                    max: (AlgorithmType::Hall.variant_count() - 1) as i32,
                },
            )
            .with_value_to_string(Arc::new(|v| {
                AlgorithmType::Hall.variant_name(v as usize).to_string()
            })),

            plate_variant: IntParam::new(
                "Plate Type",
                0,
                IntRange::Linear {
                    min: 0,
                    max: (AlgorithmType::Plate.variant_count() - 1) as i32,
                },
            )
            .with_value_to_string(Arc::new(|v| {
                AlgorithmType::Plate.variant_name(v as usize).to_string()
            })),

            spring_variant: IntParam::new(
                "Spring Type",
                0,
                IntRange::Linear {
                    min: 0,
                    max: (AlgorithmType::Spring.variant_count() - 1) as i32,
                },
            )
            .with_value_to_string(Arc::new(|v| {
                AlgorithmType::Spring.variant_name(v as usize).to_string()
            })),

            decay: FloatParam::new("Decay", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" ")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            size: FloatParam::new("Size", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" ")
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
            .with_unit(" ms"),

            diffusion: FloatParam::new("Diffusion", 0.7, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" ")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            damping: FloatParam::new("Damping", 0.3, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" ")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            modulation: FloatParam::new("Mod", 0.2, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" ")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            tone: FloatParam::new(
                "Tone",
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            ),

            mix: FloatParam::new("Mix", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" ")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            width: FloatParam::new("Width", 1.0, FloatRange::Linear { min: 0.0, max: 2.0 }),

            input_hp: FloatParam::new(
                "Input HP",
                20.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 2000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz"),

            input_lp: FloatParam::new(
                "Input LP",
                20000.0,
                FloatRange::Skewed {
                    min: 1000.0,
                    max: 20000.0,
                    factor: FloatRange::skew_factor(2.0),
                },
            )
            .with_unit(" Hz"),

            extra_a: FloatParam::new("Extra A", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" ")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            extra_b: FloatParam::new("Extra B", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" ")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
        }
    }
}

impl Default for FtsReverb {
    fn default() -> Self {
        Self {
            params: Arc::new(FtsReverbParams::default()),
            chain: ReverbChain::new(),
            sample_rate: 48000.0,
        }
    }
}

impl FtsReverb {
    fn sync_params(&mut self) {
        // Algorithm + per-algorithm variant selection
        let algo = AlgorithmType::from_index(self.params.algorithm.value() as usize);
        let variant = match algo {
            AlgorithmType::Room => self.params.room_variant.value() as usize,
            AlgorithmType::Hall => self.params.hall_variant.value() as usize,
            AlgorithmType::Plate => self.params.plate_variant.value() as usize,
            AlgorithmType::Spring => self.params.spring_variant.value() as usize,
            _ => 0,
        };
        self.chain.set_algorithm_variant(algo, variant);

        // Shared params
        self.chain.params.decay = self.params.decay.value() as f64;
        self.chain.params.size = self.params.size.value() as f64;
        self.chain.params.diffusion = self.params.diffusion.value() as f64;
        self.chain.params.damping = self.params.damping.value() as f64;
        self.chain.params.modulation = self.params.modulation.value() as f64;
        self.chain.params.tone = self.params.tone.value() as f64;
        self.chain.params.extra_a = self.params.extra_a.value() as f64;
        self.chain.params.extra_b = self.params.extra_b.value() as f64;

        self.chain.predelay_ms = self.params.predelay.value() as f64;
        self.chain.mix = self.params.mix.value() as f64;
        self.chain.width = self.params.width.value() as f64;
        self.chain.input_hp_freq = self.params.input_hp.value() as f64;
        self.chain.input_lp_freq = self.params.input_lp.value() as f64;

        self.chain.update_params();
    }
}

impl Plugin for FtsReverb {
    const NAME: &'static str = "FTS Reverb";
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

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
        self.chain.update(fts_dsp::AudioConfig {
            sample_rate: self.sample_rate,
            max_buffer_size: buffer_config.max_buffer_size as usize,
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
        self.sync_params();

        const CHUNK: usize = 128;
        let num_samples = buffer.samples();
        let mut offset = 0;

        while offset < num_samples {
            let len = (num_samples - offset).min(CHUNK);

            let mut left_f64 = [0.0f64; CHUNK];
            let mut right_f64 = [0.0f64; CHUNK];

            // Convert f32 → f64
            let channel_slices = buffer.as_slice();
            for i in 0..len {
                left_f64[i] = channel_slices[0][offset + i] as f64;
                right_f64[i] = channel_slices[1][offset + i] as f64;
            }

            // Process
            self.chain
                .process(&mut left_f64[..len], &mut right_f64[..len]);

            // Convert f64 → f32
            let channel_slices = buffer.as_slice();
            for i in 0..len {
                channel_slices[0][offset + i] = left_f64[i] as f32;
                channel_slices[1][offset + i] = right_f64[i] as f32;
            }

            offset += len;
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsReverb {
    const CLAP_ID: &'static str = "com.fasttrackstudio.reverb";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Comprehensive reverb with 12 algorithm types and sub-variants");
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

nih_export_clap!(FtsReverb);
nih_export_vst3!(FtsReverb);
