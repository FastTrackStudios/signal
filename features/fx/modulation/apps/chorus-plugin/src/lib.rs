//! FTS Chorus — nih-plug entry point.
//!
//! Multi-engine stereo chorus/flanger/vibrato with four engine types
//! (Cubic, BBD, Tape, Orbit) and 1–4 voices per channel.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use atomic_float::AtomicF32;
use chorus_dsp::chain::ChorusChain;
use chorus_dsp::engine::{EffectType, EngineType};
use fts_dsp::{AudioConfig, Processor};
use fts_plugin_core::prelude::*;

// ── Parameters ──────────────────────────────────────────────────────

#[derive(Params)]
pub struct FtsChorusParams {
    #[id = "effect_type"]
    pub effect_type: IntParam,
    #[id = "engine"]
    pub engine: IntParam,
    #[id = "rate"]
    pub rate: FloatParam,
    #[id = "depth"]
    pub depth: FloatParam,
    #[id = "feedback"]
    pub feedback: FloatParam,
    #[id = "color"]
    pub color: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "width"]
    pub width: FloatParam,
    #[id = "voices"]
    pub voices: IntParam,
}

impl Default for FtsChorusParams {
    fn default() -> Self {
        Self {
            effect_type: IntParam::new("Type", 0, IntRange::Linear { min: 0, max: 2 })
                .with_value_to_string(Arc::new(|v| match v {
                    0 => "Chorus".to_string(),
                    1 => "Flanger".to_string(),
                    2 => "Vibrato".to_string(),
                    _ => "Chorus".to_string(),
                })),

            engine: IntParam::new("Engine", 0, IntRange::Linear { min: 0, max: 4 })
                .with_value_to_string(Arc::new(|v| match v {
                    0 => "Cubic".to_string(),
                    1 => "BBD".to_string(),
                    2 => "Tape".to_string(),
                    3 => "Orbit".to_string(),
                    4 => "Juno".to_string(),
                    _ => "Cubic".to_string(),
                })),

            rate: FloatParam::new(
                "Rate",
                1.0,
                FloatRange::Skewed {
                    min: 0.01,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            depth: FloatParam::new("Depth", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            feedback: FloatParam::new("Feedback", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            color: FloatParam::new("Color", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            mix: FloatParam::new("Mix", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            width: FloatParam::new("Width", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            voices: IntParam::new("Voices", 2, IntRange::Linear { min: 1, max: 4 }),
        }
    }
}

// ── Plugin ──────────────────────────────────────────────────────────

struct FtsChorus {
    params: Arc<FtsChorusParams>,
    input_peak_db: AtomicF32,
    output_peak_db: AtomicF32,
    chain: ChorusChain,
    sample_rate: f64,
    /// Tracks the current engine index so we only call set_engine on change.
    current_engine: i32,
}

impl Default for FtsChorus {
    fn default() -> Self {
        Self {
            params: Arc::new(FtsChorusParams::default()),
            input_peak_db: AtomicF32::new(-100.0),
            output_peak_db: AtomicF32::new(-100.0),
            chain: ChorusChain::new(),
            sample_rate: 48000.0,
            current_engine: 0,
        }
    }
}

impl FtsChorus {
    fn sync_params(&mut self) {
        let p = &self.params;
        let c = &mut self.chain;

        // Engine switching
        let engine_idx = p.engine.value();
        if engine_idx != self.current_engine {
            self.current_engine = engine_idx;
            let engine = match engine_idx {
                1 => EngineType::Bbd,
                2 => EngineType::Tape,
                3 => EngineType::Orbit,
                4 => EngineType::Juno,
                _ => EngineType::Cubic,
            };
            c.set_engine(engine);
            c.update(AudioConfig {
                sample_rate: self.sample_rate,
                max_buffer_size: 512,
            });
        }

        // Effect type
        c.effect_type = match p.effect_type.value() {
            1 => EffectType::Flanger,
            2 => EffectType::Vibrato,
            _ => EffectType::Chorus,
        };

        // Core parameters
        c.rate_hz = p.rate.value() as f64;
        c.depth = p.depth.value() as f64;
        c.feedback = p.feedback.value() as f64;
        c.color = p.color.value() as f64;
        c.width = p.width.value() as f64;
        c.num_voices = p.voices.value() as usize;

        // Mix — vibrato forces wet-only (DSP already handles this,
        // but we also override the param value for consistency)
        if c.effect_type == EffectType::Vibrato {
            c.mix = 1.0;
        } else {
            c.mix = p.mix.value() as f64;
        }
    }
}

impl Plugin for FtsChorus {
    const NAME: &'static str = "FTS Chorus";
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
        self.chain.update(AudioConfig {
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

        // Process in chunks, converting f32 <-> f64
        const CHUNK: usize = 128;
        let num_samples = buffer.samples();
        let mut offset = 0;
        let mut input_peak: f32 = 0.0;
        let mut output_peak: f32 = 0.0;

        while offset < num_samples {
            let end = (offset + CHUNK).min(num_samples);
            let len = end - offset;

            let mut left_f64 = [0.0f64; CHUNK];
            let mut right_f64 = [0.0f64; CHUNK];

            // f32 -> f64 + measure input
            let channel_slices = buffer.as_slice();
            for i in 0..len {
                let l = channel_slices[0][offset + i];
                let r = channel_slices[1][offset + i];
                input_peak = input_peak.max(l.abs()).max(r.abs());
                left_f64[i] = l as f64;
                right_f64[i] = r as f64;
            }

            // Process
            self.chain
                .process(&mut left_f64[..len], &mut right_f64[..len]);

            // f64 -> f32 + measure output
            let channel_slices = buffer.as_slice();
            for i in 0..len {
                let l = left_f64[i] as f32;
                let r = right_f64[i] as f32;
                output_peak = output_peak.max(l.abs()).max(r.abs());
                channel_slices[0][offset + i] = l;
                channel_slices[1][offset + i] = r;
            }

            offset = end;
        }

        // Update metering with decay
        let in_db = if input_peak > 0.0 {
            20.0 * input_peak.log10()
        } else {
            -100.0
        };
        let out_db = if output_peak > 0.0 {
            20.0 * output_peak.log10()
        } else {
            -100.0
        };

        let prev_in = self.input_peak_db.load(Ordering::Relaxed);
        self.input_peak_db.store(
            if in_db > prev_in {
                in_db
            } else {
                prev_in - 0.3
            },
            Ordering::Relaxed,
        );

        let prev_out = self.output_peak_db.load(Ordering::Relaxed);
        self.output_peak_db.store(
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

impl ClapPlugin for FtsChorus {
    const CLAP_ID: &'static str = "com.fasttrackstudio.chorus";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Multi-engine chorus/flanger/vibrato");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Chorus,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsChorus {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsChorusPlugn01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nih_export_clap!(FtsChorus);
nih_export_vst3!(FtsChorus);
