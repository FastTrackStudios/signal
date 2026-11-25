//! FTS Reverb — CLAP/VST3 reverb plugin.
//!
//! A thin nice-plug shell over a **stack** of the reverb engine's realtime
//! chains (docs/spec/fx/stack.md): up to
//! [`dsp::reverb::ui::params::MAX_STAGES`] complete reverbs — each a
//! [`ReverbChain`] (input filtering → pre-delay → algorithm → output EQ →
//! width → mix) with its own space, knobs, Post / Decay Rate EQs and impulse
//! response — arranged into parallel lanes of serial stages by the stack
//! params and run through [`dsp::stack::StagePool`]. The default is one stage
//! on one lane: exactly the pre-stack plugin, bit-for-bit (stage 1 keeps the
//! original param ids, and a lone stage on a lone lane is multiplied by
//! exactly 1.0).
//!
//! Each chain is stereo-coupled (one engine, two channels), so a stage is
//! one [`ReverbChain`] for both channels — never one per channel.
//!
//! Decay/damping automation is click-free — the chain ramps those
//! coefficients internally (30 ms smoothers); we just push targets via
//! `update_params()` every block. The `BigSky` MX per-engine params,
//! ducking, and saturation are chain features not yet surfaced here.

use dsp::reverb::ir::PreparedIrPair;
use dsp::reverb::ui::params::{MAX_STAGES, ReverbParams, ReverbStageParams, ReverbUiState};
use plugin_core::prelude::*;
use std::sync::Arc;

use crossbeam_channel::Receiver;
use dsp::reverb::ReverbChain;
use dsp::stack::{Stage, StagePool};
use dsp_core::audio::{AudioConfig, Processor};

const PLUGIN_NAME: &str = "FTS Reverb";

// ── Stage adapter ─────────────────────────────────────────────────────────

/// One [`ReverbChain`] as a stack [`Stage`].
///
/// The chain is built lazily: a reverb is the heaviest stage there is — a
/// 1.5 s stereo pre-delay line plus the algorithm's delay network, megabytes
/// at high sample rates — and eight of them for a stack that is almost
/// always one stage would be most of the plugin's memory spent on nothing.
/// See [`FtsReverb::sync_params`] for when the allocation happens.
struct ReverbStage {
    chain: Option<Box<ReverbChain>>,
    /// This stage's end of its IR channel (`ReverbUiState::ir_tx`), handed
    /// to the chain when it is built — so a chain built late still hears the
    /// editor's worker.
    ir_rx: Receiver<PreparedIrPair>,
}

impl ReverbStage {
    /// Build the chain if it is not built yet. Params land first so
    /// `update()` snaps the smoothers onto the real values (no ramp from
    /// defaults) — the pre-stack `activate` order. Allocates.
    fn ensure_built(&mut self, p: &ReverbStageParams, tempo: Option<f64>, config: AudioConfig) {
        if self.chain.is_some() {
            return;
        }
        let mut chain = Box::new(ReverbChain::new());
        chain.set_prepared_ir_receiver(self.ir_rx.clone());
        sync_chain(&mut chain, p, tempo);
        chain.update(config);
        self.chain = Some(chain);
    }
}

impl Stage for ReverbStage {
    fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
        // The per-block sync builds a stage before the pool sees it in use,
        // so an unbuilt chain here is only belt and braces: it passes the
        // signal through.
        if let Some(chain) = self.chain.as_mut() {
            chain.process(l, r);
        }
    }

    fn reset(&mut self) {
        if let Some(chain) = self.chain.as_mut() {
            chain.reset();
        }
    }
}

/// Push one stage's params into its chain (no allocation on the steady
/// path; `set_algorithm` rebuilds engine state only when the selector
/// actually changes). Decay/damping ramp inside the chain's smoothers.
fn sync_chain(chain: &mut ReverbChain, p: &ReverbStageParams, tempo: Option<f64>) {
    // The engine comes from the profile, not from a raw algorithm index:
    // a profile names both the algorithm and which variant of it, and the
    // variant is half of what makes a Cathedral not an Arena. Resolved
    // through the persisted id so a session opens on the space it saved.
    let profile = p.resolved_profile();
    chain.set_algorithm_variant(profile.algorithm, profile.variant);

    chain.params.decay = f64::from(p.decay.value());
    chain.params.size = f64::from(p.size.value());
    chain.params.damping = f64::from(p.damping.value());
    chain.params.tone = f64::from(p.tone.value());
    chain.params.diffusion = f64::from(p.diffusion.value());
    chain.params.modulation = f64::from(p.modulation.value());
    // Frequency-dependent decay: the low band rings longer or shorter
    // than the rest, which is most of what separates a warm hall from a
    // plate. The high band moves the other way, so one control does the
    // tilt rather than two that have to be kept in step.
    let bass = f64::from(p.bass.value());
    chain.params.low_decay_mult = bass;
    chain.params.high_decay_mult = (2.0 - bass).clamp(0.0, 2.0);
    // A time-set tail: the engine's own setting for that time in this
    // space at this size — after the tilt, which the conversion
    // compensates for. `%` keeps the position as it is.
    if let Some(seconds) = p.decay_seconds(tempo)
        && let Some(decay) = chain.decay_for_seconds(seconds)
    {
        chain.params.decay = decay;
    }
    // The engine's own two controls. What they mean is the algorithm's
    // business — the panel is what names them.
    chain.params.extra_a = f64::from(p.character_a.value());
    chain.params.extra_b = f64::from(p.character_b.value());

    // …and the per-engine settings the shared pair cannot express. Each
    // struct is only read by its own algorithm, so setting them all every
    // block costs nothing and means switching profiles never lands on a
    // stale value from the last one.
    chain.shimmer.shift1_semitones = Some(f64::from(p.shimmer_interval.value()));
    chain.spring.springs = p.springs.value().clamp(1, 3) as u8;
    chain.bloom.harmonics = f64::from(p.harmonics.value());
    chain.chorale.mod_amount = f64::from(p.singers.value());
    chain.magneto.feedback = f64::from(p.regen.value());
    chain.nonlinear.chop_depth = f64::from(p.chop.value());

    // Either the dialled milliseconds or the note value against the
    // host's tempo; the params own that decision so the face and the
    // audio thread cannot disagree about what the control means.
    chain.predelay_ms = p.predelay_ms(tempo);
    chain.width = f64::from(p.width.value());
    chain.mix = f64::from(p.mix.value());

    // The two embedded EQs (docs/spec/fx/embedded-eq.md): the Post EQ on
    // the wet output, the Decay Rate EQ into the algorithm's feedback
    // path (exact on the Hall family, collapsed elsewhere). Both are
    // change-detected downstream, so writing every block costs a compare.
    for i in 0..dsp::reverb::chain::POST_EQ_BANDS {
        chain.post_eq[i] = p.post_eq[i].to_band();
    }
    for i in 0..dsp::reverb::algorithm::DECAY_BANDS {
        chain.params.decay_bands[i] = p.decay_eq[i].to_band();
    }

    chain.update_params();
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsReverb {
    params: Arc<ReverbParams>,
    ui_state: Arc<ReverbUiState>,
    editor_state: Arc<DioxusState>,
    /// The stack: a fixed pool of complete reverbs, topology from params.
    pool: StagePool<ReverbStage>,
    /// Pre-allocated f64 scratch (the chain processes f64 planar stereo).
    /// Sized to the host's max buffer in `activate`; never grown in
    /// `process`.
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
    config: AudioConfig,
    /// The latency last reported to the host (`fx.stack.latency`). Every
    /// reverb stage is zero-latency today; followed anyway so a stage that
    /// gains lookahead cannot silently misalign the host.
    latency: usize,
}

impl Default for FtsReverb {
    fn default() -> Self {
        let params = Arc::new(ReverbParams::default());
        let ui_state = Arc::new(ReverbUiState::default());
        // The editor's worker sends finished partitions; each stage's chain
        // receives them inside `process` and swaps a pointer. The audio
        // thread never decodes, allocates or blocks for an IR. One channel
        // per stage, because each stage convolves with its own impulse.
        let stages = (0..MAX_STAGES)
            .map(|i| {
                let (ir_tx, ir_rx) = crossbeam_channel::bounded(4);
                *ui_state.ir_tx[i].lock() = Some(ir_tx);
                ReverbStage { chain: None, ir_rx }
            })
            .collect();
        Self {
            params,
            ui_state,
            // The editor sizes itself — see dsp::reverb::ui::control_view.
            editor_state: DioxusState::new(|| {
                (
                    dsp::reverb::ui::control_view::EDITOR_W,
                    dsp::reverb::ui::control_view::EDITOR_H,
                )
            })
            .with_resize_hint(dsp::reverb::ui::control_view::resize_hint()),
            pool: StagePool::new(stages),
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
            config: AudioConfig {
                sample_rate: 48_000.0,
                max_buffer_size: 1,
            },
            latency: 0,
        }
    }
}

impl FtsReverb {
    /// Push params into every in-use stage, then the pool topology.
    ///
    /// Allocation: none on the steady path. A stage entering the stack for
    /// the first time in this instance's life is built here, on the audio
    /// thread — the trade the memory note on [`ReverbStage`] makes. It is no
    /// worse than what this shell has always done: switching the space
    /// rebuilds the algorithm's network on the audio thread
    /// (`set_algorithm_variant`), and adding a stage sets its space anyway.
    /// A built chain is never freed here; removing a stage keeps it for the
    /// next add.
    fn sync_params(&mut self, tempo: Option<f64>) {
        let config = self.config;
        for i in 0..MAX_STAGES {
            let sp = self.params.stage(i);
            if !sp.place.in_use.value() {
                continue;
            }
            if let Some(stage) = self.pool.stage_mut(i) {
                stage.ensure_built(sp, tempo, config);
                if let Some(chain) = stage.chain.as_mut() {
                    sync_chain(chain, sp, tempo);
                }
            }
        }
        dsp::stack::host::sync_pool(&mut self.pool, &*self.params);
    }

    /// Load every stage's saved impulse again.
    ///
    /// A session saves each stage's impulse by path, not by content, so
    /// reopening it has to read the file again, at the rate the host just
    /// gave us. That includes stages outside the stack: their result waits
    /// in the stage's queue, and the chain picks it up when the stage is
    /// added — so a stage brought back into the stack plays the impulse it
    /// was saved with. The worker does the reading and resampling; the
    /// chains pick the result up in `process`, as they do when the user
    /// picks a file.
    fn reload_saved_irs(&self) {
        for i in 0..MAX_STAGES {
            let path = self.params.stage(i).ir_path.read().clone();
            if !path.is_empty() {
                self.ui_state.load_ir(i, std::path::PathBuf::from(path));
            }
        }
    }

    /// Whether stage `i`'s chain has been built — for tests.
    #[cfg(test)]
    fn built(&self, i: usize) -> bool {
        self.pool.stage(i).is_some_and(|s| s.chain.is_some())
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

    type Editor = plugin_core::nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            Arc::new(dsp::reverb::ui::control_view::ReverbUi {
                params: self.params.clone(),
                state: self.ui_state.clone(),
            }),
            dsp::reverb::ui::control_view::App,
        )
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl ActivateContext<Self>,
    ) -> bool {
        // The worker resamples to whatever the host is running at.
        self.ui_state.sample_rate.store(
            buffer_config.sample_rate,
            std::sync::atomic::Ordering::Relaxed,
        );
        let max = (buffer_config.max_buffer_size as usize).max(1);
        self.config = AudioConfig {
            sample_rate: f64::from(buffer_config.sample_rate),
            max_buffer_size: max,
        };
        self.scratch_l = vec![0.0; max];
        self.scratch_r = vec![0.0; max];
        // Stage 1 is always built here, and so is every stage a loaded
        // session already has in the stack; the rest wait until they are
        // added (see `sync_params`). A chain built earlier is reconfigured
        // for the new rate. Either way the params land before the
        // reconfigure so `update()` snaps the chain's smoothers onto the
        // real values (no ramp from defaults).
        //
        // No transport at activate time — the first `process` call brings
        // the tempo, and until then a synced pre-delay runs at its free time.
        for i in 0..MAX_STAGES {
            let sp = self.params.stage(i);
            let Some(stage) = self.pool.stage_mut(i) else {
                continue;
            };
            if let Some(chain) = stage.chain.as_mut() {
                sync_chain(chain, sp, None);
                chain.update(self.config);
            } else if i == 0 || sp.place.in_use.value() {
                stage.ensure_built(sp, None, self.config);
            } else {
                // Nothing drains an unbuilt stage's queue, and what waits
                // there was resampled for the last activation. The reload
                // below queues it again at this rate.
                while stage.ir_rx.try_recv().is_ok() {}
            }
        }
        self.reload_saved_irs();
        // Every reverb stage is zero-latency, so lane alignment needs no
        // buffer beyond the pool's minimum.
        self.pool.prepare(max, 0);
        dsp::stack::host::sync_pool(&mut self.pool, &*self.params);
        self.latency = self.pool.latency();
        context.set_latency_samples(u32::try_from(self.latency).unwrap_or(u32::MAX));
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
        if buffer.channels() < 2 || self.scratch_l.is_empty() {
            return ProcessStatus::Normal;
        }
        // The host's tempo, published for the face as well as used here: it
        // has to show what a note value currently works out to, and this is
        // the only thread that is told.
        let tempo = context.transport().tempo;
        self.ui_state.tempo_bpm.store(
            tempo.map_or(0.0, |t| t as f32),
            std::sync::atomic::Ordering::Relaxed,
        );
        self.sync_params(tempo);

        // Process in scratch-sized chunks: f32 interleave → f64 planar →
        // stack → back. No allocation — scratch was sized in `activate`, and
        // the pool processes at most that many frames per call.
        let chunk = self.scratch_l.len();
        let total = buffer.samples();
        let mut offset = 0;
        while offset < total {
            let len = (total - offset).min(chunk);
            {
                let ch = buffer.as_slice();
                for i in 0..len {
                    self.scratch_l[i] = f64::from(ch[0][offset + i]);
                    self.scratch_r[i] = f64::from(ch[1][offset + i]);
                }
            }
            self.pool
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

        // Latency follows the slowest lane (`fx.stack.latency`).
        let latency = self.pool.latency();
        if latency != self.latency {
            self.latency = latency;
            context.set_latency_samples(u32::try_from(latency).unwrap_or(u32::MAX));
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

#[cfg(feature = "export")]
nice_export_clap!(FtsReverb);
#[cfg(feature = "export")]
nice_export_vst3!(FtsReverb);

#[cfg(test)]
mod tests {
    use super::*;
    use dsp::stack::host::StackHost;

    /// A fresh instance is the plain plugin (`fx.stack.model`): one stage,
    /// on lane 0, and only that stage's chain built.
    #[test]
    fn default_stack_is_one_stage_on_lane_zero() {
        let mut plugin = FtsReverb::default();
        assert_eq!(plugin.params.stages_in_use(), vec![0]);
        assert_eq!(plugin.params.placement(0).lane_index(), 0);
        assert!(plugin.params.placement(0).stage_on.value());
        plugin.sync_params(None);
        assert!(plugin.built(0));
        assert!((1..MAX_STAGES).all(|i| !plugin.built(i)));
        assert_eq!(plugin.pool.latency(), 0);
    }

    /// A saved impulse is loaded again on activate for every stage, in the
    /// stack or not, so a stage added later plays the impulse it was saved
    /// with. A missing file is the observable proof the worker went looking:
    /// it reports the failure on that stage.
    #[test]
    fn saved_irs_reload_for_every_stage() {
        let plugin = FtsReverb::default();
        for i in [0, 2] {
            *plugin.params.stage(i).ir_path.write() = format!("/nonexistent/sta560-{i}.wav");
        }
        plugin.reload_saved_irs();
        let state = &plugin.ui_state;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while [0, 2].iter().any(|&i| state.ir_error[i].lock().is_none())
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(state.ir_error[0].lock().is_some());
        assert!(state.ir_error[2].lock().is_some());
        assert!(state.ir_error[1].lock().is_none());
        assert!(!state.ir_loading[1].load(std::sync::atomic::Ordering::Relaxed));
    }
}
