//! Pro-Q 4 Peak/Bell MZT biquad — analog prototype matched at DC + Nyquist + corner.

use std::f64::consts::PI;

use crate::biquad::Coeffs;
use crate::constants::LN10_OVER_20;

use super::mzt_quadratic;

/// Peak/Bell biquad — Pro-Q 4's analog peak EQ with matched Z-transform (MZT).
///
/// Analog prototype:
///   H(s) = (s² + (A/Q_bw)·w0·s + w0²) / (s² + (1/(A·Q_bw))·w0·s + w0²)
/// with A = 10^(gain_dB/40), Q_bw = Q/√2.
///
/// Both poles and zeros map via z = e^{sT} (matched Z, not BLT). After mapping,
/// the numerator is rescaled to enforce |H(z=1)| = 1 (DC unity gain).
///
/// Identified via 64-point ground-truth biquad sweep through live Pro-Q 4
/// (probe.exe LSQ extraction). MZT yields ~7× lower fit error than standard
/// BLT (median 2.4e-3 vs 1.8e-2 across the test grid). See
/// docs/reports/proq4/re/bell_analog_model_identified.md.
pub fn design_peak(freq_hz: f64, q: f64, gain_db: f64, sample_rate: f64) -> Coeffs {
    if gain_db.abs() < 1e-9 {
        return [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    }
    // A = 10^(gain_dB/40)
    let a = (gain_db * LN10_OVER_20 * 0.5).exp();
    let q_bw = q.max(1e-6) * std::f64::consts::FRAC_1_SQRT_2;
    let w0 = 2.0 * PI * freq_hz / sample_rate;

    // Denominator pole: damping = 1/(A·Q_bw), unit-magnitude analog frequency
    let (a1, a2) = mzt_quadratic(w0, 1.0 / (a * q_bw));
    // Numerator zero: damping = A/Q_bw
    let (b1_raw, b2_raw) = mzt_quadratic(w0, a / q_bw);
    let b0_raw = 1.0;

    // Renormalize numerator so |H(z=1)| = 1 (DC unity gain).
    let dc_num = b0_raw + b1_raw + b2_raw;
    let dc_den = 1.0 + a1 + a2;
    let scale = dc_den / dc_num;

    [1.0, a1, a2, b0_raw * scale, b1_raw * scale, b2_raw * scale]
}
