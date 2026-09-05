//! The three-point Lagrange synthesis kernel, and the paths built on it.

use super::{Coeffs, PASSTHROUGH, PI};

/// Pro-Q 4 audio-path Lagrange-MZT **alt 2-point** synthesis.
///
/// This is the `byte[0x48] = 1` branch inside
/// `compute_audio_biquad_lagrange_mzt @ 0x1801103c0`
/// (sub-block `0x18011072c..0x180110850`).  The binary takes this branch
/// for "high-Q" per-section configurations of the Bell brick-wall
/// cascade (slope ≥ 4) where the per-section auxiliary frequency
/// `w_zero` lies *above* `w_pole`.  Only `w_pole`, `w_zero`, `w_eval`
/// are consumed (no `w_third`, no 3-point Lagrange interpolation).
///
/// Verified bit-exact (≤ 1.9e-15 abs error across all 5 biquad
/// coefficients) on 32 captured per-section rows from
/// `lagrange_per_section_sweep.csv` joined with `solve_bq_sweep.csv`
/// (slope=4, fc ∈ {500, 1000, 5000, 10000} Hz, `Q_user` ∈ {4, 10} plus
/// the Q=1 sec=1 fc∈{500,1000} cases).
///
/// See `docs/reports/proq4/re/high_q_correction_decoded.md` for the
/// full ASM-to-formula mapping.
///
/// Inputs are the captured prototype polynomial values:
///   - `cap_a..cap_f`: `|P(jω)|² = (A·ω⁴+B·ω²+C) / (D·ω⁴+E·ω²+F)`
///   - `w_pole`, `w_zero`, `w_eval`: pre-clamped sub-frequencies
///   - `g_ref`: `C/F` (per-section squared gain ratio)
///
/// Returns `[a0=1, a1, a2, b0, b1, b2]` matching the layout used
/// elsewhere in `cascade.rs`, or `PASSTHROUGH` if the formula
/// degenerates (zero divisor, non-finite intermediate).
pub fn lagrange_synth_alt_path(
    cap_a: f64,
    cap_b: f64,
    cap_c: f64,
    cap_d: f64,
    cap_e: f64,
    cap_f: f64,
    w_pole: f64,
    w_zero: f64,
    w_eval: f64,
    g_ref: f64,
) -> Coeffs {
    const W_POLE_MAX: f64 = 3.078_760_800_517_997;
    const W_ZERO_MAX: f64 = 2.827_433_388_230_814;

    let w_pole = w_pole.min(W_POLE_MAX);
    let w_zero = w_zero.min(W_ZERO_MAX);
    let w_eval = w_eval.clamp(0.0, PI);

    let hsq = |w: f64| -> f64 {
        let w2 = w * w;
        let w4 = w2 * w2;
        let den = cap_d.mul_add(w4, cap_e.mul_add(w2, cap_f));
        if den.abs() < 1e-300 {
            0.0
        } else {
            (cap_a.mul_add(w4, cap_b.mul_add(w2, cap_c))) / den
        }
    };

    let u_pole = hsq(w_pole);
    let u_zero = hsq(w_zero);
    let u_eval = hsq(w_eval);

    let p3 = u_eval.max(0.0).sqrt();
    let p2 = g_ref.max(0.0).sqrt();

    let t1 = (w_pole * 0.5).tan();
    let t2 = (w_zero * 0.5).tan();
    let t1s = t1 * t1;
    let t2s = t2 * t2;

    if (u_pole - g_ref).abs() < 1e-300 || t1s.abs() < 1e-300 {
        return PASSTHROUGH;
    }

    let s_inner = ((u_pole - u_eval) / (u_pole - g_ref)).max(0.0).sqrt();
    let s_val = (s_inner * t1s).max(0.0);
    let p4 = s_val;

    let inv_t1s = 1.0 / t1s;

    // sp5 numerator/denominator (named after captured ASM register flow)
    let term_a = 2.0 * s_val * (u_zero - u_pole);
    let term_b = (u_pole - g_ref) * s_val * s_val * inv_t1s;
    let term_c = (u_eval - u_zero) * t2s;
    let term_d = (u_pole - u_eval) * t1s;
    let bracket = term_a + term_b + term_c + term_d;
    let sp5_den = (u_zero - u_pole) * t2s;
    if sp5_den.abs() < 1e-300 {
        return PASSTHROUGH;
    }
    let sp5 = ((bracket * t2s + (g_ref - u_zero) * s_val * s_val) / sp5_den).max(0.0);

    // sp6 (combined post-sp5)
    let sp6 = ((2.0 * p2 * p3).mul_add(s_val, (t1s - s_val).powi(2) * u_pole / t1s - t1s * u_eval)
        - s_val * s_val * g_ref / t1s
        + sp5 * u_pole)
        .max(0.0);

    let sq5 = sp5.sqrt();
    let sq6 = sp6.sqrt();
    let big_d = (1.0 + p4) + sq5;
    if !big_d.is_finite() || big_d.abs() < 1e-30 {
        return PASSTHROUGH;
    }
    let inv_d = 1.0 / big_d;
    let b0 = (p2 * p4 + p3 + sq6) * inv_d;
    let b1 = -2.0 * (p3 - p2 * p4) * inv_d;
    let b2 = (p2 * p4 + p3 - sq6) * inv_d;
    let a1 = -2.0 * (1.0 - p4) * inv_d;
    let a2 = (1.0 + p4 - sq5) * inv_d;

    [1.0, a1, a2, b0, b1, b2]
}
