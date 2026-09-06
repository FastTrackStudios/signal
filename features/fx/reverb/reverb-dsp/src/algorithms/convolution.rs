//! Uniformly partitioned FFT convolution reverb.
//!
//! Loads an arbitrary impulse response (stereo) and convolves the input
//! against it in real time using overlap-save with `realfft`. Partition
//! size is fixed at 512 samples (≈10 ms latency at 48 kHz), which keeps
//! per-partition cost low while bounding the FFT size.
//!
//! Three independent modulation options (see [`ConvolutionModParams`])
//! lift the "frozen room" quality of static convolution:
//!
//! ```text
//! in ─► predelay (ModulatedDelay ×2, LFO)          [option 2, gated]
//!    ─► conv A ──┐
//!    └► conv B ──┤ equal-power morph (LFO-sweepable)  [option 3, gated]
//!                ▼
//!      Motion: 2× series ModulatedAllpass per ch      [option 1, gated]
//!                ▼
//!      damping Lp1 per ch (LFO on cutoff)             [option 2, gated]
//!                ▼
//!      wet gain (LFO ±dB · envelope duck)             [option 2, gated]
//! ```
//!
//! Every option is hard-gated at its neutral setting: with all depths at
//! 0 the signal path is bit-identical to the unmodulated convolver and
//! the extra stages cost (nearly) nothing.
//!
//! References:
//! - W. G. Gardner, "Efficient Convolution Without Input/Output Delay"
//!   (JAES 1995). Uniform partitioned convolution algorithm.
//! - Stockham, "High Speed Convolution and Correlation" (1966) —
//!   overlap-save FFT convolution baseline.
//! - <https://github.com/HiFi-LoFi/FFTConvolver> (open-source reference).
//! - <https://github.com/tiagolr/reevr> — modulated-convolution design
//!   the mod-source option mirrors.

use dsp_core::num;

use std::f64::consts::{FRAC_PI_2, PI};
use std::ops::{Index, IndexMut};
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::envelope::EnvelopeFollower;
use audiocore_dsp::smoothing::ParamSmoother;

use crate::algorithm::{
    AlgorithmParams, ConvolutionModParams, ImpulseParams, IrSlot, ReverbAlgorithm,
};
use crate::ir::prepared::{IrTrash, PreparedIr, PreparedIrPair, BLOCK, FFT_LEN, SPECTRUM_LEN};
use crate::ir::transforms::IrTransforms;
use crate::ir::IrAsset;
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::modulated_delay::ModulatedDelay;
use crate::primitives::one_pole::Lp1;

const MAX_IR_SECONDS: f64 = 8.0;

/// Control-rate granularity for coefficient-level updates (damping
/// cutoff, predelay retune, motion depth). Matches the chain's
/// `SMOOTH_BLOCK`: 0.67 ms at 48 kHz.
const CTRL_BLOCK: u32 = 32;

/// Below this a smoothed depth counts as "off" and its stage is gated out.
const GATE_EPS: f64 = 1.0e-4;

/// Max LFO→wet-gain swing at full depth, in dB.
const WET_MOD_DB: f64 = 6.0;
/// Max LFO→predelay swing at full depth, in seconds.
const PREDELAY_MOD_S: f64 = 0.020;
/// Max base predelay, in seconds.
const PREDELAY_MAX_S: f64 = 0.2;
/// Max LFO→damping swing at full depth, in octaves.
const DAMP_MOD_OCTAVES: f64 = 2.0;
/// Motion allpass max excursion at full depth, in seconds.
const MOTION_EXCURSION_S: f64 = 0.0015;
/// One motion-allpass voice. Three parallel `[_; 4]` tables before, all
/// walked by the same index.
struct MotionVoice {
    /// Allpass delay in samples at 48 kHz. The four are mutually prime,
    /// ~5–12 ms; scaled by the actual sample rate.
    delay_48k: usize,
    /// Rate multiplier, so the four motion LFOs never phase-lock.
    rate_mult: f64,
    /// LFO starting phase (stereo decorrelation).
    phase: f64,
}

/// The four motion voices, in stage order.
const MOTION_VOICES: [MotionVoice; 4] = [
    MotionVoice { delay_48k: 241, rate_mult: 1.0, phase: 0.0 },
    MotionVoice { delay_48k: 379, rate_mult: 1.13, phase: 0.25 },
    MotionVoice { delay_48k: 467, rate_mult: 0.91, phase: 0.5 },
    MotionVoice { delay_48k: 587, rate_mult: 1.07, phase: 0.75 },
];

/// Per-channel partitioned convolver.
struct PartitionedConv {
    fft_fwd: Arc<dyn RealToComplex<f64>>,
    fft_inv: Arc<dyn ComplexToReal<f64>>,

    /// Frequency-domain partitions of the IR (one Vec<Complex> per partition).
    ir_partitions: Vec<Vec<Complex<f64>>>,
    /// Outgoing IR during a hot-swap crossfade. Because the input
    /// history is IR-independent, both partition sets convolve against
    /// the SAME history — both tails are exact from the first fade
    /// block, no warmup pass needed (unlike engines whose convolver
    /// state couples to the IR).
    old_partitions: Vec<Vec<Complex<f64>>>,
    old_gain: f64,
    /// Remaining / total crossfade blocks (equal-power in the
    /// frequency domain — mixing spectra is linear, one IFFT).
    xfade_pos: u32,
    xfade_len: u32,
    /// Ring buffer of past input partitions in the frequency domain.
    /// The ring modulus is the BUFFER length (not the partition
    /// count), so partition sets of different lengths index it
    /// consistently during crossfades.
    input_history: Vec<Vec<Complex<f64>>>,
    history_head: usize,

    /// Time-domain working buffers.
    input_block: [f64; FFT_LEN],
    input_block_fill: usize,
    /// Last block of input samples kept around for overlap-save.
    prev_input_tail: [f64; BLOCK],
    /// Output block — second half of the IFFT result.
    output_block: [f64; BLOCK],
    output_block_read: usize,

    /// Reusable scratch.
    spectrum_scratch: Vec<Complex<f64>>,
    accumulator: Vec<Complex<f64>>,
    ifft_out: [f64; FFT_LEN],

    gain: f64,
}

impl PartitionedConv {
    fn new(planner: &mut RealFftPlanner<f64>) -> Self {
        let fft_fwd = planner.plan_fft_forward(FFT_LEN);
        let fft_inv = planner.plan_fft_inverse(FFT_LEN);
        Self {
            fft_fwd,
            fft_inv,
            ir_partitions: Vec::new(),
            old_partitions: Vec::new(),
            old_gain: 1.0,
            xfade_pos: 0,
            xfade_len: 0,
            input_history: Vec::new(),
            history_head: 0,
            input_block: [0.0; FFT_LEN],
            input_block_fill: 0,
            prev_input_tail: [0.0; BLOCK],
            output_block: [0.0; BLOCK],
            output_block_read: BLOCK, // empty initially
            spectrum_scratch: vec![Complex::new(0.0, 0.0); SPECTRUM_LEN],
            accumulator: vec![Complex::new(0.0, 0.0); SPECTRUM_LEN],
            ifft_out: [0.0; FFT_LEN],
            gain: 1.0,
        }
    }

    /// Replace the IR. Empty IR disables convolution (passthrough silence).
    /// Heavy — runs forward FFTs across every partition. Prefer
    /// [`Self::swap_prepared`] on the audio thread.
    fn load_ir(&mut self, ir: &[f64]) {
        let mut planner = RealFftPlanner::<f64>::new();
        let prepared = PreparedIr::build_with_planner(ir, &mut planner);
        // Control path — dropping the displaced buffers inline is fine.
        let _old = self.swap_prepared(prepared);
    }

    /// Audio-thread-safe IR replacement: buffer swaps only. Returns the
    /// DISPLACED partitions (inside the passed-in `PreparedIr`) so the
    /// caller can route them off-thread for deallocation — dropping the
    /// old `Vec<Vec<Complex>>` here would free on the audio thread.
    ///
    /// The input-history ring only ever GROWS (to the high-water
    /// partition count) and is zeroed on swap; the accumulate loop
    /// indexes modulo the partition count, so surplus entries are
    /// simply never touched. Steady-state Impulse knob sweeps therefore
    /// allocate nothing once the largest stretch has been visited.
    fn swap_prepared(&mut self, prepared: PreparedIr) -> PreparedIr {
        self.swap_prepared_ext(prepared, false)
    }

    /// `crossfade` = knob-driven reshape sweeps (equal-power fade, both
    /// tails exact against the shared history). Fresh IR LOADS pass
    /// false: a new impulse is a musical event and replaces instantly.
    fn swap_prepared_ext(&mut self, mut prepared: PreparedIr, crossfade: bool) -> PreparedIr {
        let n = prepared.partitions.len();
        if crossfade && n <= self.input_history.len() && !self.ir_partitions.is_empty() {
            // Crossfade path: retire the current IR into old_partitions
            // (whatever was there before goes back to the caller for
            // disposal) and equal-power fade over ~80 ms. Both sets
            // convolve the SAME input history, so the outgoing tail
            // rings on and the incoming tail is exact immediately.
            core::mem::swap(&mut self.old_partitions, &mut prepared.partitions);
            core::mem::swap(&mut self.ir_partitions, &mut self.old_partitions);
            self.old_gain = self.gain;
            self.gain = prepared.gain;
            self.xfade_len =
                num::f64_to_u32((0.08 * 48_000.0) / num::count_to_f64(BLOCK)).saturating_add(1);
            self.xfade_pos = self.xfade_len;
        } else {
            // Growth (or first load): the history ring must be resized,
            // which invalidates ring continuity — instant swap.
            core::mem::swap(&mut self.ir_partitions, &mut prepared.partitions);
            self.gain = prepared.gain;
            if self.input_history.len() < n {
                self.input_history.reserve(n.saturating_sub(self.input_history.len()));
                while self.input_history.len() < n {
                    self.input_history
                        .push(vec![Complex::new(0.0, 0.0); SPECTRUM_LEN]);
                }
            }
            // Zero so we don't multiply stale frequency-domain data
            // against the new IR.
            for h in &mut self.input_history {
                h.fill(Complex::new(0.0, 0.0));
            }
            self.history_head = 0;
            self.xfade_pos = 0;
            self.old_partitions.clear();
        }
        prepared
    }

    fn reset(&mut self) {
        for h in &mut self.input_history {
            h.fill(Complex::new(0.0, 0.0));
        }
        self.history_head = 0;
        self.input_block.fill(0.0);
        self.input_block_fill = 0;
        self.prev_input_tail.fill(0.0);
        self.output_block.fill(0.0);
        self.output_block_read = BLOCK;
    }

    /// Zero the frequency-domain history + time-domain context, leaving
    /// Pop the next sample of the finished output block, or silence once
    /// the block is exhausted. Total: `get` returns `None` exactly where
    /// the read pointer has run past `BLOCK`.
    #[inline]
    fn next_output(&mut self) -> f64 {
        let Some(&v) = self.output_block.get(self.output_block_read) else {
            return 0.0;
        };
        self.output_block_read = self.output_block_read.saturating_add(1);
        v
    }

    /// Stage one input sample at `fill`. Total: a `fill` past `FFT_LEN`
    /// drops the sample, which is what the caller's bounds check did.
    #[inline]
    fn push_input(&mut self, fill: usize, sample: f64) {
        if let Some(cell) = self.input_block.get_mut(fill) {
            *cell = sample;
        }
    }

    /// the staged input samples alone. Used when a gated-off slot B
    /// re-engages, so seconds-old audio doesn't burst out of its tail.
    fn clear_history(&mut self) {
        for h in &mut self.input_history {
            h.fill(Complex::new(0.0, 0.0));
        }
        self.history_head = 0;
        self.prev_input_tail.fill(0.0);
        self.output_block.fill(0.0);
        self.xfade_pos = 0;
    }

    /// Process one block of size BLOCK in/out.
    fn process_block(&mut self, in_block: &[f64; BLOCK]) {
        if self.ir_partitions.is_empty() {
            self.output_block.fill(0.0);
            return;
        }

        // Build FFT input: [prev_tail | current_block] of length FFT_LEN.
        self.input_block[..BLOCK].copy_from_slice(&self.prev_input_tail);
        self.input_block[BLOCK..].copy_from_slice(in_block);
        self.prev_input_tail.copy_from_slice(in_block);

        // Forward FFT.
        // Buffer lengths are fixed at construction, so neither the slot lookup
        // nor the transform can fail. Skip rather than panic: this runs on the
        // render callback.
        if let Some(spec) = self.input_history.get_mut(self.history_head) {
            let _ = self.fft_fwd.process(&mut self.input_block.clone(), spec);
        }

        // Accumulate Σ_k IR[k] * Input[t - k]. Ring modulus is the
        // history length so differently-sized partition sets (during a
        // crossfade) index consistently.
        for s in &mut self.accumulator {
            *s = Complex::new(0.0, 0.0);
        }
        let hist_len = self.input_history.len();
        let n_parts = self.ir_partitions.len();
        let (mut w_new, mut w_old) = (1.0f64, 0.0f64);
        if self.xfade_pos > 0 {
            // Equal-power fade, advanced per block.
            let t = 1.0 - f64::from(self.xfade_pos) / f64::from(self.xfade_len);
            let theta = t * core::f64::consts::FRAC_PI_2;
            w_new = theta.sin();
            w_old = theta.cos();
            self.xfade_pos = self.xfade_pos.saturating_sub(1);
        }
        let w = Complex::new(w_new * self.gain, 0.0);
        for (p, ir_p) in self.ir_partitions.iter().enumerate().take(n_parts) {
            let hist_idx = self
                .history_head
                .saturating_add(hist_len)
                .saturating_sub(p)
                .checked_rem(hist_len)
                .unwrap_or(0);
            let Some(in_p) = self.input_history.get(hist_idx) else {
                continue;
            };
            for ((acc, ir), inp) in self
                .accumulator
                .iter_mut()
                .zip(ir_p.iter())
                .zip(in_p.iter())
                .take(SPECTRUM_LEN)
            {
                *acc += *ir * *inp * w;
            }
        }
        if w_old > 0.0 {
            let n_old = self.old_partitions.len().min(hist_len);
            let w = Complex::new(w_old * self.old_gain, 0.0);
            for (p, ir_p) in self.old_partitions.iter().enumerate().take(n_old) {
                let hist_idx = self
                    .history_head
                    .saturating_add(hist_len)
                    .saturating_sub(p)
                    .checked_rem(hist_len)
                    .unwrap_or(0);
                let Some(in_p) = self.input_history.get(hist_idx) else {
                    continue;
                };
                for ((acc, ir), inp) in self
                    .accumulator
                    .iter_mut()
                    .zip(ir_p.iter())
                    .zip(in_p.iter())
                    .take(SPECTRUM_LEN)
                {
                    *acc += *ir * *inp * w;
                }
            }
        }

        // Inverse FFT.
        self.spectrum_scratch.copy_from_slice(&self.accumulator);
        // Fixed-length buffers; cannot fail. Never panic on the render path.
        let _ = self.fft_inv
            .process(&mut self.spectrum_scratch, &mut self.ifft_out);

        // Take the second half — discard wrap-around (overlap-save).
        // Gains are already folded into the accumulation weights (the
        // two IRs in a crossfade can carry different makeup gains).
        // Take the second half — discard wrap-around (overlap-save).
        for (out, tail) in self
            .output_block
            .iter_mut()
            .zip(self.ifft_out.iter().skip(BLOCK))
            .take(BLOCK)
        {
            *out = *tail;
        }

        // Advance ring buffer head (modulo the history length).
        self.history_head = self
            .history_head
            .saturating_add(1)
            .checked_rem(hist_len)
            .unwrap_or(0);
    }
}

/// Truncate an IR to at most `max` samples. Total: an IR already shorter
/// than the cap passes through unchanged.
fn cap_ir(ir: &[f64], max: usize) -> &[f64] {
    ir.get(..max).unwrap_or(ir)
}

/// Generate a synthetic stereo IR — exponentially decaying velvet-style
/// pattern. Used as the default IR until the user loads their own.
fn synthesize_ir(sample_rate: f64, seconds: f64, seed: u64) -> Vec<f64> {
    use crate::primitives::lcg_random::LcgRandom;
    let n = num::f64_to_index(sample_rate * seconds);
    let mut rng = LcgRandom::new(seed);
    let mut ir = vec![0.0; n];

    // Sparse positive/negative impulses with exponential envelope.
    let density = 2500.0;
    let spacing = num::f64_to_index(sample_rate / density).max(1);
    let t60_samples = num::count_to_f64(n);
    let mut pos = 0usize;
    while pos < n {
        let jitter = num::f64_to_index(rng.next_float() * num::count_to_f64(spacing));
        let idx = (pos.saturating_add(jitter)).min(n.saturating_sub(1));
        let sign = if rng.next_float() < 0.5 { -1.0 } else { 1.0 };
        let env = 10f64.powf(-3.0 * num::count_to_f64(idx) / t60_samples);
        if let Some(sample) = ir.get_mut(idx) {
            *sample = sign * env;
        }
        pos = pos.saturating_add(spacing);
    }

    // Pre-delay window: blend out the first ~5ms so direct signal isn't
    // doubled when wet/dry are summed.
    let predelay = num::f64_to_index(sample_rate * 0.005);
    let window = num::count_to_f64(predelay);
    for (i, sample) in ir.iter_mut().take(predelay).enumerate() {
        *sample *= num::count_to_f64(i) / window;
    }
    ir
}

/// Smoothers for the continuous modulation params. Ramp times follow the
/// chain's round-2 conventions: gain-like depths 10 ms, coefficient-level
/// params (morph position, base predelay) 30 ms.
struct ModSmoothers {
    motion_depth: ParamSmoother, // 10 ms
    wet_depth: ParamSmoother,    // 10 ms
    pd_depth: ParamSmoother,     // 10 ms
    damp_depth: ParamSmoother,   // 10 ms
    duck_depth: ParamSmoother,   // 10 ms
    predelay_ms: ParamSmoother,  // 30 ms
    morph: ParamSmoother,        // 30 ms
    morph_lfo: ParamSmoother,    // 10 ms
}

impl ModSmoothers {
    fn new(sample_rate: f64) -> Self {
        let mk = |ms: f64| {
            let mut s = ParamSmoother::new(0.0);
            s.set_time_ms(ms, sample_rate);
            s.set_epsilon(1e-5);
            s
        };
        Self {
            motion_depth: mk(10.0),
            wet_depth: mk(10.0),
            pd_depth: mk(10.0),
            damp_depth: mk(10.0),
            duck_depth: mk(10.0),
            predelay_ms: mk(30.0),
            morph: mk(30.0),
            morph_lfo: mk(10.0),
        }
    }

    fn set_sample_rate(&mut self, sr: f64) {
        self.motion_depth.set_time_ms(10.0, sr);
        self.wet_depth.set_time_ms(10.0, sr);
        self.pd_depth.set_time_ms(10.0, sr);
        self.damp_depth.set_time_ms(10.0, sr);
        self.duck_depth.set_time_ms(10.0, sr);
        self.predelay_ms.set_time_ms(30.0, sr);
        self.morph.set_time_ms(30.0, sr);
        self.morph_lfo.set_time_ms(10.0, sr);
    }

    fn set_targets(&mut self, p: &ConvolutionModParams, snap: bool) {
        let pairs: [(&mut ParamSmoother, f64); 8] = [
            (&mut self.motion_depth, p.motion_depth.clamp(0.0, 1.0)),
            (&mut self.wet_depth, p.mod_wet_depth.clamp(-1.0, 1.0)),
            (&mut self.pd_depth, p.mod_predelay_depth.clamp(-1.0, 1.0)),
            (&mut self.damp_depth, p.mod_damp_depth.clamp(-1.0, 1.0)),
            (&mut self.duck_depth, p.duck_wet_depth.clamp(0.0, 1.0)),
            (
                &mut self.predelay_ms,
                p.predelay_ms.clamp(0.0, PREDELAY_MAX_S * 1000.0),
            ),
            (&mut self.morph, p.morph.clamp(0.0, 1.0)),
            (&mut self.morph_lfo, p.morph_lfo_depth.clamp(0.0, 1.0)),
        ];
        for (s, v) in pairs {
            if snap {
                s.set_immediate(v);
            } else {
                s.set_target(v);
            }
        }
    }

    #[inline]
    fn tick(&mut self) {
        self.motion_depth.tick();
        self.wet_depth.tick();
        self.pd_depth.tick();
        self.damp_depth.tick();
        self.duck_depth.tick();
        self.predelay_ms.tick();
        self.morph.tick();
        self.morph_lfo.tick();
    }
}

/// A stereo pair of convolvers. `.l` is always the leg fed by the left
/// input — for `cross` that is the L → R leg.
///
/// Six `conv_*` fields before, addressed in pairs at every call site.
struct ConvPair {
    l: PartitionedConv,
    r: PartitionedConv,
}

impl ConvPair {
    fn new(planner: &mut RealFftPlanner<f64>) -> Self {
        Self {
            l: PartitionedConv::new(planner),
            r: PartitionedConv::new(planner),
        }
    }

    fn load_ir(&mut self, left: &[f64], right: &[f64]) {
        self.l.load_ir(left);
        self.r.load_ir(right);
    }
}

/// The optional stages around the convolver, engaged independently by
/// `ctrl_refresh`. One struct rather than three loose `*_engaged` bools.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Stages {
    predelay: bool,
    motion: bool,
    damping: bool,
}

pub struct Convolution {
    planner: RealFftPlanner<f64>,
    /// IR slot A's direct legs.
    direct_a: ConvPair,
    /// IR slot B — pre-allocated, gated off until the morph engages.
    direct_b: ConvPair,
    /// True-stereo cross legs (input L → out R, input R → out L),
    /// engaged only while a 4-leg IR occupies slot A.
    cross: ConvPair,
    true_stereo: bool,
    /// Un-shaped cross originals (LR, RL) for Impulse re-shaping.
    #[expect(clippy::type_complexity, reason = "true-stereo cross-channel IR storage requires paired tuples")]
    cross_originals: Option<(Arc<Vec<f64>>, Arc<Vec<f64>>)>,
    sample_rate: f64,
    ir_seconds: f64,
    /// True once a user-supplied IR has been loaded — disables the
    /// synthetic-IR rebuild on `set_params` so user choices stick.
    /// Disposal channel for buffers displaced by audio-thread swaps.
    /// `None` (tests / offline) drops inline.
    trash_tx: Option<crossbeam_channel::Sender<IrTrash>>,

    // ── Modulation options ────────────────────────────────────────────
    mod_params: ConvolutionModParams,
    sm: ModSmoothers,
    /// Shared mod-LFO phase (0..1) and per-sample increment.
    lfo_phase: f64,
    lfo_inc: f64,
    /// Envelope follower on the input (5 ms / 200 ms) for wet ducking.
    env: EnvelopeFollower,
    /// Control-rate countdown for coefficient-level refreshes.
    ctrl_countdown: u32,

    /// Which of the three post/pre-convolution stages are engaged. The
    /// control-rate pass sets them; the sample loop reads them.
    engaged: Stages,

    // Option 2: modulatable predelay before the convolver.
    predelay_l: ModulatedDelay,
    predelay_r: ModulatedDelay,

    // Option 1: post-conv motion stage [L1, L2, R1, R2].
    motion: [ModulatedAllpass; 4],

    // Option 2: post-conv damping filters.
    damp_l: Lp1,
    damp_r: Lp1,
    /// Base damping cutoff derived from `AlgorithmParams::damping`.
    base_damp_cutoff: f64,

    // Option 3: morph gate state.
    b_engaged: bool,

    // ── BigSky MX Impulse live params ─────────────────────────────────
    impulse: ImpulseParams,
    /// Per-slot IR state — see [`IrSlotState`].
    slots: IrSlots,
    /// Smoothed feedback amount (10 ms) + recirculation state.
    fb_smoother: ParamSmoother,
    fb_l: f64,
    fb_r: f64,
    fb_dc_l: DcBlocker,
    fb_dc_r: DcBlocker,
}

/// Everything one IR slot owns.
///
/// Three parallel `[_; 2]` arrays before, all indexed by `slot_idx(slot)` at
/// twenty-two call sites.
#[derive(Default)]
struct IrSlotState {
    /// Shape actually baked into the active partitions.
    applied_shape: ImpulseParams,
    /// Set when the partitions are stale against `impulse`'s shaping params.
    shape_dirty: bool,
    /// Set once the user loads their own IR into this slot; until then
    /// `set_params` keeps rebuilding the synthetic one.
    user_loaded: bool,
    /// The original, un-shaped stereo IR, kept for re-preparation.
    #[expect(
        clippy::type_complexity,
        reason = "a stereo IR is a pair of shared buffers; naming the pair would not make it simpler"
    )]
    original: Option<(Arc<Vec<f64>>, Arc<Vec<f64>>)>,
}

/// The two IR slots, addressed by [`IrSlot`] rather than by a raw index.
///
/// The `Index` impls are total — every `IrSlot` names a field — so the slot
/// lookups that used to be `self.slots[slot]` cannot panic.
#[derive(Default)]
struct IrSlots {
    a: IrSlotState,
    b: IrSlotState,
}

impl IrSlots {
    /// Both slots, mutably, for the passes that touch each in turn.
    fn iter_mut(&mut self) -> impl Iterator<Item = &mut IrSlotState> {
        [&mut self.a, &mut self.b].into_iter()
    }
}

impl Index<IrSlot> for IrSlots {
    type Output = IrSlotState;

    fn index(&self, slot: IrSlot) -> &IrSlotState {
        match slot {
            IrSlot::A => &self.a,
            IrSlot::B => &self.b,
        }
    }
}

impl IndexMut<IrSlot> for IrSlots {
    fn index_mut(&mut self, slot: IrSlot) -> &mut IrSlotState {
        match slot {
            IrSlot::A => &mut self.a,
            IrSlot::B => &mut self.b,
        }
    }
}

impl Convolution {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let mut planner = RealFftPlanner::<f64>::new();
        let mut direct_a = ConvPair::new(&mut planner);
        let mut direct_b = ConvPair::new(&mut planner);
        let cross = ConvPair::new(&mut planner);

        let ir_l = synthesize_ir(sample_rate, 1.5, 0x00C0_FFEE);
        let ir_r = synthesize_ir(sample_rate, 1.5, 0x0BAD_BEEF);
        direct_a.load_ir(&ir_l, &ir_r);
        // Slot B ships with a differently-seeded velvet IR so the morph
        // is audible before the user loads anything.
        let b_ir_l = synthesize_ir(sample_rate, 1.5, 0x005E_ED0B);
        let b_ir_r = synthesize_ir(sample_rate, 1.5, 0x000D_DB17);
        direct_b.load_ir(&b_ir_l, &b_ir_r);
        let slots = IrSlots {
            a: IrSlotState {
                original: Some((Arc::new(ir_l), Arc::new(ir_r))),
                ..IrSlotState::default()
            },
            b: IrSlotState {
                original: Some((Arc::new(b_ir_l), Arc::new(b_ir_r))),
                ..IrSlotState::default()
            },
        };

        let mut env = EnvelopeFollower::new(0.0);
        env.set_times_ms(5.0, 200.0, sample_rate);

        let mut predelay_l = ModulatedDelay::new();
        let mut predelay_r = ModulatedDelay::new();
        predelay_l.set_sample_rate(sample_rate);
        predelay_r.set_sample_rate(sample_rate);
        predelay_l.sample_delay = 1;
        predelay_r.sample_delay = 1;

        let motion = Self::build_motion(sample_rate);

        let mod_params = ConvolutionModParams::default();

        Self {
            planner,
            engaged: Stages::default(),
            direct_a,
            direct_b,
            cross,
            true_stereo: false,
            cross_originals: None,
            sample_rate,
            ir_seconds: 1.5,
            trash_tx: None,
            mod_params,
            sm: ModSmoothers::new(sample_rate),
            lfo_phase: 0.0,
            lfo_inc: mod_params.lfo_rate / sample_rate,
            env,
            ctrl_countdown: 0,
            predelay_l,
            predelay_r,
            motion,
            damp_l: Lp1::new(),
            damp_r: Lp1::new(),
            base_damp_cutoff: (1.0_f64 - 0.3).mul_add(14000.0, 2000.0),
            b_engaged: false,
            impulse: ImpulseParams::default(),

            slots,
            fb_smoother: {
                let mut s = ParamSmoother::new(0.0);
                s.set_time_ms(10.0, sample_rate);
                s.set_epsilon(1e-5);
                s
            },
            fb_l: 0.0,
            fb_r: 0.0,
            fb_dc_l: DcBlocker::new(),
            fb_dc_r: DcBlocker::new(),
        }
    }

    fn build_motion(sample_rate: f64) -> [ModulatedAllpass; 4] {
        let s = sample_rate / 48000.0;
        MOTION_VOICES.map(|voice| {
            let mut ap = ModulatedAllpass::with_phase(voice.phase);
            ap.set_sample_rate(sample_rate);
            ap.set_delay_samples(num::f64_to_index(num::count_to_f64(voice.delay_48k) * s));
            ap.set_feedback(0.5);
            ap
        })
    }

    /// Replace the convolution IR in slot A. `ir_l` and `ir_r` may be
    /// different lengths (zero-padded internally) and are truncated to
    /// `MAX_IR_SECONDS`.
    pub fn load_ir_stereo(&mut self, ir_l: &[f64], ir_r: &[f64]) {
        self.load_ir_stereo_slot(ir_l, ir_r, IrSlot::A);
    }

    /// Slot-addressed synchronous IR load (runs FFTs — background/setup
    /// use only).
    ///
    /// Per the `BigSky` MX manual, loading a new IR resets the Impulse
    /// shaping params to defaults (the chain preserves mix).
    pub fn load_ir_stereo_slot(&mut self, ir_l: &[f64], ir_r: &[f64], slot: IrSlot) {
        let max = num::f64_to_index(self.sample_rate * MAX_IR_SECONDS);
        let cap_l = cap_ir(ir_l, max);
        let cap_r = cap_ir(ir_r, max);
        match slot {
            IrSlot::A => {
                self.direct_a.load_ir(cap_l, cap_r);
                self.slots[IrSlot::A].user_loaded = true;
                self.disengage_true_stereo();
            }
            IrSlot::B => {
                self.direct_b.load_ir(cap_l, cap_r);
                self.slots[IrSlot::B].user_loaded = true;
            }
        }
        self.slots[slot].original = Some((Arc::new(cap_l.to_vec()), Arc::new(cap_r.to_vec())));
        self.on_new_r_loaded(slot);
    }

    /// True-stereo (4-leg) synchronous load into slot A: LL/RR feed
    /// the direct convolvers, LR/RL the cross pair. Runs FFTs —
    /// background/setup use only.
    pub fn load_ir_true_stereo(&mut self, ll: &[f64], lr: &[f64], rl: &[f64], rr: &[f64]) -> bool {
        let max = num::f64_to_index(self.sample_rate * MAX_IR_SECONDS);
        let cap = |x: &[f64]| cap_ir(x, max).to_vec();
        let (ll, lr, rl, rr) = (cap(ll), cap(lr), cap(rl), cap(rr));
        self.direct_a.load_ir(&ll, &rr);
        self.cross.load_ir(&lr, &rl);
        self.slots[IrSlot::A].user_loaded = true;
        self.true_stereo = true;
        self.slots[IrSlot::A].original = Some((Arc::new(ll), Arc::new(rr)));
        self.cross_originals = Some((Arc::new(lr), Arc::new(rl)));
        self.on_new_r_loaded(IrSlot::A);
        true
    }

    /// Cross reshape originals for the chain's reshape pump.
    #[expect(clippy::type_complexity, reason = "true-stereo cross-channel IR return type")]
    #[must_use]
    pub fn cross_reshape_source(&self) -> Option<(Arc<Vec<f64>>, Arc<Vec<f64>>)> {
        if self.true_stereo {
            self.cross_originals.clone()
        } else {
            None
        }
    }

    /// Drop back to plain stereo: flag off, cross originals discarded
    /// off-thread. Cross partitions stay allocated (silent — never
    /// processed while disengaged) so re-engaging costs no allocation
    /// churn beyond the loads themselves.
    fn disengage_true_stereo(&mut self) {
        self.true_stereo = false;
        if let Some((lr, rl)) = self.cross_originals.take() {
            self.discard(IrTrash::Raw(lr));
            self.discard(IrTrash::Raw(rl));
        }
    }

    /// New-IR bookkeeping: shaping params reset to defaults (manual
    /// behavior), fresh partitions are the identity shape.
    fn on_new_r_loaded(&mut self, slot: IrSlot) {
        let fb = 0.0;
        self.impulse = ImpulseParams::default();
        self.fb_smoother.set_target(fb);
        self.slots[slot].applied_shape = ImpulseParams::default();
        self.slots[slot].shape_dirty = false;
    }

    /// Forget the user IRs (both slots) and resume synthetic-IR rebuilds
    /// on `set_params`. Restores the default procedural reverb.
    pub fn clear_user_ir(&mut self) {
        self.slots[IrSlot::A].user_loaded = false;
        self.slots[IrSlot::B].user_loaded = false;
        self.disengage_true_stereo();
        self.rebuild_synth_ir(self.ir_seconds);
    }

    /// Audio-thread-safe IR replacement for slot A. Accepts a pair
    /// already FFT-precomputed on a background thread.
    pub fn swap_prepared_pair(&mut self, pair: PreparedIrPair) {
        self.swap_prepared_pair_slot(pair, IrSlot::A);
    }

    /// Slot-addressed audio-thread-safe IR replacement. No allocations
    /// beyond the input-history resize (which only happens when the
    /// partition count changes). A `reshape`-tagged pair is routed to
    /// `Self::swap_reshaped` (no param reset); a fresh load resets the
    /// impulse shaping params per the MX manual and retains the raw IR
    /// (when carried) as the re-shape original.
    pub fn swap_prepared_pair_slot(&mut self, pair: PreparedIrPair, slot: IrSlot) {
        if pair.reshape {
            self.swap_reshaped(pair, slot);
            return;
        }
        let raw = pair.raw.clone();
        let (old_l, old_r) = match slot {
            IrSlot::A => {
                let l = self.direct_a.l.swap_prepared(pair.left);
                let r = self.direct_a.r.swap_prepared(pair.right);
                self.slots[IrSlot::A].user_loaded = true;
                // True-stereo pairs install the cross legs; plain
                // stereo pairs disengage them.
                if let Some(cross) = pair.cross {
                    let (plr, prl) = *cross;
                    let old_lr = self.cross.l.swap_prepared(plr);
                    let old_rl = self.cross.r.swap_prepared(prl);
                    self.discard(IrTrash::Prepared(old_lr));
                    self.discard(IrTrash::Prepared(old_rl));
                    self.true_stereo = true;
                    let prev = match pair.cross_raw {
                        Some(cr) => self.cross_originals.replace(cr),
                        None => self.cross_originals.take(),
                    };
                    if let Some((plr, prl)) = prev {
                        self.discard(IrTrash::Raw(plr));
                        self.discard(IrTrash::Raw(prl));
                    }
                } else {
                    self.disengage_true_stereo();
                }
                (l, r)
            }
            IrSlot::B => {
                let l = self.direct_b.l.swap_prepared(pair.left);
                let r = self.direct_b.r.swap_prepared(pair.right);
                self.slots[IrSlot::B].user_loaded = true;
                (l, r)
            }
        };
        self.discard(IrTrash::Prepared(old_l));
        self.discard(IrTrash::Prepared(old_r));
        // Retain (or clear) the un-shaped original; the previous Arc's
        // refcount decrement also goes through the trash chute — it may
        // be the last reference.
        let prev = match raw {
            Some(raw) => self.slots[slot].original.replace(raw),
            // No raw retained (direct PreparedIrPair::build) — shaping
            // can't re-derive from this load.
            None => self.slots[slot].original.take(),
        };
        if let Some((prev_l, prev_r)) = prev {
            self.discard(IrTrash::Raw(prev_l));
            self.discard(IrTrash::Raw(prev_r));
        }
        self.on_new_r_loaded(slot);
    }

    /// Route displaced buffers to the disposal worker; drop inline when
    /// no worker is attached (offline / tests).
    #[inline]
    fn discard(&self, trash: IrTrash) {
        if let Some(tx) = &self.trash_tx {
            // Send failure falls through to dropping inline.
            let _ = tx.send(trash);
        }
    }

    /// Return leg of the re-preparation pipeline: swap partitions only,
    /// leaving impulse params and user-IR flags untouched.
    fn swap_reshaped(&mut self, pair: PreparedIrPair, slot: IrSlot) {
        if slot == IrSlot::A && self.true_stereo {
            if let Some(cross) = pair.cross {
                let (plr, prl) = *cross;
                let old_lr = self.cross.l.swap_prepared_ext(plr, true);
                let old_rl = self.cross.r.swap_prepared_ext(prl, true);
                self.discard(IrTrash::Prepared(old_lr));
                self.discard(IrTrash::Prepared(old_rl));
            }
        }
        let (old_l, old_r) = match slot {
            IrSlot::A => (
                self.direct_a.l.swap_prepared_ext(pair.left, true),
                self.direct_a.r.swap_prepared_ext(pair.right, true),
            ),
            IrSlot::B => (
                self.direct_b.l.swap_prepared_ext(pair.left, true),
                self.direct_b.r.swap_prepared_ext(pair.right, true),
            ),
        };
        self.discard(IrTrash::Prepared(old_l));
        self.discard(IrTrash::Prepared(old_r));
    }

    /// Impulse shaping params currently targeted (not necessarily baked
    /// into the partitions yet — re-preparation is asynchronous).
    #[must_use]
    pub const fn impulse_params(&self) -> ImpulseParams {
        self.impulse
    }

    /// Push Impulse params. Feedback applies at the next tick (10 ms
    /// smoothed; `snap` lands it instantly); shaping changes mark the
    /// affected slots dirty for background re-preparation.
    pub fn set_impulse(&mut self, p: &ImpulseParams, snap: bool) {
        let fb = p.feedback.clamp(0.0, 1.0);
        if snap {
            self.fb_smoother.set_immediate(fb);
        } else {
            self.fb_smoother.set_target(fb);
        }
        let shape_changed_a = p.shape_key() != self.slots[IrSlot::A].applied_shape.shape_key();
        let shape_changed_b = p.shape_key() != self.slots[IrSlot::B].applied_shape.shape_key();
        self.impulse = *p;
        self.slots[IrSlot::A].shape_dirty = shape_changed_a && self.slots[IrSlot::A].original.is_some();
        self.slots[IrSlot::B].shape_dirty = shape_changed_b && self.slots[IrSlot::B].original.is_some();
    }

    /// Synchronous re-preparation for headless/test use. Applies the
    /// current shaping params to both slots' originals and reloads the
    /// partitions in place. Runs FFTs on the calling thread — NOT
    /// RT-safe; real-time hosts use `ImpulseReshaper` instead.
    pub fn reprepare_now(&mut self) {
        for slot in [IrSlot::A, IrSlot::B] {
            let Some((l, r)) = self.slots[slot].original.clone() else {
                continue;
            };
            let t = IrTransforms::from_impulse(&self.impulse);
            let asset = IrAsset::from_stereo((*l).clone(), (*r).clone(), self.sample_rate);
            let (sl, sr) = t.apply(&asset);
            match slot {
                IrSlot::A => {
                    self.direct_a.load_ir(&sl, &sr);
                    if self.true_stereo {
                        if let Some((lr, rl)) = self.cross_originals.clone() {
                            let (slr, srl) =
                                t.apply_pair((*lr).clone(), (*rl).clone(), self.sample_rate);
                            self.cross.load_ir(&slr, &srl);
                        }
                    }
                }
                IrSlot::B => {
                    self.direct_b.load_ir(&sl, &sr);
                }
            }
            self.slots[slot].applied_shape = self.impulse;
            self.slots[slot].shape_dirty = false;
        }
    }

    /// Push the modulation options. `snap` lands the continuous params
    /// instantly (preset load); otherwise they ramp (automation).
    pub fn set_mod_params(&mut self, p: &ConvolutionModParams, snap: bool) {
        self.mod_params = *p;
        self.lfo_inc = p.lfo_rate.clamp(0.01, 20.0) / self.sample_rate;
        self.sm.set_targets(p, snap);
        // Rates apply immediately (rate zipper is inaudible on sub-Hz LFOs).
        self.refresh_motion_rates();
        let pd_rate = self.lfo_inc;
        self.predelay_l.mod_rate = pd_rate;
        self.predelay_r.mod_rate = pd_rate;
    }

    /// Current modulation options (targets, not ramp positions).
    #[must_use]
    pub const fn mod_params(&self) -> ConvolutionModParams {
        self.mod_params
    }

    fn refresh_motion_rates(&mut self) {
        let rate = self.mod_params.motion_rate.clamp(0.02, 10.0);
        let depth_samples = self.sm.motion_depth.value() * MOTION_EXCURSION_S * self.sample_rate;
        for (ap, voice) in self.motion.iter_mut().zip(&MOTION_VOICES) {
            ap.set_modulation(rate * voice.rate_mult, depth_samples, self.sample_rate);
        }
    }

    fn rebuild_synth_ir(&mut self, seconds: f64) {
        if !self.slots[IrSlot::A].user_loaded {
            let ir_l = synthesize_ir(self.sample_rate, seconds, 0x00C0_FFEE);
            let ir_r = synthesize_ir(self.sample_rate, seconds, 0x0BAD_BEEF);
            self.direct_a.l.load_ir(&ir_l);
            self.direct_a.r.load_ir(&ir_r);
            self.slots[IrSlot::A].original = Some((Arc::new(ir_l), Arc::new(ir_r)));
            self.slots[IrSlot::A].applied_shape = ImpulseParams::default();
        }
        if !self.slots[IrSlot::B].user_loaded {
            let ir_l = synthesize_ir(self.sample_rate, seconds, 0x005E_ED0B);
            let ir_r = synthesize_ir(self.sample_rate, seconds, 0x000D_DB17);
            self.direct_b.l.load_ir(&ir_l);
            self.direct_b.r.load_ir(&ir_r);
            self.slots[IrSlot::B].original = Some((Arc::new(ir_l), Arc::new(ir_r)));
            self.slots[IrSlot::B].applied_shape = ImpulseParams::default();
        }
        self.ir_seconds = seconds;
    }

    /// Control-rate maintenance: engage/disengage gates and refresh
    /// coefficient-level targets. Runs every [`CTRL_BLOCK`] samples.
    fn ctrl_refresh(&mut self, lfo: f64) {
        // ── Predelay gate ──
        let base_ms = self.sm.predelay_ms.value();
        let pd_depth = self.sm.pd_depth.value();
        let want_pd = base_ms > 0.01 || pd_depth.abs() > GATE_EPS;
        if want_pd {
            let base_samples = num::f64_to_index(base_ms * 0.001 * self.sample_rate).max(1);
            let mod_amount = pd_depth * PREDELAY_MOD_S * self.sample_rate;
            self.predelay_l.sample_delay = base_samples;
            self.predelay_r.sample_delay = base_samples;
            self.predelay_l.mod_amount = mod_amount;
            self.predelay_r.mod_amount = mod_amount;
        }
        self.engaged.predelay = want_pd;

        // ── Motion gate ──
        let want_motion = self.sm.motion_depth.value() > GATE_EPS;
        if want_motion {
            if !self.engaged.motion {
                for ap in &mut self.motion {
                    ap.reset();
                }
            }
            if !self.sm.motion_depth.is_settled() || !self.engaged.motion {
                self.refresh_motion_rates();
            }
        }
        self.engaged.motion = want_motion;

        // ── Damping gate ──
        let damp_depth = self.sm.damp_depth.value();
        let want_damp = damp_depth.abs() > GATE_EPS;
        if want_damp {
            if !self.engaged.damping {
                self.damp_l.reset();
                self.damp_r.reset();
            }
            // Neutral point is wide open: as |depth| → 0 the cutoff glides
            // to 20 kHz so the gate transition is inaudible. The LFO then
            // swings the cutoff ±DAMP_MOD_OCTAVES·depth around that.
            let w = damp_depth.abs().min(1.0);
            let base = self.base_damp_cutoff.clamp(200.0, 20000.0);
            let anchored = base.powf(w) * 20000.0_f64.powf(1.0 - w);
            let swung = anchored * (DAMP_MOD_OCTAVES * damp_depth * lfo).exp2();
            let cutoff = swung.clamp(200.0, 20000.0);
            self.damp_l.set_freq(cutoff, self.sample_rate);
            self.damp_r.set_freq(cutoff, self.sample_rate);
        }
        self.engaged.damping = want_damp;

        // ── Morph gate ──
        let want_b = self.sm.morph.value() > GATE_EPS
            || self.sm.morph_lfo.value() > GATE_EPS
            || !self.sm.morph.is_settled();
        if want_b && !self.b_engaged {
            // B has been idle — its history holds seconds-old audio.
            // Clear so the tail fades in from silence under the ramp.
            self.direct_b.l.clear_history();
            self.direct_b.r.clear_history();
        }
        self.b_engaged = want_b;
    }
}

impl ReverbAlgorithm for Convolution {
    fn reset(&mut self) {
        self.direct_a.l.reset();
        self.direct_a.r.reset();
        self.direct_b.l.reset();
        self.direct_b.r.reset();
        self.cross.l.reset();
        self.cross.r.reset();
        self.predelay_l.reset();
        self.predelay_r.reset();
        for ap in &mut self.motion {
            ap.reset();
        }
        self.damp_l.reset();
        self.damp_r.reset();
        self.env.reset(0.0);
        self.lfo_phase = 0.0;
        self.ctrl_countdown = 0;
        self.fb_l = 0.0;
        self.fb_r = 0.0;
        self.fb_dc_l.reset();
        self.fb_dc_r.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let mut planner = RealFftPlanner::<f64>::new();
        self.direct_a.l = PartitionedConv::new(&mut planner);
        self.direct_a.r = PartitionedConv::new(&mut planner);
        self.direct_b.l = PartitionedConv::new(&mut planner);
        self.direct_b.r = PartitionedConv::new(&mut planner);
        self.cross.l = PartitionedConv::new(&mut planner);
        self.cross.r = PartitionedConv::new(&mut planner);
        // Cross originals were sampled at the old rate; the host (or
        // loader) re-submits the IR on rate changes, which re-engages.
        self.true_stereo = false;
        self.cross_originals = None;
        self.planner = planner;
        self.rebuild_synth_ir(self.ir_seconds);

        self.predelay_l.set_sample_rate(sample_rate);
        self.predelay_r.set_sample_rate(sample_rate);
        self.motion = Self::build_motion(sample_rate);
        self.env.set_times_ms(5.0, 200.0, sample_rate);
        self.sm.set_sample_rate(sample_rate);
        self.fb_smoother.set_time_ms(10.0, sample_rate);
        // Partitions were rebuilt from originals at identity shape; a
        // non-identity shape needs re-baking.
        if !self.impulse.shape_is_identity() {
            for slot in self.slots.iter_mut() {
                slot.shape_dirty = slot.original.is_some();
            }
        }
        // Reconfiguration point — land on the targets instantly.
        let p = self.mod_params;
        self.set_mod_params(&p, true);
        self.ctrl_countdown = 0;
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Damping always feeds the (gated) post-conv damping filters,
        // matching the cutoff map the algorithmic reverbs use.
        self.base_damp_cutoff = (1.0 - params.damping).mul_add(14000.0, 2000.0);

        if self.slots[IrSlot::A].user_loaded {
            // User IR locked in — size/decay no longer regenerate
            // synthetic IRs (for EITHER slot: set_params runs on the
            // audio thread during automation, and a slot-B synth rebuild
            // is a storm of forward FFTs). A still-synthetic slot B keeps
            // its construction-time IR; load a real B IR to change it.
            // To re-engage synth IRs, call clear_user_ir().
            return;
        }
        let target_seconds = params.size.mul_add(0.5, params.decay * 0.5).mul_add(5.0, 0.2);
        if (target_seconds - self.ir_seconds).abs() > 0.1 {
            self.rebuild_synth_ir(target_seconds);
        }
    }

    fn try_load_ir(&mut self, left: &[f64], right: &[f64]) -> bool {
        self.load_ir_stereo(left, right);
        true
    }

    fn try_load_prepared_ir(&mut self, pair: PreparedIrPair) -> bool {
        self.swap_prepared_pair(pair);
        true
    }

    fn supports_ir_loading(&self) -> bool {
        true
    }

    fn try_load_ir_slot(&mut self, left: &[f64], right: &[f64], slot: IrSlot) -> bool {
        self.load_ir_stereo_slot(left, right, slot);
        true
    }

    fn try_load_ir_true_stereo(&mut self, ll: &[f64], lr: &[f64], rl: &[f64], rr: &[f64]) -> bool {
        self.load_ir_true_stereo(ll, lr, rl, rr)
    }

    fn impulse_reshape_cross_source(&self) -> Option<(Arc<Vec<f64>>, Arc<Vec<f64>>)> {
        self.cross_reshape_source()
    }

    fn try_load_prepared_ir_slot(&mut self, pair: PreparedIrPair, slot: IrSlot) -> bool {
        self.swap_prepared_pair_slot(pair, slot);
        true
    }

    fn set_ir_trash_sender(&mut self, tx: crossbeam_channel::Sender<IrTrash>) -> bool {
        self.trash_tx = Some(tx);
        true
    }

    fn set_conv_mod_params(&mut self, params: &ConvolutionModParams, snap: bool) -> bool {
        self.set_mod_params(params, snap);
        true
    }

    fn set_impulse_params(&mut self, params: &ImpulseParams, snap: bool) -> bool {
        self.set_impulse(params, snap);
        true
    }

    fn impulse_reshape_source(&mut self, slot: IrSlot) -> Option<(Arc<Vec<f64>>, Arc<Vec<f64>>)> {
        if !self.slots[slot].shape_dirty {
            return None;
        }
        let src = self.slots[slot].original.clone()?;
        // Optimistic: mark the current shape as applied so we don't
        // resubmit every block. If the job is lost, the next param
        // change re-dirties the slot.
        self.slots[slot].applied_shape = self.impulse;
        self.slots[slot].shape_dirty = false;
        Some(src)
    }

    fn swap_reshaped_ir(&mut self, pair: PreparedIrPair, slot: IrSlot) -> bool {
        self.swap_reshaped(pair, slot);
        true
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // ── Modulation bookkeeping ────────────────────────────────────
        self.sm.tick();
        self.lfo_phase += self.lfo_inc;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        // Rectified input drives the wet ducker.
        let env = self.env.tick(0.5 * (left.abs() + right.abs()));

        let need_lfo = self.engaged.predelay
            || self.engaged.damping
            || self.b_engaged
            || self.sm.wet_depth.value().abs() > GATE_EPS
            || self.sm.morph_lfo.value() > GATE_EPS;
        let lfo = if need_lfo {
            (self.lfo_phase * 2.0 * PI).sin()
        } else {
            0.0
        };

        if self.ctrl_countdown == 0 {
            self.ctrl_refresh(lfo);
            self.ctrl_countdown = CTRL_BLOCK;
        }
        self.ctrl_countdown = self.ctrl_countdown.saturating_sub(1);

        // ── Impulse feedback: wet recirculated into the pre-delay ────
        // (BigSky MX Impulse "Feedback" — character depends on the
        // pre-delay time). DC-blocked and soft-clamped so fb = 1.0
        // rings without running away; internal loop gain caps at 0.85.
        let fb = self.fb_smoother.tick();
        let (left, right) = if fb > GATE_EPS {
            let g = fb * 0.85;
            let inj_l = self.fb_dc_l.tick(self.fb_l) * g;
            let inj_r = self.fb_dc_r.tick(self.fb_r) * g;
            (
                left + inj_l / (1.0 + inj_l.abs()),
                right + inj_r / (1.0 + inj_r.abs()),
            )
        } else {
            (left, right)
        };

        // ── Option 2: modulatable predelay before the convolver ──────
        let (in_l, in_r) = if self.engaged.predelay {
            (self.predelay_l.tick(left), self.predelay_r.tick(right))
        } else {
            // Keep the buffers warm so engaging later reads real audio,
            // not silence.
            self.predelay_l.write_only(left);
            self.predelay_r.write_only(right);
            (left, right)
        };

        // ── Convolution slot A (and mirrored staging for slot B) ─────
        // Take output sample if available.
        let out_l = self.direct_a.l.next_output();
        let out_r = self.direct_a.r.next_output();
        let b_out_l = self.direct_b.l.next_output();
        let b_out_r = self.direct_b.r.next_output();
        // True-stereo cross legs: input L convolved toward R and vice
        // versa. Zero cost while disengaged (blocks never processed).
        let (out_cross_r, out_cross_l) = if self.true_stereo {
            let cr = self.cross.l.next_output();
            let cl = self.cross.r.next_output();
            (cr, cl)
        } else {
            (0.0, 0.0)
        };

        // Push input sample. When block is full, run FFT and reset read
        // ptr. Slot B stages in lockstep (cheap) but only pays for
        // process_block while the morph has it engaged.
        let fill = self.direct_a.l.input_block_fill;
        debug_assert_eq!(fill, self.direct_a.r.input_block_fill);
        self.direct_a.l.push_input(fill, in_l);
        self.direct_a.r.push_input(fill, in_r);
        self.direct_b.l.push_input(fill, in_l);
        self.direct_b.r.push_input(fill, in_r);
        if self.true_stereo {
            self.cross.l.push_input(fill, in_l);
            self.cross.r.push_input(fill, in_r);
        }
        let new_fill = fill.saturating_add(1);

        if new_fill >= BLOCK {
            let mut block_l = [0.0; BLOCK];
            let mut block_r = [0.0; BLOCK];
            block_l.copy_from_slice(&self.direct_a.l.input_block[..BLOCK]);
            block_r.copy_from_slice(&self.direct_a.r.input_block[..BLOCK]);
            self.direct_a.l.input_block_fill = 0;
            self.direct_a.r.input_block_fill = 0;
            self.direct_b.l.input_block_fill = 0;
            self.direct_b.r.input_block_fill = 0;
            self.direct_a.l.process_block(&block_l);
            self.direct_a.r.process_block(&block_r);
            if self.true_stereo {
                self.cross.l.input_block_fill = 0;
                self.cross.l.process_block(&block_l);
                self.cross.r.input_block_fill = 0;
                self.cross.r.process_block(&block_r);
                self.cross.l.output_block_read = 0;
                self.cross.r.output_block_read = 0;
            }
            if self.b_engaged {
                let mut b_block_l = [0.0; BLOCK];
                let mut b_block_r = [0.0; BLOCK];
                b_block_l.copy_from_slice(&self.direct_b.l.input_block[..BLOCK]);
                b_block_r.copy_from_slice(&self.direct_b.r.input_block[..BLOCK]);
                self.direct_b.l.process_block(&b_block_l);
                self.direct_b.r.process_block(&b_block_r);
            } else {
                self.direct_b.l.output_block.fill(0.0);
                self.direct_b.r.output_block.fill(0.0);
            }
            self.direct_a.l.output_block_read = 0;
            self.direct_a.r.output_block_read = 0;
            self.direct_b.l.output_block_read = 0;
            self.direct_b.r.output_block_read = 0;
        } else {
            self.direct_a.l.input_block_fill = new_fill;
            self.direct_a.r.input_block_fill = new_fill;
            self.direct_b.l.input_block_fill = new_fill;
            self.direct_b.r.input_block_fill = new_fill;
            if self.true_stereo {
                self.cross.l.input_block_fill = new_fill;
                self.cross.r.input_block_fill = new_fill;
            }
        }

        // ── Option 3: equal-power A/B morph ───────────────────────────
        let out_l = out_l + out_cross_l;
        let out_r = out_r + out_cross_r;
        let (mut wet_l, mut wet_r) = if self.b_engaged {
            let pos = self.sm.morph_lfo.value().mul_add(lfo, self.sm.morph.value()).clamp(0.0, 1.0);
            let theta = pos * FRAC_PI_2;
            let (ga, gb) = (theta.cos(), theta.sin());
            (
                out_l.mul_add(ga, b_out_l * gb),
                out_r.mul_add(ga, b_out_r * gb),
            )
        } else {
            (out_l, out_r)
        };

        // Capture post-morph wet for the impulse feedback loop
        // (pre-motion, per the recirculation point in the MX design).
        self.fb_l = wet_l;
        self.fb_r = wet_r;

        // ── Option 1: motion stage ────────────────────────────────────
        if self.engaged.motion {
            // Depth blends dry/moved so the gate edge is click-free and
            // depth doubles as an intensity control.
            let d = self.sm.motion_depth.value();
            let stage_l = self.motion[0].tick(wet_l);
            let moved_l = self.motion[1].tick(stage_l);
            let stage_r = self.motion[2].tick(wet_r);
            let moved_r = self.motion[3].tick(stage_r);
            wet_l += (moved_l - wet_l) * d;
            wet_r += (moved_r - wet_r) * d;
        }

        // ── Option 2: damping + wet gain ──────────────────────────────
        if self.engaged.damping {
            wet_l = self.damp_l.tick(wet_l);
            wet_r = self.damp_r.tick(wet_r);
        }

        let wet_depth = self.sm.wet_depth.value();
        let duck_depth = self.sm.duck_depth.value();
        if wet_depth.abs() > GATE_EPS || duck_depth > GATE_EPS {
            let lfo_gain = if wet_depth.abs() > GATE_EPS {
                audiocore_dsp::db::db_to_linear(WET_MOD_DB * wet_depth * lfo)
            } else {
                1.0
            };
            let duck_gain = duck_depth.mul_add(-env.min(1.0), 1.0);
            let g = lfo_gain * duck_gain;
            wet_l *= g;
            wet_r *= g;
        }

        (wet_l, wet_r)
    }
}
