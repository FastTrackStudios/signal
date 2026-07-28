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

use std::f64::consts::{FRAC_PI_2, PI};
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
/// SMOOTH_BLOCK: 0.67 ms at 48 kHz.
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
/// Motion allpass delays in samples at 48 kHz — mutually prime,
/// ~5–12 ms. Scaled by the actual sample rate.
const MOTION_DELAYS_48K: [usize; 4] = [241, 379, 467, 587];
/// Per-stage rate multipliers so the four motion LFOs never phase-lock.
const MOTION_RATE_MULT: [f64; 4] = [1.0, 1.13, 0.91, 1.07];
/// Per-stage LFO starting phases (stereo decorrelation).
const MOTION_PHASE: [f64; 4] = [0.0, 0.25, 0.5, 0.75];

/// Per-channel partitioned convolver.
struct PartitionedConv {
    fft_fwd: Arc<dyn RealToComplex<f64>>,
    fft_inv: Arc<dyn ComplexToReal<f64>>,

    /// Frequency-domain partitions of the IR (one Vec<Complex> per partition).
    ir_partitions: Vec<Vec<Complex<f64>>>,
    /// Ring buffer of past input partitions in the frequency domain.
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
    fn swap_prepared(&mut self, mut prepared: PreparedIr) -> PreparedIr {
        core::mem::swap(&mut self.ir_partitions, &mut prepared.partitions);
        self.gain = prepared.gain;
        let n = self.ir_partitions.len();
        if self.input_history.len() < n {
            self.input_history.reserve(n - self.input_history.len());
            while self.input_history.len() < n {
                self.input_history
                    .push(vec![Complex::new(0.0, 0.0); SPECTRUM_LEN]);
            }
        }
        // Zero so we don't multiply stale frequency-domain data against
        // the new IR.
        for h in &mut self.input_history {
            h.fill(Complex::new(0.0, 0.0));
        }
        self.history_head = 0;
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
    /// the staged input samples alone. Used when a gated-off slot B
    /// re-engages, so seconds-old audio doesn't burst out of its tail.
    fn clear_history(&mut self) {
        for h in &mut self.input_history {
            h.fill(Complex::new(0.0, 0.0));
        }
        self.history_head = 0;
        self.prev_input_tail.fill(0.0);
        self.output_block.fill(0.0);
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
        let spec = &mut self.input_history[self.history_head];
        self.fft_fwd
            .process(&mut self.input_block.clone(), spec)
            .unwrap();

        // Accumulate Σ_k IR[k] * Input[t - k].
        for s in self.accumulator.iter_mut() {
            *s = Complex::new(0.0, 0.0);
        }
        let n_parts = self.ir_partitions.len();
        for p in 0..n_parts {
            let hist_idx = (self.history_head + n_parts - p) % n_parts;
            let ir_p = &self.ir_partitions[p];
            let in_p = &self.input_history[hist_idx];
            for k in 0..SPECTRUM_LEN {
                self.accumulator[k] += ir_p[k] * in_p[k];
            }
        }

        // Inverse FFT.
        self.spectrum_scratch.copy_from_slice(&self.accumulator);
        self.fft_inv
            .process(&mut self.spectrum_scratch, &mut self.ifft_out)
            .unwrap();

        // Take the second half — discard wrap-around (overlap-save).
        let g = self.gain;
        for i in 0..BLOCK {
            self.output_block[i] = self.ifft_out[BLOCK + i] * g;
        }

        // Advance ring buffer head.
        self.history_head = (self.history_head + 1) % n_parts;
    }
}

/// Generate a synthetic stereo IR — exponentially decaying velvet-style
/// pattern. Used as the default IR until the user loads their own.
fn synthesize_ir(sample_rate: f64, seconds: f64, seed: u64) -> Vec<f64> {
    use crate::primitives::lcg_random::LcgRandom;
    let n = (sample_rate * seconds) as usize;
    let mut rng = LcgRandom::new(seed);
    let mut ir = vec![0.0; n];

    // Sparse positive/negative impulses with exponential envelope.
    let density = 2500.0;
    let spacing = ((sample_rate / density) as usize).max(1);
    let t60_samples = n as f64;
    let mut pos = 0usize;
    while pos < n {
        let jitter = (rng.next_float() * spacing as f64) as usize;
        let idx = (pos + jitter).min(n - 1);
        let sign = if rng.next_float() < 0.5 { -1.0 } else { 1.0 };
        let env = 10f64.powf(-3.0 * idx as f64 / t60_samples);
        ir[idx] = sign * env;
        pos += spacing;
    }

    // Pre-delay window: blend out the first ~5ms so direct signal isn't
    // doubled when wet/dry are summed.
    let predelay = (sample_rate * 0.005) as usize;
    #[allow(clippy::needless_range_loop)]
    for i in 0..predelay.min(n) {
        ir[i] *= i as f64 / predelay as f64;
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

pub struct Convolution {
    planner: RealFftPlanner<f64>,
    conv_l: PartitionedConv,
    conv_r: PartitionedConv,
    // IR slot B — pre-allocated, gated off until the morph engages.
    conv_l_b: PartitionedConv,
    conv_r_b: PartitionedConv,
    sample_rate: f64,
    ir_seconds: f64,
    /// True once a user-supplied IR has been loaded — disables the
    /// synthetic-IR rebuild on `set_params` so user choices stick.
    user_ir_loaded: bool,
    user_ir_loaded_b: bool,
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

    // Option 2: modulatable predelay before the convolver.
    predelay_l: ModulatedDelay,
    predelay_r: ModulatedDelay,
    predelay_engaged: bool,

    // Option 1: post-conv motion stage [L1, L2, R1, R2].
    motion: [ModulatedAllpass; 4],
    motion_engaged: bool,

    // Option 2: post-conv damping filters.
    damp_l: Lp1,
    damp_r: Lp1,
    damp_engaged: bool,
    /// Base damping cutoff derived from `AlgorithmParams::damping`.
    base_damp_cutoff: f64,

    // Option 3: morph gate state.
    b_engaged: bool,

    // ── BigSky MX Impulse live params ─────────────────────────────────
    impulse: ImpulseParams,
    /// Shape actually baked into the active partitions, per slot.
    applied_shape: [ImpulseParams; 2],
    /// Slots whose partitions are stale vs `impulse`'s shaping params.
    shape_dirty: [bool; 2],
    /// Original (un-shaped) IRs per slot, kept for re-preparation.
    #[allow(clippy::type_complexity)]
    originals: [Option<(Arc<Vec<f64>>, Arc<Vec<f64>>)>; 2],
    /// Smoothed feedback amount (10 ms) + recirculation state.
    fb_smoother: ParamSmoother,
    fb_l: f64,
    fb_r: f64,
    fb_dc_l: DcBlocker,
    fb_dc_r: DcBlocker,
}

#[inline]
fn slot_idx(slot: IrSlot) -> usize {
    match slot {
        IrSlot::A => 0,
        IrSlot::B => 1,
    }
}


impl Convolution {
    pub fn new(sample_rate: f64) -> Self {
        let mut planner = RealFftPlanner::<f64>::new();
        let mut conv_l = PartitionedConv::new(&mut planner);
        let mut conv_r = PartitionedConv::new(&mut planner);
        let mut conv_l_b = PartitionedConv::new(&mut planner);
        let mut conv_r_b = PartitionedConv::new(&mut planner);

        let ir_l = synthesize_ir(sample_rate, 1.5, 0xC0FFEE);
        let ir_r = synthesize_ir(sample_rate, 1.5, 0xBADBEEF);
        conv_l.load_ir(&ir_l);
        conv_r.load_ir(&ir_r);
        // Slot B ships with a differently-seeded velvet IR so the morph
        // is audible before the user loads anything.
        let ir_l_b = synthesize_ir(sample_rate, 1.5, 0x5EED0B);
        let ir_r_b = synthesize_ir(sample_rate, 1.5, 0x0DDB17);
        conv_l_b.load_ir(&ir_l_b);
        conv_r_b.load_ir(&ir_r_b);
        let originals = [
            Some((Arc::new(ir_l), Arc::new(ir_r))),
            Some((Arc::new(ir_l_b), Arc::new(ir_r_b))),
        ];

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
            conv_l,
            conv_r,
            conv_l_b,
            conv_r_b,
            sample_rate,
            ir_seconds: 1.5,
            user_ir_loaded: false,
            user_ir_loaded_b: false,
            trash_tx: None,
            mod_params,
            sm: ModSmoothers::new(sample_rate),
            lfo_phase: 0.0,
            lfo_inc: mod_params.lfo_rate / sample_rate,
            env,
            ctrl_countdown: 0,
            predelay_l,
            predelay_r,
            predelay_engaged: false,
            motion,
            motion_engaged: false,
            damp_l: Lp1::new(),
            damp_r: Lp1::new(),
            damp_engaged: false,
            base_damp_cutoff: 2000.0 + (1.0 - 0.3) * 14000.0,
            b_engaged: false,
            impulse: ImpulseParams::default(),
            applied_shape: [ImpulseParams::default(); 2],
            shape_dirty: [false; 2],
            originals,
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
        std::array::from_fn(|i| {
            let mut ap = ModulatedAllpass::with_phase(MOTION_PHASE[i]);
            ap.set_sample_rate(sample_rate);
            ap.set_delay_samples(((MOTION_DELAYS_48K[i] as f64) * s) as usize);
            ap.set_feedback(0.5);
            ap
        })
    }

    /// Replace the convolution IR in slot A. `ir_l` and `ir_r` may be
    /// different lengths (zero-padded internally) and are truncated to
    /// MAX_IR_SECONDS.
    pub fn load_ir_stereo(&mut self, ir_l: &[f64], ir_r: &[f64]) {
        self.load_ir_stereo_slot(ir_l, ir_r, IrSlot::A);
    }

    /// Slot-addressed synchronous IR load (runs FFTs — background/setup
    /// use only).
    ///
    /// Per the BigSky MX manual, loading a new IR resets the Impulse
    /// shaping params to defaults (the chain preserves mix).
    pub fn load_ir_stereo_slot(&mut self, ir_l: &[f64], ir_r: &[f64], slot: IrSlot) {
        let max = (self.sample_rate * MAX_IR_SECONDS) as usize;
        let cap_l = &ir_l[..ir_l.len().min(max)];
        let cap_r = &ir_r[..ir_r.len().min(max)];
        match slot {
            IrSlot::A => {
                self.conv_l.load_ir(cap_l);
                self.conv_r.load_ir(cap_r);
                self.user_ir_loaded = true;
            }
            IrSlot::B => {
                self.conv_l_b.load_ir(cap_l);
                self.conv_r_b.load_ir(cap_r);
                self.user_ir_loaded_b = true;
            }
        }
        self.originals[slot_idx(slot)] =
            Some((Arc::new(cap_l.to_vec()), Arc::new(cap_r.to_vec())));
        self.on_new_r_loaded(slot);
    }

    /// New-IR bookkeeping: shaping params reset to defaults (manual
    /// behavior), fresh partitions are the identity shape.
    fn on_new_r_loaded(&mut self, slot: IrSlot) {
        let fb = 0.0;
        self.impulse = ImpulseParams::default();
        self.fb_smoother.set_target(fb);
        self.applied_shape[slot_idx(slot)] = ImpulseParams::default();
        self.shape_dirty[slot_idx(slot)] = false;
    }

    /// Forget the user IRs (both slots) and resume synthetic-IR rebuilds
    /// on `set_params`. Restores the default procedural reverb.
    pub fn clear_user_ir(&mut self) {
        self.user_ir_loaded = false;
        self.user_ir_loaded_b = false;
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
                let l = self.conv_l.swap_prepared(pair.left);
                let r = self.conv_r.swap_prepared(pair.right);
                self.user_ir_loaded = true;
                (l, r)
            }
            IrSlot::B => {
                let l = self.conv_l_b.swap_prepared(pair.left);
                let r = self.conv_r_b.swap_prepared(pair.right);
                self.user_ir_loaded_b = true;
                (l, r)
            }
        };
        self.discard(IrTrash::Prepared(old_l));
        self.discard(IrTrash::Prepared(old_r));
        // Retain (or clear) the un-shaped original; the previous Arc's
        // refcount decrement also goes through the trash chute — it may
        // be the last reference.
        let prev = match raw {
            Some(raw) => self.originals[slot_idx(slot)].replace(raw),
            // No raw retained (direct PreparedIrPair::build) — shaping
            // can't re-derive from this load.
            None => self.originals[slot_idx(slot)].take(),
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
            if tx.send(trash).is_ok() {
                return;
            }
        }
    }

    /// Return leg of the re-preparation pipeline: swap partitions only,
    /// leaving impulse params and user-IR flags untouched.
    fn swap_reshaped(&mut self, pair: PreparedIrPair, slot: IrSlot) {
        let (old_l, old_r) = match slot {
            IrSlot::A => (
                self.conv_l.swap_prepared(pair.left),
                self.conv_r.swap_prepared(pair.right),
            ),
            IrSlot::B => (
                self.conv_l_b.swap_prepared(pair.left),
                self.conv_r_b.swap_prepared(pair.right),
            ),
        };
        self.discard(IrTrash::Prepared(old_l));
        self.discard(IrTrash::Prepared(old_r));
    }

    /// Impulse shaping params currently targeted (not necessarily baked
    /// into the partitions yet — re-preparation is asynchronous).
    pub fn impulse_params(&self) -> ImpulseParams {
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
        let shape_changed_a = p.shape_key() != self.applied_shape[0].shape_key();
        let shape_changed_b = p.shape_key() != self.applied_shape[1].shape_key();
        self.impulse = *p;
        self.shape_dirty[0] = shape_changed_a && self.originals[0].is_some();
        self.shape_dirty[1] = shape_changed_b && self.originals[1].is_some();
    }

    /// Synchronous re-preparation for headless/test use. Applies the
    /// current shaping params to both slots' originals and reloads the
    /// partitions in place. Runs FFTs on the calling thread — NOT
    /// RT-safe; real-time hosts use `ImpulseReshaper` instead.
    pub fn reprepare_now(&mut self) {
        for slot in [IrSlot::A, IrSlot::B] {
            let idx = slot_idx(slot);
            let Some((l, r)) = self.originals[idx].clone() else {
                continue;
            };
            let t = IrTransforms::from_impulse(&self.impulse);
            let asset = IrAsset::from_stereo((*l).clone(), (*r).clone(), self.sample_rate);
            let (sl, sr) = t.apply(&asset);
            match slot {
                IrSlot::A => {
                    self.conv_l.load_ir(&sl);
                    self.conv_r.load_ir(&sr);
                }
                IrSlot::B => {
                    self.conv_l_b.load_ir(&sl);
                    self.conv_r_b.load_ir(&sr);
                }
            }
            self.applied_shape[idx] = self.impulse;
            self.shape_dirty[idx] = false;
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
    pub fn mod_params(&self) -> ConvolutionModParams {
        self.mod_params
    }

    fn refresh_motion_rates(&mut self) {
        let rate = self.mod_params.motion_rate.clamp(0.02, 10.0);
        let depth_samples =
            self.sm.motion_depth.value() * MOTION_EXCURSION_S * self.sample_rate;
        for (i, ap) in self.motion.iter_mut().enumerate() {
            ap.set_modulation(rate * MOTION_RATE_MULT[i], depth_samples, self.sample_rate);
        }
    }

    fn rebuild_synth_ir(&mut self, seconds: f64) {
        if !self.user_ir_loaded {
            let ir_l = synthesize_ir(self.sample_rate, seconds, 0xC0FFEE);
            let ir_r = synthesize_ir(self.sample_rate, seconds, 0xBADBEEF);
            self.conv_l.load_ir(&ir_l);
            self.conv_r.load_ir(&ir_r);
            self.originals[0] = Some((Arc::new(ir_l), Arc::new(ir_r)));
            self.applied_shape[0] = ImpulseParams::default();
        }
        if !self.user_ir_loaded_b {
            let ir_l = synthesize_ir(self.sample_rate, seconds, 0x5EED0B);
            let ir_r = synthesize_ir(self.sample_rate, seconds, 0x0DDB17);
            self.conv_l_b.load_ir(&ir_l);
            self.conv_r_b.load_ir(&ir_r);
            self.originals[1] = Some((Arc::new(ir_l), Arc::new(ir_r)));
            self.applied_shape[1] = ImpulseParams::default();
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
            let base_samples =
                ((base_ms * 0.001 * self.sample_rate) as usize).max(1);
            let mod_amount = pd_depth * PREDELAY_MOD_S * self.sample_rate;
            self.predelay_l.sample_delay = base_samples;
            self.predelay_r.sample_delay = base_samples;
            self.predelay_l.mod_amount = mod_amount;
            self.predelay_r.mod_amount = mod_amount;
        }
        self.predelay_engaged = want_pd;

        // ── Motion gate ──
        let want_motion = self.sm.motion_depth.value() > GATE_EPS;
        if want_motion {
            if !self.motion_engaged {
                for ap in &mut self.motion {
                    ap.reset();
                }
            }
            if !self.sm.motion_depth.is_settled() || !self.motion_engaged {
                self.refresh_motion_rates();
            }
        }
        self.motion_engaged = want_motion;

        // ── Damping gate ──
        let damp_depth = self.sm.damp_depth.value();
        let want_damp = damp_depth.abs() > GATE_EPS;
        if want_damp {
            if !self.damp_engaged {
                self.damp_l.reset();
                self.damp_r.reset();
            }
            // Neutral point is wide open: as |depth| → 0 the cutoff glides
            // to 20 kHz so the gate transition is inaudible. The LFO then
            // swings the cutoff ±DAMP_MOD_OCTAVES·depth around that.
            let w = damp_depth.abs().min(1.0);
            let base = self.base_damp_cutoff.clamp(200.0, 20000.0);
            let anchored = base.powf(w) * 20000.0_f64.powf(1.0 - w);
            let swung = anchored
                * (2.0_f64).powf(DAMP_MOD_OCTAVES * damp_depth * lfo);
            let cutoff = swung.clamp(200.0, 20000.0);
            self.damp_l.set_freq(cutoff, self.sample_rate);
            self.damp_r.set_freq(cutoff, self.sample_rate);
        }
        self.damp_engaged = want_damp;

        // ── Morph gate ──
        let want_b = self.sm.morph.value() > GATE_EPS
            || self.sm.morph_lfo.value() > GATE_EPS
            || !self.sm.morph.is_settled();
        if want_b && !self.b_engaged {
            // B has been idle — its history holds seconds-old audio.
            // Clear so the tail fades in from silence under the ramp.
            self.conv_l_b.clear_history();
            self.conv_r_b.clear_history();
        }
        self.b_engaged = want_b;
    }
}

impl ReverbAlgorithm for Convolution {
    fn reset(&mut self) {
        self.conv_l.reset();
        self.conv_r.reset();
        self.conv_l_b.reset();
        self.conv_r_b.reset();
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
        self.conv_l = PartitionedConv::new(&mut planner);
        self.conv_r = PartitionedConv::new(&mut planner);
        self.conv_l_b = PartitionedConv::new(&mut planner);
        self.conv_r_b = PartitionedConv::new(&mut planner);
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
            self.shape_dirty = [self.originals[0].is_some(), self.originals[1].is_some()];
        }
        // Reconfiguration point — land on the targets instantly.
        let p = self.mod_params;
        self.set_mod_params(&p, true);
        self.ctrl_countdown = 0;
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Damping always feeds the (gated) post-conv damping filters,
        // matching the cutoff map the algorithmic reverbs use.
        self.base_damp_cutoff = 2000.0 + (1.0 - params.damping) * 14000.0;

        if self.user_ir_loaded {
            // User IR locked in — size/decay no longer regenerate
            // synthetic IRs (for EITHER slot: set_params runs on the
            // audio thread during automation, and a slot-B synth rebuild
            // is a storm of forward FFTs). A still-synthetic slot B keeps
            // its construction-time IR; load a real B IR to change it.
            // To re-engage synth IRs, call clear_user_ir().
            return;
        }
        let target_seconds = 0.2 + (params.size * 0.5 + params.decay * 0.5) * 5.0;
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

    fn try_load_prepared_ir_slot(&mut self, pair: PreparedIrPair, slot: IrSlot) -> bool {
        self.swap_prepared_pair_slot(pair, slot);
        true
    }

    fn set_ir_trash_sender(
        &mut self,
        tx: crossbeam_channel::Sender<IrTrash>,
    ) -> bool {
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

    fn impulse_reshape_source(
        &mut self,
        slot: IrSlot,
    ) -> Option<(Arc<Vec<f64>>, Arc<Vec<f64>>)> {
        let idx = slot_idx(slot);
        if !self.shape_dirty[idx] {
            return None;
        }
        let src = self.originals[idx].clone()?;
        // Optimistic: mark the current shape as applied so we don't
        // resubmit every block. If the job is lost, the next param
        // change re-dirties the slot.
        self.applied_shape[idx] = self.impulse;
        self.shape_dirty[idx] = false;
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

        let need_lfo = self.predelay_engaged
            || self.damp_engaged
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
        self.ctrl_countdown -= 1;

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
        let (in_l, in_r) = if self.predelay_engaged {
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
        let out_l = if self.conv_l.output_block_read < BLOCK {
            let v = self.conv_l.output_block[self.conv_l.output_block_read];
            self.conv_l.output_block_read += 1;
            v
        } else {
            0.0
        };
        let out_r = if self.conv_r.output_block_read < BLOCK {
            let v = self.conv_r.output_block[self.conv_r.output_block_read];
            self.conv_r.output_block_read += 1;
            v
        } else {
            0.0
        };
        let out_l_b = if self.conv_l_b.output_block_read < BLOCK {
            let v = self.conv_l_b.output_block[self.conv_l_b.output_block_read];
            self.conv_l_b.output_block_read += 1;
            v
        } else {
            0.0
        };
        let out_r_b = if self.conv_r_b.output_block_read < BLOCK {
            let v = self.conv_r_b.output_block[self.conv_r_b.output_block_read];
            self.conv_r_b.output_block_read += 1;
            v
        } else {
            0.0
        };

        // Push input sample. When block is full, run FFT and reset read
        // ptr. Slot B stages in lockstep (cheap) but only pays for
        // process_block while the morph has it engaged.
        let fill = self.conv_l.input_block_fill;
        debug_assert_eq!(fill, self.conv_r.input_block_fill);
        self.conv_l.input_block[fill] = in_l;
        self.conv_r.input_block[fill] = in_r;
        self.conv_l_b.input_block[fill] = in_l;
        self.conv_r_b.input_block[fill] = in_r;
        let new_fill = fill + 1;

        if new_fill >= BLOCK {
            let mut block_l = [0.0; BLOCK];
            let mut block_r = [0.0; BLOCK];
            block_l.copy_from_slice(&self.conv_l.input_block[..BLOCK]);
            block_r.copy_from_slice(&self.conv_r.input_block[..BLOCK]);
            self.conv_l.input_block_fill = 0;
            self.conv_r.input_block_fill = 0;
            self.conv_l_b.input_block_fill = 0;
            self.conv_r_b.input_block_fill = 0;
            self.conv_l.process_block(&block_l);
            self.conv_r.process_block(&block_r);
            if self.b_engaged {
                let mut block_l_b = [0.0; BLOCK];
                let mut block_r_b = [0.0; BLOCK];
                block_l_b.copy_from_slice(&self.conv_l_b.input_block[..BLOCK]);
                block_r_b.copy_from_slice(&self.conv_r_b.input_block[..BLOCK]);
                self.conv_l_b.process_block(&block_l_b);
                self.conv_r_b.process_block(&block_r_b);
            } else {
                self.conv_l_b.output_block.fill(0.0);
                self.conv_r_b.output_block.fill(0.0);
            }
            self.conv_l.output_block_read = 0;
            self.conv_r.output_block_read = 0;
            self.conv_l_b.output_block_read = 0;
            self.conv_r_b.output_block_read = 0;
        } else {
            self.conv_l.input_block_fill = new_fill;
            self.conv_r.input_block_fill = new_fill;
            self.conv_l_b.input_block_fill = new_fill;
            self.conv_r_b.input_block_fill = new_fill;
        }

        // ── Option 3: equal-power A/B morph ───────────────────────────
        let (mut wet_l, mut wet_r) = if self.b_engaged {
            let pos = (self.sm.morph.value() + self.sm.morph_lfo.value() * lfo)
                .clamp(0.0, 1.0);
            let theta = pos * FRAC_PI_2;
            let (ga, gb) = (theta.cos(), theta.sin());
            (out_l * ga + out_l_b * gb, out_r * ga + out_r_b * gb)
        } else {
            (out_l, out_r)
        };

        // Capture post-morph wet for the impulse feedback loop
        // (pre-motion, per the recirculation point in the MX design).
        self.fb_l = wet_l;
        self.fb_r = wet_r;

        // ── Option 1: motion stage ────────────────────────────────────
        if self.motion_engaged {
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
        if self.damp_engaged {
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
            let duck_gain = 1.0 - duck_depth * env.min(1.0);
            let g = lfo_gain * duck_gain;
            wet_l *= g;
            wet_r *= g;
        }

        (wet_l, wet_r)
    }
}
