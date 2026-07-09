//! Pro-Q 4 bandpass MZT biquad.

use std::f64::consts::PI;

use crate::biquad::Coeffs;

/// Bandpass via MZT — from bp_notch_exact.md RE.
///
/// p2 = 0, p3 = (3/4)·sp6, sp5 = sp6 = √2·t/Q
/// Gives b0 = (7√2/4)·t/Q/D, b1 = -(6/7)·b0, b2 = -(1/7)·b0.
/// ~0.7% error from actual p3/sp6 ratio (0.74863 vs 3/4).
pub fn design_bandpass_mzt(freq_hz: f64, q: f64, sample_rate: f64) -> Coeffs {
    // Pro-Q 4 Bandpass (filter_type=6) — non-standard form with TWO zeros:
    // - one at z=+1 (DC suppression)
    // - one at z≈-0.144 (≈14 kHz analog-equivalent extra zero)
    // Pole pair: complex conjugate at fc with damping 1/(2·Q_bw), Q_bw=Q/√2.
    //
    // Analog interpretation:
    //   H(s) = K · s · (s + Ω_z) / (s² + (Ω0/Q_bw)·s + Ω0²)
    // where Ω_z corresponds to digital z≈-0.144 (BLT-prewarp inverse:
    // z=-0.144 → analog x = 1.336, fc_analog ≈ 14150 Hz).
    //
    // Implementation: BLT-prewarp pole pair at fc; place numerator zero at
    // z=+1 (BLT-prewarped DC) and z=z_extra (fixed digital position), then
    // scale to make |H(e^{j·w0})| = 1 (unity peak gain).
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

    // Numerator zeros: one at z=+1, one at z_extra (Pro-Q 4 placement).
    // Refit (2026-04-29) from dense fc sweep (50 Hz..14 kHz, 16 captures).
    // Order-5 polynomial in t² (= tan²(πfc/sr)). Max abs error vs Pro-Q 4: 8e-7.
    // Above fc≈16 kHz Pro-Q 4 switches to a different design (z_extra → -0.75
    // near Nyquist); not modeled here — most BP filters sit below 14 kHz.
    let t4 = t2 * t2;
    let t6 = t4 * t2;
    let t8 = t4 * t4;
    let t10 = t8 * t2;
    let z_extra = -1.4353418643331e-01
        + -5.1681616455089e-02 * t2
        + 3.5155728635281e-02 * t4
        + -1.7769301791135e-02 * t6
        + 7.1849909782389e-03 * t8
        + -1.3739788582112e-03 * t10;
    // Numerator polynomial: (z-1)(z-z_extra) = z² - (1+z_extra)·z + z_extra
    // Or in z⁻¹ form: 1 - (1+z_extra)·z⁻¹ + z_extra·z⁻²
    // Wait — that has zeros at z=1 and z=z_extra. Coefficients (b0, b1, b2)
    // with z⁻¹ convention: b0=1, b1=-(1+z_extra), b2=z_extra.
    let b0_raw = 1.0_f64;
    let b1_raw = -(1.0 + z_extra);
    let b2_raw = z_extra;

    // Scale numerator so |H(e^{-jw0})| = 1.
    let cw = w0.cos();
    let sw = w0.sin();
    // Compute |num(e^{-jw0})| and |den(e^{-jw0})|
    let num_re = b0_raw + b1_raw * cw + b2_raw * (cw * cw - sw * sw);
    let num_im = -b1_raw * sw - b2_raw * 2.0 * cw * sw;
    let den_re = 1.0 + a1 * cw + a2 * (cw * cw - sw * sw);
    let den_im = -a1 * sw - a2 * 2.0 * cw * sw;
    let num_mag = (num_re * num_re + num_im * num_im).sqrt();
    let den_mag = (den_re * den_re + den_im * den_im).sqrt();
    let scale = if num_mag > 1e-30 {
        den_mag / num_mag
    } else {
        1.0
    };

    [1.0, a1, a2, b0_raw * scale, b1_raw * scale, b2_raw * scale]
}
