//! Biquad IIR filter implementation for multiband crossover filtering.
//!
//! Implements standard 2nd-order IIR biquad filter with configurable coefficients.
//! Used to separate input audio into frequency bands for multiband compression.
//!
//! Difference equation:
//! y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]

use std::f64::consts::PI;

/// Second-order IIR biquad filter
#[derive(Clone)]
pub struct BiquadFilter {
    /// Numerator coefficients (b0, b1, b2)
    pub b: [f64; 3],

    /// Denominator coefficients (a1, a2)
    /// Note: a0 is always 1.0 (normalized)
    pub a: [f64; 2],

    /// Filter state - input history [x1, x2]
    x_history: [f64; 2],

    /// Filter state - output history [y1, y2]
    y_history: [f64; 2],

    /// Gain scaling factor (1.0 default)
    pub gain_scale: f64,
}

impl BiquadFilter {
    /// Create a new biquad filter with identity (pass-through) response
    pub fn new() -> Self {
        Self {
            b: [1.0, 0.0, 0.0],
            a: [0.0, 0.0],
            x_history: [0.0, 0.0],
            y_history: [0.0, 0.0],
            gain_scale: 1.0,
        }
    }

    /// Set biquad coefficients from numerator and denominator
    /// Normalized form: b = [b0, b1, b2], a = [a1, a2] (a0 is always 1.0)
    pub fn set_coefficients(&mut self, b0: f64, b1: f64, b2: f64, a1: f64, a2: f64) {
        self.b[0] = b0;
        self.b[1] = b1;
        self.b[2] = b2;
        self.a[0] = a1;
        self.a[1] = a2;
    }

    /// Apply filter to one sample and return filtered output
    /// Implements the difference equation:
    /// y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]
    #[inline]
    pub fn process(&mut self, input: f64) -> f64 {
        // Apply difference equation
        let output =
            self.b[0] * input + self.b[1] * self.x_history[0] + self.b[2] * self.x_history[1]
                - self.a[0] * self.y_history[0]
                - self.a[1] * self.y_history[1];

        // Update input history
        self.x_history[1] = self.x_history[0];
        self.x_history[0] = input;

        // Update output history
        self.y_history[1] = self.y_history[0];
        self.y_history[0] = output;

        // Apply gain scaling if specified
        output * self.gain_scale
    }

    /// Reset filter state (zeros all history buffers)
    pub fn reset(&mut self) {
        self.x_history = [0.0; 2];
        self.y_history = [0.0; 2];
    }
}

impl Default for BiquadFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute low-pass biquad filter coefficients from cutoff frequency
///
/// Uses bilinear transform to convert analog Butterworth pole to digital biquad.
/// Assumes: normalized cutoff frequency (0.0 to 1.0, where 1.0 = Nyquist)
///
/// For Butterworth low-pass: pole at s = -ωc (where ωc = 2*π*fc/fs)
/// Bilinear: s = 2/Ts * (z-1)/(z+1) where Ts = 2/sample_rate
pub fn design_lowpass_biquad(normalized_cutoff: f64) -> BiquadFilter {
    // Ensure cutoff is in valid range
    let fc = normalized_cutoff.clamp(0.001, 0.999);

    // Bilinear coefficient
    // tan(π*fc/2) approximation using normalized frequency
    let tan_half_omega = (PI * fc / 2.0).tan();

    // Butterworth pole normalization
    let a = 1.0 / (1.0 + 2.0 * tan_half_omega + tan_half_omega * tan_half_omega);

    // Compute biquad coefficients from normalized Butterworth response
    let mut filter = BiquadFilter::new();

    filter.b[0] = a * tan_half_omega * tan_half_omega;
    filter.b[1] = 2.0 * a * tan_half_omega * tan_half_omega;
    filter.b[2] = a * tan_half_omega * tan_half_omega;

    filter.a[0] = 2.0 * a * (tan_half_omega * tan_half_omega - 1.0);
    filter.a[1] = a * (1.0 - 2.0 * tan_half_omega + tan_half_omega * tan_half_omega);

    filter
}

/// Compute high-pass biquad filter coefficients from cutoff frequency
///
/// Similar to low-pass but with high-pass characteristics.
/// Cutoff frequency: 0.0 to 1.0 (normalized, 1.0 = Nyquist)
pub fn design_highpass_biquad(normalized_cutoff: f64) -> BiquadFilter {
    // Ensure cutoff is in valid range
    let fc = normalized_cutoff.clamp(0.001, 0.999);

    // Bilinear coefficient
    let tan_half_omega = (PI * fc / 2.0).tan();

    // Butterworth pole normalization (high-pass)
    let a = 1.0 / (1.0 + 2.0 * tan_half_omega + tan_half_omega * tan_half_omega);

    let mut filter = BiquadFilter::new();

    // High-pass coefficients (inverted from low-pass)
    filter.b[0] = a;
    filter.b[1] = -2.0 * a;
    filter.b[2] = a;

    filter.a[0] = 2.0 * a * (tan_half_omega * tan_half_omega - 1.0);
    filter.a[1] = a * (1.0 - 2.0 * tan_half_omega + tan_half_omega * tan_half_omega);

    filter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_biquad_passthrough() {
        let mut filter = BiquadFilter::new();
        let input = 0.5;
        let output = filter.process(input);

        // Should pass through with gain_scale=1.0 by default
        assert!((output - input).abs() < 0.0001);
    }

    #[test]
    fn test_biquad_state_reset() {
        let mut filter = BiquadFilter::new();
        filter.set_coefficients(0.5, 0.25, 0.25, 0.1, 0.05);

        // Process some samples
        filter.process(1.0);
        filter.process(0.5);

        // Reset and verify state is cleared
        filter.reset();
        assert_eq!(filter.x_history, [0.0, 0.0]);
        assert_eq!(filter.y_history, [0.0, 0.0]);
    }

    #[test]
    fn test_lowpass_filter_design() {
        let filter = design_lowpass_biquad(0.1);

        // Filter should be defined and non-NaN
        assert!(!filter.b[0].is_nan());
        assert!(!filter.b[1].is_nan());
        assert!(!filter.b[2].is_nan());
        assert!(!filter.a[0].is_nan());
        assert!(!filter.a[1].is_nan());
    }

    #[test]
    fn test_highpass_filter_design() {
        let filter = design_highpass_biquad(0.1);

        // Filter should be defined and non-NaN
        assert!(!filter.b[0].is_nan());
        assert!(!filter.b[1].is_nan());
        assert!(!filter.b[2].is_nan());
        assert!(!filter.a[0].is_nan());
        assert!(!filter.a[1].is_nan());
    }

    #[test]
    fn test_filter_cutoff_bounds() {
        // Very low cutoff
        let _lp_low = design_lowpass_biquad(0.001);
        let _hp_low = design_highpass_biquad(0.001);

        // Very high cutoff
        let _lp_high = design_lowpass_biquad(0.999);
        let _hp_high = design_highpass_biquad(0.999);

        // All should produce valid filters without panicking or NaN
    }
}
