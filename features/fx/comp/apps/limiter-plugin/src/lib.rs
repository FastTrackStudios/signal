//! FTS Limiter — nih-plug entry point.

use nih_plug::prelude::*;

struct FtsLimiter {
    params: std::sync::Arc<FtsLimiterParams>,
    // limiter_chain: limiter_dsp::LimiterChain,
}

#[derive(Params)]
struct FtsLimiterParams {
    // TODO: Define nih-plug params that bridge to limiter-dsp + limiter-profiles
}

impl Default for FtsLimiter {
    fn default() -> Self {
        Self {
            params: std::sync::Arc::new(FtsLimiterParams {}),
        }
    }
}

impl Plugin for FtsLimiter {
    const NAME: &'static str = "FTS Limiter";
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

    fn params(&self) -> std::sync::Arc<dyn Params> {
        self.params.clone()
    }

    fn process(
        &mut self,
        _buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // TODO: Bridge nih-plug buffer to limiter_dsp::LimiterChain::process
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsLimiter {
    const CLAP_ID: &'static str = "com.fasttrackstudio.limiter";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Limiter with hardware profiles");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Limiter,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsLimiter {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsLimitPlugin01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_clap!(FtsLimiter);
nih_export_vst3!(FtsLimiter);
