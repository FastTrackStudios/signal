//! Tilt EQ — single control rotates spectrum around a pivot frequency.
//!
//! Implemented as complementary low/high shelves driven by `tilt_db`:
//! negative values darken (boost lows, cut highs), positive brighten.
//!
//! Reference: cytomic.com "Technical Papers — Filter cookbook" (Andy
//! Simper). The shelving math is the standard RBJ audio cookbook form
//! adapted for symmetric tilt.

use std::f64::consts::PI;

pub struct TiltEq {
    // Low-shelf biquad
    l_b0: f64, l_b1: f64, l_b2: f64, l_a1: f64, l_a2: f64,
    l_x1: f64, l_x2: f64, l_y1: f64, l_y2: f64,
    // High-shelf biquad
    h_b0: f64, h_b1: f64, h_b2: f64, h_a1: f64, h_a2: f64,
    h_x1: f64, h_x2: f64, h_y1: f64, h_y2: f64,

    sample_rate: f64,
    pivot_hz: f64,
    tilt_db: f64,
}

impl TiltEq {
    pub fn new(sample_rate: f64) -> Self {
        let mut t = Self {
            l_b0: 1.0, l_b1: 0.0, l_b2: 0.0, l_a1: 0.0, l_a2: 0.0,
            l_x1: 0.0, l_x2: 0.0, l_y1: 0.0, l_y2: 0.0,
            h_b0: 1.0, h_b1: 0.0, h_b2: 0.0, h_a1: 0.0, h_a2: 0.0,
            h_x1: 0.0, h_x2: 0.0, h_y1: 0.0, h_y2: 0.0,
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

    fn shelf_low(&self, gain_db: f64) -> (f64, f64, f64, f64, f64) {
        // RBJ low-shelf, S=1.
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * self.pivot_hz / self.sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 * 0.5 * ((a + 1.0 / a) * (1.0 / 1.0 - 1.0) + 2.0).sqrt();
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha;

        (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0)
    }

    fn shelf_high(&self, gain_db: f64) -> (f64, f64, f64, f64, f64) {
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * self.pivot_hz / self.sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 * 0.5 * ((a + 1.0 / a) * (1.0 / 1.0 - 1.0) + 2.0).sqrt();
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha;

        (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0)
    }

    fn update(&mut self) {
        // Tilt: low shelf gets +tilt/2, high shelf gets -tilt/2 (negative=dark).
        // Halved because each shelf contributes half the tilt at its band edge.
        let low_db = -self.tilt_db * 0.5;
        let high_db = self.tilt_db * 0.5;

        let (b0, b1, b2, a1, a2) = self.shelf_low(low_db);
        self.l_b0 = b0; self.l_b1 = b1; self.l_b2 = b2;
        self.l_a1 = a1; self.l_a2 = a2;

        let (b0, b1, b2, a1, a2) = self.shelf_high(high_db);
        self.h_b0 = b0; self.h_b1 = b1; self.h_b2 = b2;
        self.h_a1 = a1; self.h_a2 = a2;
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        let y1 = self.l_b0 * input + self.l_b1 * self.l_x1 + self.l_b2 * self.l_x2
            - self.l_a1 * self.l_y1 - self.l_a2 * self.l_y2;
        self.l_x2 = self.l_x1; self.l_x1 = input;
        self.l_y2 = self.l_y1; self.l_y1 = y1;

        let y2 = self.h_b0 * y1 + self.h_b1 * self.h_x1 + self.h_b2 * self.h_x2
            - self.h_a1 * self.h_y1 - self.h_a2 * self.h_y2;
        self.h_x2 = self.h_x1; self.h_x1 = y1;
        self.h_y2 = self.h_y1; self.h_y1 = y2;

        y2
    }

    pub fn reset(&mut self) {
        self.l_x1 = 0.0; self.l_x2 = 0.0; self.l_y1 = 0.0; self.l_y2 = 0.0;
        self.h_x1 = 0.0; self.h_x2 = 0.0; self.h_y1 = 0.0; self.h_y2 = 0.0;
    }
}
