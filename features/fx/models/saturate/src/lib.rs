//! FTS Saturate — CLAP/VST3 saturation plugin.
//!
//! Five circuits on a rail — Tube, Tape, Transformer, Transistor, Digital —
//! and nine profiles inside them. What each one *is* lives in
//! [`saturate_profiles`]: the per-side shaper pair, the operating point, the
//! sag and its ballistics, the skew that makes the even harmonics, and which
//! band meets the knee first. Nothing about a circuit lives in this file.
//!
//! That is deliberate and it is the point of the crate split. The plugin
//! shell used to carry its own `ModelParam` table — Preamp/Tube/Tape/
//! Transformer/Console/Fuzz — which named nearly the same circuits as the
//! rail did, mapped them to slightly different settings, and drifted from it.
//! Two vocabularies for one choice is a bug that ships as a sound. There is
//! one now, and [`dsp::saturate::profiles::apply`] is the only thing that knows how
//! a knob reaches the DSP — which is also why the editor's curve is the
//! engine's curve rather than a drawing of one.
//!
//! Engine: [`dsp::saturate::preamp::ClassAPreamp`] for the nonlinearity, plus
//! [`dsp::saturate::digital::DigitalStage`] on the wet path for the digital
//! family (there is no transfer curve that produces an alias, so quantisation
//! and rate reduction cannot be a shaper).
//!
//! The shell is an FX stack (`fx.stack.every-shell`, docs/spec/fx/stack.md):
//! a [`StagePool`] of [`MAX_STAGES`] complete saturators, each a
//! [`SatStage`] — its own preamp pair, quantiser pair, emphasis pair,
//! measured engines, mix and output — arranged into lanes by the params. The
//! default is one stage on one lane, which the pool passes through at unity:
//! it sounds exactly like the single-circuit plugin it replaces.

// `plugin_core::prelude` re-exports nice-plug's, plus the Dioxus editor
// pieces — one import rather than two that overlap.
use plugin_core::prelude::*;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use dsp::saturate::bluebox::Bluebox;
use dsp::saturate::deluxe::Deluxe;
use dsp::saturate::digital::DigitalStage;
use dsp::saturate::emphasis::EmphasisEq;
use dsp::saturate::preamp::ClassAPreamp;
use dsp::saturate::profiles::Engine;
use dsp::saturate::ui::params::{MAX_STAGES, SatParams, SatStageParams, SatUiState};
use dsp::stack::{Stage, StagePool};

const PLUGIN_NAME: &str = "FTS Saturate";

// ── Stage adapter ─────────────────────────────────────────────────────────

/// One complete saturator — the whole pre-stack plugin — as a stack
/// [`Stage`].
///
/// The pool runs in f64; the circuits run in f32, as they always have. The
/// stage narrows at its input and widens at its output: the pool's input is
/// the host's f32 buffer widened, so the narrowing is exact, and the
/// arithmetic between is the pre-stack plugin's to the bit.
struct SatStage {
    /// One stage per channel: the preamp's sag envelope, DC blocker and tilt
    /// filters are all per-channel state, so the two sides must not share.
    pre: [ClassAPreamp; 2],
    /// …and one quantiser per channel, for the same reason (the hold is
    /// state) plus one more: a shared dither sequence would collapse into the
    /// middle of the stereo image instead of sitting behind it.
    quantiser: [DigitalStage; 2],
    /// The emphasis / de-emphasis EQ pair (`fx.sat.emphasis`): applied
    /// around the whole wet path, per channel like the preamps.
    emph: [EmphasisEq; 2],
    /// The measured models. Stereo engines (Deluxe's detector hears the
    /// sum of both channels), built for the session's rate in `activate`
    /// because their filters are designed there — never on the audio
    /// thread.
    deluxe: Deluxe,
    bluebox: Bluebox,
    /// What the last [`Self::sync`] resolved.
    engine: Engine,
    crushing: bool,
    mix: f32,
    /// Output gain, linear.
    output: f32,
}

impl SatStage {
    fn new(sample_rate: f32) -> Self {
        Self {
            pre: [
                ClassAPreamp::new(sample_rate),
                ClassAPreamp::new(sample_rate),
            ],
            quantiser: [DigitalStage::new(), DigitalStage::new()],
            emph: [EmphasisEq::new(sample_rate), EmphasisEq::new(sample_rate)],
            deluxe: Deluxe::new(f64::from(sample_rate)),
            bluebox: Bluebox::new(f64::from(sample_rate)),
            engine: Engine::Preamp,
            crushing: false,
            mix: 1.0,
            output: 1.0,
        }
    }

    /// Re-home every processor at `sample_rate` and clear its state. The
    /// measured engines are rebuilt — allocation, so `activate` only.
    fn prepare(&mut self, sample_rate: f32) {
        for (pre, quantiser) in self.pre.iter_mut().zip(self.quantiser.iter_mut()) {
            pre.set_sample_rate(sample_rate);
            pre.reset();
            quantiser.reset();
        }
        for emph in &mut self.emph {
            emph.set_sample_rate(sample_rate);
        }
        self.deluxe = Deluxe::new(f64::from(sample_rate));
        self.bluebox = Bluebox::new(f64::from(sample_rate));
    }

    /// Point both channels at the stage's resolved profile (no allocation).
    ///
    /// Resolved through the *persisted id* rather than the automatable index,
    /// so growing the rail never repoints an old session at a different
    /// circuit — same contract the reverb and the compressor keep.
    fn sync(&mut self, p: &SatStageParams) {
        let profile = p.resolved_profile();
        let controls = dsp::saturate::profiles::Controls {
            drive: p.drive.value(),
            bias: p.bias.value(),
            sag: p.sag.value(),
            tilt: p.tilt.value(),
            character_a: p.character_a.value(),
            character_b: p.character_b.value(),
            mix: p.mix.value(),
        };
        // The emphasis bands, designed once per block (setters early-out on
        // unchanged values inside the biquad design's own cheapness — the
        // whole table is a handful of arithmetic).
        let bands = std::array::from_fn(|i| p.emph[i].to_band());
        for emph in &mut self.emph {
            if *emph.bands() != bands {
                emph.set_bands(&bands);
            }
        }
        // The makeup calibrates at the level the shaper actually sees — the
        // emphasis raises it exactly like the tilt does
        // (`fx.sat.emphasis.makeup`). Before `apply`, which refreshes makeup.
        let sigma_gain = self.emph[0].sigma_gain();
        for (pre, quantiser) in self.pre.iter_mut().zip(self.quantiser.iter_mut()) {
            pre.set_emphasis_sigma_gain(sigma_gain);
            dsp::saturate::profiles::apply(profile, &controls, pre, quantiser);
        }
        // The measured models take the same knobs through their own mapping.
        match profile.engine {
            Engine::Deluxe => self
                .deluxe
                .set(dsp::saturate::profiles::deluxe_params(&controls)),
            Engine::Bluebox { sat_freq, alt_tube } => {
                self.bluebox.set(dsp::saturate::profiles::bluebox_params(
                    &controls, sat_freq, alt_tube,
                ));
            }
            Engine::Preamp => {}
        }
        self.crushing = profile.voicing.digital && !self.quantiser[0].is_transparent();
        self.engine = profile.engine;
        self.mix = p.mix.value();
        self.output = 10.0f32.powf(p.output.value() / 20.0);
    }
}

impl Stage for SatStage {
    fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
        let output = self.output;
        if self.engine != Engine::Preamp {
            // A measured model is a whole stereo algorithm with its own mix
            // law (handed over in `sync`); the emphasis pair still wraps
            // what it distorts, and Output stays ours.
            for (left, right) in l.iter_mut().zip(r.iter_mut()) {
                let el = self.emph[0].pre(to_f32(*left));
                let er = self.emph[1].pre(to_f32(*right));
                let (wl, wr) = match self.engine {
                    Engine::Deluxe => self.deluxe.process(f64::from(el), f64::from(er)),
                    _ => self.bluebox.process(f64::from(el), f64::from(er)),
                };
                *left = f64::from(self.emph[0].post(to_f32(wl)) * output);
                *right = f64::from(self.emph[1].post(to_f32(wr)) * output);
            }
            return;
        }

        let mix = self.mix;
        for (left, right) in l.iter_mut().zip(r.iter_mut()) {
            for (ch, sample) in [left, right].into_iter().enumerate() {
                let dry = to_f32(*sample);
                // Emphasis in, the stage, the quantiser, de-emphasis out,
                // and only THEN the mix (`fx.sat.emphasis`): the EQ pair
                // wraps the whole wet path so it shapes what distorts —
                // including what the bitcrusher chews on — and is net-flat
                // around it. A flat EQ short-circuits to identity, keeping
                // the default path bit-exact.
                let emphasized = self.emph[ch].pre(dry);
                let mut wet = self.pre[ch].process_wet(ch, emphasized);
                if self.crushing {
                    wet = self.quantiser[ch].process(ch, wet);
                }
                wet = self.emph[ch].post(wet);
                *sample = f64::from((wet - dry).mul_add(mix, dry) * output);
            }
        }
    }

    /// Bluebox's oversampler delays its wet path (and its dry, to match);
    /// the other engines are zero-latency.
    fn latency(&self) -> usize {
        match self.engine {
            Engine::Bluebox { .. } => self.bluebox.latency(),
            _ => 0,
        }
    }

    fn reset(&mut self) {
        for (pre, quantiser) in self.pre.iter_mut().zip(self.quantiser.iter_mut()) {
            pre.reset();
            quantiser.reset();
        }
        for emph in &mut self.emph {
            emph.reset();
        }
        self.deluxe.reset();
        self.bluebox.reset();
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsSaturate {
    params: Arc<SatParams>,
    ui_state: Arc<SatUiState>,
    editor_state: Arc<DioxusState>,
    /// The stack: a fixed pool of complete saturators, topology from params.
    pool: StagePool<SatStage>,
    /// f64 scratch the pool processes in place, sized in `activate`.
    buf_l: Vec<f64>,
    buf_r: Vec<f64>,
    /// The latency last reported to the host — the slowest lane's
    /// (`fx.stack.latency`).
    latency: usize,
}

impl Default for FtsSaturate {
    fn default() -> Self {
        Self {
            params: Arc::new(SatParams::default()),
            ui_state: Arc::new(SatUiState::default()),
            // The editor sizes itself — see dsp::saturate::ui::control_view.
            editor_state: DioxusState::new(|| {
                (
                    dsp::saturate::ui::control_view::EDITOR_W,
                    dsp::saturate::ui::control_view::EDITOR_H,
                )
            })
            .with_resize_hint(dsp::saturate::ui::control_view::resize_hint()),
            pool: StagePool::new((0..MAX_STAGES).map(|_| SatStage::new(48_000.0)).collect()),
            buf_l: Vec::new(),
            buf_r: Vec::new(),
            latency: 0,
        }
    }
}

impl FtsSaturate {
    /// Push every in-use stage's params into its processors, then the
    /// topology into the pool (no allocation). A stage outside the stack is
    /// left alone; it is synced the block it is added.
    fn sync_params(&mut self) {
        for i in 0..MAX_STAGES {
            let sp = self.params.stage(i);
            if !sp.place.in_use.value() {
                continue;
            }
            if let Some(stage) = self.pool.stage_mut(i) {
                stage.sync(sp);
            }
        }
        dsp::stack::host::sync_pool(&mut self.pool, &*self.params);
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

    type Editor = plugin_core::nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            Arc::new(dsp::saturate::ui::control_view::SatUi {
                params: self.params.clone(),
                state: self.ui_state.clone(),
            }),
            dsp::saturate::ui::control_view::App,
        )
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl ActivateContext<Self>,
    ) -> bool {
        // Every stage is prepared here, not just the ones in the stack: a
        // stage's processors are small (filters, a few envelopes, a short
        // oversampler), and adding one on the audio thread must not
        // allocate.
        let mut max_stage_latency = 0;
        for i in 0..MAX_STAGES {
            if let Some(stage) = self.pool.stage_mut(i) {
                stage.prepare(buffer_config.sample_rate);
                max_stage_latency = max_stage_latency.max(stage.bluebox.latency());
            }
        }
        let max_block = buffer_config.max_buffer_size as usize;
        // Worst case a lane could need compensating by: every stage serial
        // on one lane, each a Bluebox.
        self.pool.prepare(max_block, max_stage_latency * MAX_STAGES);
        self.buf_l = vec![0.0; max_block];
        self.buf_r = vec![0.0; max_block];
        // Land the settings before the first block, so the tilt filters and
        // the sag ballistics start on the real values rather than ramping
        // out of a default nobody chose.
        self.sync_params();
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
        self.sync_params();
        let latency = self.pool.latency();
        if latency != self.latency {
            self.latency = latency;
            context.set_latency_samples(u32::try_from(latency).unwrap_or(u32::MAX));
        }

        let n = buffer.samples().min(self.buf_l.len());
        let mut input_peak = 0.0f32;
        let mut output_peak = 0.0f32;

        // Widen into the pool's f64 scratch (exact). A mono bus mirrors its
        // one channel, so a stage's stereo engines see plain mono.
        for (i, mut frame) in buffer.iter_samples().take(n).enumerate() {
            let mut it = frame.iter_mut();
            let (Some(l), r) = (it.next(), it.next()) else {
                continue;
            };
            input_peak = input_peak.max(l.abs());
            let left = *l;
            let right = r.map_or(left, |s| {
                input_peak = input_peak.max(s.abs());
                *s
            });
            self.buf_l[i] = f64::from(left);
            self.buf_r[i] = f64::from(right);
        }

        self.pool
            .process(&mut self.buf_l[..n], &mut self.buf_r[..n]);

        for (i, mut frame) in buffer.iter_samples().take(n).enumerate() {
            let mut it = frame.iter_mut();
            let (Some(l), r) = (it.next(), it.next()) else {
                continue;
            };
            let left = to_f32(self.buf_l[i]);
            output_peak = output_peak.max(left.abs());
            *l = left;
            if let Some(r) = r {
                let right = to_f32(self.buf_r[i]);
                output_peak = output_peak.max(right.abs());
                *r = right;
            }
        }

        // Meter ballistics: instant attack, a slow fall the editor can
        // read at whatever rate it happens to draw at. Both are the stack's
        // edges — what goes in, what comes out.
        store_peak(&self.ui_state.input_db, input_peak);
        store_peak(&self.ui_state.out_db, output_peak);

        ProcessStatus::Normal
    }
}

/// The measured models run in f64; the host's buffer is f32.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "narrowing to the host's f32 buffer is the point"
)]
const fn to_f32(x: f64) -> f32 {
    x as f32
}

/// Peak-hold in dB, falling 0.3 dB a block. The editor samples this when it
/// draws; a frame that misses an update simply draws the next one.
fn store_peak(slot: &atomic_float::AtomicF32, peak: f32) {
    let db = if peak > 0.0 {
        20.0 * peak.log10()
    } else {
        -100.0
    };
    let previous = slot.load(Ordering::Relaxed);
    slot.store(
        if db > previous { db } else { previous - 0.3 },
        Ordering::Relaxed,
    );
}

impl ClapPlugin for FtsSaturate {
    const CLAP_ID: &'static str = "com.fasttrackstudio.saturate";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Five circuits: tube, tape, transformer, transistor, and digital");
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

#[cfg(feature = "export")]
nice_export_clap!(FtsSaturate);
#[cfg(feature = "export")]
nice_export_vst3!(FtsSaturate);
