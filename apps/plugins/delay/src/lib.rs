//! FTS Delay — CLAP/VST3 stereo delay plugin.
//!
//! A thin nice-plug shell over the [`delay`] facade's [`DelayChain`] — the
//! full stereo processor (per-side [`delay::DelayEngine`]s with runtime style
//! switching, ducking, mix). This shell exposes the CORE knob set: time L/R
//! (free-running ms; tempo sync is a follow-up), link, feedback, style,
//! tone (feedback hi-cut), drive, wow/flutter wobble, duck amount, and mix.
//! The chain's deeper surface (ping-pong, diffusion, accent/groove/feel,
//! heads, per-style extras) stays at its defaults until a richer shell lands.
//!
//! GUI is deliberately absent for now (headless, host-generic params),
//! matching level-plugin; the nice-plug-dioxus editor is a follow-up.

use audiocore_core::prelude::*;
use std::sync::Arc;

use delay_ui::params::{DelayParams, DelayUiState};

use audiocore_dsp::{AudioConfig, Processor};
use delay::DelayChain;

const PLUGIN_NAME: &str = "FTS Delay";

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsDelay {
    ui_state: Arc<DelayUiState>,
    editor_state: Arc<DioxusState>,
    params: Arc<DelayParams>,
    /// The full stereo chain (inherently stereo — one instance).
    chain: DelayChain,
    sample_rate: f64,
    max_buffer_size: usize,
}

impl Default for FtsDelay {
    fn default() -> Self {
        Self {
            params: Arc::new(DelayParams::default()),
            ui_state: Arc::new(DelayUiState::default()),
            // The editor sizes itself — see delay_ui::control_view.
            editor_state: DioxusState::new(|| {
                (
                    delay_ui::control_view::EDITOR_W,
                    delay_ui::control_view::EDITOR_H,
                )
            })
            .with_resize_hint(delay_ui::control_view::resize_hint()),
            chain: DelayChain::new(),
            sample_rate: 48_000.0,
            max_buffer_size: 512,
        }
    }
}

impl FtsDelay {
    /// Push the current params into the chain and refresh coefficients.
    ///
    /// `DelayChain::update` only reallocates when the sample rate grows
    /// beyond what `initialize()` already provisioned, so calling it per
    /// block is allocation-free on the audio thread.
    fn sync_params(&mut self) {
        let p = &self.params;
        let c = &mut self.chain;

        // The engine comes from the profile, not from a raw style index: the
        // rail's six families are the vocabulary, and the persisted id is
        // what a session reopens with.
        c.set_style(p.resolved_profile().style);

        // Time — free-running ms; Link mirrors L onto R.
        let time_l = p.time_l.value() as f64;
        c.delay_l.time_ms = time_l;
        c.delay_r.time_ms = if p.link.value() {
            time_l
        } else {
            p.time_r.value() as f64
        };

        let fb = p.feedback.value() as f64;
        c.delay_l.feedback = fb;
        c.delay_r.feedback = fb;

        // Tone = feedback-loop hi-cut.
        let tone = p.tone.value() as f64;
        c.delay_l.hicut_freq = tone;
        c.delay_r.hicut_freq = tone;

        let drive = p.drive.value() as f64;
        c.delay_l.drive = drive;
        c.delay_r.drive = drive;

        // Wobble (rates stay at the engine defaults).
        let wow = p.wow.value() as f64;
        c.delay_l.wow_depth = wow;
        c.delay_r.wow_depth = wow;
        let flutter = p.flutter.value() as f64;
        c.delay_l.flutter_depth = flutter;
        c.delay_r.flutter_depth = flutter;

        // Ducking: the amount knob doubles as the enable (0 = off).
        let duck = p.duck.value() as f64;
        c.ducking_enabled = duck > 0.001;
        c.ducker.amount = duck;
        c.ducker.threshold = 0.1;

        c.mix = p.mix.value() as f64;

        c.update(AudioConfig {
            sample_rate: self.sample_rate,
            max_buffer_size: self.max_buffer_size,
        });
    }
}

impl Plugin for FtsDelay {
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

    // No editor yet — the host shows its generic parameter UI.
    type Editor = audiocore_core::nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            Arc::new(delay_ui::control_view::DelayUi {
                params: self.params.clone(),
                state: self.ui_state.clone(),
            }),
            delay_ui::control_view::App,
        )
    }

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
        self.max_buffer_size = buffer_config.max_buffer_size as usize;
        // First update at the real sample rate provisions every delay line
        // (the engines size for their 5 s maximum) so process() never
        // allocates.
        self.sync_params();
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

        // The chain processes f64 stereo slices; bridge from the host's f32
        // buffers in fixed stack chunks (no heap allocation).
        const CHUNK: usize = 128;
        let num_samples = buffer.samples();
        let channels = buffer.channels();
        if channels < 2 {
            return ProcessStatus::Normal;
        }

        let mut offset = 0;
        while offset < num_samples {
            let len = (num_samples - offset).min(CHUNK);

            let mut left = [0.0f64; CHUNK];
            let mut right = [0.0f64; CHUNK];

            {
                let slices = buffer.as_slice();
                for i in 0..len {
                    left[i] = slices[0][offset + i] as f64;
                    right[i] = slices[1][offset + i] as f64;
                }
            }

            self.chain.process(&mut left[..len], &mut right[..len]);

            {
                let slices = buffer.as_slice();
                for i in 0..len {
                    slices[0][offset + i] = left[i] as f32;
                    slices[1][offset + i] = right[i] as f32;
                }
            }

            offset += len;
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsDelay {
    const CLAP_ID: &'static str = "com.fasttrackstudio.delay";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Stereo delay: tape/digital/BBD/lo-fi styles with wobble, saturation, and ducking");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Delay,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsDelay {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsDelayPlugn001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Delay];
}

nice_export_clap!(FtsDelay);
nice_export_vst3!(FtsDelay);
