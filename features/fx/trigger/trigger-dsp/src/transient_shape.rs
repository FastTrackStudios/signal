//! Transient shaping preprocessor for onset detection.
//!
//! Enhances attack transients relative to sustain/bleed before feeding
//! the signal into an onset detector. This improves detection reliability
//! on compressed, limited, or heavily processed drum tracks.
//!
//! Algorithm:
//! - Fast envelope follower (sub-millisecond attack, ~50ms release)
//! - Slow envelope follower (~10ms attack, ~200ms release)
//! - Transient ratio = fast / (slow + eps)
//! - Output = input * (ratio ^ amount)
//!
//! When a transient hits, the fast envelope jumps up immediately while
//! the slow envelope lags behind, producing a high ratio. During sustain,
//! both envelopes converge, producing a ratio near 1.0.

/// Transient shaper for onset detection preprocessing.
pub struct TransientShaper {
    fast_env: f64,
    slow_env: f64,
    fast_attack: f64,
    fast_release: f64,
    slow_attack: f64,
    slow_release: f64,

    /// Shaping amount: 0.0 = bypass, 1.0 = moderate, 2.0+ = aggressive.
    pub amount: f64,
    /// Whether the shaper is enabled.
    pub enabled: bool,

    sample_rate: f64,
}

impl TransientShaper {
    pub fn new(sample_rate: f64) -> Self {
        let mut s = Self {
            fast_env: 0.0,
            slow_env: 0.0,
            fast_attack: 0.0,
            fast_release: 0.0,
            slow_attack: 0.0,
            slow_release: 0.0,
            amount: 1.0,
            enabled: true,
            sample_rate,
        };
        s.compute_coefficients(sample_rate);
        s
    }

    /// Update sample rate.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.compute_coefficients(sample_rate);
    }

    /// Reset envelope followers.
    pub fn reset(&mut self) {
        self.fast_env = 0.0;
        self.slow_env = 0.0;
    }

    /// Process one sample. Returns the transient-enhanced sample.
    #[inline]
    pub fn tick(&mut self, sample: f64) -> f64 {
        if !self.enabled {
            return sample;
        }

        let abs = sample.abs();

        // Fast envelope: very fast attack, moderate release
        let fast_coeff = if abs > self.fast_env {
            self.fast_attack
        } else {
            self.fast_release
        };
        self.fast_env = fast_coeff * self.fast_env + (1.0 - fast_coeff) * abs;

        // Slow envelope: moderate attack, slow release
        let slow_coeff = if abs > self.slow_env {
            self.slow_attack
        } else {
            self.slow_release
        };
        self.slow_env = slow_coeff * self.slow_env + (1.0 - slow_coeff) * abs;

        // Transient ratio
        let eps = 1e-10;
        let ratio = self.fast_env / (self.slow_env + eps);

        // Apply shaping: boost transients, attenuate sustain
        // ratio > 1 during transients, ~1 during sustain, < 1 during release
        let gain = ratio.powf(self.amount);

        // Soft-limit gain to prevent extreme values
        let gain = gain.min(10.0);

        sample * gain
    }

    fn compute_coefficients(&mut self, sr: f64) {
        // Fast envelope: 0.1ms attack, 50ms release
        self.fast_attack = Self::time_constant(0.0001, sr);
        self.fast_release = Self::time_constant(0.050, sr);
        // Slow envelope: 10ms attack, 200ms release
        self.slow_attack = Self::time_constant(0.010, sr);
        self.slow_release = Self::time_constant(0.200, sr);
    }

    /// Convert time constant in seconds to one-pole filter coefficient.
    fn time_constant(time_s: f64, sr: f64) -> f64 {
        if time_s <= 0.0 || sr <= 0.0 {
            return 0.0;
        }
        (-1.0 / (time_s * sr)).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_when_disabled() {
        let mut ts = TransientShaper::new(48000.0);
        ts.enabled = false;
        for i in 0..100 {
            let input = (i as f64 * 0.1).sin();
            assert_eq!(ts.tick(input), input);
        }
    }

    #[test]
    fn boosts_transient() {
        let mut ts = TransientShaper::new(48000.0);
        ts.amount = 2.0;

        // Feed silence, then a sudden burst
        for _ in 0..4800 {
            ts.tick(0.0);
        }

        // Transient — the first few samples should be boosted
        let mut transient_outputs = Vec::new();
        for _ in 0..480 {
            transient_outputs.push(ts.tick(0.5));
        }

        // During sustained signal, output should settle near input level
        let mut sustained_outputs = Vec::new();
        for _ in 0..48000 {
            sustained_outputs.push(ts.tick(0.5));
        }

        let transient_peak = transient_outputs.iter().cloned().fold(0.0_f64, f64::max);
        let sustained_avg: f64 = sustained_outputs[sustained_outputs.len() - 100..]
            .iter()
            .sum::<f64>()
            / 100.0;

        assert!(
            transient_peak > sustained_avg * 1.2,
            "Transient should be boosted: peak={:.3}, sustained_avg={:.3}",
            transient_peak,
            sustained_avg
        );
    }

    #[test]
    fn output_finite() {
        let mut ts = TransientShaper::new(48000.0);
        ts.amount = 3.0;

        let signals = [0.0, 1.0, -1.0, 0.001, 0.999, -0.5];
        for &s in &signals {
            for _ in 0..100 {
                let out = ts.tick(s);
                assert!(out.is_finite(), "Output must be finite for input {}", s);
            }
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut ts = TransientShaper::new(48000.0);
        for _ in 0..1000 {
            ts.tick(0.5);
        }
        ts.reset();
        assert_eq!(ts.fast_env, 0.0);
        assert_eq!(ts.slow_env, 0.0);
    }
}
