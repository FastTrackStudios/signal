//! FTS Comp — CLAP/VST3 compressor plugin.
//!
//! A nice-plug shell over a **stack** of [`comp::CompChain`]s
//! (docs/spec/fx/stack.md): up to [`comp_ui::params::MAX_STAGES`] complete
//! compressors — each with its own profile, classic and extended surface —
//! arranged into parallel lanes of serial stages by the stack params and run
//! through [`fx_stack::StagePool`]. The default is one stage on one lane:
//! exactly the pre-stack plugin, bit-for-bit (stage 1 keeps the original
//! param ids).
//!
//! Detection in each stage is stereo linked by default: `CompChain` feeds
//! both channels a max-linked key (blended by `channel_link`), while gain
//! smoothing and metering stay per channel inside the shared
//! `ProC3Compressor` core.
//!
//! Params + shared UI state live in [`comp_ui::params`] (like `eq-ui`), so
//! the Dioxus editor ([`comp_ui::control_view::App`]) renders against them
//! without a circular dep.

use audiocore_core::prelude::*;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use comp::CompChain;
use comp_ui::params::{CompParams, CompStageParams, CompUiState, MAX_STAGES};
use fx_stack::{LaneCtl, Stage, StagePool, SumMode};

const PLUGIN_NAME: &str = "FTS Comp";

/// The longest lookahead a stage can ask for, in ms — bounds the pool's lane
/// alignment buffers (`fx.stack.latency`).
const MAX_LOOKAHEAD_MS: f64 = 10.0;

// ── Stage adapter ─────────────────────────────────────────────────────────

/// One [`CompChain`] as a stack [`Stage`].
struct CompStage {
    chain: CompChain,
    sample_rate: f64,
}

impl CompStage {
    fn new() -> Self {
        Self {
            chain: CompChain::new(),
            sample_rate: 48_000.0,
        }
    }

    /// Push one stage's params into its chain (no allocation; every setter
    /// early-outs on an unchanged value).
    fn sync(&mut self, p: &CompStageParams) {
        let c = &mut self.chain.comp;
        c.set_threshold(p.threshold_db.value() as f64);
        c.set_ratio(p.ratio.value() as f64);
        c.set_attack_ms(p.attack_ms.value() as f64);
        c.set_release_ms(p.release_ms.value() as f64);
        c.set_knee(p.knee_db.value() as f64);
        c.set_fold(p.mix.value() as f64);
        c.output_gain_db = p.makeup_db.value() as f64;
        c.channel_link = p.stereo_link.value() as f64;

        // Extended surface — set_style / set_range_db mirror into the gain
        // curve; the rest are plain fields the core smooths itself.
        c.set_style(p.style.value());
        c.set_range_db(p.range_db.value() as f64);
        c.character_mode = p.character_mode.value();
        c.drive = p.drive.value() as f64;
        c.input_gain_db = p.input_gain_db.value() as f64;
        c.auto_makeup = p.auto_makeup.value();
        c.detector_rms_mix = p.detector_rms_mix.value() as f64;
        c.feedback = p.feedback.value() as f64;
        c.hold_ms = p.hold_ms.value() as f64;
        c.inertia = p.inertia.value() as f64;
        c.inertia_decay = p.inertia_decay.value() as f64;
        c.expander_threshold_db = p.expander_threshold_db.value() as f64;
        c.expander_ratio = p.expander_ratio.value() as f64;
        c.upward_threshold_db = p.upward_threshold_db.value() as f64;
        c.upward_ratio = p.upward_ratio.value() as f64;
        c.ceiling = p.ceiling.value() as f64;
        c.update(self.sample_rate);

        // Chain-level params: the sidechain setters early-out on an unchanged
        // frequency; `set_lookahead` only reallocates when the sample count
        // moves, so the buffer alloc happens on an actual edit.
        self.chain
            .set_sidechain_freq(p.sidechain_freq.value() as f64);
        self.chain
            .set_sidechain_lowpass_freq(p.sidechain_lowpass_freq.value() as f64);
        self.chain
            .set_lookahead((p.lookahead_ms.value() as f64).min(MAX_LOOKAHEAD_MS));
    }
}

impl Stage for CompStage {
    fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
        for i in 0..l.len() {
            self.chain.process_sample(&mut l[i], &mut r[i]);
        }
    }

    fn latency(&self) -> usize {
        self.chain.lookahead_samples
    }

    fn reset(&mut self) {
        self.chain.reset();
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsComp {
    params: Arc<CompParams>,
    ui_state: Arc<CompUiState>,
    editor_state: Arc<DioxusState>,
    /// The stack: a fixed pool of complete compressors, topology from params.
    pool: StagePool<CompStage>,
    /// Deinterleave scratch, sized at activate.
    buf_l: Vec<f64>,
    buf_r: Vec<f64>,
    sample_rate: f64,
    last_latency: usize,
}

impl Default for FtsComp {
    fn default() -> Self {
        let params = Arc::new(CompParams::default());
        let ui_state = Arc::new(CompUiState::new(params.clone()));
        Self {
            params,
            ui_state,
            // Sized by the editor surface itself — see comp_ui::control_view::
            // EDITOR_W/EDITOR_H for why it is a hard constraint rather than a
            // preference.
            editor_state: DioxusState::new(|| {
                (
                    comp_ui::control_view::EDITOR_W,
                    comp_ui::control_view::EDITOR_H,
                )
            })
            .with_resize_hint(comp_ui::control_view::resize_hint()),
            pool: StagePool::new((0..MAX_STAGES).map(|_| CompStage::new()).collect()),
            buf_l: Vec::new(),
            buf_r: Vec::new(),
            sample_rate: 48_000.0,
            last_latency: 0,
        }
    }
}

impl FtsComp {
    /// Push params into every stage and the pool topology (no allocation).
    fn sync_params(&mut self) {
        for i in 0..MAX_STAGES {
            let sp = self.params.stage(i);
            let in_use = sp.in_use.value();
            if in_use {
                if let Some(stage) = self.pool.stage_mut(i) {
                    stage.sync(sp);
                }
            }
            self.pool.set_slot(
                i,
                in_use,
                sp.stage_on.value(),
                sp.lane.value().max(0) as usize,
            );
        }
        for l in 0..MAX_STAGES {
            let lp = &self.params.lanes[l];
            self.pool.set_lane(
                l,
                LaneCtl {
                    gain: db_to_gain_f64(lp.gain_db.value() as f64),
                    mute: lp.mute.value(),
                    solo: lp.solo.value(),
                },
            );
        }
        self.pool.sum_mode = match self.params.sum_mode.value() {
            1 => SumMode::Power,
            2 => SumMode::Raw,
            _ => SumMode::Coherent,
        };
        self.pool.output_trim = db_to_gain_f64(self.params.output_trim_db.value() as f64);
        self.pool.update_alignment();
    }
}

fn db_to_gain_f64(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

impl Plugin for FtsComp {
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

    // `Plugin::Editor` is an associated type as of the baseview 0.3 rework,
    // so the editor is named here rather than returned as a trait object.
    type Editor = audiocore_core::nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            self.ui_state.clone(),
            comp_ui::control_view::App,
        )
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        self.ui_state
            .sample_rate
            .store(buffer_config.sample_rate, Ordering::Relaxed);
        for i in 0..MAX_STAGES {
            if let Some(stage) = self.pool.stage_mut(i) {
                stage.sample_rate = self.sample_rate;
                stage.chain.update_sample_rate(self.sample_rate);
            }
        }
        // Worst case each lane could compensate: every stage on one lane at
        // full lookahead.
        let max_latency = (MAX_LOOKAHEAD_MS / 1000.0 * self.sample_rate).ceil() as usize
            * MAX_STAGES;
        self.pool
            .prepare(buffer_config.max_buffer_size as usize, max_latency);
        self.buf_l = vec![0.0; buffer_config.max_buffer_size as usize];
        self.buf_r = vec![0.0; buffer_config.max_buffer_size as usize];
        self.sync_params();
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
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.sync_params();

        let n = buffer.samples();
        let mut input_peak: f32 = 0.0;
        {
            // Deinterleave into the f64 scratch (mono buses mirror the one
            // channel so the linked detector behaves as plain mono).
            let mut i = 0;
            for mut frame in buffer.iter_samples() {
                let mut it = frame.iter_mut();
                let (Some(l), r) = (it.next(), it.next()) else {
                    continue;
                };
                let left = *l as f64;
                let right = r.map(|s| *s as f64).unwrap_or(left);
                input_peak = input_peak.max(left.abs().max(right.abs()) as f32);
                self.buf_l[i] = left;
                self.buf_r[i] = right;
                i += 1;
            }
        }

        self.pool.process(&mut self.buf_l[..n], &mut self.buf_r[..n]);

        let mut output_peak: f32 = 0.0;
        {
            let mut i = 0;
            for mut frame in buffer.iter_samples() {
                let mut it = frame.iter_mut();
                let (Some(l), r) = (it.next(), it.next()) else {
                    continue;
                };
                let (left, right) = (self.buf_l[i], self.buf_r[i]);
                output_peak = output_peak.max(left.abs().max(right.abs()) as f32);
                *l = left as f32;
                if let Some(r) = r {
                    *r = right as f32;
                }
                i += 1;
            }
        }

        // ── UI metering (lock-free atomics; ~0.3 dB/block decay) ────────
        // The meters describe the stage the editor is focused on
        // (`fx.stack.focus`).
        let focused = self
            .ui_state
            .focused_stage
            .load(Ordering::Relaxed)
            .min(MAX_STAGES - 1);
        let gr_db = self
            .pool
            .stage(focused)
            .map(|s| s.chain.comp.gain_reduction_db() as f32)
            .unwrap_or(0.0);
        self.ui_state.gain_reduction_db.store(gr_db, Ordering::Relaxed);

        // Graph history rings: one input peak + one GR value per block —
        // lock-free stores, no allocation on the audio thread.
        self.ui_state.input_wave.push(input_peak);
        self.ui_state.gr_wave.push(gr_db);

        let prev_in = self.ui_state.input_peak_db.load(Ordering::Relaxed);
        let in_db = if input_peak > 0.0 {
            20.0 * input_peak.log10()
        } else {
            -100.0
        };
        self.ui_state.input_peak_db.store(
            if in_db > prev_in { in_db } else { prev_in - 0.3 },
            Ordering::Relaxed,
        );

        let prev_out = self.ui_state.output_peak_db.load(Ordering::Relaxed);
        let out_db = if output_peak > 0.0 {
            20.0 * output_peak.log10()
        } else {
            -100.0
        };
        self.ui_state.output_peak_db.store(
            if out_db > prev_out { out_db } else { prev_out - 0.3 },
            Ordering::Relaxed,
        );

        // Latency follows the slowest lane (`fx.stack.latency`).
        let latency = self.pool.latency();
        if latency != self.last_latency {
            self.last_latency = latency;
            _context.set_latency_samples(latency as u32);
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsComp {
    const CLAP_ID: &'static str = "com.fasttrackstudio.comp";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Compressor stack: up to eight stacked compressor stages in serial/parallel lanes",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Compressor,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsComp {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsCompPlugin001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nice_export_clap!(FtsComp);
nice_export_vst3!(FtsComp);
