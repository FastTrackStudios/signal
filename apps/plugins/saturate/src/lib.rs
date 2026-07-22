//! FTS Saturate — CLAP/VST3 saturation plugin.
//!
//! A thin nice-plug shell over the [`saturate`] facade's [`StereoSaturator`]:
//! drive → curve (tanh/tape/tube/hard) → mix → output trim. The stage is
//! memoryless, so one shared instance serves every channel.
//!
//! GUI is deliberately absent for now (headless, host-generic params),
//! matching level-plugin; the nice-plug-dioxus editor is a follow-up.

use nice_plug::prelude::*;
use std::sync::Arc;

use saturate::{SaturationCurve, StereoSaturator};

const PLUGIN_NAME: &str = "FTS Saturate";

// ── Parameters ────────────────────────────────────────────────────────────

/// The transfer-curve selector, mirrored onto [`SaturationCurve`].
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveParam {
    #[name = "Tanh"]
    Tanh,
    #[name = "Tape"]
    Tape,
    #[name = "Tube"]
    Tube,
    #[name = "Hard"]
    Hard,
}

impl From<CurveParam> for SaturationCurve {
    fn from(value: CurveParam) -> Self {
        match value {
            CurveParam::Tanh => SaturationCurve::Tanh,
            CurveParam::Tape => SaturationCurve::Tape,
            CurveParam::Tube => SaturationCurve::Tube,
            CurveParam::Hard => SaturationCurve::Hard,
        }
    }
}

#[derive(Params)]
pub struct SaturateParams {
    /// Drive amount (0 = clean, 1 = heavy; 1x–8x internal pre-gain).
    #[id = "drive"]
    pub drive: FloatParam,
    /// Transfer curve family.
    #[id = "curve"]
    pub curve: EnumParam<CurveParam>,
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
            drive: FloatParam::new("Drive", 0.3, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_unit("%"),
            curve: EnumParam::new("Curve", CurveParam::Tanh),
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
    saturator: StereoSaturator,
}

impl Default for FtsSaturate {
    fn default() -> Self {
        Self {
            params: Arc::new(SaturateParams::default()),
            saturator: StereoSaturator::new(),
        }
    }
}

impl FtsSaturate {
    fn sync_params(&mut self) {
        self.saturator.set_drive(self.params.drive.value());
        self.saturator.set_curve(self.params.curve.value().into());
        self.saturator.set_mix(self.params.mix.value());
        self.saturator.set_output_db(self.params.output_db.value());
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

    fn reset(&mut self) {
        self.saturator.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync_params();

        for mut frame in buffer.iter_samples() {
            for sample in frame.iter_mut() {
                *sample = self.saturator.process_sample(*sample);
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
