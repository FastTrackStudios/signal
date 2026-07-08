//! Gain reduction smoothing with attack/release time constants.

use audiocore_dsp::envelope::EnvelopeFollower;

/// Exponential smoother for gain reduction.
///
/// State lives in shared [`EnvelopeFollower`]s (one per channel). The
/// coefficient formula is comp-specific — `exp(-2 / (sr * t))` applied to
/// the *input* term rather than the state term — so coefficients are
/// mapped as `1 - coeff` when handed to the follower, which computes
/// `coeff * (state - input) + input`. Numerics are identical to the
/// original hand-rolled implementation.
pub struct GainReductionSmoother {
    sample_rate: f64,
    attack_s: f64,
    release_s: f64,
    /// Per-channel smoothing state.
    env: [EnvelopeFollower; 2],
}

impl GainReductionSmoother {
    pub fn new(sample_rate: f64) -> Self {
        let mut s = Self {
            sample_rate,
            attack_s: 0.01,
            release_s: 0.05,
            env: [EnvelopeFollower::new(1.0), EnvelopeFollower::new(1.0)],
        };
        s.update_coeffs();
        s
    }

    pub fn set_attack(&mut self, attack_s: f64) {
        self.attack_s = attack_s.max(0.0001);
        self.update_coeffs();
    }

    pub fn set_release(&mut self, release_s: f64) {
        self.release_s = release_s.max(0.001);
        self.update_coeffs();
    }

    fn update_coeffs(&mut self) {
        // EnvelopeFollower weights its *state* by coeff; the original
        // formula weighted the *input* by exp(-2/(sr*t)) — hence 1 - c.
        let attack = 1.0 - self.compute_coeff(self.attack_s);
        let release = 1.0 - self.compute_coeff(self.release_s);
        for e in &mut self.env {
            e.set_coeffs(attack, release);
        }
    }

    /// Smooth GR using exponential smoothing with attack/release coefficients.
    ///
    /// Attack when GR rises (`gr_inst >= state`), release when it falls —
    /// the follower's `input > value` test matches on the rising side, and
    /// at exact equality both branches produce the same output.
    pub fn smooth_gr(&mut self, gr_inst: f64, ch: usize) -> f64 {
        self.env[ch].tick(gr_inst)
    }

    #[inline]
    fn compute_coeff(&self, time_s: f64) -> f64 {
        const SCALE: f64 = 2.0;
        (-SCALE / (self.sample_rate * time_s)).exp()
    }

    pub fn reset(&mut self) {
        for e in &mut self.env {
            e.reset(1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The original hand-rolled formula, kept as a reference oracle.
    fn reference_smooth(state: f64, gr_inst: f64, sr: f64, attack_s: f64, release_s: f64) -> f64 {
        let time_s = if gr_inst < state { release_s } else { attack_s };
        let coeff = (-2.0 / (sr * time_s)).exp();
        coeff * gr_inst + (1.0 - coeff) * state
    }

    #[test]
    fn matches_reference_implementation() {
        let sr = 48000.0;
        let mut s = GainReductionSmoother::new(sr);
        s.set_attack(0.01);
        s.set_release(0.05);

        let mut ref_state = 1.0;
        for i in 0..2000 {
            // Alternate compress / release phases.
            let gr = if (i / 500) % 2 == 0 { 0.5 } else { 1.0 };
            let got = s.smooth_gr(gr, 0);
            ref_state = reference_smooth(ref_state, gr, sr, 0.01, 0.05);
            assert!(
                (got - ref_state).abs() < 1e-12,
                "diverged at {i}: {got} vs {ref_state}"
            );
        }
    }

    #[test]
    fn channels_independent() {
        let mut s = GainReductionSmoother::new(48000.0);
        s.smooth_gr(0.2, 0);
        let r = s.smooth_gr(1.0, 1);
        assert!((r - 1.0).abs() < 1e-9, "ch1 should be untouched: {r}");
    }
}
