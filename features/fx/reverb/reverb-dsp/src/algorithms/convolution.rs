//! Uniformly partitioned FFT convolution reverb.
//!
//! Loads an arbitrary impulse response (stereo) and convolves the input
//! against it in real time using overlap-save with `realfft`. Partition
//! size is fixed at 512 samples (≈10 ms latency at 48 kHz), which keeps
//! per-partition cost low while bounding the FFT size.
//!
//! References:
//! - W. G. Gardner, "Efficient Convolution Without Input/Output Delay"
//!   (JAES 1995). Uniform partitioned convolution algorithm.
//! - Stockham, "High Speed Convolution and Correlation" (1966) —
//!   overlap-save FFT convolution baseline.
//! - https://github.com/HiFi-LoFi/FFTConvolver (open-source reference).

use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::ir::prepared::{PreparedIr, PreparedIrPair, BLOCK, FFT_LEN, SPECTRUM_LEN};

const MAX_IR_SECONDS: f64 = 8.0;

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
        self.swap_prepared(prepared);
    }

    /// Audio-thread-safe IR replacement. No FFT work, just buffer moves
    /// and one resize. The partition count may change so we also resize
    /// the input-history ring and zero it (so the tail of the OLD IR
    /// doesn't bleed into the new convolution).
    fn swap_prepared(&mut self, prepared: PreparedIr) {
        let n = prepared.partitions.len();
        self.ir_partitions = prepared.partitions;
        self.gain = prepared.gain;
        // Resize history to match new partition count. Zero-fill so we
        // don't multiply stale frequency-domain data against the new IR.
        self.input_history.clear();
        self.input_history
            .resize(n, vec![Complex::new(0.0, 0.0); SPECTRUM_LEN]);
        self.history_head = 0;
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
    for i in 0..predelay.min(n) {
        ir[i] *= i as f64 / predelay as f64;
    }
    ir
}

pub struct Convolution {
    planner: RealFftPlanner<f64>,
    conv_l: PartitionedConv,
    conv_r: PartitionedConv,
    sample_rate: f64,
    ir_seconds: f64,
    /// True once a user-supplied IR has been loaded — disables the
    /// synthetic-IR rebuild on `set_params` so user choices stick.
    user_ir_loaded: bool,
}

impl Convolution {
    pub fn new(sample_rate: f64) -> Self {
        let mut planner = RealFftPlanner::<f64>::new();
        let mut conv_l = PartitionedConv::new(&mut planner);
        let mut conv_r = PartitionedConv::new(&mut planner);

        let ir_l = synthesize_ir(sample_rate, 1.5, 0xC0FFEE);
        let ir_r = synthesize_ir(sample_rate, 1.5, 0xBADBEEF);
        conv_l.load_ir(&ir_l);
        conv_r.load_ir(&ir_r);

        Self {
            planner,
            conv_l,
            conv_r,
            sample_rate,
            ir_seconds: 1.5,
            user_ir_loaded: false,
        }
    }

    /// Replace the convolution IR. `ir_l` and `ir_r` may be different
    /// lengths (zero-padded internally) and are truncated to
    /// MAX_IR_SECONDS.
    pub fn load_ir_stereo(&mut self, ir_l: &[f64], ir_r: &[f64]) {
        let max = (self.sample_rate * MAX_IR_SECONDS) as usize;
        let cap_l = &ir_l[..ir_l.len().min(max)];
        let cap_r = &ir_r[..ir_r.len().min(max)];
        self.conv_l.load_ir(cap_l);
        self.conv_r.load_ir(cap_r);
        self.user_ir_loaded = true;
    }

    /// Forget the user IR and resume synthetic-IR rebuilds on
    /// `set_params`. Restores the default procedural reverb.
    pub fn clear_user_ir(&mut self) {
        self.user_ir_loaded = false;
        self.rebuild_synth_ir(self.ir_seconds);
    }

    /// Audio-thread-safe IR replacement. Accepts a pair already
    /// FFT-precomputed on a background thread. No allocations beyond
    /// the input-history resize (which only happens when the partition
    /// count changes).
    pub fn swap_prepared_pair(&mut self, pair: PreparedIrPair) {
        self.conv_l.swap_prepared(pair.left);
        self.conv_r.swap_prepared(pair.right);
        self.user_ir_loaded = true;
    }

    fn rebuild_synth_ir(&mut self, seconds: f64) {
        let ir_l = synthesize_ir(self.sample_rate, seconds, 0xC0FFEE);
        let ir_r = synthesize_ir(self.sample_rate, seconds, 0xBADBEEF);
        self.conv_l.load_ir(&ir_l);
        self.conv_r.load_ir(&ir_r);
        self.ir_seconds = seconds;
    }
}

impl ReverbAlgorithm for Convolution {
    fn reset(&mut self) {
        self.conv_l.reset();
        self.conv_r.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let mut planner = RealFftPlanner::<f64>::new();
        self.conv_l = PartitionedConv::new(&mut planner);
        self.conv_r = PartitionedConv::new(&mut planner);
        self.planner = planner;
        self.rebuild_synth_ir(self.ir_seconds);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        if self.user_ir_loaded {
            // User IR locked in — size/decay no longer regenerate
            // synthetic IRs. To re-engage synth IRs, call clear_user_ir().
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

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // Buffer samples until we have a BLOCK; emit one block of output
        // per BLOCK of input. Per-sample interface — we keep a small
        // in/out pair of staging buffers using fixed state on the convers.

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

        // Push input sample. When block is full, run FFT and reset read ptr.
        let fill_l = self.conv_l.input_block_fill;
        let fill_r = self.conv_r.input_block_fill;
        // Stage left input
        // Use input_block as a temp staging area — reusing the same buffer
        // we hand to FFT later is fine because process_block() copies it.
        // We piggy-back on a dedicated staging buffer instead:
        debug_assert_eq!(fill_l, fill_r);
        let i = fill_l;
        // Stage in a slice past the first BLOCK of input_block.
        self.conv_l.input_block[i] = left;
        self.conv_r.input_block[i] = right;
        let new_fill = i + 1;

        if new_fill >= BLOCK {
            // Copy staged samples into a fixed block array.
            let mut block_l = [0.0; BLOCK];
            let mut block_r = [0.0; BLOCK];
            block_l.copy_from_slice(&self.conv_l.input_block[..BLOCK]);
            block_r.copy_from_slice(&self.conv_r.input_block[..BLOCK]);
            self.conv_l.input_block_fill = 0;
            self.conv_r.input_block_fill = 0;
            self.conv_l.process_block(&block_l);
            self.conv_r.process_block(&block_r);
            self.conv_l.output_block_read = 0;
            self.conv_r.output_block_read = 0;
        } else {
            self.conv_l.input_block_fill = new_fill;
            self.conv_r.input_block_fill = new_fill;
        }

        (out_l, out_r)
    }
}
