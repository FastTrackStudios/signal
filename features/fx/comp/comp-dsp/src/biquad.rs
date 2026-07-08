//! Butterworth crossover/sidechain filter designers.
//!
//! The filters themselves are `audiocore_dsp::biquad::Biquad` (TDF2,
//! denormal-flushed); this module only keeps the comp-specific 2nd-order
//! Butterworth bilinear-transform designers used for multiband crossover
//! and sidechain EQ.
//!
//! Difference equation realized by the shared biquad:
//! y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]

use std::f64::consts::PI;

pub use audiocore_dsp::biquad::Biquad;

/// Compute low-pass biquad filter coefficients from cutoff frequency.
///
/// Uses bilinear transform to convert analog Butterworth pole to digital
/// biquad. `normalized_cutoff` is 0.0..1.0 where 1.0 = Nyquist.
pub fn design_lowpass_biquad(normalized_cutoff: f64) -> Biquad {
    let fc = normalized_cutoff.clamp(0.001, 0.999);
    let tan_half_omega = (PI * fc / 2.0).tan();
    let a = 1.0 / (1.0 + 2.0 * tan_half_omega + tan_half_omega * tan_half_omega);

    let mut filter = Biquad::new();
    filter.b0 = a * tan_half_omega * tan_half_omega;
    filter.b1 = 2.0 * a * tan_half_omega * tan_half_omega;
    filter.b2 = a * tan_half_omega * tan_half_omega;
    filter.a1 = 2.0 * a * (tan_half_omega * tan_half_omega - 1.0);
    filter.a2 = a * (1.0 - 2.0 * tan_half_omega + tan_half_omega * tan_half_omega);
    filter
}

/// Compute high-pass biquad filter coefficients from cutoff frequency.
///
/// Butterworth high-pass counterpart of [`design_lowpass_biquad`].
pub fn design_highpass_biquad(normalized_cutoff: f64) -> Biquad {
    let fc = normalized_cutoff.clamp(0.001, 0.999);
    let tan_half_omega = (PI * fc / 2.0).tan();
    let a = 1.0 / (1.0 + 2.0 * tan_half_omega + tan_half_omega * tan_half_omega);

    let mut filter = Biquad::new();
    filter.b0 = a;
    filter.b1 = -2.0 * a;
    filter.b2 = a;
    filter.a1 = 2.0 * a * (tan_half_omega * tan_half_omega - 1.0);
    filter.a2 = a * (1.0 - 2.0 * tan_half_omega + tan_half_omega * tan_half_omega);
    filter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_biquad_passthrough() {
        let mut filter = Biquad::new();
        let input = 0.5;
        let output = filter.tick(input, 0);
        assert!((output - input).abs() < 0.0001);
    }

    #[test]
    fn test_lowpass_filter_design() {
        let filter = design_lowpass_biquad(0.1);
        assert!(!filter.b0.is_nan());
        assert!(!filter.b1.is_nan());
        assert!(!filter.b2.is_nan());
        assert!(!filter.a1.is_nan());
        assert!(!filter.a2.is_nan());
    }

    #[test]
    fn test_highpass_filter_design() {
        let filter = design_highpass_biquad(0.1);
        assert!(!filter.b0.is_nan());
        assert!(!filter.b1.is_nan());
        assert!(!filter.b2.is_nan());
        assert!(!filter.a1.is_nan());
        assert!(!filter.a2.is_nan());
    }

    #[test]
    fn test_filter_cutoff_bounds() {
        let _lp_low = design_lowpass_biquad(0.001);
        let _hp_low = design_highpass_biquad(0.001);
        let _lp_high = design_lowpass_biquad(0.999);
        let _hp_high = design_highpass_biquad(0.999);
    }

    #[test]
    fn sidechain_hp_magnitude_response() {
        // HP at 100 Hz (48 kHz): DC blocked, 1 kHz passes.
        let sr = 48000.0;
        let norm = 100.0 / (sr * 0.5);
        let mut hp = design_highpass_biquad(norm);

        let mut dc_out = 0.0;
        for _ in 0..48000 {
            dc_out = hp.tick(1.0, 0);
        }
        assert!(dc_out.abs() < 1e-3, "DC should be blocked: {dc_out}");

        let mut hp = design_highpass_biquad(norm);
        let mut peak: f64 = 0.0;
        for n in 0..48000 {
            let x = (2.0 * PI * 1000.0 * n as f64 / sr).sin();
            let y = hp.tick(x, 0);
            if n > 4800 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak > 0.9, "1 kHz should pass: {peak}");
    }

    #[test]
    fn crossover_lp_magnitude_response() {
        // LP at 200 Hz: DC passes, 10 kHz strongly attenuated.
        let sr = 48000.0;
        let norm = 200.0 / (sr * 0.5);
        let mut lp = design_lowpass_biquad(norm);

        let mut dc_out = 0.0;
        for _ in 0..48000 {
            dc_out = lp.tick(1.0, 0);
        }
        assert!((dc_out - 1.0).abs() < 1e-2, "DC should pass: {dc_out}");

        let mut lp = design_lowpass_biquad(norm);
        let mut peak: f64 = 0.0;
        for n in 0..48000 {
            let x = (2.0 * PI * 10000.0 * n as f64 / sr).sin();
            let y = lp.tick(x, 0);
            if n > 4800 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak < 0.01, "10 kHz should be attenuated: {peak}");
    }
}
