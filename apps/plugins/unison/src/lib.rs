//! FTS Unison — CLAP/VST3 audio-domain unison.
//!
//! Simulated double-tracking and beyond: 2–8 pitch-shifted voices with
//! symmetric detune, constant-power stereo spread, and per-voice
//! decorrelation delays around the dry center — the
//! [`pitch_dsp::unison::UnisonEngine`] that also serves as signal's
//! synth unison. Two voices at full spread is the classic studio
//! doubler; stack more for ensemble thickness.

use nice_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::Arc;

use pitch_dsp::chain::Algorithm;
use pitch_dsp::unison::{UnisonEngine, MAX_VOICES};

const PLUGIN_NAME: &str = "FTS Unison";

#[derive(Params)]
pub struct UnisonParams {
    /// Voice count (2..8).
    #[id = "voices"]
    pub voices: IntParam,
    /// Detune span (± cents).
    #[id = "detune"]
    pub detune: FloatParam,
    /// Stereo spread.
    #[id = "spread"]
    pub spread: FloatParam,
    /// Base decorrelation delay (ms).
    #[id = "delay"]
    pub delay_ms: FloatParam,
    /// Dry (center) level.
    #[id = "dry"]
    pub dry: FloatParam,
    /// Voices level.
    #[id = "wet"]
    pub wet: FloatParam,
    /// Shifting engine.
    #[id = "algo"]
    pub algo: IntParam,
    /// Output trim.
    #[id = "output"]
    pub output_db: FloatParam,
}

impl Default for UnisonParams {
    fn default() -> Self {
        Self {
            voices: IntParam::new(
                "Voices",
                2,
                IntRange::Linear { min: 2, max: MAX_VOICES as i32 },
            ),
            detune: FloatParam::new(
                "Detune",
                12.0,
                FloatRange::Linear { min: 0.0, max: 50.0 },
            )
            .with_unit(" ct")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            spread: FloatParam::new("Spread", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_unit("%"),
            delay_ms: FloatParam::new(
                "Delay",
                18.0,
                FloatRange::Linear { min: 0.0, max: 40.0 },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            dry: FloatParam::new("Dry", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_unit("%"),
            wet: FloatParam::new("Voices Mix", 0.8, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_unit("%"),
            algo: IntParam::new("Engine", 0, IntRange::Linear { min: 0, max: 2 })
                .with_value_to_string(Arc::new(|v| {
                    match v {
                        1 => "WSOLA",
                        2 => "Granular",
                        _ => "PSOLA",
                    }
                    .to_string()
                })),
            output_db: FloatParam::new(
                "Output",
                0.0,
                FloatRange::Linear { min: -24.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
        }
    }
}

pub struct FtsUnison {
    params: Arc<UnisonParams>,
    engine: UnisonEngine,
    left: Vec<f64>,
    right: Vec<f64>,
}

impl Default for FtsUnison {
    fn default() -> Self {
        Self {
            params: Arc::new(UnisonParams::default()),
            engine: UnisonEngine::new(),
            left: Vec::new(),
            right: Vec::new(),
        }
    }
}

impl Plugin for FtsUnison {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    // No editor yet — the host shows its generic parameter UI.
    type Editor = ();
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.engine.prepare(
            buffer_config.sample_rate as f64,
            buffer_config.max_buffer_size as usize,
        );
        let cap = buffer_config.max_buffer_size as usize;
        self.left = vec![0.0; cap];
        self.right = vec![0.0; cap];
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let n = buffer.samples();
        if n == 0 || self.left.len() < n {
            return ProcessStatus::Normal;
        }
        self.engine.config.voices = self.params.voices.value() as usize;
        self.engine.config.detune_cents = self.params.detune.value() as f64;
        self.engine.config.spread = self.params.spread.value() as f64;
        self.engine.config.delay_ms = self.params.delay_ms.value() as f64;
        self.engine.config.dry_level = self.params.dry.value() as f64;
        self.engine.config.wet_level = self.params.wet.value() as f64;
        self.engine.config.algorithm = match self.params.algo.value() {
            1 => Algorithm::Wsola,
            2 => Algorithm::Granular,
            _ => Algorithm::Psola,
        };
        self.engine.update_voicing();
        let out_gain = util::db_to_gain(self.params.output_db.value()) as f64;

        for (i, frame) in buffer.iter_samples().enumerate() {
            let mut it = frame.into_iter();
            let l = it.next().map(|s| *s as f64).unwrap_or(0.0);
            let r = it.next().map(|s| *s as f64).unwrap_or(l);
            self.left[i] = l;
            self.right[i] = r;
        }
        self.engine.process(&mut self.left[..n], &mut self.right[..n]);
        for (i, mut frame) in buffer.iter_samples().enumerate() {
            let mut it = frame.iter_mut();
            if let Some(s) = it.next() {
                *s = (self.left[i] * out_gain) as f32;
            }
            if let Some(s) = it.next() {
                *s = (self.right[i] * out_gain) as f32;
            }
        }
        context.set_latency_samples(self.engine.latency() as u32);
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsUnison {
    const CLAP_ID: &'static str = "com.fasttrackstudio.unison";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Audio-domain unison: 2-8 detuned voices, spread + decorrelation — double-tracking to ensemble");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Chorus,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsUnison {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsUnisonPlug001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nice_export_clap!(FtsUnison);
nice_export_vst3!(FtsUnison);
