//! Envelope noise gate.
//!
//! A conventional threshold / attack / hold / release gate operating on a
//! peak-tracking detector. Below the threshold the gain falls to `range`
//! (a floor, not necessarily `-inf`), so it can be used either as a hard gate
//! or a gentle downward expander for bleed. Alloc-free; state clears on
//! [`Gate::reset`].

use audiocore_dsp::db::{db_to_linear, linear_to_db};

/// Gate tuning.
#[derive(Clone, Copy, Debug)]
pub struct GateConfig {
    /// Open threshold, dBFS.
    pub threshold_db: f64,
    /// Hysteresis below the open threshold at which the gate closes, dB.
    pub hysteresis_db: f64,
    /// Attack (open) time, ms.
    pub attack_ms: f64,
    /// Minimum hold-open time once opened, ms.
    pub hold_ms: f64,
    /// Release (close) time, ms.
    pub release_ms: f64,
    /// Closed-gain floor, dB (e.g. -80 for a hard gate, -12 for expansion).
    pub range_db: f64,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            threshold_db: -45.0,
            hysteresis_db: 3.0,
            attack_ms: 1.0,
            hold_ms: 30.0,
            release_ms: 120.0,
            range_db: -80.0,
        }
    }
}

/// Streaming gate.
pub struct Gate {
    cfg: GateConfig,
    sample_rate: f64,
    detector: f64,
    gain: f64,
    open: bool,
    hold_left: usize,
    attack_coeff: f64,
    release_coeff: f64,
    det_coeff: f64,
    floor: f64,
}

impl Gate {
    /// Create a gate.
    pub fn new(sample_rate: f64, cfg: GateConfig) -> Self {
        let mut g = Self {
            cfg,
            sample_rate: sample_rate.max(1.0),
            detector: 0.0,
            gain: db_to_linear(cfg.range_db),
            open: false,
            hold_left: 0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            det_coeff: 0.0,
            floor: db_to_linear(cfg.range_db),
        };
        g.recompute();
        g
    }

    fn coeff(ms: f64, sr: f64) -> f64 {
        (-1.0 / ((ms / 1000.0).max(1e-5) * sr)).exp()
    }

    fn recompute(&mut self) {
        self.attack_coeff = Self::coeff(self.cfg.attack_ms, self.sample_rate);
        self.release_coeff = Self::coeff(self.cfg.release_ms, self.sample_rate);
        self.det_coeff = Self::coeff(2.0, self.sample_rate); // 2 ms detector
        self.floor = db_to_linear(self.cfg.range_db);
    }

    /// Replace the tuning.
    pub fn set_config(&mut self, cfg: GateConfig) {
        self.cfg = cfg;
        self.recompute();
    }

    /// Clear runtime state.
    pub fn reset(&mut self) {
        self.detector = 0.0;
        self.gain = self.floor;
        self.open = false;
        self.hold_left = 0;
    }

    /// Process one sample, keyed on `key` (pass `input` for self-keying).
    #[inline]
    pub fn process_sample_keyed(&mut self, input: f64, key: f64) -> f64 {
        // Peak detector with fast attack / 2 ms release.
        let rectified = key.abs();
        if rectified > self.detector {
            self.detector = rectified;
        } else {
            self.detector = rectified + self.det_coeff * (self.detector - rectified);
        }
        let level_db = linear_to_db(self.detector);

        let close_db = self.cfg.threshold_db - self.cfg.hysteresis_db;
        if level_db >= self.cfg.threshold_db {
            self.open = true;
            self.hold_left = ((self.cfg.hold_ms / 1000.0) * self.sample_rate) as usize;
        } else if level_db < close_db {
            if self.hold_left > 0 {
                self.hold_left -= 1;
            } else {
                self.open = false;
            }
        }

        let target = if self.open { 1.0 } else { self.floor };
        let coeff = if target > self.gain {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.gain = target + coeff * (self.gain - target);
        input * self.gain
    }

    /// Process one self-keyed sample.
    #[inline]
    pub fn process_sample(&mut self, input: f64) -> f64 {
        self.process_sample_keyed(input, input)
    }

    /// Current gate gain (linear).
    #[inline]
    pub fn gain(&self) -> f64 {
        self.gain
    }
}
