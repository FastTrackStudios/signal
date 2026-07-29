//! FTS Meter — CLAP/VST3 metering plugin.
//!
//! Ships the `meter-dsp` + `meter-ui` suite as a product: EBU R128
//! LUFS (momentary / short-term / integrated / range) with dBTP true
//! peak, stereo phase correlation + goniometer, and a log spectrum —
//! painted by the vello `SceneOverlay` painters that were built for
//! exactly this, over a nice-plug-dioxus editor.
//!
//! The audio path is a bit-exact passthrough: analysis taps only.

use nice_plug::prelude::*;
use nice_plug_dioxus::{create_dioxus_editor_with_state, DioxusState};
use std::num::NonZeroU32;
use std::sync::Arc;

use meter_dsp::lufs::{LufsMeter, LufsState};
use meter_dsp::phase::{PhaseCorrelation, PhaseState};
use meter_dsp::spectrum::{SpectrumAnalyzer, SpectrumState};

mod editor;

const PLUGIN_NAME: &str = "FTS Meter";
/// Fixed editor size (the painter rects are laid out against this).
pub const EDITOR_W: u32 = 920;
pub const EDITOR_H: u32 = 540;
const SPECTRUM_FFT: usize = 4096;

/// Shared analysis state the editor reads (all interior-mutable).
pub struct MeterShared {
    pub lufs: Arc<LufsState>,
    pub phase: Arc<PhaseState>,
    pub spectrum: Arc<SpectrumState>,
}

#[derive(Params)]
pub struct MeterParams {}

impl Default for MeterParams {
    fn default() -> Self {
        Self {}
    }
}

pub struct FtsMeter {
    params: Arc<MeterParams>,
    editor_state: Arc<DioxusState>,
    lufs: LufsMeter,
    phase: PhaseCorrelation,
    spectrum: SpectrumAnalyzer,
    shared: Arc<MeterShared>,
    /// Mono scratch for the spectrum feed.
    mono: Vec<f32>,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl Default for FtsMeter {
    fn default() -> Self {
        let lufs = LufsMeter::new(48_000.0);
        let phase = PhaseCorrelation::new(48_000.0);
        let spectrum = SpectrumAnalyzer::new(48_000.0, SPECTRUM_FFT);
        let shared = Arc::new(MeterShared {
            lufs: lufs.state.clone(),
            phase: phase.state.clone(),
            spectrum: spectrum.state.clone(),
        });
        Self {
            params: Arc::new(MeterParams::default()),
            editor_state: DioxusState::new(|| (EDITOR_W, EDITOR_H)),
            lufs,
            phase,
            spectrum,
            shared,
            mono: Vec::new(),
            left: Vec::new(),
            right: Vec::new(),
        }
    }
}

impl Plugin for FtsMeter {
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

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            self.shared.clone(),
            editor::App,
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        let sr = buffer_config.sample_rate;
        // Rebuild the meters at the real rate, re-share their states.
        self.lufs = LufsMeter::new(sr);
        self.phase = PhaseCorrelation::new(sr);
        self.spectrum = SpectrumAnalyzer::new(sr, SPECTRUM_FFT);
        self.shared = Arc::new(MeterShared {
            lufs: self.lufs.state.clone(),
            phase: self.phase.state.clone(),
            spectrum: self.spectrum.state.clone(),
        });
        let cap = buffer_config.max_buffer_size as usize;
        self.mono = vec![0.0; cap];
        self.left = vec![0.0; cap];
        self.right = vec![0.0; cap];
        true
    }

    fn reset(&mut self) {
        self.lufs.reset();
        self.spectrum.reset_stats();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let n = buffer.samples();
        if n == 0 || self.left.len() < n {
            return ProcessStatus::Normal;
        }
        // Deinterleave taps (audio passes through untouched).
        for (i, frame) in buffer.iter_samples().enumerate() {
            let mut it = frame.into_iter();
            let l = it.next().map(|s| *s).unwrap_or(0.0);
            let r = it.next().map(|s| *s).unwrap_or(l);
            self.left[i] = l;
            self.right[i] = r;
            self.mono[i] = 0.5 * (l + r);
            self.phase.process(l, r);
        }
        self.lufs.process(&self.left[..n], &self.right[..n]);
        self.spectrum.process(&self.mono[..n]);
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsMeter {
    const CLAP_ID: &'static str = "com.fasttrackstudio.meter";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("LUFS / true-peak / correlation / goniometer / spectrum metering");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Analyzer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsMeter {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsMeterPlug0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Analyzer];
}

nice_export_clap!(FtsMeter);
nice_export_vst3!(FtsMeter);
