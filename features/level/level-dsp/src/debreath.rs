//! Breath ducker.
//!
//! Breaths are audible but noise-like: moderate-to-high zero-crossing rate, a
//! mid/low spectral centroid (unlike bright sibilants), and a level that sits
//! *below* sung material but *above* the noise floor — typically in the gaps
//! between tonal segments. The ducker reuses the shared [`Classifier`] to score
//! each ~10 ms block and, when a block looks like a breath, drives a smoothed
//! attenuation of the audio. Unlike the gate it is spectrum-aware, so it leaves
//! quiet *sung* tails (which the gate might chew) alone.
//!
//! Detection has one block (~10 ms) of latency; the applied gain is slewed so
//! ducking is inaudible. Alloc-free after construction.

use audiocore_dsp::db::db_to_linear;

use crate::classify::{BlockClass, Classifier, ClassifyConfig};

/// De-breath tuning.
#[derive(Clone, Copy, Debug)]
pub struct DeBreathConfig {
    /// Attenuation applied to detected breaths, dB.
    pub reduction_db: f64,
    /// Upper level bound for a breath, dBFS (louder ⇒ not a breath).
    pub max_level_db: f64,
    /// Lower level bound for a breath, dBFS (quieter ⇒ noise floor / silence).
    pub min_level_db: f64,
    /// Centroid ceiling, Hz (brighter ⇒ sibilant, handled by the de-esser).
    pub max_centroid_hz: f64,
    /// Duck slew time, ms.
    pub slew_ms: f64,
}

impl Default for DeBreathConfig {
    fn default() -> Self {
        Self {
            reduction_db: 12.0,
            max_level_db: -28.0,
            min_level_db: -55.0,
            max_centroid_hz: 4000.0,
            slew_ms: 25.0,
        }
    }
}

/// Streaming breath ducker.
pub struct DeBreather {
    classifier: Classifier,
    cfg: DeBreathConfig,
    silence_db: f64,
    block: Vec<f64>,
    fill: usize,
    target_gain: f64,
    current_gain: f64,
    slew_coeff: f64,
}

impl DeBreather {
    /// Create a de-breather.
    pub fn new(
        sample_rate: f64,
        class_cfg: ClassifyConfig,
        cfg: DeBreathConfig,
        silence_db: f64,
    ) -> Self {
        let classifier = Classifier::new(sample_rate, class_cfg);
        let block_samples = classifier.block_samples();
        let slew_coeff = (-1.0 / ((cfg.slew_ms / 1000.0).max(1e-4) * sample_rate.max(1.0))).exp();
        Self {
            classifier,
            cfg,
            silence_db,
            block: vec![0.0; block_samples],
            fill: 0,
            target_gain: 1.0,
            current_gain: 1.0,
            slew_coeff,
        }
    }

    /// Update the adaptive silence floor.
    pub fn set_silence_db(&mut self, db: f64) {
        self.silence_db = db;
    }

    /// Replace the tuning in place (no allocation). `sample_rate` is needed to
    /// recompute the slew coefficient.
    pub fn set_config(&mut self, cfg: DeBreathConfig, sample_rate: f64) {
        self.cfg = cfg;
        self.slew_coeff = (-1.0 / ((cfg.slew_ms / 1000.0).max(1e-4) * sample_rate.max(1.0))).exp();
    }

    /// Clear runtime state.
    pub fn reset(&mut self) {
        self.classifier.reset();
        self.fill = 0;
        self.target_gain = 1.0;
        self.current_gain = 1.0;
    }

    /// Process one mono sample.
    #[inline]
    pub fn process_sample(&mut self, input: f64) -> f64 {
        self.block[self.fill] = input;
        self.fill += 1;
        if self.fill >= self.block.len() {
            self.finish_block();
            self.fill = 0;
        }
        self.current_gain =
            self.target_gain + self.slew_coeff * (self.current_gain - self.target_gain);
        input * self.current_gain
    }

    fn finish_block(&mut self) {
        let f = self.classifier.analyze_block(&self.block);
        let class = self.classifier.classify(&f, self.silence_db);
        // A breath is a non-tonal, audible-but-quiet, not-too-bright block.
        let is_breath = class == BlockClass::Consonant
            && f.rms_db <= self.cfg.max_level_db
            && f.rms_db >= self.cfg.min_level_db
            && f.centroid_hz <= self.cfg.max_centroid_hz;
        self.target_gain = if is_breath {
            db_to_linear(-self.cfg.reduction_db)
        } else {
            1.0
        };
    }
}
