//! Realtime vocal rider.
//!
//! A streaming counterpart to the offline analyser: audio is buffered into
//! ~10 ms blocks, each block classified, and a target gain computed for tonal
//! blocks (drive their RMS toward `target_db`, scaled by `amount`). The gain is
//! then slewed per-sample so the ride is click-free. Consonant and silent
//! blocks hold the current gain rather than pumping.
//!
//! Unlike the offline path there is no look-ahead: a block's gain applies to the
//! *following* block, i.e. one block (~10 ms) of latency in the ride envelope,
//! which is inaudible for level moves. The rider allocates only in
//! [`VocalRider::new`]; `process` is alloc-free.

use audiocore_dsp::db::{db_to_linear, linear_to_db};

use crate::classify::{BlockClass, Classifier, ClassifyConfig};

/// Rider tuning.
#[derive(Clone, Copy, Debug)]
pub struct RiderConfig {
    /// Target RMS for tonal material, dBFS.
    pub target_db: f64,
    /// How strongly to correct toward the target, 0..1 (1 = full correction).
    pub amount: f64,
    /// Maximum boost, dB.
    pub max_gain_db: f64,
    /// Maximum cut, dB (as a positive magnitude; applied as -range).
    pub max_cut_db: f64,
    /// Blocks below this level ride nothing (hold), dBFS.
    pub gate_db: f64,
    /// Gain slew time toward the target, ms.
    pub slew_ms: f64,
}

impl Default for RiderConfig {
    fn default() -> Self {
        Self {
            target_db: -18.0,
            amount: 1.0,
            max_gain_db: 12.0,
            max_cut_db: 18.0,
            gate_db: -40.0,
            slew_ms: 40.0,
        }
    }
}

/// Streaming vocal rider. Mono detection, applies the same gain to all
/// interleaved channels of the block.
pub struct VocalRider {
    classifier: Classifier,
    cfg: RiderConfig,
    sample_rate: f64,
    silence_db: f64,
    // Block accumulation.
    block: Vec<f64>,
    fill: usize,
    // Gain state.
    target_gain: f64,
    current_gain: f64,
    slew_coeff: f64,
}

impl VocalRider {
    /// Create a rider. `silence_db` seeds the classifier's silence threshold and
    /// can be refined later via [`VocalRider::set_silence_db`] (e.g. from an
    /// offline pre-analysis of the same source).
    pub fn new(
        sample_rate: f64,
        class_cfg: ClassifyConfig,
        cfg: RiderConfig,
        silence_db: f64,
    ) -> Self {
        let classifier = Classifier::new(sample_rate, class_cfg);
        let block_samples = classifier.block_samples();
        let mut r = Self {
            classifier,
            cfg,
            sample_rate: sample_rate.max(1.0),
            silence_db,
            block: vec![0.0; block_samples],
            fill: 0,
            target_gain: 1.0,
            current_gain: 1.0,
            slew_coeff: 0.0,
        };
        r.recompute_slew();
        r
    }

    fn recompute_slew(&mut self) {
        // One-pole coefficient for the configured slew time.
        let t = (self.cfg.slew_ms / 1000.0).max(1e-4);
        self.slew_coeff = (-1.0 / (t * self.sample_rate)).exp();
    }

    /// Update the adaptive silence floor (dB).
    pub fn set_silence_db(&mut self, db: f64) {
        self.silence_db = db;
    }

    /// Replace the rider tuning; recomputes the slew coefficient.
    pub fn set_config(&mut self, cfg: RiderConfig) {
        self.cfg = cfg;
        self.recompute_slew();
    }

    /// Reset all runtime state (call on transport (re)start).
    pub fn reset(&mut self) {
        self.classifier.reset();
        self.fill = 0;
        self.target_gain = 1.0;
        self.current_gain = 1.0;
    }

    /// Current smoothed ride gain (linear) — useful for meters/automation.
    #[inline]
    pub fn current_gain(&self) -> f64 {
        self.current_gain
    }

    /// Process one mono sample, returning the ride-corrected sample. The
    /// detection is driven by `key` (defaults to the input when equal), so a
    /// sidechain can be supplied by passing a different key signal.
    #[inline]
    pub fn process_sample_keyed(&mut self, input: f64, key: f64) -> f64 {
        // Accumulate detection block.
        self.block[self.fill] = key;
        self.fill += 1;
        if self.fill >= self.block.len() {
            self.finish_block();
            self.fill = 0;
        }
        // Slew current toward target and apply.
        self.current_gain =
            self.target_gain + self.slew_coeff * (self.current_gain - self.target_gain);
        input * self.current_gain
    }

    /// Process one mono sample (key == input).
    #[inline]
    pub fn process_sample(&mut self, input: f64) -> f64 {
        self.process_sample_keyed(input, input)
    }

    fn finish_block(&mut self) {
        let feats = self.classifier.analyze_block(&self.block);
        let class = self.classifier.classify(&feats, self.silence_db);
        // Only ride tonal, audible blocks; otherwise hold.
        if class == BlockClass::Tonal && feats.rms_db > self.cfg.gate_db {
            let delta_db = (self.cfg.target_db - feats.rms_db) * self.cfg.amount;
            let clamped = delta_db.clamp(-self.cfg.max_cut_db, self.cfg.max_gain_db);
            self.target_gain = db_to_linear(clamped);
        }
        // Consonant / silence: keep the last tonal gain (no pumping).
        let _ = linear_to_db; // kept for symmetry / future metering
    }
}
