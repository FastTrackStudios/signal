//! Filter primitives for the Wurlitzer 200A signal chain.
//!
//! Self-contained RBJ Audio EQ Cookbook biquad, Direct Form II Transposed.
//!
//! Upstream openwurli backs this with `melange-primitives::Biquad`. This
//! vendored copy reimplements the same cookbook coefficients and DF-II
//! Transposed structure locally so the crate has no external git dependency
//! (monorepo rule: path deps only). Coefficients are the standard RBJ forms;
//! bandpass uses the constant-skirt-gain variant (peak gain = Q), matching the
//! upstream comment.

/// Biquad filter — Direct Form II Transposed, RBJ cookbook coefficients.
pub struct Biquad {
    // Normalized (divided by a0) coefficients.
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    // DF-II Transposed state.
    s1: f64,
    s2: f64,
}

enum Kind {
    Bandpass,
    Lowpass,
    Highpass,
}

impl Biquad {
    fn coeffs(kind: Kind, fc: f64, q: f64, sample_rate: f64) -> (f64, f64, f64, f64, f64) {
        let w0 = std::f64::consts::TAU * fc / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b1, b2) = match kind {
            // Bandpass, constant skirt gain (peak gain = Q).
            Kind::Bandpass => (sin_w0 / 2.0, 0.0, -sin_w0 / 2.0),
            Kind::Lowpass => {
                let c = (1.0 - cos_w0) / 2.0;
                (c, 1.0 - cos_w0, c)
            }
            Kind::Highpass => {
                let c = (1.0 + cos_w0) / 2.0;
                (c, -(1.0 + cos_w0), c)
            }
        };
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;
        (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0)
    }

    fn from_coeffs(c: (f64, f64, f64, f64, f64)) -> Self {
        Self {
            b0: c.0,
            b1: c.1,
            b2: c.2,
            a1: c.3,
            a2: c.4,
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// Bandpass filter (constant skirt gain, Audio EQ Cookbook).
    pub fn bandpass(center_hz: f64, q: f64, sample_rate: f64) -> Self {
        Self::from_coeffs(Self::coeffs(Kind::Bandpass, center_hz, q, sample_rate))
    }

    /// Low-pass filter (Audio EQ Cookbook).
    pub fn lowpass(cutoff_hz: f64, q: f64, sample_rate: f64) -> Self {
        Self::from_coeffs(Self::coeffs(Kind::Lowpass, cutoff_hz, q, sample_rate))
    }

    /// High-pass filter (Audio EQ Cookbook).
    pub fn highpass(cutoff_hz: f64, q: f64, sample_rate: f64) -> Self {
        Self::from_coeffs(Self::coeffs(Kind::Highpass, cutoff_hz, q, sample_rate))
    }

    fn set_coeffs(&mut self, c: (f64, f64, f64, f64, f64)) {
        self.b0 = c.0;
        self.b1 = c.1;
        self.b2 = c.2;
        self.a1 = c.3;
        self.a2 = c.4;
    }

    /// Update coefficients to highpass without resetting filter state.
    pub fn set_highpass(&mut self, cutoff_hz: f64, q: f64, sample_rate: f64) {
        self.set_coeffs(Self::coeffs(Kind::Highpass, cutoff_hz, q, sample_rate));
    }

    /// Update coefficients to lowpass without resetting filter state.
    pub fn set_lowpass(&mut self, cutoff_hz: f64, q: f64, sample_rate: f64) {
        self.set_coeffs(Self::coeffs(Kind::Lowpass, cutoff_hz, q, sample_rate));
    }

    /// Process one sample (Direct Form II Transposed).
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y
    }

    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }
}
