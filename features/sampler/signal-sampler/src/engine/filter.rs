//! Biquad filter — used for the Con Sordino bus-level EQ.
//!
//! The default coefficients are a placeholder: a lowpass at 2.5 kHz with
//! Q = 0.9, which gives a rough approximation of the veiled, high-frequency-
//! reduced character of muted strings.
//!
//! # Future work
//!
//! Replace the placeholder with a measured multi-band EQ curve derived by
//! comparing CSS with/without Con Sordino across the full note range:
//!   1. Record the same passage with Con Sordino off → reference IR.
//!   2. Record with Con Sordino on → muted IR.
//!   3. Compute the ratio in the frequency domain.
//!   4. Fit a minimum-phase biquad cascade to the difference curve.
//!   5. Drop the result here.

/// Placeholder Con Sordino lowpass cutoff (Hz).
pub const SORD_FC: f32 = 2500.0;
/// Placeholder Q factor for the lowpass.
pub const SORD_Q: f32 = 0.9;

/// Stereo biquad filter — Direct Form I.
///
/// Coefficients are pre-normalised by `a0` so the inner loop is coefficient-
/// multiply-only (no division per sample).
pub struct BiquadFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    // Delay line — L channel
    x1l: f32,
    x2l: f32,
    y1l: f32,
    y2l: f32,
    // Delay line — R channel
    x1r: f32,
    x2r: f32,
    y1r: f32,
    y2r: f32,
}

impl BiquadFilter {
    /// Build a second-order lowpass at `fc` Hz with quality factor `q`.
    pub fn lowpass(fc: f32, q: f32, sample_rate: u32) -> Self {
        let omega = 2.0 * std::f32::consts::PI * fc / sample_rate as f32;
        let sin_w = omega.sin();
        let cos_w = omega.cos();
        let alpha = sin_w / (2.0 * q);

        let a0 = 1.0 + alpha;
        let b0 = (1.0 - cos_w) * 0.5 / a0;
        let b1 = (1.0 - cos_w) / a0;
        let b2 = (1.0 - cos_w) * 0.5 / a0;
        let a1 = -2.0 * cos_w / a0;
        let a2 = (1.0 - alpha) / a0;

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            x1l: 0.0,
            x2l: 0.0,
            y1l: 0.0,
            y2l: 0.0,
            x1r: 0.0,
            x2r: 0.0,
            y1r: 0.0,
            y2r: 0.0,
        }
    }

    /// Process an interleaved stereo buffer in-place.
    pub fn process(&mut self, buf: &mut [f32]) {
        let mut i = 0;
        while i + 1 < buf.len() {
            buf[i] = self.tick_l(buf[i]);
            buf[i + 1] = self.tick_r(buf[i + 1]);
            i += 2;
        }
    }

    /// Clear filter memory. Call when bypassing to avoid clicks on re-engage.
    pub fn reset(&mut self) {
        self.x1l = 0.0;
        self.x2l = 0.0;
        self.y1l = 0.0;
        self.y2l = 0.0;
        self.x1r = 0.0;
        self.x2r = 0.0;
        self.y1r = 0.0;
        self.y2r = 0.0;
    }

    #[inline]
    fn tick_l(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1l + self.b2 * self.x2l
            - self.a1 * self.y1l
            - self.a2 * self.y2l;
        self.x2l = self.x1l;
        self.x1l = x;
        self.y2l = self.y1l;
        self.y1l = y;
        y
    }

    #[inline]
    fn tick_r(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1r + self.b2 * self.x2r
            - self.a1 * self.y1r
            - self.a2 * self.y2r;
        self.x2r = self.x1r;
        self.x1r = x;
        self.y2r = self.y1r;
        self.y1r = y;
        y
    }
}
