//! K-system metering (Bob Katz, 2000).
//!
//! Implements VU-ballistic RMS metering with K-20, K-14, and K-12 reference levels.
//! The reference level is the 0 VU mark (e.g. −20 dBFS for K-20).

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::RwLock;

// ── K-mode ───────────────────────────────────────────────────────────────────

/// K-system reference level (offset from 0 dBFS to 0 VU).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KMode {
    /// K-20: 0 VU = −20 dBFS. Film, classical music, full dynamics.
    K20,
    /// K-14: 0 VU = −14 dBFS. Pop/rock — moderate limiting.
    K14,
    /// K-12: 0 VU = −12 dBFS. Broadcast / streaming.
    K12,
}

impl KMode {
    /// Reference level in dBFS that aligns to 0 VU on the scale.
    pub fn reference_dbfs(self) -> f32 {
        match self {
            KMode::K20 => -20.0,
            KMode::K14 => -14.0,
            KMode::K12 => -12.0,
        }
    }
}

// ── Shared state ──────────────────────────────────────────────────────────────

/// K-meter measurement results shared with the UI thread.
pub struct KMeterState {
    /// RMS level (dBFS).
    pub rms_db: RwLock<f32>,
    /// True peak level (dBFS).
    pub peak_db: RwLock<f32>,
    /// Held peak level (dBFS) — falls slowly over ~2 s.
    pub peak_hold_db: RwLock<f32>,
    /// Active K-mode, for scale rendering.
    pub mode: RwLock<KMode>,
}

impl KMeterState {
    fn new(mode: KMode) -> Arc<Self> {
        Arc::new(Self {
            rms_db: RwLock::new(f32::NEG_INFINITY),
            peak_db: RwLock::new(f32::NEG_INFINITY),
            peak_hold_db: RwLock::new(f32::NEG_INFINITY),
            mode: RwLock::new(mode),
        })
    }
}

// ── K-meter ──────────────────────────────────────────────────────────────────

/// K-system VU meter for one channel.
///
/// Use one `KMeter` per channel; both can share one [`KMeterState`] if you
/// want them metered together, or keep separate states for L/R display.
///
/// Call [`KMeter::process`] once per audio block from the audio thread.
pub struct KMeter {
    sample_rate: f32,

    /// Ring buffer of squared samples for 300 ms RMS window.
    rms_buf: VecDeque<f32>,
    rms_window: usize,
    rms_sum: f64,

    /// Instantaneous peak (first-order IIR with 300 ms attack/release).
    peak_smooth: f32,
    peak_coeff: f32,

    /// Held peak.
    peak_hold: f32,
    /// Samples remaining at held peak before decay begins.
    hold_samples: usize,
    hold_window: usize,

    pub state: Arc<KMeterState>,
}

impl KMeter {
    /// Create a new K-meter.
    pub fn new(sample_rate: f32, mode: KMode) -> Self {
        let rms_window = (sample_rate * 0.300) as usize; // 300 ms
        let hold_window = (sample_rate * 2.0) as usize; // 2 s hold

        // 300 ms first-order IIR time constant
        let tau = 0.300_f32;
        let peak_coeff = (-1.0_f32 / (sample_rate * tau)).exp();

        Self {
            sample_rate,
            rms_buf: VecDeque::with_capacity(rms_window + 1),
            rms_window,
            rms_sum: 0.0,
            peak_smooth: 0.0,
            peak_coeff,
            peak_hold: f32::NEG_INFINITY,
            hold_samples: 0,
            hold_window,
            state: KMeterState::new(mode),
        }
    }

    /// Process a mono block of audio samples.
    ///
    /// For stereo, call this on each channel separately.
    pub fn process(&mut self, samples: &[f32]) {
        for &s in samples {
            let sq = s * s;

            // RMS ring buffer
            self.rms_buf.push_back(sq);
            self.rms_sum += sq as f64;
            if self.rms_buf.len() > self.rms_window {
                let old = self.rms_buf.pop_front().unwrap_or(0.0);
                self.rms_sum -= old as f64;
            }

            // VU ballistics: first-order IIR on |sample|
            let abs_s = s.abs();
            let c = self.peak_coeff;
            if abs_s > self.peak_smooth {
                // Fast attack: 300 ms (same as release per VU spec)
                self.peak_smooth = abs_s + c * (self.peak_smooth - abs_s);
            } else {
                self.peak_smooth = self.peak_smooth * c + abs_s * (1.0 - c);
            }

            // Peak hold
            if abs_s >= self.peak_hold {
                self.peak_hold = abs_s;
                self.hold_samples = self.hold_window;
            } else if self.hold_samples > 0 {
                self.hold_samples -= 1;
            } else {
                // Slow decay after hold: release at ~10 dB/s
                let decay = (-1.0_f32 / (self.sample_rate * 0.100)).exp();
                self.peak_hold *= decay;
            }
        }

        self.update_state();
    }

    fn update_state(&self) {
        let n = self.rms_buf.len() as f64;
        let rms_db = if n > 0.0 && self.rms_sum > 0.0 {
            let rms = (self.rms_sum / n).sqrt() as f32;
            20.0 * rms.log10()
        } else {
            f32::NEG_INFINITY
        };

        let peak_db = if self.peak_smooth > 1e-10 {
            20.0 * self.peak_smooth.log10()
        } else {
            f32::NEG_INFINITY
        };

        let peak_hold_db = if self.peak_hold > 1e-10 {
            20.0 * self.peak_hold.log10()
        } else {
            f32::NEG_INFINITY
        };

        *self.state.rms_db.write() = rms_db;
        *self.state.peak_db.write() = peak_db;
        *self.state.peak_hold_db.write() = peak_hold_db;
    }

    /// Change the K-mode reference level.
    pub fn set_mode(&mut self, mode: KMode) {
        *self.state.mode.write() = mode;
    }

    /// Reset all accumulators.
    pub fn reset(&mut self) {
        self.rms_buf.clear();
        self.rms_sum = 0.0;
        self.peak_smooth = 0.0;
        self.peak_hold = f32::NEG_INFINITY;
        self.hold_samples = 0;
    }
}
