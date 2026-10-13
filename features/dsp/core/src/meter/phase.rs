//! Phase correlation and goniometer.
//!
//! Phase correlation is computed as the normalized cross-correlation of the
//! left and right channels over a ~300 ms sliding window.
//!
//! The goniometer stores a recent history of (mid, side) sample pairs for
//! use by the dot-plot painter.

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::RwLock;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Number of (mid, side) pairs retained for the goniometer dot plot.
pub const GONIO_HISTORY: usize = 1024;

// ── Shared state ──────────────────────────────────────────────────────────────

/// Phase correlation and goniometer state shared with the UI painter.
pub struct PhaseState {
    /// Normalized phase correlation in [−1, 1].
    ///
    /// +1 = perfectly in-phase (mono), −1 = perfectly out-of-phase.
    pub correlation: RwLock<f32>,
    /// Recent (mid, side) pairs for the goniometer dot plot.
    ///
    /// `mid  = (L + R) / √2`,  `side = (L − R) / √2`.
    pub goniometer_samples: RwLock<Vec<(f32, f32)>>,
}

impl PhaseState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            correlation: RwLock::new(0.0),
            goniometer_samples: RwLock::new(Vec::with_capacity(GONIO_HISTORY)),
        })
    }
}

// ── Phase correlator ──────────────────────────────────────────────────────────

/// Stereo phase correlator with goniometer sample history.
///
/// Call [`PhaseCorrelation::process`] once per sample from the audio thread.
/// Call [`PhaseCorrelation::process_block`] for block processing.
///
/// Read results via the shared [`PhaseState`] arc.
pub struct PhaseCorrelation {
    /// Window size in samples for correlation (~300 ms).
    window: usize,

    // Ring buffers for L, R, and the cross/auto products.
    sum_ll: f64,
    sum_rr: f64,
    sum_lr: f64,
    buf_ll: VecDeque<f64>,
    buf_rr: VecDeque<f64>,
    buf_lr: VecDeque<f64>,

    /// Decimated goniometer history (updated every `decimate` samples).
    gonio_buf: VecDeque<(f32, f32)>,
    decimate: usize,
    decimate_ctr: usize,

    pub state: Arc<PhaseState>,
}

impl PhaseCorrelation {
    /// Create a new phase correlator for the given sample rate.
    pub fn new(sample_rate: f32) -> Self {
        let window = (sample_rate * 0.300) as usize; // 300 ms
                                                     // Decimate goniometer to ~60 fps worth of points in GONIO_HISTORY.
        let decimate = ((sample_rate / 60.0) as usize).max(1);
        Self {
            window,
            sum_ll: 0.0,
            sum_rr: 0.0,
            sum_lr: 0.0,
            buf_ll: VecDeque::with_capacity(window + 1),
            buf_rr: VecDeque::with_capacity(window + 1),
            buf_lr: VecDeque::with_capacity(window + 1),
            gonio_buf: VecDeque::with_capacity(GONIO_HISTORY + 1),
            decimate,
            decimate_ctr: 0,
            state: PhaseState::new(),
        }
    }

    /// Process a single stereo sample pair.
    pub fn process(&mut self, left: f32, right: f32) {
        let ll = left as f64 * left as f64;
        let rr = right as f64 * right as f64;
        let lr = left as f64 * right as f64;

        self.sum_ll += ll;
        self.sum_rr += rr;
        self.sum_lr += lr;

        self.buf_ll.push_back(ll);
        self.buf_rr.push_back(rr);
        self.buf_lr.push_back(lr);

        if self.buf_ll.len() > self.window {
            self.sum_ll -= self.buf_ll.pop_front().unwrap_or(0.0);
            self.sum_rr -= self.buf_rr.pop_front().unwrap_or(0.0);
            self.sum_lr -= self.buf_lr.pop_front().unwrap_or(0.0);
        }

        // Goniometer decimation
        self.decimate_ctr += 1;
        if self.decimate_ctr >= self.decimate {
            self.decimate_ctr = 0;
            let sqrt2_inv = std::f32::consts::FRAC_1_SQRT_2;
            let mid = (left + right) * sqrt2_inv;
            let side = (left - right) * sqrt2_inv;
            self.gonio_buf.push_back((mid, side));
            if self.gonio_buf.len() > GONIO_HISTORY {
                self.gonio_buf.pop_front();
            }
        }
    }

    /// Process a block of stereo samples.
    ///
    /// Calls [`PhaseCorrelation::process`] for each pair and updates shared state once after.
    pub fn process_block(&mut self, left: &[f32], right: &[f32]) {
        debug_assert_eq!(left.len(), right.len());
        for (&l, &r) in left.iter().zip(right.iter()) {
            self.process(l, r);
        }
        self.update_state();
    }

    fn update_state(&self) {
        // Pearson correlation: r = sum_lr / sqrt(sum_ll * sum_rr)
        let denom = (self.sum_ll * self.sum_rr).sqrt();
        let correlation = if denom > 1e-30 {
            (self.sum_lr / denom).clamp(-1.0, 1.0) as f32
        } else {
            0.0
        };

        *self.state.correlation.write() = correlation;

        let mut guard = self.state.goniometer_samples.write();
        guard.clear();
        guard.extend(self.gonio_buf.iter().copied());
    }

    /// Reset all accumulators.
    pub fn reset(&mut self) {
        self.sum_ll = 0.0;
        self.sum_rr = 0.0;
        self.sum_lr = 0.0;
        self.buf_ll.clear();
        self.buf_rr.clear();
        self.buf_lr.clear();
        self.gonio_buf.clear();
        self.decimate_ctr = 0;
    }
}
