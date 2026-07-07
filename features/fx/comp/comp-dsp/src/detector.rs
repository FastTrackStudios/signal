//! Level detection for the compressor.
//!
//! Uses peak detection with an optional RMS blend. The implementation is
//! intentionally simple and testable; deeper Pro-C reference notes live under
//! `docs/reports/proc3`.

use audiocore_dsp::db::{linear_to_db, DB_FLOOR};

/// Level detector using peak detection.
#[derive(Clone)]
pub struct Detector {
    peak: f64,
    rms_power: f64,
    sample_rate: f64,
}

impl Detector {
    pub fn new() -> Self {
        Self {
            peak: 0.0,
            rms_power: 0.0,
            sample_rate: 48_000.0,
        }
    }

    pub fn update_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1.0);
    }

    /// Detect level as simple 20*log10(|sample|).
    #[inline]
    pub fn detect_level(&mut self, input_abs: f64) -> f64 {
        self.detect_level_with_rms_mix(input_abs, 0.0)
    }

    /// Detect level with a peak-to-RMS blend.
    ///
    /// `rms_mix = 0.0` is sample peak detection and `1.0` is smoothed RMS
    /// detection. Intermediate values are useful for program-dependent
    /// compressors that need transient sensitivity without full peak behavior.
    #[inline]
    pub fn detect_level_with_rms_mix(&mut self, input_abs: f64, rms_mix: f64) -> f64 {
        self.peak = self.peak.max(input_abs);

        let rms_window_s = 0.01;
        let coeff = (-1.0 / (self.sample_rate * rms_window_s).max(1.0)).exp();
        self.rms_power = coeff * self.rms_power + (1.0 - coeff) * input_abs * input_abs;

        let rms = self.rms_power.sqrt();
        let mix = rms_mix.clamp(0.0, 1.0);
        let detected = input_abs * (1.0 - mix) + rms * mix;
        linear_to_db(detected).max(DB_FLOOR)
    }

    pub fn reset(&mut self) {
        self.peak = 0.0;
        self.rms_power = 0.0;
    }
}
