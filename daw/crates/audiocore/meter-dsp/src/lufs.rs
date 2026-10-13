//! EBU R128 LUFS metering.
//!
//! Implements the full ITU-R BS.1770 / EBU R128 loudness measurement chain:
//! - K-weighting filter (pre-filter shelf + RLB high-pass)
//! - Momentary loudness (400 ms sliding window)
//! - Short-term loudness (3 s sliding window)
//! - Integrated loudness with two-stage gating
//! - Loudness Range (LRA) via percentile analysis
//! - True peak per channel

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::RwLock;

// ── Shared state ────────────────────────────────────────────────────────────

/// Loudness measurement results shared with the UI thread.
pub struct LufsState {
    /// Momentary loudness, 400 ms sliding window (LUFS).
    pub momentary_lufs: RwLock<f32>,
    /// Short-term loudness, 3 s sliding window (LUFS).
    pub short_term_lufs: RwLock<f32>,
    /// Integrated loudness over the full programme (LUFS).
    pub integrated_lufs: RwLock<f32>,
    /// Loudness range (LU): 95th − 10th percentile of short-term distribution.
    pub loudness_range: RwLock<f32>,
    /// True peak, left channel (dBTP).
    pub true_peak_l: RwLock<f32>,
    /// True peak, right channel (dBTP).
    pub true_peak_r: RwLock<f32>,
}

impl LufsState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            momentary_lufs: RwLock::new(f32::NEG_INFINITY),
            short_term_lufs: RwLock::new(f32::NEG_INFINITY),
            integrated_lufs: RwLock::new(f32::NEG_INFINITY),
            loudness_range: RwLock::new(0.0),
            true_peak_l: RwLock::new(f32::NEG_INFINITY),
            true_peak_r: RwLock::new(f32::NEG_INFINITY),
        })
    }
}

// ── K-weighting biquad ──────────────────────────────────────────────────────

/// Second-order IIR biquad section (Direct Form I).
#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Two-stage K-weighting filter for one channel.
#[derive(Clone, Copy, Default)]
struct KWeightingFilter {
    stage1: Biquad,
    stage2: Biquad,
}

impl KWeightingFilter {
    /// Construct K-weighting coefficients for the given sample rate.
    ///
    /// At 48 kHz the exact EBU R128 reference coefficients are used.
    /// For other rates the analog prototype is bilinear-transformed.
    fn new(sample_rate: f32) -> Self {
        let (s1, s2) = k_weighting_coefficients(sample_rate as f64);
        Self {
            stage1: s1,
            stage2: s2,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.stage1.process(x as f64);
        let y = self.stage2.process(y);
        y as f32
    }
}

/// Compute K-weighting biquad coefficients via bilinear transform.
///
/// Reference: EBU R128 (2014), Annex 1; ITU-R BS.1770-4.
fn k_weighting_coefficients(fs: f64) -> (Biquad, Biquad) {
    // Stage 1: High-shelf pre-filter (+4 dB above ~1.5 kHz)
    // Analog prototype: H(s) = (s^2 + 1.9952623149688797 * db4 * s + 1) / (s^2 + s·√2 + 1)
    // For fs == 48000 the exact EBU coefficients are:
    let stage1 = if (fs - 48000.0).abs() < 1.0 {
        Biquad {
            b0: 1.53512485958697,
            b1: -2.69169618940638,
            b2: 1.19839281085285,
            a1: -1.69065929318241,
            a2: 0.73248077421585,
            ..Default::default()
        }
    } else {
        // Bilinear transform of the analog pre-filter prototype.
        // Analog: H(s) = (s² + K_1·s + 1) / (s² + K_2·s + 1)
        // where K_1 = 2·db4  (gain ~4 dB → linear ≈ 1.58489)  — shelf numerator
        //       K_2 = √2 (Butterworth denominator)
        let k = (std::f64::consts::PI * 1681.81 / fs).tan(); // ~1.68 kHz pivot
        let db4 = 10.0_f64.powf(4.0 / 20.0); // 4 dB linear gain
        let vh = db4;
        let vb = db4.sqrt();
        let k2 = k * k;
        let norm = 1.0 + k / std::f64::consts::FRAC_1_SQRT_2 + k2;
        let b0 = (vh + vb * k / std::f64::consts::FRAC_1_SQRT_2 + k2) / norm;
        let b1 = 2.0 * (k2 - vh) / norm;
        let b2 = (vh - vb * k / std::f64::consts::FRAC_1_SQRT_2 + k2) / norm;
        let a1 = 2.0 * (k2 - 1.0) / norm;
        let a2 = (1.0 - k / std::f64::consts::FRAC_1_SQRT_2 + k2) / norm;
        Biquad {
            b0,
            b1,
            b2,
            a1,
            a2,
            ..Default::default()
        }
    };

    // Stage 2: 2nd-order Butterworth HPF at 38.13507 Hz (RLB weighting)
    let stage2 = if (fs - 48000.0).abs() < 1.0 {
        Biquad {
            b0: 1.0,
            b1: -2.0,
            b2: 1.0,
            a1: -1.99004745483398,
            a2: 0.99007225036621,
            ..Default::default()
        }
    } else {
        let f0 = 38.13507;
        let k = (std::f64::consts::PI * f0 / fs).tan();
        let k2 = k * k;
        let sqrt2 = std::f64::consts::SQRT_2;
        let norm = k2 + sqrt2 * k + 1.0;
        let b0 = 1.0 / norm;
        let b1 = -2.0 / norm;
        let b2 = 1.0 / norm;
        let a1 = 2.0 * (k2 - 1.0) / norm;
        let a2 = (k2 - sqrt2 * k + 1.0) / norm;
        Biquad {
            b0,
            b1,
            b2,
            a1,
            a2,
            ..Default::default()
        }
    };

    (stage1, stage2)
}

// ── 4× oversampled true-peak detector (inline, simple) ─────────────────────

/// Compute the inter-sample peak of a block using 4-point cubic interpolation.
fn inter_sample_peak(samples: &[f32]) -> f32 {
    let mut peak = 0.0_f32;
    let n = samples.len();
    for i in 1..n.saturating_sub(2) {
        // Estimate up to 3 interpolated points between samples[i] and samples[i+1].
        let s0 = samples[i.saturating_sub(1)] as f64;
        let s1 = samples[i] as f64;
        let s2 = samples[(i + 1).min(n - 1)] as f64;
        let s3 = samples[(i + 2).min(n - 1)] as f64;
        for frac in &[0.25, 0.5, 0.75_f64] {
            let t = *frac;
            // Catmull-Rom interpolation
            let v = 0.5
                * ((2.0 * s1)
                    + (-s0 + s2) * t
                    + (2.0 * s0 - 5.0 * s1 + 4.0 * s2 - s3) * t * t
                    + (-s0 + 3.0 * s1 - 3.0 * s2 + s3) * t * t * t);
            peak = peak.max(v.abs() as f32);
        }
        peak = peak.max(s1.abs() as f32);
    }
    peak
}

// ── LUFS meter ──────────────────────────────────────────────────────────────

/// EBU R128 loudness meter.
///
/// Call [`LufsMeter::process`] once per audio block on the audio thread.
/// Read results via the shared [`LufsState`] arc.
pub struct LufsMeter {
    sample_rate: f32,
    filter_l: KWeightingFilter,
    filter_r: KWeightingFilter,

    /// Short-term mean-square blocks (100 ms overlap blocks).
    /// Momentary = last 4 blocks (400 ms).
    /// Short-term = last 30 blocks (3 s).
    block_ms_buf: VecDeque<f64>,
    /// Samples accumulated for the current 100 ms block.
    current_block_l: Vec<f32>,
    current_block_r: Vec<f32>,
    block_size: usize,

    /// Integrated loudness: all gated 400 ms blocks.
    integrated_blocks: Vec<f64>,

    /// Short-term loudness history for LRA (ring buffer of values in LUFS).
    lra_history: VecDeque<f32>,

    /// True peak accumulators.
    true_peak_l: f32,
    true_peak_r: f32,

    pub state: Arc<LufsState>,
}

impl LufsMeter {
    /// Create a new LUFS meter for the given sample rate.
    pub fn new(sample_rate: f32) -> Self {
        let block_size = (sample_rate * 0.1) as usize; // 100 ms
        Self {
            sample_rate,
            filter_l: KWeightingFilter::new(sample_rate),
            filter_r: KWeightingFilter::new(sample_rate),
            block_ms_buf: VecDeque::with_capacity(32),
            current_block_l: Vec::with_capacity(block_size),
            current_block_r: Vec::with_capacity(block_size),
            block_size,
            integrated_blocks: Vec::new(),
            lra_history: VecDeque::with_capacity(600),
            true_peak_l: f32::NEG_INFINITY,
            true_peak_r: f32::NEG_INFINITY,
            state: LufsState::new(),
        }
    }

    /// Process one block of stereo audio.
    ///
    /// `left` and `right` must have the same length.
    pub fn process(&mut self, left: &[f32], right: &[f32]) {
        debug_assert_eq!(left.len(), right.len());

        // True peak
        let tp_l = inter_sample_peak(left);
        let tp_r = inter_sample_peak(right);
        if tp_l > self.true_peak_l {
            self.true_peak_l = tp_l;
        }
        if tp_r > self.true_peak_r {
            self.true_peak_r = tp_r;
        }

        // K-weight and accumulate
        for (&l, &r) in left.iter().zip(right.iter()) {
            let wl = self.filter_l.process(l);
            let wr = self.filter_r.process(r);
            self.current_block_l.push(wl);
            self.current_block_r.push(wr);

            if self.current_block_l.len() >= self.block_size {
                self.flush_block();
            }
        }

        self.update_state();
    }

    /// Flush the current 100 ms block into the ring buffer.
    fn flush_block(&mut self) {
        let n = self.current_block_l.len().min(self.block_size) as f64;
        let sum_sq: f64 = self
            .current_block_l
            .iter()
            .zip(self.current_block_r.iter())
            .map(|(&l, &r)| l as f64 * l as f64 + r as f64 * r as f64)
            .sum();
        let ms = sum_sq / (2.0 * n); // mean square (stereo average)

        self.block_ms_buf.push_back(ms);
        // Keep only 30 blocks (3 s)
        while self.block_ms_buf.len() > 30 {
            self.block_ms_buf.pop_front();
        }
        self.integrated_blocks.push(ms);

        self.current_block_l.clear();
        self.current_block_r.clear();
    }

    /// Compute loudness from N blocks of mean-square values.
    fn loudness_from_blocks(blocks: &[f64]) -> f32 {
        if blocks.is_empty() {
            return f32::NEG_INFINITY;
        }
        let mean_sq: f64 = blocks.iter().sum::<f64>() / blocks.len() as f64;
        if mean_sq <= 0.0 {
            return f32::NEG_INFINITY;
        }
        (-0.691 + 10.0 * mean_sq.log10()) as f32
    }

    /// Update shared state after processing.
    fn update_state(&mut self) {
        let buf: Vec<f64> = self.block_ms_buf.iter().copied().collect();

        // Momentary: last 4 × 100 ms = 400 ms
        let momentary = if !buf.is_empty() {
            let start = buf.len().saturating_sub(4);
            Self::loudness_from_blocks(&buf[start..])
        } else {
            f32::NEG_INFINITY
        };

        // Short-term: all available (up to 30 × 100 ms = 3 s)
        let short_term = Self::loudness_from_blocks(&buf);

        // Integrated: two-stage gating per EBU R128 §2.10
        let integrated = self.compute_integrated();

        // LRA: append short_term if above absolute gate
        if short_term > -70.0 {
            self.lra_history.push_back(short_term);
            while self.lra_history.len() > 600 {
                self.lra_history.pop_front();
            }
        }
        let lra = self.compute_lra();

        let tp_l_db = if self.true_peak_l > 0.0 {
            20.0 * self.true_peak_l.log10()
        } else {
            f32::NEG_INFINITY
        };
        let tp_r_db = if self.true_peak_r > 0.0 {
            20.0 * self.true_peak_r.log10()
        } else {
            f32::NEG_INFINITY
        };

        *self.state.momentary_lufs.write() = momentary;
        *self.state.short_term_lufs.write() = short_term;
        *self.state.integrated_lufs.write() = integrated;
        *self.state.loudness_range.write() = lra;
        *self.state.true_peak_l.write() = tp_l_db;
        *self.state.true_peak_r.write() = tp_r_db;
    }

    fn compute_integrated(&self) -> f32 {
        // Absolute gate: -70 LUFS
        let abs_gate = 1e-7_f64; // -70 dBFS in linear mean-square
        let above_abs: Vec<f64> = self
            .integrated_blocks
            .iter()
            .copied()
            .filter(|&ms| ms > abs_gate)
            .collect();

        if above_abs.is_empty() {
            return f32::NEG_INFINITY;
        }

        // Relative gate: -10 LU below ungated integrated loudness
        let ungated_mean: f64 = above_abs.iter().sum::<f64>() / above_abs.len() as f64;
        let ungated_lufs = -0.691 + 10.0 * ungated_mean.log10();
        let rel_threshold = ungated_lufs - 10.0;
        let rel_gate_ms = 10.0_f64.powf((rel_threshold + 0.691) / 10.0);

        let above_rel: Vec<f64> = above_abs
            .iter()
            .copied()
            .filter(|&ms| ms > rel_gate_ms)
            .collect();

        if above_rel.is_empty() {
            return f32::NEG_INFINITY;
        }

        let final_mean: f64 = above_rel.iter().sum::<f64>() / above_rel.len() as f64;
        (-0.691 + 10.0 * final_mean.log10()) as f32
    }

    fn compute_lra(&self) -> f32 {
        if self.lra_history.len() < 2 {
            return 0.0;
        }
        let mut sorted: Vec<f32> = self.lra_history.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = sorted.len();
        let lo_idx = (n as f32 * 0.10) as usize;
        let hi_idx = ((n as f32 * 0.95) as usize).min(n - 1);
        let lo = sorted[lo_idx];
        let hi = sorted[hi_idx];
        (hi - lo).max(0.0)
    }

    /// Reset all accumulators (e.g., at the start of a new programme).
    pub fn reset(&mut self) {
        self.block_ms_buf.clear();
        self.current_block_l.clear();
        self.current_block_r.clear();
        self.integrated_blocks.clear();
        self.lra_history.clear();
        self.true_peak_l = f32::NEG_INFINITY;
        self.true_peak_r = f32::NEG_INFINITY;
        self.filter_l = KWeightingFilter::new(self.sample_rate);
        self.filter_r = KWeightingFilter::new(self.sample_rate);
    }
}
