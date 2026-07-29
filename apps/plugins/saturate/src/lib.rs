//! FTS Saturate — CLAP/VST3 saturation plugin.
//!
//! Runs the Class-A preamp engine (`saturate_dsp::preamp`) — the same
//! core behind `signal.fx.saturate`/`signal.fx.preamp`: model voicing
//! (Preamp/Tube/Tape/Transformer/Console/Fuzz per-side shaper pairs),
//! Q-point bias (raise it and even-order harmonics spring up), bias
//! sag (tube bloom — program-dependent asymmetry), and a 1073-style
//! output DC blocker. The legacy 4-curve Custom mode maps onto the
//! same per-side shapers, so old presets keep their sound.
//!
//! GUI is deliberately absent for now (headless, host-generic params);
//! the big-sine + harmonic-bars editor is a follow-up (the analysis
//! probes in saturate-dsp are ready for it).

use nice_plug::prelude::*;
use std::sync::Arc;

use saturate::preamp::{ClassAPreamp, SideShaper};

const PLUGIN_NAME: &str = "FTS Saturate";

// ── Parameters ────────────────────────────────────────────────────────────

/// Saturation model — voices the per-side shapers, Q point, and sag
/// (mirrors NativeSaturate's model table).
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelParam {
    #[name = "Preamp"]
    Preamp,
    #[name = "Tube"]
    Tube,
    #[name = "Tape"]
    Tape,
    #[name = "Transformer"]
    Transformer,
    #[name = "Console"]
    Console,
    #[name = "Fuzz"]
    Fuzz,
}

impl ModelParam {
    /// (positive shaper, negative shaper, q_point, sag).
    fn voicing(self) -> (SideShaper, SideShaper, f32, f32) {
        match self {
            ModelParam::Preamp => (SideShaper::Transformer, SideShaper::Transformer, 0.25, 0.1),
            ModelParam::Tube => (SideShaper::Tube, SideShaper::Tube, 0.15, 0.6),
            ModelParam::Tape => (SideShaper::Transformer, SideShaper::Transformer, 0.0, 0.2),
            ModelParam::Transformer => {
                (SideShaper::Transformer, SideShaper::Transformer, 0.0, 0.0)
            }
            ModelParam::Console => (SideShaper::OpAmp, SideShaper::OpAmp, 0.0, 0.0),
            ModelParam::Fuzz => (SideShaper::Hard, SideShaper::Diode, 0.4, 0.3),
        }
    }
}

#[derive(Params)]
pub struct SaturateParams {
    /// Drive into the stage in dB.
    #[id = "drive"]
    pub drive: FloatParam,
    /// Saturation model (voices shapers + Q + sag).
    #[id = "curve"]
    pub model: EnumParam<ModelParam>,
    /// Q-point trim around the model's voicing (−1..1).
    #[id = "q_point"]
    pub q_point: FloatParam,
    /// Bias sag trim (0..1) — program-dependent bloom.
    #[id = "sag"]
    pub sag: FloatParam,
    /// Dry/wet mix.
    #[id = "mix"]
    pub mix: FloatParam,
    /// Output trim.
    #[id = "output"]
    pub output_db: FloatParam,
}

impl Default for SaturateParams {
    fn default() -> Self {
        Self {
            drive: FloatParam::new(
                "Drive",
                6.0,
                FloatRange::Linear { min: 0.0, max: 36.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            model: EnumParam::new("Model", ModelParam::Preamp),
            q_point: FloatParam::new(
                "Q Point",
                0.0,
                FloatRange::Linear { min: -1.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            sag: FloatParam::new("Sag", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_unit("%"),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_unit("%"),
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

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsSaturate {
    params: Arc<SaturateParams>,
    pre: [ClassAPreamp; 2],
}

impl Default for FtsSaturate {
    fn default() -> Self {
        Self {
            params: Arc::new(SaturateParams::default()),
            pre: [ClassAPreamp::new(48_000.0), ClassAPreamp::new(48_000.0)],
        }
    }
}

impl FtsSaturate {
    fn sync_params(&mut self) {
        let (pos, neg, q_base, sag_base) = self.params.model.value().voicing();
        let drive = 10.0f32.powf(self.params.drive.value() / 20.0);
        let q = (q_base + self.params.q_point.value()).clamp(-1.0, 1.0);
        let sag = (sag_base + self.params.sag.value()).clamp(0.0, 1.0);
        let mix = self.params.mix.value();
        let out = 10.0f32.powf(self.params.output_db.value() / 20.0);
        for p in &mut self.pre {
            p.positive = pos;
            p.negative = neg;
            p.drive = drive;
            p.q_point = q;
            p.sag = sag;
            p.mix = mix;
            p.output_gain = out;
        }
    }
}

impl Plugin for FtsSaturate {
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
        for p in &mut self.pre {
            p.set_sample_rate(buffer_config.sample_rate);
            p.reset();
        }
        true
    }

    fn reset(&mut self) {
        for p in &mut self.pre {
            p.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync_params();

        for mut frame in buffer.iter_samples() {
            for (c, sample) in frame.iter_mut().enumerate() {
                let pre = &mut self.pre[c.min(1)];
                *sample = pre.process(c.min(1), *sample);
            }
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsSaturate {
    const CLAP_ID: &'static str = "com.fasttrackstudio.saturate";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Musical saturation: tanh, tape, tube, and hard-clip curves");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Distortion,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsSaturate {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsSaturatePlg01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nice_export_clap!(FtsSaturate);
nice_export_vst3!(FtsSaturate);
