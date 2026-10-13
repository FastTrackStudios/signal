//! RC smoother — one-pole lowpass with separate attack/release coefficients.
//!
//! Used to smooth the raw pattern output, preventing zipper noise.
//! Attack coefficient is used when the value is rising, release when falling.
//!
//! Based on tiagolr's RCSmoother shared across gate12, filtr, time12, reevr.

/// One-pole RC smoother with dual attack/release coefficients.
///
/// The coefficient formula is: `1.0 / (param^2 * 0.25 * sample_rate + 1.0)`,
/// where lower values = more smoothing.
pub struct RcSmoother {
    value: f64,
    attack_coeff: f64,
    release_coeff: f64,
}

impl RcSmoother {
    pub fn new(initial: f64) -> Self {
        Self {
            value: initial,
            attack_coeff: 1.0,
            release_coeff: 1.0,
        }
    }

    /// Compute the RC coefficient from a normalized parameter (0..1) and sample rate.
    ///
    /// `param = 0.0` → instant (coeff = 1.0)
    /// `param = 1.0` → maximum smoothing
    #[inline]
    pub fn coeff(param: f64, sample_rate: f64) -> f64 {
        1.0 / (param * param * 0.25 * sample_rate + 1.0)
    }

    /// Set attack and release from normalized parameters (0..1).
    pub fn set_params(&mut self, attack: f64, release: f64, sample_rate: f64) {
        self.attack_coeff = Self::coeff(attack, sample_rate);
        self.release_coeff = Self::coeff(release, sample_rate);
    }

    /// Set a single symmetric smoothing coefficient.
    pub fn set_symmetric(&mut self, smooth: f64, sample_rate: f64) {
        let c = Self::coeff(smooth, sample_rate);
        self.attack_coeff = c;
        self.release_coeff = c;
    }

    /// Process one value. Selects attack or release based on direction.
    #[inline]
    pub fn tick(&mut self, target: f64) -> f64 {
        let coeff = if target > self.value {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.value += (target - self.value) * coeff;
        self.value
    }

    /// Get the current smoothed value.
    #[inline]
    pub fn value(&self) -> f64 {
        self.value
    }

    /// Set the value directly (for initialization or snap).
    pub fn set_value(&mut self, v: f64) {
        self.value = v;
    }

    pub fn reset(&mut self, initial: f64) {
        self.value = initial;
    }
}

impl Default for RcSmoother {
    fn default() -> Self {
        Self::new(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instant_smoothing() {
        let mut s = RcSmoother::new(0.0);
        s.set_params(0.0, 0.0, 48000.0);
        // Coeff should be 1.0, so instant tracking
        let v = s.tick(1.0);
        assert!((v - 1.0).abs() < 1e-10);
    }

    #[test]
    fn smoothing_converges() {
        let mut s = RcSmoother::new(0.0);
        s.set_params(0.5, 0.5, 48000.0);

        for _ in 0..48000 {
            s.tick(1.0);
        }
        assert!((s.value() - 1.0).abs() < 0.001);
    }

    #[test]
    fn asymmetric_smoothing() {
        // Fast attack, slow release
        let mut s = RcSmoother::new(0.0);
        s.set_params(0.0, 0.5, 48000.0); // instant attack, smooth release

        // Attack should be instant
        s.tick(1.0);
        assert!((s.value() - 1.0).abs() < 1e-10);

        // Release should be gradual
        s.tick(0.0);
        assert!(s.value() > 0.9, "Release should be slow: {}", s.value());
    }

    #[test]
    fn coeff_at_zero_is_one() {
        assert_eq!(RcSmoother::coeff(0.0, 48000.0), 1.0);
    }

    #[test]
    fn coeff_decreases_with_param() {
        let c1 = RcSmoother::coeff(0.1, 48000.0);
        let c2 = RcSmoother::coeff(0.5, 48000.0);
        let c3 = RcSmoother::coeff(1.0, 48000.0);
        assert!(c1 > c2);
        assert!(c2 > c3);
    }
}
