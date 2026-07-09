//! Pro-Q 4 notch MZT biquad.

use std::f64::consts::PI;

use crate::biquad::Coeffs;

/// Notch via MZT — from notch_bandpass_lp_hp_mzt.md RE.
///
/// p2=1, p3=1, p4=t², sp5=√2·t/Q, sp6=0.
/// Numerator: b1/b0 = -2cos(2π·fc/fs) (zeros exactly at unit circle at corner).
pub fn design_notch(freq_hz: f64, q: f64, sample_rate: f64) -> Coeffs {
    // Standard analog notch H(s) = (s²+1)/(s² + (1/Q_bw)·s + 1) via BLT
    // with Q_bw = Q/√2. Verified |H(0)|=1, |H(w0)|≈0, |H(π)|=1.
    let q = q.max(1e-6);
    let q_bw = q * std::f64::consts::FRAC_1_SQRT_2;
    let w0 = 2.0 * PI * freq_hz / sample_rate;
    let t = (w0 * 0.5).tan();
    let t2 = t * t;
    let alpha = t / q_bw;
    let d = 1.0 + alpha + t2;
    let inv_d = 1.0 / d;
    let a1 = 2.0 * (t2 - 1.0) * inv_d;
    let a2 = (1.0 - alpha + t2) * inv_d;
    let b0 = (1.0 + t2) * inv_d;
    let b1 = 2.0 * (t2 - 1.0) * inv_d;
    let b2 = (1.0 + t2) * inv_d;
    [1.0, a1, a2, b0, b1, b2]
}
