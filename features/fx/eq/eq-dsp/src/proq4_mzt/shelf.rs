//! Pro-Q 4 single-section shelf MZT biquads (low_shelf, high_shelf, tilt_shelf).

use std::f64::consts::PI;

use crate::biquad::Coeffs;
use crate::constants::LN10_OVER_20;

use super::biquad_from_mode0_params;

/// Low shelf via MZT — Pro-Q 4 formulation.
///
/// From RE (docs/reports/proq4/re/shelf_mzt_formula.md):
///   p2 = gain, p3 = 1, p4 = (t/G)², sp5 = √2·t/G, sp6 = √2·t·G  (at Q=1)
///   where gain = 10^(dB/20), G = gain^(1/4), t = tan(πfc/fs)
/// For other Q: damping factor scales as (√2/Q) replacing √2.
pub fn design_low_shelf(freq_hz: f64, q: f64, gain_db: f64, sample_rate: f64) -> Coeffs {
    if gain_db.abs() < 1e-9 {
        return [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    }
    // Pro-Q 4 LowShelf — slope-2 mode-0 closed-form, per
    // docs/reports/proq4/re/shelf_filters_formula.md §1.
    //   t      = tan(π·fc/fs)
    //   gain   = 10^(dB/20)
    //   G      = gain^(1/4)
    //   q_int  = Q^(5/10.644)        (UI-Q → bandwidth scale)
    //   p2 = gain, p3 = 1
    //   p4 = (t/G)²
    //   sp5 = √2·t / (G·q_int)
    //   sp6 = √2·t·G / q_int
    let gain = (gain_db * LN10_OVER_20).exp();
    let g = gain.powf(0.25);
    let t = (PI * freq_hz / sample_rate).tan();
    let q_int = q.max(1e-6).powf(5.0 / 10.644);
    let inv_g = 1.0 / g;
    let sqrt2 = std::f64::consts::SQRT_2;

    let p2 = gain;
    let p3 = 1.0;
    let p4 = (t * inv_g) * (t * inv_g);
    let sp5 = sqrt2 * t * inv_g / q_int;
    let sp6 = sqrt2 * t * g / q_int;
    biquad_from_mode0_params(p2, p3, p4, sp5, sp6)
}
/// High shelf via MZT — Pro-Q 4 formulation.
///
/// From RE:
///   p2 = 1, p3 = gain, p4 = (t·G)², sp5 = √2·t·G, sp6 = √2·t·G·A
///   where A = gain^(1/2)
pub fn design_high_shelf(freq_hz: f64, q: f64, gain_db: f64, sample_rate: f64) -> Coeffs {
    if gain_db.abs() < 1e-9 {
        return [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    }
    // Pro-Q 4 HighShelf — slope-2 mode-0 closed-form, per
    // docs/reports/proq4/re/shelf_filters_formula.md §2.
    //   gain  = 10^(dB/20), G = gain^(1/4), A = √gain
    //   t     = tan(π·fc/fs)
    //   q_int = Q^(5/10.644)
    //   p2 = 1, p3 = gain
    //   p4 = (t·G)²
    //   sp5 = √2·t·G / q_int
    //   sp6 = √2·t·G·A / q_int
    let gain = (gain_db * LN10_OVER_20).exp();
    let g = gain.powf(0.25);
    let a_sym = gain.sqrt();
    let t = (PI * freq_hz / sample_rate).tan();
    let q_int = q.max(1e-6).powf(5.0 / 10.644);
    let sqrt2 = std::f64::consts::SQRT_2;

    let p2 = 1.0;
    let p3 = gain;
    let p4 = (t * g) * (t * g);
    let sp5 = sqrt2 * t * g / q_int;
    let sp6 = sqrt2 * t * g * a_sym / q_int;
    biquad_from_mode0_params(p2, p3, p4, sp5, sp6)
}
/// Tilt shelf via MZT — Pro-Q 4 formulation.
///
/// From RE (per shelf_filters_formula.md §3): symmetric — poles scale by A, zeros by 1/A:
///   p2 = 1/gain, p3 = gain, p4 = t²·gain, sp5 = sp6 = t·√(2·gain)/Q_eff
///
/// where Q_eff = Q^(5/10.644) is Pro-Q 4's UI Q → bandwidth mapping
/// (verified bit-exact against tiltshelf_biquad_sweep.csv at fc ≤ 1 kHz).
pub fn design_tilt_shelf(freq_hz: f64, q: f64, gain_db: f64, sample_rate: f64) -> Coeffs {
    if gain_db.abs() < 1e-9 {
        return [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    }
    let gain = (gain_db * LN10_OVER_20).exp();
    let w0 = 2.0 * PI * freq_hz / sample_rate;
    let t = (w0 * 0.5).tan();
    let q_pre = q.max(1e-6) * (0.5 * w0 / t).abs();

    let p2 = 1.0 / gain;
    let p3 = gain;
    let p4 = t * t * gain;
    let sp5 = t * (2.0 * gain).sqrt() / q_pre.max(1e-6);
    let sp6 = sp5;
    biquad_from_mode0_params(p2, p3, p4, sp5, sp6)
}
/// First-order tilt shelf section — Pro-Q 4 odd-slope cascade tail.
///
/// Analog prototype (decoded from `PROBE_HOOK_AUDIO_BIQUAD` SEC_PRE on
/// tilt slope=3,5,7 — sections with `b2z=0`):
///
///   B(s) = g·s + 1
///   A(s) = s + g
///
/// where `g = gain^(1/N)` for slope=N (i.e. half a 2nd-order section's
/// gain exponent).  Bilinear transform with prewarped corner `t = tan(πfc/fs)`:
///
///   B(z) = g·t + 1 + (g·t − 1)·z⁻¹     (numerator)
///   A(z) = t + g + (t − g)·z⁻¹         (denominator)
///
/// Normalized so a0=1.  Returned as a biquad with b2 = a2 = 0.
pub fn design_tilt_shelf_first_order(freq_hz: f64, gain_db: f64, sample_rate: f64) -> Coeffs {
    if gain_db.abs() < 1e-9 {
        return [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    }
    let g = (gain_db * LN10_OVER_20).exp();
    let w0 = 2.0 * PI * freq_hz / sample_rate;
    let t = (w0 * 0.5).tan();

    let n0 = g * t + 1.0;
    let n1 = g * t - 1.0;
    let d0 = t + g;
    let d1 = t - g;
    let inv = 1.0 / d0;
    [1.0, d1 * inv, 0.0, n0 * inv, n1 * inv, 0.0]
}
