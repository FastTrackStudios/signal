//! Display ballistics (attack/release) and Freeze.
//!
//! Operates on a dB spectrum with **asymmetric one-pole smoothing**: a fast
//! attack so transients still register, and a slower release so the spectrum
//! settles smoothly instead of flickering. This is what makes an analyzer look
//! "nice" rather than jagged — a purely instant attack snaps to every sample
//! and reads as noise. Freeze replaces the ballistics with a running maximum
//! (the spectrum holds and builds its peak).

const INIT_DB: f32 = -240.0;

/// One-pole smoothing coefficient for a given time constant: the per-frame
/// fraction of the gap to close so the value reaches ~63% of a step in `tau_s`
/// seconds at `refresh_rate` Hz. `tau_s <= 0` means instant (coefficient 1).
fn one_pole(refresh_rate: f32, tau_s: f32) -> f32 {
    if tau_s <= 0.0 || refresh_rate <= 0.0 {
        return 1.0;
    }
    let dt = 1.0 / refresh_rate;
    (1.0 - (-dt / tau_s).exp()).clamp(0.0, 1.0)
}

pub struct SpectrumDecayer {
    state: Vec<f32>,
    attack_p: f32,
    release_p: f32,
}

impl SpectrumDecayer {
    pub fn new(num_bins: usize) -> Self {
        Self {
            state: vec![INIT_DB; num_bins],
            attack_p: 1.0,
            release_p: 1.0,
        }
    }

    pub fn resize(&mut self, num_bins: usize) {
        self.state.resize(num_bins, INIT_DB);
    }

    pub fn reset(&mut self) {
        self.state.iter_mut().for_each(|s| *s = INIT_DB);
    }

    /// Configure attack/release time constants (seconds) for the UI refresh
    /// rate. Attack is normally short (fast rise); release is the user's "Speed"
    /// (slow fall = smoother, calmer display).
    pub fn set_ballistics(&mut self, refresh_rate: f32, attack_s: f32, release_s: f32) {
        self.attack_p = one_pole(refresh_rate, attack_s);
        self.release_p = one_pole(refresh_rate, release_s);
    }

    /// Update the display from a fresh dB spectrum in place. When `frozen`, the
    /// running maximum is held instead of moving.
    pub fn decay(&mut self, spectrum_db: &mut [f32], frozen: bool) {
        debug_assert_eq!(spectrum_db.len(), self.state.len());
        if frozen {
            for (s, st) in spectrum_db.iter_mut().zip(self.state.iter_mut()) {
                let v = s.max(*st);
                *s = v;
                *st = v;
            }
        } else {
            let attack_p = self.attack_p;
            let release_p = self.release_p;
            for (s, st) in spectrum_db.iter_mut().zip(self.state.iter_mut()) {
                // Rising → attack coefficient (fast); falling → release (slow).
                let p = if *s > *st { attack_p } else { release_p };
                let v = *st + p * (*s - *st);
                *s = v;
                *st = v;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_rises_quickly_but_smoothly() {
        let mut d = SpectrumDecayer::new(1);
        d.set_ballistics(60.0, 0.05, 0.8);
        // One frame should move toward the target (from the deep init floor)
        // but not snap all the way.
        let mut s = [0.0];
        d.decay(&mut s, false);
        assert!(s[0] > -240.0 && s[0] < 0.0, "first attack frame: {}", s[0]);
        // After enough frames it should converge to the target.
        for _ in 0..40 {
            let mut q = [0.0];
            d.decay(&mut q, false);
            s = q;
        }
        assert!(s[0] > -0.5, "attack did not converge: {}", s[0]);
    }

    #[test]
    fn release_is_slower_than_attack() {
        let mut d = SpectrumDecayer::new(1);
        d.set_ballistics(60.0, 0.05, 0.8);
        // Drive up to 0.
        for _ in 0..40 {
            let mut q = [0.0];
            d.decay(&mut q, false);
        }
        // Now fall toward -100: should descend, monotonically, but gently.
        let mut prev = 0.0;
        for _ in 0..10 {
            let mut q = [-100.0];
            d.decay(&mut q, false);
            assert!(q[0] <= prev + 1e-4, "{} !<= {}", q[0], prev);
            prev = q[0];
        }
        // 10 frames of slow release shouldn't have fallen the whole way.
        assert!(prev > -50.0, "release too fast: {prev}");
    }

    #[test]
    fn freeze_holds_max() {
        let mut d = SpectrumDecayer::new(1);
        let mut s = [-20.0];
        d.decay(&mut s, true);
        let mut q = [-50.0];
        d.decay(&mut q, true);
        assert!((q[0] - -20.0).abs() < 1e-3, "froze at {}", q[0]);
    }
}
