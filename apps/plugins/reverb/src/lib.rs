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

use audiocore_core::prelude::*;
use reverb_ui::params::{ReverbParams, ReverbUiState};
use std::sync::Arc;

use audiocore_dsp::{AudioConfig, Processor};
use reverb::ReverbChain;

const PLUGIN_NAME: &str = "FTS Reverb";

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsReverb {
    params: Arc<ReverbParams>,
    ui_state: Arc<ReverbUiState>,
    editor_state: Arc<DioxusState>,
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
        let params = Arc::new(ReverbParams::default());
        // The editor's worker sends finished partitions; the chain receives
        // them inside `process` and swaps a pointer. The audio thread never
        // decodes, allocates or blocks for an IR.
        let (ir_tx, ir_rx) = crossbeam_channel::bounded(4);
        let mut chain = ReverbChain::new();
        chain.set_prepared_ir_receiver(ir_rx);
        let ui_state = Arc::new(ReverbUiState::default());
        *ui_state.ir_tx.lock() = Some(ir_tx);
        Self {
            params,
            ui_state,
            // The editor sizes itself — see reverb_ui::control_view.
            editor_state: DioxusState::new(|| {
                (
                    reverb_ui::control_view::EDITOR_W,
                    reverb_ui::control_view::EDITOR_H,
                )
            })
            .with_resize_hint(reverb_ui::control_view::resize_hint()),
            chain,
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
        // The engine comes from the profile, not from a raw algorithm index:
        // a profile names both the algorithm and which variant of it, and the
        // variant is half of what makes a Cathedral not an Arena. Resolved
        // through the persisted id so a session opens on the space it saved.
        let profile = self.params.resolved_profile();
        self.chain
            .set_algorithm_variant(profile.algorithm, profile.variant);

        self.chain.params.decay = self.params.decay.value() as f64;
        self.chain.params.size = self.params.size.value() as f64;
        self.chain.params.damping = self.params.damping.value() as f64;
        self.chain.params.tone = self.params.tone.value() as f64;
        self.chain.params.diffusion = self.params.diffusion.value() as f64;
        self.chain.params.modulation = self.params.modulation.value() as f64;
        // Frequency-dependent decay: the low band rings longer or shorter
        // than the rest, which is most of what separates a warm hall from a
        // plate. The high band moves the other way, so one control does the
        // tilt rather than two that have to be kept in step.
        let bass = self.params.bass.value() as f64;
        self.chain.params.low_decay_mult = bass;
        self.chain.params.high_decay_mult = (2.0 - bass).clamp(0.0, 2.0);
        // The engine's own two controls. What they mean is the algorithm's
        // business — the panel is what names them.
        self.chain.params.extra_a = self.params.character_a.value() as f64;
        self.chain.params.extra_b = self.params.character_b.value() as f64;

        // …and the per-engine settings the shared pair cannot express. Each
        // struct is only read by its own algorithm, so setting them all every
        // block costs nothing and means switching profiles never lands on a
        // stale value from the last one.
        self.chain.shimmer.shift1_semitones = Some(self.params.shimmer_interval.value() as f64);
        self.chain.spring.springs = self.params.springs.value().clamp(1, 3) as u8;
        self.chain.bloom.harmonics = self.params.harmonics.value() as f64;
        self.chain.chorale.mod_amount = self.params.singers.value() as f64;
        self.chain.magneto.feedback = self.params.regen.value() as f64;
        self.chain.nonlinear.chop_depth = self.params.chop.value() as f64;

        self.chain.predelay_ms = self.params.predelay.value() as f64;
        self.chain.width = self.params.width.value() as f64;
        self.chain.mix = self.params.mix.value() as f64;

        // The two embedded EQs (docs/spec/fx/embedded-eq.md): the Post EQ on
        // the wet output, the Decay Rate EQ into the algorithm's feedback
        // path (exact on the Hall family, collapsed elsewhere). Both are
        // change-detected downstream, so writing every block costs a compare.
        for i in 0..reverb::chain::POST_EQ_BANDS {
            self.chain.post_eq[i] = self.params.post_eq[i].to_band();
        }
        for i in 0..reverb::algorithm::DECAY_BANDS {
            self.chain.params.decay_bands[i] = self.params.decay_eq[i].to_band();
        }

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

    type Editor = audiocore_core::nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            Arc::new(reverb_ui::control_view::ReverbUi {
                params: self.params.clone(),
                state: self.ui_state.clone(),
            }),
            reverb_ui::control_view::App,
        )
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        // The worker resamples to whatever the host is running at.
        self.ui_state.sample_rate.store(
            buffer_config.sample_rate,
            std::sync::atomic::Ordering::Relaxed,
        );
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
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Reverb with 15 algorithm engines: rooms, halls, plates, springs, shimmer, convolution",
    );
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
