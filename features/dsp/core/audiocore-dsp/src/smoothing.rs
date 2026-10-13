//! One-pole parameter smoothers.
//!
//! Shared primitive for smooth, click-free parameter changes in real-time
//! audio processing. Used by delay time smoothing, gain smoothing, etc.

use crate::envelope::EnvelopeFollower;

// r[impl dsp.smoothing.one-pole]
/// One-pole parameter smoother with snap-to-target.
///
/// Smoothly interpolates toward a target value using an exponential one-pole
/// filter. Snaps to target when within epsilon to avoid endless chasing.
pub struct ParamSmoother {
    follower: EnvelopeFollower,
    target: f64,
    coeff: f64,
    epsilon: f64,
}

impl ParamSmoother {
    pub fn new(initial: f64) -> Self {
        Self {
            follower: EnvelopeFollower::new(initial),
            target: initial,
            coeff: 0.0,
            epsilon: 0.1,
        }
    }

    /// Set smoothing time in seconds.
    pub fn set_time(&mut self, time_s: f64, sample_rate: f64) {
        self.coeff = EnvelopeFollower::coeff(time_s, sample_rate);
    }

    /// Set smoothing time in milliseconds.
    pub fn set_time_ms(&mut self, time_ms: f64, sample_rate: f64) {
        self.set_time(time_ms * 0.001, sample_rate);
    }

    /// Set smoothing time and seed a fresh smoother: while the value is
    /// still the zero it was constructed with, jump straight to `target`
    /// instead of audibly sweeping up from zero on the first buffer.
    pub fn set_time_seeded(&mut self, time_s: f64, sample_rate: f64, target: f64) {
        self.set_time(time_s, sample_rate);
        if self.value() == 0.0 {
            self.set_immediate(target);
        }
    }

    /// Set the snap-to-target threshold. When `|value - target| < epsilon`,
    /// the smoother jumps to the target. Default: 0.1.
    pub fn set_epsilon(&mut self, eps: f64) {
        self.epsilon = eps;
    }

    /// Set a new target value.
    pub fn set_target(&mut self, target: f64) {
        self.target = target;
    }

    /// Advance one sample. Returns the current smoothed value.
    #[inline]
    pub fn tick(&mut self) -> f64 {
        let val = self.follower.tick_symmetric(self.target, self.coeff);
        if (val - self.target).abs() < self.epsilon {
            self.follower.set_value(self.target);
            return self.target;
        }
        val
    }

    /// Get the current smoothed value without advancing.
    #[inline]
    pub fn value(&self) -> f64 {
        self.follower.value()
    }

    /// Get the current target.
    pub fn target(&self) -> f64 {
        self.target
    }

    /// Whether the smoother has reached the target.
    pub fn is_settled(&self) -> bool {
        (self.follower.value() - self.target).abs() < self.epsilon
    }

    /// Jump immediately to a value (no smoothing).
    pub fn set_immediate(&mut self, value: f64) {
        self.target = value;
        self.follower.set_value(value);
    }

    pub fn reset(&mut self, value: f64) {
        self.set_immediate(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn immediate_on_creation() {
        let s = ParamSmoother::new(440.0);
        assert_eq!(s.value(), 440.0);
    }

    #[test]
    fn smooths_toward_target() {
        let mut s = ParamSmoother::new(0.0);
        s.set_time_ms(10.0, SR);
        s.set_target(1.0);

        // After 1ms, should be partway there
        for _ in 0..48 {
            s.tick();
        }
        let v = s.value();
        assert!(v > 0.05 && v < 0.95, "Should be moving: {v}");

        // After 100ms, should be settled
        for _ in 0..4800 {
            s.tick();
        }
        assert_eq!(s.value(), 1.0);
        assert!(s.is_settled());
    }

    #[test]
    fn snap_to_target() {
        let mut s = ParamSmoother::new(0.0);
        s.set_time_ms(10.0, SR);
        s.set_epsilon(0.01);
        s.set_target(1.0);

        // Run until settled
        for _ in 0..48000 {
            s.tick();
            if s.is_settled() {
                break;
            }
        }
        assert_eq!(s.value(), 1.0, "Should snap exactly to target");
    }

    #[test]
    fn set_immediate_skips_smoothing() {
        let mut s = ParamSmoother::new(0.0);
        s.set_time_ms(1000.0, SR);
        s.set_immediate(42.0);
        assert_eq!(s.value(), 42.0);
        assert!(s.is_settled());
    }
}
