//! Tilt EQ — single control rotates spectrum around a pivot frequency.
//!
//! Implemented as complementary RBJ low/high shelves (Q = 0.707) from the
//! shared `audiocore_dsp::Biquad`: negative tilt darkens (boost lows, cut
//! highs), positive brightens. Each shelf carries half the tilt so the
//! response pivots symmetrically around `pivot_hz`.

use audiocore_dsp::biquad::{Biquad, FilterType};

const SHELF_Q: f64 = 0.707;

pub struct TiltEq {
    low_shelf: Biquad,
    high_shelf: Biquad,
    sample_rate: f64,
    pivot_hz: f64,
    tilt_db: f64,
}

impl TiltEq {
    pub fn new(sample_rate: f64) -> Self {
        let mut t = Self {
            low_shelf: Biquad::new(),
            high_shelf: Biquad::new(),
            sample_rate,
            pivot_hz: 700.0,
            tilt_db: 0.0,
        };
        t.update();
        t
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sample_rate = sr;
        self.update();
    }

    pub fn set_pivot(&mut self, hz: f64) {
        self.pivot_hz = hz.clamp(20.0, 20000.0);
        self.update();
    }

    /// Negative = darker, positive = brighter. ±12 dB is the practical range.
    pub fn set_tilt_db(&mut self, db: f64) {
        self.tilt_db = db.clamp(-24.0, 24.0);
        self.update();
    }

    fn update(&mut self) {
        // Each shelf contributes half the tilt at its band edge.
        let low_db = -self.tilt_db * 0.5;
        let high_db = self.tilt_db * 0.5;

        self.low_shelf.set(
            FilterType::LowShelf { gain_db: low_db },
            self.pivot_hz,
            SHELF_Q,
            self.sample_rate,
        );
        self.high_shelf.set(
            FilterType::HighShelf { gain_db: high_db },
            self.pivot_hz,
            SHELF_Q,
            self.sample_rate,
        );
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        // Each TiltEq instance is mono — use channel 0 of the stereo biquads.
        self.high_shelf.tick(self.low_shelf.tick(input, 0), 0)
    }

    pub fn reset(&mut self) {
        self.low_shelf.reset();
        self.high_shelf.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn tone_energy(tilt: f64, freq: f64) -> f64 {
        let mut eq = TiltEq::new(SR);
        eq.set_tilt_db(tilt);
        let n = 9600;
        let mut sum = 0.0;
        for i in 0..n {
            let x = (2.0 * PI * freq * i as f64 / SR).sin();
            let y = eq.tick(x);
            if i > n / 2 {
                sum += y * y;
            }
        }
        sum
    }

    #[test]
    fn positive_tilt_brightens() {
        // +12 dB tilt: highs louder than flat, lows quieter than flat.
        let hi_tilted = tone_energy(12.0, 8000.0);
        let hi_flat = tone_energy(0.0, 8000.0);
        assert!(
            hi_tilted > hi_flat * 1.5,
            "highs should be boosted: {hi_tilted} vs {hi_flat}"
        );

        let lo_tilted = tone_energy(12.0, 100.0);
        let lo_flat = tone_energy(0.0, 100.0);
        assert!(
            lo_tilted < lo_flat * 0.7,
            "lows should be cut: {lo_tilted} vs {lo_flat}"
        );
    }

    #[test]
    fn zero_tilt_is_transparent() {
        let mut eq = TiltEq::new(SR);
        eq.set_tilt_db(0.0);
        for i in 0..4800 {
            let x = (2.0 * PI * 1000.0 * i as f64 / SR).sin() * 0.5;
            let y = eq.tick(x);
            assert!(y.is_finite());
        }
        // tone_energy sums x^2 over the last 4799 samples of a unit sine —
        // expected energy for unity gain is count * 0.5.
        let e = tone_energy(0.0, 1000.0);
        let expected = 4799.0 * 0.5;
        assert!(
            (e / expected - 1.0).abs() < 0.05,
            "flat tilt should be ~unity: {}",
            e / expected
        );
    }

    #[test]
    fn pivot_region_stays_put() {
        // Energy at the pivot should barely move with tilt.
        let at_pivot_tilted = tone_energy(12.0, 700.0);
        let at_pivot_flat = tone_energy(0.0, 700.0);
        let ratio = at_pivot_tilted / at_pivot_flat;
        assert!(
            (0.5..2.0).contains(&ratio),
            "pivot should be roughly unaffected: ratio {ratio}"
        );
    }
}
