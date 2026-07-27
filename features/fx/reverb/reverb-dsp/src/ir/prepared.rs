//! Pre-FFT'd impulse responses, ready to swap into a partitioned
//! convolver with zero FFT cost on the audio thread.
//!
//! The hard part of changing IRs in real time is the FFT precompute:
//! for a 5 s IR at 48 kHz with a 512-sample partition that's ~470
//! forward FFTs of length 1024. That work belongs on a worker thread.
//! Once it's done, the audio thread just needs to swap the partition
//! vectors and resize its input-history ring.
//!
//! [`PreparedIr`] / [`PreparedIrPair`] are the wire format between the
//! loader thread and the audio thread.

use realfft::num_complex::Complex;
use realfft::RealFftPlanner;

/// Partition size in samples. Sets latency (BLOCK / sample_rate) and
/// per-block FFT size (2 × BLOCK). 512 → ~10.7 ms @ 48 kHz.
pub const BLOCK: usize = 512;
/// FFT frame size for overlap-save. Must be 2 × BLOCK.
pub const FFT_LEN: usize = BLOCK * 2;
/// Number of complex bins in each real FFT result.
pub const SPECTRUM_LEN: usize = FFT_LEN / 2 + 1;

/// Single-channel pre-FFT'd IR.
#[derive(Clone)]
pub struct PreparedIr {
    /// One `Vec<Complex>` per partition. Length = ceil(ir_len / BLOCK).
    pub partitions: Vec<Vec<Complex<f64>>>,
    /// Compensation gain to apply to the IFFT output.
    pub gain: f64,
    /// Original IR length in samples (for diagnostics / UI).
    pub original_len: usize,
}

impl PreparedIr {
    /// Empty IR — convolver will produce silence.
    pub fn empty() -> Self {
        Self {
            partitions: Vec::new(),
            gain: 1.0,
            original_len: 0,
        }
    }

    pub fn num_partitions(&self) -> usize {
        self.partitions.len()
    }

    /// Build a `PreparedIr` from time-domain samples. Allocates and
    /// FFTs every partition — heavy work, meant for a background thread.
    pub fn build(ir: &[f64]) -> Self {
        let mut planner = RealFftPlanner::<f64>::new();
        Self::build_with_planner(ir, &mut planner)
    }

    /// Same as [`Self::build`] but reuses an existing planner — useful
    /// inside loops or long-lived workers that prepare many IRs.
    pub fn build_with_planner(ir: &[f64], planner: &mut RealFftPlanner<f64>) -> Self {
        if ir.is_empty() {
            return Self::empty();
        }

        let fft_fwd = planner.plan_fft_forward(FFT_LEN);
        let num_partitions = ir.len().div_ceil(BLOCK);

        let mut partitions = Vec::with_capacity(num_partitions);
        let mut padded = vec![0.0_f64; FFT_LEN];

        for p in 0..num_partitions {
            let start = p * BLOCK;
            let end = (start + BLOCK).min(ir.len());

            padded.iter_mut().for_each(|x| *x = 0.0);
            // Overlap-save convention: IR data lives in the second half.
            padded[BLOCK..(BLOCK + (end - start))].copy_from_slice(&ir[start..end]);

            let mut spec = vec![Complex::new(0.0, 0.0); SPECTRUM_LEN];
            // process() consumes the input buffer in place; we hand it
            // a clone so `padded` survives for the next iteration.
            let mut input = padded.clone();
            fft_fwd.process(&mut input, &mut spec).expect("FFT failed");
            partitions.push(spec);
        }

        let ir_energy: f64 = ir.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-9);
        let gain = 1.0 / (FFT_LEN as f64 * ir_energy * 0.5).max(1.0);

        Self {
            partitions,
            gain,
            original_len: ir.len(),
        }
    }
}

/// Stereo bundle ready to ship to the audio thread.
#[derive(Clone)]
pub struct PreparedIrPair {
    pub left: PreparedIr,
    pub right: PreparedIr,
    /// Destination IR slot (dual-IR morph). Defaults to A.
    pub slot: crate::algorithm::IrSlot,
    /// True when this pair is the return leg of an Impulse-param
    /// re-preparation — the receiver must swap WITHOUT resetting
    /// impulse params or marking a user IR as loaded.
    pub reshape: bool,
    /// The un-shaped time-domain IR, carried along so the Impulse
    /// engine can re-shape later without re-decoding from disk.
    /// `Arc` so audio-thread clones are allocation-free.
    #[allow(clippy::type_complexity)]
    pub raw: Option<(std::sync::Arc<Vec<f64>>, std::sync::Arc<Vec<f64>>)>,
}

impl PreparedIrPair {
    pub fn empty() -> Self {
        Self {
            left: PreparedIr::empty(),
            right: PreparedIr::empty(),
            slot: crate::algorithm::IrSlot::A,
            reshape: false,
            raw: None,
        }
    }

    /// Build a stereo prepared IR using a single planner instance.
    /// Slot A, not a reshape, no raw retention — use the field syntax
    /// to override.
    pub fn build(left: &[f64], right: &[f64]) -> Self {
        let mut planner = RealFftPlanner::<f64>::new();
        Self {
            left: PreparedIr::build_with_planner(left, &mut planner),
            right: PreparedIr::build_with_planner(right, &mut planner),
            slot: crate::algorithm::IrSlot::A,
            reshape: false,
            raw: None,
        }
    }
}
