//! FTS Delay — CLAP/VST3 stereo delay plugin.
//!
//! A thin nice-plug shell over a **stack** of the [`delay`] facade's
//! [`DelayChain`]s (docs/spec/fx/stack.md): up to
//! [`dsp::delay::ui::params::MAX_STAGES`] complete stereo delays — each with
//! its own profile, time/sync, feedback, tone, drive, wow/flutter, duck and
//! mix — arranged into parallel lanes of serial stages by the stack params
//! and run through [`dsp::stack::StagePool`]. Every vocal delay a mix wants
//! (slap, eighth, quarter throw) in one instance, in parallel. The default
//! is one stage on one lane: exactly the pre-stack plugin, bit-for-bit
//! (stage 1 keeps the original param ids).
//!
//! Each stage exposes the CORE knob set: time L/R (free-running ms or a note
//! value against the host tempo), link, feedback, style, tone (feedback
//! hi-cut), drive, wow/flutter wobble, duck amount, and mix. The chain's
//! deeper surface (ping-pong, diffusion, accent/groove/feel, heads,
//! per-style extras) stays at its defaults until a richer shell lands.

use plugin_core::prelude::*;
use std::sync::Arc;

use dsp::delay::ui::params::{DelayParams, DelayStageParams, DelayUiState, MAX_STAGES};

use dsp::delay::DelayChain;
use dsp::stack::{Stage, StagePool};
use dsp_core::audio::{AudioConfig, Processor};

/// The host's f32 buffers are bridged to the chain's f64 in fixed stack
/// chunks of this many frames; the pool is prepared for exactly this block.
/// Same chunking as the pre-stack shell, so stage 1 sees identical blocks.
const CHUNK: usize = 128;

const PLUGIN_NAME: &str = "FTS Delay";

// ── Stage adapter ─────────────────────────────────────────────────────────

/// One [`DelayChain`] as a stack [`Stage`] — the complete pre-stack
/// processor, mix included.
struct DelayStage {
    chain: DelayChain,
    config: AudioConfig,
}

impl DelayStage {
    fn new() -> Self {
        Self {
            chain: DelayChain::new(),
            config: AudioConfig {
                sample_rate: 48_000.0,
                max_buffer_size: CHUNK,
            },
        }
    }

    /// Push one stage's params into its chain and refresh coefficients.
    ///
    /// `DelayChain::update` only reallocates when the sample rate grows
    /// beyond what the lines were provisioned for, so calling it per block
    /// is allocation-free once a stage has been updated at the session rate.
    /// A *style change* is not: `set_style` builds the new engine — that has
    /// always been true of a profile change in this plugin.
    fn sync(&mut self, p: &DelayStageParams, tempo: Option<f64>) {
        let c = &mut self.chain;

        // The engine comes from the profile, not from a raw style index: the
        // rail's six families are the vocabulary, and the persisted id is
        // what a session reopens with.
        c.set_style(p.resolved_profile().style);

        // Time — either the dialled milliseconds or the note value against
        // the host's tempo; Link mirrors L onto R either way. The params
        // own that decision so the editor and the audio thread cannot
        // disagree about what the control currently means. Each stage
        // resolves its own sync against the same host tempo.
        c.delay_l.time_ms = p.time_l_ms(tempo);
        c.delay_r.time_ms = p.time_r_ms(tempo);

        let fb = f64::from(p.feedback.value());
        c.delay_l.feedback = fb;
        c.delay_r.feedback = fb;

        // Tone = feedback-loop hi-cut.
        let tone = f64::from(p.tone.value());
        c.delay_l.hicut_freq = tone;
        c.delay_r.hicut_freq = tone;

        let drive = f64::from(p.drive.value());
        c.delay_l.drive = drive;
        c.delay_r.drive = drive;

        // Wobble (rates stay at the engine defaults).
        let wow = f64::from(p.wow.value());
        c.delay_l.wow_depth = wow;
        c.delay_r.wow_depth = wow;
        let flutter = f64::from(p.flutter.value());
        c.delay_l.flutter_depth = flutter;
        c.delay_r.flutter_depth = flutter;

        // Ducking: the amount knob doubles as the enable (0 = off).
        let duck = f64::from(p.duck.value());
        c.ducking_enabled = duck > 0.001;
        c.ducker.amount = duck;
        c.ducker.threshold = 0.1;

        c.mix = f64::from(p.mix.value());

        c.update(self.config);
    }
}

impl Stage for DelayStage {
    fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
        self.chain.process(l, r);
    }

    fn reset(&mut self) {
        self.chain.reset();
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsDelay {
    ui_state: Arc<DelayUiState>,
    editor_state: Arc<DioxusState>,
    params: Arc<DelayParams>,
    /// The stack: a fixed pool of complete stereo delays, topology from
    /// params.
    pool: StagePool<DelayStage>,
    last_latency: usize,
}

impl Default for FtsDelay {
    fn default() -> Self {
        Self {
            params: Arc::new(DelayParams::default()),
            ui_state: Arc::new(DelayUiState::default()),
            // The editor sizes itself — see dsp::delay::ui::control_view.
            editor_state: DioxusState::new(|| {
                (
                    dsp::delay::ui::control_view::EDITOR_W,
                    dsp::delay::ui::control_view::EDITOR_H,
                )
            })
            .with_resize_hint(dsp::delay::ui::control_view::resize_hint()),
            pool: StagePool::new((0..MAX_STAGES).map(|_| DelayStage::new()).collect()),
            last_latency: 0,
        }
    }
}

impl FtsDelay {
    /// Push params into every in-use stage, then the pool topology.
    ///
    /// Only stages in the stack are synced: an unused stage is silent, and
    /// skipping it keeps its delay lines unprovisioned at the session rate
    /// until it is added (see `activate`).
    fn sync_params(&mut self, tempo: Option<f64>) {
        for i in 0..MAX_STAGES {
            let sp = self.params.stage(i);
            if sp.place.in_use.value() {
                if let Some(stage) = self.pool.stage_mut(i) {
                    stage.sync(sp, tempo);
                }
            }
        }
        dsp::stack::host::sync_pool(&mut self.pool, &*self.params);
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

    type Editor = plugin_core::nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            Arc::new(dsp::delay::ui::control_view::DelayUi {
                params: self.params.clone(),
                state: self.ui_state.clone(),
            }),
            dsp::delay::ui::control_view::App,
        )
    }

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl ActivateContext<Self>,
    ) -> bool {
        let config = AudioConfig {
            sample_rate: f64::from(buffer_config.sample_rate),
            // The chunked bridge below never hands a stage more than this.
            max_buffer_size: CHUNK,
        };
        for i in 0..MAX_STAGES {
            if let Some(stage) = self.pool.stage_mut(i) {
                stage.config = config;
            }
        }
        // Delay lines are prepared for the stages IN the stack only — slot 0
        // by default. Each `DelayChain` carries two engines with 5 s lines
        // (plus diffusers), so updating all eight at, say, 96 kHz would hold
        // tens of megabytes for stages most sessions never add. A stage that
        // enters the stack later is provisioned by its first per-block
        // `update` (and, for any space but the default, by `set_style`
        // building its engine) on the audio thread. That is the same
        // allocation this plugin has always done on a profile change —
        // `set_style` rebuilds the engine — and adding a stage is that same
        // gesture (a profile written to a slot), so it is no new class of
        // audio-thread allocation.
        //
        // No transport at activation time — the first `process` call brings
        // the tempo, and until then a synced delay runs at its free time.
        self.sync_params(None);
        // The chain reports no latency, so lanes never need alignment.
        self.pool.prepare(CHUNK, 0);
        self.last_latency = self.pool.latency();
        context.set_latency_samples(self.last_latency as u32);
        true
    }

    fn reset(&mut self) {
        self.pool.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // The host's tempo, published for the editor as well as used here:
        // the face has to show what a note value currently works out to, and
        // this is the only thread that is told.
        let tempo = context.transport().tempo;
        self.ui_state.tempo_bpm.store(
            tempo.map_or(0.0, |t| t as f32),
            std::sync::atomic::Ordering::Relaxed,
        );
        self.sync_params(tempo);

        // The stack processes f64 stereo slices; bridge from the host's f32
        // buffers in fixed stack chunks (no heap allocation).
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
                    left[i] = f64::from(slices[0][offset + i]);
                    right[i] = f64::from(slices[1][offset + i]);
                }
            }

            self.pool.process(&mut left[..len], &mut right[..len]);

            {
                let slices = buffer.as_slice();
                for i in 0..len {
                    slices[0][offset + i] = left[i] as f32;
                    slices[1][offset + i] = right[i] as f32;
                }
            }

            offset += len;
        }

        // Latency follows the slowest lane (`fx.stack.latency`) — zero for
        // today's chains, kept so a stage that grows latency is reported.
        let latency = self.pool.latency();
        if latency != self.last_latency {
            self.last_latency = latency;
            context.set_latency_samples(latency as u32);
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsDelay {
    const CLAP_ID: &'static str = "com.fasttrackstudio.delay";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Delay stack: up to eight stereo delays (tape/digital/BBD/lo-fi/...) in serial/parallel lanes",
    );
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

#[cfg(feature = "export")]
nice_export_clap!(FtsDelay);
#[cfg(feature = "export")]
nice_export_vst3!(FtsDelay);

#[cfg(test)]
mod tests {
    use super::*;
    use dsp::stack::host::StackHost;

    /// A fresh instance is the plain plugin: one stage, on lane 0.
    #[test]
    fn default_stack_is_one_stage_on_lane_zero() {
        let plugin = FtsDelay::default();
        assert_eq!(plugin.params.stages_in_use(), vec![0]);
        assert_eq!(plugin.params.stage(0).place.lane_index(), 0);
        assert!(plugin.params.stage(0).place.stage_on.value());
        assert_eq!(plugin.pool.len(), MAX_STAGES);
    }
}
