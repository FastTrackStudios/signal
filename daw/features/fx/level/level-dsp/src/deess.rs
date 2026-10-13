//! Split-band de-esser.
//!
//! The signal is split into a low band and a high (sibilance) band by a matched
//! low-pass / high-pass pair at the crossover. A fast RMS detector on the high
//! band drives downward gain reduction *of the high band only*; the low band is
//! passed untouched and the two are summed. This keeps the body of the voice
//! intact while taming "ess"/"sh" energy — the standard split-band de-ess
//! topology. Alloc-free; per-channel state.

use audiocore_dsp::db::{db_to_linear, linear_to_db};

use crate::filter::Biquad;

/// De-esser tuning.
#[derive(Clone, Copy, Debug)]
pub struct DeEssConfig {
    /// Crossover frequency between body and sibilance bands, Hz.
    pub crossover_hz: f64,
    /// High-band level above which reduction begins, dBFS.
    pub threshold_db: f64,
    /// Reduction ratio applied to high-band overshoot (e.g. 4 = 4:1).
    pub ratio: f64,
    /// Maximum high-band attenuation, dB.
    pub range_db: f64,
    /// Detector attack, ms.
    pub attack_ms: f64,
    /// Detector release, ms.
    pub release_ms: f64,
}

impl Default for DeEssConfig {
    fn default() -> Self {
        Self {
            crossover_hz: 6500.0,
            threshold_db: -30.0,
            ratio: 4.0,
            range_db: 12.0,
            attack_ms: 0.5,
            release_ms: 40.0,
        }
    }
}

/// Single-channel de-esser.
pub struct DeEsser {
    cfg: DeEssConfig,
    sample_rate: f64,
    lp: Biquad,
    hp: Biquad,
    detector: f64,
    gr_db: f64,
    attack_coeff: f64,
    release_coeff: f64,
}

impl DeEsser {
    /// Create a de-esser for one channel.
    pub fn new(sample_rate: f64, cfg: DeEssConfig) -> Self {
        let sr = sample_rate.max(1.0);
        let mut d = Self {
            cfg,
            sample_rate: sr,
            lp: Biquad::lowpass(sr, cfg.crossover_hz, core::f64::consts::FRAC_1_SQRT_2),
            hp: Biquad::highpass(sr, cfg.crossover_hz, core::f64::consts::FRAC_1_SQRT_2),
            detector: 0.0,
            gr_db: 0.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
        };
        d.recompute();
        d
    }

    fn coeff(ms: f64, sr: f64) -> f64 {
        (-1.0 / ((ms / 1000.0).max(1e-5) * sr)).exp()
    }

    fn recompute(&mut self) {
        self.attack_coeff = Self::coeff(self.cfg.attack_ms, self.sample_rate);
        self.release_coeff = Self::coeff(self.cfg.release_ms, self.sample_rate);
        self.lp = Biquad::lowpass(
            self.sample_rate,
            self.cfg.crossover_hz,
            core::f64::consts::FRAC_1_SQRT_2,
        );
        self.hp = Biquad::highpass(
            self.sample_rate,
            self.cfg.crossover_hz,
            core::f64::consts::FRAC_1_SQRT_2,
        );
    }

    /// Replace the tuning (recomputes filters and coefficients).
    pub fn set_config(&mut self, cfg: DeEssConfig) {
        self.cfg = cfg;
        self.recompute();
    }

    /// Clear runtime state.
    pub fn reset(&mut self) {
        self.lp.reset();
        self.hp.reset();
        self.detector = 0.0;
        self.gr_db = 0.0;
    }

    /// Current high-band gain reduction (positive dB).
    #[inline]
    pub fn reduction_db(&self) -> f64 {
        self.gr_db
    }

    /// Process one sample.
    #[inline]
    pub fn process_sample(&mut self, input: f64) -> f64 {
        let low = self.lp.process(input);
        let high = self.hp.process(input);

        // Fast RMS-ish detector on the high band.
        let rect = high.abs();
        let coeff = if rect > self.detector {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.detector = rect + coeff * (self.detector - rect);
        let level_db = linear_to_db(self.detector);

        // Downward compression of the high band above threshold.
        let over = level_db - self.cfg.threshold_db;
        let target_gr = if over > 0.0 {
            (over * (1.0 - 1.0 / self.cfg.ratio.max(1.0))).min(self.cfg.range_db)
        } else {
            0.0
        };
        // Smooth GR with the same attack/release feel.
        let gr_coeff = if target_gr > self.gr_db {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.gr_db = target_gr + gr_coeff * (self.gr_db - target_gr);

        low + high * db_to_linear(-self.gr_db)
    }
}
