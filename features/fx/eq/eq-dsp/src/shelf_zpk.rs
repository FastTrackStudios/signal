//! Pro-Q 4 shelf filter design — ZPK pipeline based on RE reports.
//!
//! See `docs/reports/proq4/re/` for reverse-engineering details. Key functions:
//! - `apply_eq_band_parameters_full` @ 0x1801110b0 — Q→BW conversion
//! - `compute_cascade_coefficients` @ 0x1800fec20 — direct-Z prototype for shelves
//! - `apply_shelf_gain_to_zpk` @ 0x1800fcce0 — pole/zero repositioning by shelf gain
//! - `zpk_to_biquad_coefficients` @ 0x1800fe040 — first-section Q scaling
//! - `setup_eq_band_filter` @ 0x1800fdf10 — Tilt Shelf Q*√2 post-norm
//!
//! UI → binary type map:
//!   UI "Low Shelf"  → binary types 1, 2 (prototype=peak, transform=0 direct)
//!   UI "High Shelf" → binary types 3, 4 (prototype=HP,   transform=1 LP→BP)
//!   UI "Tilt Shelf" → binary type 9     (prototype=LP,   transform=2 bilinear)
//!   UI "Band Shelf" → binary type 10    (prototype=LP,   transform=3 LP→BP+bilinear)

use crate::biquad::{self, Coeffs};
use crate::constants::{INV_SQRT2, LN10_OVER_20, Q_BW_BASE, Q_BW_MULT, Q_BW_OFFSET, Q_BW_SCALE};
use crate::prototype;
use crate::transform;
use crate::zpk::Zpk;

use std::f64::consts::PI;

// ═══════════════════════════════════════════════════════════════════════════
// Q → bandwidth (shelf/cut types)
// ═══════════════════════════════════════════════════════════════════════════

/// Pro-Q 4's cos/pow Q→bandwidth formula for shelf/cut filter types.
///
/// From `apply_eq_band_parameters_full`:
/// ```text
///   q_bw = 32^(cos(Q) * 0.1355 + 0.5) * 0.125
/// ```
/// Sanity: at Q = π/2, cos(Q)=0 → q_bw = 32^0.5 · 0.125 = √32/8 = 1/√2 ≈ 0.707.
fn q_to_bandwidth(q: f64) -> f64 {
    let exponent = q.cos() * Q_BW_SCALE + Q_BW_OFFSET;
    Q_BW_BASE.powf(exponent) * Q_BW_MULT
}

// ═══════════════════════════════════════════════════════════════════════════
// Low Shelf — UI "Low Shelf" (binary type 1/2)
// ═══════════════════════════════════════════════════════════════════════════

/// Low shelf filter design.
///
/// Approach: RBJ cookbook 2nd-order analog low shelf, bilinear-transformed.
/// For `order > 2`, distributes total linear gain across `order/2` cascaded biquads.
/// User Q is converted via the cos/pow Q→BW formula from the binary.
///
/// RBJ analog prototype:
///   H(s) = A · (s² + (√A · w0/Q_bw) · s + A·w0²) / (A·s² + (√A · w0/Q_bw) · s + w0²)
/// where A = √(linear_gain).
pub fn design_low_shelf_zpk(
    n_sections: usize,
    freq_hz: f64,
    user_q: f64,
    user_gain_db: f64,
    sample_rate: f64,
) -> Vec<Coeffs> {
    let n = n_sections.max(1);
    if user_gain_db.abs() < 0.001 {
        return vec![biquad::PASSTHROUGH; n];
    }

    let total_linear = (user_gain_db * LN10_OVER_20).exp();
    let section_gain = total_linear.powf(1.0 / n as f64);
    // Pro-Q4's cos/pow bandwidth — used as RBJ shelf slope `S`. Best match at Q≈1.
    let q_bw = q_to_bandwidth(user_q);
    let w0 = 2.0 * PI * freq_hz / sample_rate;

    (0..n)
        .map(|_| rbj_low_shelf_slope_biquad(w0, q_bw, section_gain))
        .collect()
}

/// RBJ low shelf using slope (S) parameter instead of Q.
///
/// From the cookbook:
///   alpha = sin(w0) / 2 · √((A + 1/A)·(1/S − 1) + 2)
///
/// At S=1, this reduces to alpha = sin(w0)/√2. For S<1, alpha increases (broader).
/// For S>1, the argument can go negative — clamp to 0 to avoid NaN.
fn rbj_low_shelf_slope_biquad(w0: f64, slope: f64, linear_gain: f64) -> Coeffs {
    let a = linear_gain.sqrt();
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let s = slope.max(1e-6);
    let disc = (a + 1.0 / a) * (1.0 / s - 1.0) + 2.0;
    let alpha = sin_w0 / 2.0 * disc.max(0.0).sqrt();
    let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

    let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
    let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
    let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
    let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
    let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
    let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha;

    [1.0, a1 / a0, a2 / a0, b0 / a0, b1 / a0, b2 / a0]
}

// ═══════════════════════════════════════════════════════════════════════════
// High Shelf / Tilt Shelf / Band Shelf — placeholders until rewritten
// ═══════════════════════════════════════════════════════════════════════════

/// High shelf (UI) — placeholder using existing bilinear approach.
pub fn design_high_shelf_zpk(
    n_sections: usize,
    freq_hz: f64,
    user_q: f64,
    user_gain_db: f64,
    sample_rate: f64,
) -> Vec<Coeffs> {
    let n = n_sections.max(1);
    if user_gain_db.abs() < 0.001 {
        return vec![biquad::PASSTHROUGH; n];
    }
    let total_linear = (user_gain_db * LN10_OVER_20).exp();
    let section_gain = total_linear.powf(1.0 / n as f64);
    let q_bw = q_to_bandwidth(user_q);
    let w0 = 2.0 * PI * freq_hz / sample_rate;
    (0..n)
        .map(|_| rbj_high_shelf_biquad(w0, q_bw, section_gain))
        .collect()
}

/// RBJ high shelf biquad.
fn rbj_high_shelf_biquad(w0: f64, q: f64, linear_gain: f64) -> Coeffs {
    let a = linear_gain.sqrt();
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let alpha = sin_w0 / (2.0 * q.max(1e-6));
    let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

    let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
    let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
    let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
    let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
    let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
    let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha;

    [1.0, a1 / a0, a2 / a0, b0 / a0, b1 / a0, b2 / a0]
}

/// Tilt shelf — placeholder until rewritten (uses bilinear approach).
pub fn design_tilt_shelf_zpk(
    n_sections: usize,
    freq_hz: f64,
    user_q: f64,
    user_gain_db: f64,
    sample_rate: f64,
) -> Vec<Coeffs> {
    let n = n_sections.max(1);
    if user_gain_db.abs() < 0.001 {
        return vec![biquad::PASSTHROUGH; n];
    }

    let section_gain_db = user_gain_db / n as f64;
    let linear_gain = (section_gain_db * LN10_OVER_20).exp();
    let gain_param = linear_gain.sqrt();

    let mut sections = Vec::new();
    for _k in 0..n {
        let mut zpk = prototype::butterworth_lp_prewarped(2, freq_hz, sample_rate);
        apply_shelf_gain(&mut zpk, gain_param, false);
        let digital = transform::bilinear(&zpk, sample_rate);
        let mut sos = biquad::zpk_to_sos(&digital);
        apply_q_to_shelf_section(&mut sos, user_q);
        sections.extend(sos);
    }
    sections
}

/// Band shelf — transform type 3 (LP→BP + bilinear). Kept as-is (currently passing).
pub fn design_band_shelf_zpk(
    n_sections: usize,
    freq_hz: f64,
    user_q: f64,
    user_gain_db: f64,
    sample_rate: f64,
) -> Vec<Coeffs> {
    let n = n_sections.max(1);
    if user_gain_db.abs() < 0.001 {
        return vec![biquad::PASSTHROUGH; n];
    }

    let linear_gain = (user_gain_db * LN10_OVER_20).exp();
    let gain_param = linear_gain.sqrt();

    let mut bp = crate::prototype::butterworth_bp_elliptic(n, freq_hz, user_q, sample_rate);
    for zero in &mut bp.zeros {
        *zero = *zero * gain_param;
    }
    bp.gain *= gain_param.powi(bp.poles.len() as i32);

    let digital = crate::transform::bilinear(&bp, sample_rate);
    let mut sos = crate::biquad::zpk_to_sos(&digital);

    let w0 = 2.0 * PI * freq_hz / sample_rate;
    let peak = crate::biquad::eval_sos(&sos, w0).mag();
    if peak > 1e-10 {
        let target = 10.0_f64.powf(user_gain_db / 20.0);
        let scale = target / peak;
        if let Some(first) = sos.first_mut() {
            first[3] *= scale;
            first[4] *= scale;
            first[5] *= scale;
        }
    }

    apply_q_to_shelf_section(&mut sos, user_q);
    sos
}

// ═══════════════════════════════════════════════════════════════════════════
// Internal helpers (legacy — kept until tilt/band shelf are rewritten)
// ═══════════════════════════════════════════════════════════════════════════

fn apply_shelf_gain(zpk: &mut Zpk, gain_param: f64, is_low_type: bool) {
    for zero in zpk.zeros.iter_mut() {
        *zero = *zero * gain_param;
    }
    if is_low_type {
        let inv_gain = 1.0 / gain_param;
        for pole in zpk.poles.iter_mut() {
            *pole = *pole * inv_gain;
        }
    }
}

fn apply_q_to_shelf_section(sos: &mut [Coeffs], user_q: f64) {
    if sos.is_empty() || user_q.abs() < 1e-15 {
        return;
    }
    let q_scale = INV_SQRT2 / user_q;
    let coeffs = &mut sos[0];
    coeffs[1] *= q_scale;
    coeffs[4] *= q_scale;
}
