//! Minimal RBJ biquad, self-contained so the vocal tools don't couple to any
//! particular EQ crate. Transposed Direct-Form II, `f64` state.

/// Second-order IIR section.
#[derive(Clone, Copy, Debug, Default)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// Identity (pass-through) section.
    pub fn new() -> Self {
        Self {
            b0: 1.0,
            ..Self::default()
        }
    }

    fn set(&mut self, b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) {
        let inv = 1.0 / a0;
        self.b0 = b0 * inv;
        self.b1 = b1 * inv;
        self.b2 = b2 * inv;
        self.a1 = a1 * inv;
        self.a2 = a2 * inv;
    }

    /// Design an RBJ high-pass at `freq` Hz with quality `q`.
    pub fn highpass(sample_rate: f64, freq: f64, q: f64) -> Self {
        let mut f = Self::new();
        let w0 = core::f64::consts::TAU * (freq / sample_rate).clamp(1e-5, 0.49);
        let (sn, cs) = w0.sin_cos();
        let alpha = sn / (2.0 * q.max(1e-3));
        let b0 = (1.0 + cs) / 2.0;
        let b1 = -(1.0 + cs);
        let b2 = (1.0 + cs) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cs;
        let a2 = 1.0 - alpha;
        f.set(b0, b1, b2, a0, a1, a2);
        f
    }

    /// Design an RBJ low-pass at `freq` Hz with quality `q`.
    pub fn lowpass(sample_rate: f64, freq: f64, q: f64) -> Self {
        let mut f = Self::new();
        let w0 = core::f64::consts::TAU * (freq / sample_rate).clamp(1e-5, 0.49);
        let (sn, cs) = w0.sin_cos();
        let alpha = sn / (2.0 * q.max(1e-3));
        let b0 = (1.0 - cs) / 2.0;
        let b1 = 1.0 - cs;
        let b2 = (1.0 - cs) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cs;
        let a2 = 1.0 - alpha;
        f.set(b0, b1, b2, a0, a1, a2);
        f
    }

    /// Process one sample.
    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    /// Clear the filter state.
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}
