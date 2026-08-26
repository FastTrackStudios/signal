//! FTS Pitch — CLAP/VST3 pitch shifter.
//!
//! Classic stereo pitch shift (semitones + fine cents) over the
//! `pitch-dsp` engines (PSOLA with the fixed period-recentering,
//! WSOLA, granular). Double-tracking/unison lives in its own plugin —
//! FTS Unison — which drives the same engines through
//! `pitch_dsp::unison::UnisonEngine`.

use nice_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::Arc;

use audiocore_dsp::{AudioConfig, Processor};
use pitch_dsp::chain::{Algorithm, PitchChain};

const PLUGIN_NAME: &str = "FTS Pitch";

#[derive(Params)]
pub struct PitchParams {
    /// Coarse shift in semitones.
    #[id = "semitones"]
    pub semitones: IntParam,
    /// Fine shift in cents.
    #[id = "cents"]
    pub cents: FloatParam,
    /// Shifting algorithm: 0 PSOLA (voice), 1 WSOLA (poly), 2 Granular.
    #[id = "algo"]
    pub algo: IntParam,
    /// Wet mix (shifted vs dry).
    #[id = "mix"]
    pub mix: FloatParam,
    /// Output trim.
    #[id = "output"]
    pub output_db: FloatParam,
}

impl Default for PitchParams {
    fn default() -> Self {
        Self {
            semitones: IntParam::new("Semitones", 0, IntRange::Linear { min: -24, max: 24 }),
            cents: FloatParam::new(
                "Fine",
                0.0,
                FloatRange::Linear {
                    min: -100.0,
                    max: 100.0,
                },
            )
            .with_unit(" ct")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            algo: IntParam::new("Engine", 0, IntRange::Linear { min: 0, max: 2 })
                .with_value_to_string(Arc::new(|v| {
                    match v {
                        1 => "WSOLA",
                        2 => "Granular",
                        _ => "PSOLA",
                    }
                    .to_string()
                })),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_unit("%"),
            output_db: FloatParam::new(
                "Output",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
        }
    }
}

pub struct FtsPitch {
    params: Arc<PitchParams>,
    chain: PitchChain,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
    sample_rate: f64,
}

impl Default for FtsPitch {
    fn default() -> Self {
        let mk = || {
            let mut c = PitchChain::new();
            c.algorithm = Algorithm::Psola;
            c.semitones = 0.0;
            c.mix = 1.0;
            c
        };
        Self {
            params: Arc::new(PitchParams::default()),
            chain: mk(),
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
            sample_rate: 48_000.0,
        }
    }
}

impl Plugin for FtsPitch {
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
        self.sample_rate = buffer_config.sample_rate as f64;
        let cfg = AudioConfig {
            sample_rate: self.sample_rate,
            max_buffer_size: buffer_config.max_buffer_size as usize,
        };
        self.chain.update(cfg);
        let cap = buffer_config.max_buffer_size as usize;
        self.scratch_l = vec![0.0; cap];
        self.scratch_r = vec![0.0; cap];
        true
    }

    fn reset(&mut self) {
        self.chain.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let n = buffer.samples();
        if n == 0 || self.scratch_l.len() < n {
            return ProcessStatus::Normal;
        }
        self.chain.algorithm = match self.params.algo.value() {
            1 => Algorithm::Wsola,
            2 => Algorithm::Granular,
            _ => Algorithm::Psola,
        };
        self.chain.semitones = (self.params.semitones.value() as f64
            + self.params.cents.value() as f64 / 100.0)
            .clamp(-24.0, 24.0);
        self.chain.mix = 1.0;
        let mix = self.params.mix.value() as f64;
        let out_gain = util::db_to_gain(self.params.output_db.value()) as f64;

        for (i, frame) in buffer.iter_samples().enumerate() {
            let mut it = frame.into_iter();
            let l = it.next().map(|s| *s as f64).unwrap_or(0.0);
            let r = it.next().map(|s| *s as f64).unwrap_or(l);
            self.scratch_l[i] = l;
            self.scratch_r[i] = r;
        }
        self.chain
            .process(&mut self.scratch_l[..n], &mut self.scratch_r[..n]);
        for (i, mut frame) in buffer.iter_samples().enumerate() {
            let mut it = frame.iter_mut();
            if let Some(s) = it.next() {
                let dry = *s as f64;
                *s = ((dry + (self.scratch_l[i] - dry) * mix) * out_gain) as f32;
            }
            if let Some(s) = it.next() {
                let dry = *s as f64;
                *s = ((dry + (self.scratch_r[i] - dry) * mix) * out_gain) as f32;
            }
        }
        context.set_latency_samples(self.chain.latency() as u32);
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsPitch {
    const CLAP_ID: &'static str = "com.fasttrackstudio.pitch";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Pitch shifter (PSOLA/WSOLA/granular)");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::PitchShifter,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsPitch {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsPitchPlug0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::PitchShift];
}

nice_export_clap!(FtsPitch);
nice_export_vst3!(FtsPitch);
