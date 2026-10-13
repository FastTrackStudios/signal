//! Exponential envelope follower with asymmetric attack/release.
//!
//! Shared primitive used by compressor, gate, trigger, and rider detectors.

// r[impl dsp.envelope.exponential]
// r[impl dsp.envelope.asymmetric]
/// Exponential one-pole envelope follower with separate attack and release.
///
/// Operates on a single value (not stereo-aware — callers handle channels).
/// Works in whatever domain you feed it (linear, dB, etc).
pub struct EnvelopeFollower {
    value: f64,
    attack_coeff: f64,
    release_coeff: f64,
}

impl EnvelopeFollower {
    pub fn new(initial: f64) -> Self {
        Self {
            value: initial,
            attack_coeff: 0.0,
            release_coeff: 0.0,
        }
    }

    /// Compute one-pole coefficient from time in seconds and sample rate.
    ///
    /// This is the shared formula used by all dynamics plugins:
    /// `exp(-1 / (time_s * sample_rate))`
    #[inline]
    pub fn coeff(time_s: f64, sample_rate: f64) -> f64 {
        if time_s > 0.0 {
            (-1.0 / (sample_rate * time_s)).exp()
        } else {
            0.0 // Instant
        }
    }

    /// Set both coefficients from time in seconds.
    pub fn set_times(&mut self, attack_s: f64, release_s: f64, sample_rate: f64) {
        self.attack_coeff = Self::coeff(attack_s, sample_rate);
        self.release_coeff = Self::coeff(release_s, sample_rate);
    }

    /// Set both coefficients from time in milliseconds.
    pub fn set_times_ms(&mut self, attack_ms: f64, release_ms: f64, sample_rate: f64) {
        self.set_times(attack_ms * 0.001, release_ms * 0.001, sample_rate);
    }

    /// Set coefficients directly (for cases where callers compute their own).
    pub fn set_coeffs(&mut self, attack: f64, release: f64) {
        self.attack_coeff = attack;
        self.release_coeff = release;
    }

    /// Process one sample with asymmetric attack/release.
    ///
    /// Attack when input > current value, release otherwise.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        let coeff = if input > self.value {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.value = coeff * (self.value - input) + input;
        self.value
    }

    /// Process one sample with a single coefficient (symmetric smoothing).
    #[inline]
    pub fn tick_symmetric(&mut self, input: f64, coeff: f64) -> f64 {
        self.value = coeff * (self.value - input) + input;
        self.value
    }

    /// Get the current smoothed value.
    #[inline]
    pub fn value(&self) -> f64 {
        self.value
    }

    /// Set the current value directly (for initialization or reset).
    pub fn set_value(&mut self, v: f64) {
        self.value = v;
    }

    pub fn reset(&mut self, initial: f64) {
        self.value = initial;
    }
}

impl Default for EnvelopeFollower {
    fn default() -> Self {
        Self::new(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn coeff_instant() {
        assert_eq!(EnvelopeFollower::coeff(0.0, SR), 0.0);
    }

    #[test]
    fn coeff_reasonable() {
        let c = EnvelopeFollower::coeff(0.01, SR); // 10ms
        assert!(c > 0.99 && c < 1.0, "10ms coeff should be near 1: {c}");
    }

    #[test]
    fn tracks_step_up() {
        let mut env = EnvelopeFollower::new(0.0);
        env.set_times_ms(1.0, 100.0, SR);

        // Step from 0 to 1 — fast attack
        for _ in 0..480 {
            // 10ms
            env.tick(1.0);
        }
        assert!(
            env.value() > 0.99,
            "Should reach 1.0 quickly: {}",
            env.value()
        );
    }

    #[test]
    fn tracks_step_down() {
        let mut env = EnvelopeFollower::new(1.0);
        env.set_times_ms(1.0, 10.0, SR);

        // Step from 1 to 0 — slower release
        for _ in 0..48 {
            // 1ms
            env.tick(0.0);
        }
        // Should not have fully released in 1ms with 10ms release
        assert!(
            env.value() > 0.1,
            "Should still be releasing: {}",
            env.value()
        );

        // After 50ms, should be mostly released
        for _ in 0..2400 {
            env.tick(0.0);
        }
        assert!(env.value() < 0.01, "Should be near zero: {}", env.value());
    }

    #[test]
    fn symmetric_smoothing() {
        let mut env = EnvelopeFollower::new(0.0);
        let coeff = EnvelopeFollower::coeff(0.01, SR);

        for _ in 0..4800 {
            // 100ms
            env.tick_symmetric(1.0, coeff);
        }
        assert!((env.value() - 1.0).abs() < 0.001);
    }

    #[test]
    fn no_nan() {
        let mut env = EnvelopeFollower::new(0.0);
        env.set_times_ms(0.0, 0.0, SR); // Instant

        for &input in &[0.0, 1.0, -1.0, f64::MAX, f64::MIN_POSITIVE, 1e-300] {
            let v = env.tick(input);
            assert!(v.is_finite(), "NaN for input {input}");
        }
    }
}
