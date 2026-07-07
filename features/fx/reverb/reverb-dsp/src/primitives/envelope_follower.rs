//! Peak envelope follower with separate attack/release time constants.
//!
//! Reference: Reiss & McPherson, "Audio Effects: Theory, Implementation
//! and Application" (CRC Press, 2014), Ch. 6 — Dynamics processing.

/// Single-pole peak follower. Output tracks `|input|` with one-pole
/// smoothing — fast attack, slow release.
pub struct EnvelopeFollower {
    envelope: f64,
    attack_coeff: f64,
    release_coeff: f64,
    sample_rate: f64,
    attack_ms: f64,
    release_ms: f64,
}

impl EnvelopeFollower {
    pub fn new(sample_rate: f64) -> Self {
        let mut env = Self {
            envelope: 0.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            sample_rate,
            attack_ms: 5.0,
            release_ms: 80.0,
        };
        env.update();
        env
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sample_rate = sr;
        self.update();
    }

    pub fn set_times(&mut self, attack_ms: f64, release_ms: f64) {
        self.attack_ms = attack_ms.max(0.01);
        self.release_ms = release_ms.max(0.01);
        self.update();
    }

    fn update(&mut self) {
        // -60dB time constant: coeff = exp(-2.2 / (time_s * sr))
        // Using simpler exp(-1 / (time_s * sr)) = ~63% time constant.
        let a_t = self.attack_ms * 0.001 * self.sample_rate;
        let r_t = self.release_ms * 0.001 * self.sample_rate;
        self.attack_coeff = (-1.0 / a_t).exp();
        self.release_coeff = (-1.0 / r_t).exp();
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        let rectified = input.abs();
        let coeff = if rectified > self.envelope {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.envelope = rectified + coeff * (self.envelope - rectified);
        self.envelope
    }

    pub fn reset(&mut self) {
        self.envelope = 0.0;
    }
}
