//! Pre/post collision detection.
//!
//! Where the pre- and post-EQ spectra are both loud, the EQ is fighting energy
//! that's present in both — a "collision". For each bin we take the quieter of
//! the two relative to an adaptive threshold, normalize it, and smooth the
//! resulting strength over time into `final_ps` (0 = no collision, up to 0.9 =
//! strong). The UI paints these as a red overlay. Port of ZLEqualizer's
//! `SpectrumCollision`.

const DECAY: f32 = 0.95;

pub struct SpectrumCollision {
    /// Smoothed per-bin collision strength (0..0.9), read by the UI.
    final_ps: Vec<f32>,
    /// Scratch per-bin strengths.
    ps: Vec<f32>,
}

impl SpectrumCollision {
    pub fn new(num_bins: usize) -> Self {
        Self {
            final_ps: vec![0.0; num_bins],
            ps: vec![0.0; num_bins],
        }
    }

    pub fn resize(&mut self, num_bins: usize) {
        self.final_ps.resize(num_bins, 0.0);
        self.ps.resize(num_bins, 0.0);
    }

    /// Current smoothed collision strengths.
    pub fn strengths(&self) -> &[f32] {
        &self.final_ps
    }

    /// Update collision strengths from the two dB spectra. `strength` controls
    /// sensitivity (Pro-Q's collision amount); 0.1 is a reasonable default.
    pub fn update(&mut self, db0: &[f32], db1: &[f32], strength: f32) {
        debug_assert_eq!(db0.len(), db1.len());
        debug_assert_eq!(db0.len(), self.final_ps.len());

        let avg0 = softmax_avg(db0, 0.1);
        let avg1 = softmax_avg(db1, 0.1);

        // Too quiet / invalid → just fade out any existing collisions.
        if !avg0.is_finite() || !avg1.is_finite() || avg0 < -120.0 || avg1 < -120.0 {
            self.final_ps.iter_mut().for_each(|p| *p *= DECAY);
            return;
        }

        let db_avg = 2.0 * softmax_avg(&[avg0, avg1], 0.1);
        let threshold = (strength * db_avg).min(0.0);
        let scale = 1.0 / (0.1 - threshold);

        let mut sum = 0.0f32;
        for (i, p) in self.ps.iter_mut().enumerate() {
            *p = (db0[i].min(db1[i]) - threshold) * scale;
            sum += *p;
        }

        let mean_p = sum / self.ps.len() as f32;
        let p_mult = if mean_p > strength * strength {
            0.1 / mean_p
        } else {
            1.0
        };

        for (p, fp) in self.ps.iter().zip(self.final_ps.iter_mut()) {
            let v = (p * p_mult).clamp(0.1, 1.0) - 0.1;
            *fp = (*fp * DECAY).max(v);
        }
    }
}

/// Softmax-weighted average: `sum(x * e^{kx}) / sum(e^{kx})`. Returns NaN when
/// the weights underflow (effectively empty/silent input).
fn softmax_avg(data: &[f32], k: f32) -> f32 {
    let mut sum = 0.0f32;
    let mut weight_sum = 0.0f32;
    for &x in data {
        let w = (x * k).exp();
        sum += w * x;
        weight_sum += w;
    }
    if weight_sum < 1e-10 {
        f32::NAN
    } else {
        sum / weight_sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_produces_no_collision() {
        let mut c = SpectrumCollision::new(16);
        let quiet = vec![-200.0f32; 16];
        c.update(&quiet, &quiet, 0.1);
        assert!(c.strengths().iter().all(|&p| p == 0.0));
    }

    #[test]
    fn shared_peak_produces_collision() {
        // Collisions highlight regions that stand out above the average in BOTH
        // spectra — a flat spectrum yields nothing, but a shared peak does.
        let mut c = SpectrumCollision::new(32);
        let mut spec = vec![-60.0f32; 32];
        for s in &mut spec[12..16] {
            *s = 0.0; // a loud bump present in both pre and post
        }
        for _ in 0..10 {
            c.update(&spec, &spec, 0.1);
        }
        let str = c.strengths();
        // The collision should appear in the bump region.
        assert!(
            str[12..16].iter().any(|&p| p > 0.0),
            "no collision in peak: {str:?}"
        );
    }
}
