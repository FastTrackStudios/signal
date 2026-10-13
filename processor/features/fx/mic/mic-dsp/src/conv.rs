//! Zero-latency FIR convolution: a direct-form head plus uniformly
//! partitioned FFT convolution for the rest (Gardner, "Efficient
//! Convolution Without Input/Output Delay", JAES 1995).
//!
//! The kernel is split into [`BLOCK`]-sample partitions. Partition 0 runs
//! as a plain FIR, sample by sample, so the output has no added delay.
//! Partition `j ≥ 1` only ever reaches input `j` blocks back, which is
//! complete by the time its output is due, so those run as overlap-save
//! products against a frequency-domain delay line of past input blocks,
//! once per block.
//!
//! Everything is `f64`: the point of this crate is to null against a
//! reference, and the accumulated error of single-precision FFTs over a
//! 16k-tap kernel would sit well above the target floor.

use std::collections::VecDeque;
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// Partition length, and so the length of the direct-form head.
pub const BLOCK: usize = 64;
const FFT_LEN: usize = 2 * BLOCK;
const BINS: usize = BLOCK + 1;

/// One FIR filter, run with no latency.
pub struct Convolver {
    /// Taps `0..BLOCK`, applied directly.
    head: [f64; BLOCK],
    /// The last `BLOCK` inputs, newest first.
    history: VecDeque<f64>,
    /// Spectra of partitions `1..`, `BINS` each, in order.
    parts: Vec<Complex<f64>>,
    /// Spectra of past input block pairs, `BINS` each, as a ring.
    fdl: Vec<Complex<f64>>,
    /// Slot in `fdl` holding the newest block pair.
    newest: usize,
    /// Partitions in use for the current kernel (≤ the capacity).
    used: usize,
    prev: [f64; BLOCK],
    cur: [f64; BLOCK],
    /// Position within the current block.
    pos: usize,
    /// Output of partitions `1..` for the current block.
    tail: [f64; BLOCK],
    fwd: Arc<dyn RealToComplex<f64>>,
    inv: Arc<dyn ComplexToReal<f64>>,
    time: Vec<f64>,
    spec: Vec<Complex<f64>>,
    acc: Vec<Complex<f64>>,
    scratch: Vec<Complex<f64>>,
}

impl Convolver {
    /// A convolver for kernels up to `max_taps` long, initially silent.
    #[must_use]
    pub fn new(max_taps: usize) -> Self {
        let capacity = max_taps.saturating_sub(BLOCK).div_ceil(BLOCK).max(1);
        let mut planner = RealFftPlanner::<f64>::new();
        let fwd = planner.plan_fft_forward(FFT_LEN);
        let inv = planner.plan_fft_inverse(FFT_LEN);
        let scratch_len = fwd.get_scratch_len().max(inv.get_scratch_len());
        let zero = Complex::new(0.0, 0.0);
        Self {
            head: [0.0; BLOCK],
            history: core::iter::repeat_n(0.0, BLOCK).collect(),
            parts: vec![zero; capacity.saturating_mul(BINS)],
            fdl: vec![zero; capacity.saturating_mul(BINS)],
            newest: 0,
            used: 0,
            prev: [0.0; BLOCK],
            cur: [0.0; BLOCK],
            pos: 0,
            tail: [0.0; BLOCK],
            fwd,
            inv,
            time: vec![0.0; FFT_LEN],
            spec: vec![zero; BINS],
            acc: vec![zero; BINS],
            scratch: vec![zero; scratch_len],
        }
    }

    /// Replace the kernel (truncated to the capacity given at
    /// construction). The input history is kept, so the output moves to
    /// the new kernel without a gap; no allocation.
    pub fn set_kernel(&mut self, kernel: &[f64]) {
        let mut taps = kernel.chunks(BLOCK);
        self.head.fill(0.0);
        if let Some(first) = taps.next() {
            self.head.iter_mut().zip(first).for_each(|(h, &k)| *h = k);
        }
        let zero = Complex::new(0.0, 0.0);
        let mut used = 0usize;
        for slot in self.parts.chunks_exact_mut(BINS) {
            match taps.next() {
                Some(part) => {
                    self.time.fill(0.0);
                    self.time.iter_mut().zip(part).for_each(|(t, &k)| *t = k);
                    // A length mismatch is impossible here (buffers are
                    // sized for this plan); a failed transform leaves the
                    // slot silent rather than half-written.
                    if self.fwd.process_with_scratch(&mut self.time, slot, &mut self.scratch).is_err() {
                        slot.fill(zero);
                    }
                    used = used.saturating_add(1);
                }
                None => slot.fill(zero),
            }
        }
        self.used = used;
    }

    /// Clear all signal state (the kernel is kept).
    pub fn reset(&mut self) {
        self.history.iter_mut().for_each(|h| *h = 0.0);
        self.fdl.fill(Complex::new(0.0, 0.0));
        self.prev.fill(0.0);
        self.cur.fill(0.0);
        self.tail.fill(0.0);
        self.pos = 0;
    }

    /// One sample in, one sample out.
    pub fn process(&mut self, x: f64) -> f64 {
        self.history.pop_back();
        self.history.push_front(x);
        let direct: f64 = self.head.iter().zip(&self.history).map(|(h, s)| h * s).sum();
        let tail = self.tail.get(self.pos).copied().unwrap_or(0.0);
        if let Some(c) = self.cur.get_mut(self.pos) {
            *c = x;
        }
        self.pos = self.pos.saturating_add(1);
        if self.pos >= BLOCK {
            self.block_done();
            self.pos = 0;
        }
        direct + tail
    }

    /// A block of input is complete: fold it into the delay line and
    /// compute the next block's contribution from partitions `1..`.
    fn block_done(&mut self) {
        let capacity = self.fdl.len() / BINS;
        if capacity == 0 {
            return;
        }
        self.newest = if self.newest == 0 { capacity.saturating_sub(1) } else { self.newest.saturating_sub(1) };
        let (first, second) = self.time.split_at_mut(BLOCK);
        first.copy_from_slice(&self.prev);
        second.copy_from_slice(&self.cur);
        if let Some(slot) = self.fdl.chunks_exact_mut(BINS).nth(self.newest)
            && self.fwd.process_with_scratch(&mut self.time, slot, &mut self.scratch).is_err()
        {
            slot.fill(Complex::new(0.0, 0.0));
        }
        self.prev = self.cur;

        // Newest block pair meets partition 1, the one before it
        // partition 2, …: walk the ring from `newest` forward (older).
        self.acc.fill(Complex::new(0.0, 0.0));
        let (before, from) = self.fdl.split_at(self.newest.saturating_mul(BINS));
        let ring = from.chunks_exact(BINS).chain(before.chunks_exact(BINS));
        for (x, h) in ring.zip(self.parts.chunks_exact(BINS)).take(self.used) {
            for ((a, xv), hv) in self.acc.iter_mut().zip(x).zip(h) {
                *a += xv * hv;
            }
        }
        self.spec.copy_from_slice(&self.acc);
        if self.inv.process_with_scratch(&mut self.spec, &mut self.time, &mut self.scratch).is_err() {
            self.tail.fill(0.0);
            return;
        }
        let norm = 1.0 / dsp_core::count_to_f64(FFT_LEN);
        self.tail.iter_mut().zip(self.time.iter().skip(BLOCK)).for_each(|(t, &v)| *t = v * norm);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct(kernel: &[f64], x: &[f64]) -> Vec<f64> {
        (0..x.len())
            .map(|n| {
                kernel
                    .iter()
                    .enumerate()
                    .filter_map(|(k, h)| n.checked_sub(k).and_then(|i| x.get(i)).map(|v| h * v))
                    .sum()
            })
            .collect()
    }

    #[test]
    fn matches_direct_convolution_with_no_delay() {
        let kernel: Vec<f64> = (0..1000).map(|i| (f64::from(i) * 0.37).sin() * (-f64::from(i) / 300.0).exp()).collect();
        let x: Vec<f64> = (0..3000).map(|i| (f64::from(i) * 1.7).cos().mul_add(0.3, (f64::from(i) * 0.011).sin())).collect();
        let mut c = Convolver::new(1024);
        c.set_kernel(&kernel);
        let y: Vec<f64> = x.iter().map(|&v| c.process(v)).collect();
        let want = direct(&kernel, &x);
        let err = y.iter().zip(&want).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        assert!(err < 1e-12, "max error {err}");
    }
}
