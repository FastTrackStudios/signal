//! The filter blocks the measurements resolved to.
//!
//! Every filter in the reference is the same fifth-order Butterworth built
//! the classic way: two cookbook (RBJ) biquads at the Butterworth Qs
//! (`1/(2cos(π/5))` and `1/(2cos(2π/5))`) and one exponential one-pole,
//! all at the same corner. The one-pole is the `y += (1 − p)(x − y)` kind —
//! no zero at Nyquist — which is what distinguishes the measured response
//! from a bilinear design (−72 dB vs −48 dB fit). The high-pass variant's
//! one-pole is `x − lowpass(x)`.

use core::f64::consts::PI;

/// Butterworth Q of the first fifth-order pole pair.
const Q1: f64 = 0.618_033_988_749_895;
/// Butterworth Q of the second fifth-order pole pair.
const Q2: f64 = 1.618_033_988_749_895;

/// Direct-form-I biquad (cookbook coefficients).
#[derive(Clone, Copy, Debug, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn new(sample_rate: f64, f0: f64, q: f64, high_pass: bool) -> Self {
        let w = 2.0 * PI * f0 / sample_rate;
        let (sin, cos) = (libm::sin(w), libm::cos(w));
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b1) = if high_pass {
            (f64::midpoint(1.0, cos), -(1.0 + cos))
        } else {
            ((1.0 - cos) / 2.0, 1.0 - cos)
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            ..Self::default()
        }
    }

    #[inline]
    fn tick(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    const fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// Exponential one-pole low-pass, optionally used as its `x − lp` high-pass.
#[derive(Clone, Copy, Debug, Default)]
struct OnePole {
    pole: f64,
    state: f64,
    high_pass: bool,
}

impl OnePole {
    fn new(sample_rate: f64, fc: f64, high_pass: bool) -> Self {
        Self {
            pole: libm::exp(-2.0 * PI * fc / sample_rate),
            state: 0.0,
            high_pass,
        }
    }

    #[inline]
    fn tick(&mut self, x: f64) -> f64 {
        let lp = (1.0 - self.pole) * x + self.pole * self.state;
        if self.high_pass {
            // HP(z) = p(1 − z⁻¹)/(1 − p z⁻¹); `state` holds the LP output.
            let hp = x - lp;
            self.state = lp;
            hp
        } else {
            self.state = lp;
            lp
        }
    }

    const fn reset(&mut self) {
        self.state = 0.0;
    }
}

/// Fifth-order Butterworth: biquad(Q1) → biquad(Q2) → one-pole, one corner.
#[derive(Clone, Copy, Debug, Default)]
pub struct Bw5 {
    s1: Biquad,
    s2: Biquad,
    s3: OnePole,
}

impl Bw5 {
    /// A low-pass (`high_pass = false`) or high-pass at `fc`.
    #[must_use]
    pub fn new(sample_rate: f64, fc: f64, high_pass: bool) -> Self {
        Self {
            s1: Biquad::new(sample_rate, fc, Q1, high_pass),
            s2: Biquad::new(sample_rate, fc, Q2, high_pass),
            s3: OnePole::new(sample_rate, fc, high_pass),
        }
    }

    /// Filter one sample.
    #[inline]
    pub fn tick(&mut self, x: f64) -> f64 {
        self.s3.tick(self.s2.tick(self.s1.tick(x)))
    }

    /// Clear the filter state.
    pub const fn reset(&mut self) {
        self.s1.reset();
        self.s2.reset();
        self.s3.reset();
    }
}
