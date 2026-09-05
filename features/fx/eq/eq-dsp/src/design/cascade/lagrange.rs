//! The three-point Lagrange synthesis kernel, and the paths built on it.

use super::*;

/// Bell slope-2 — Pro-Q 4 audio-path Lagrange synthesis (s=2 closed form).
///
/// Implements the full Lagrange-MZT synthesis decoded from Pro-Q 4 binary
/// (see `docs/reports/proq4/re/bell_s2_full_pipeline.md`).  Steps:
///
/// 1. Q correction: `Q_corr = Q · k(Q)` where `k(Q) = 1 + c·ln(Q)`,
///    `c ≈ -1.3507e-5` (fitted from a 12-point dense Q sweep, fc/gain-
///    independent — `lagrange_proto_q_sweep.csv`).
/// 2. Build Bell prototype ZPK polynomial (`A,B,C` num, `D,E,F` den) using
///    `b1z = √2·A/Q_corr`, `b1p = √2/(A·Q_corr)`, `g = ω₀`.
/// 3. Sub-frequency selection: `delta = clamp(1/Q, 0.1, 0.8)`,
///    `w_pole = ω₀`, `w_zero = ω₀·(1-delta)`, `w_third = ω₀·(1-delta/20)`,
///    `w_eval = 0.9π`.
/// 4. Evaluate `|P(jω)|²` on the analog prototype at all 4 ω.
/// 5. 3-point Lagrange synthesis in tan² space → `(p4, sp5, sp6)`.
/// 6. Mode-0 ASM closed form → final biquad coefficients.
/// Pro-Q 4 audio-path Lagrange-MZT 3-point synthesis kernel.
///
/// This is the post-(u_*, w_*) tail of `compute_biquad_response_magnitude`
/// @ 0x1801103c0 (the `byte[0x48] = 0` branch).  Given:
///   - per-section sub-frequencies (`w_pole`, `w_zero`, `w_third`, `w_eval`) in
///     digital rad/sample,
///   - the corresponding analog magnitude-squared values (`u_pole`, `u_zero`,
///     `u_third`, `u_eval`) — typically `|H(jΩ)|²` evaluated at the warped
///     `Ω = tan(w/2) / tan(ω₀/2)` for bucket-B sections, or directly at
///     digital ω scaled into bell-s2's normalized form,
///   - the per-section `g_ref` — `cap_c/cap_f` for bell-s2 (= 1) or
///     `sec_gain_ref` for bucket-B,
///
/// runs the determinant + closed-form coefficient extraction and emits
/// `[1, a1, a2, b0, b1, b2]`.
///
/// Bit-exact against `compute_biquad_response_magnitude` for bucket-A
/// (Bell s=2, validated via 100% conformance) and bucket-B (Bell s∈{3..9},
/// validated via `bell_bucketB_synth_v3.py` to f64 noise floor at LF).
#[must_use]
pub fn lagrange3pt_synth_kernel(
    w_pole: f64,
    w_zero: f64,
    w_third: f64,
    _w_eval: f64,
    u_pole: f64,
    u_zero: f64,
    u_third: f64,
    u_eval: f64,
    g_ref: f64,
) -> Coeffs {
    let p3 = u_eval.max(0.0).sqrt();
    let p2 = g_ref.max(0.0).sqrt();
    let t1 = (w_pole * 0.5).tan();
    let t2 = (w_zero * 0.5).tan();
    let t3 = (w_third * 0.5).tan();
    let t1s = t1 * t1;
    let t2s = t2 * t2;
    let t3s = t3 * t3;
    let den = t3s
        * ((u_zero - u_third) * (g_ref - u_pole) * t2s
            - (u_pole - u_third) * (g_ref - u_zero) * t1s)
        + (g_ref - u_third) * (u_pole - u_zero) * t1s * t2s;
    let num = u_pole * ((t2s - t3s) * u_eval + (t1s - t2s) * u_zero + (t3s - t1s) * u_third)
        + u_eval * ((t3s - t1s) * u_zero + (t1s - t2s) * u_third)
        + (t2s - t3s) * u_third * u_zero;
    let s2 = if den.abs() > 1e-30 {
        (num / den).max(0.0)
    } else {
        0.0
    };
    let s_val = s2.sqrt();
    let p4 = s_val * t1 * t2 * t3;
    let a1_term = t1s * p3 - p4 * p2;
    let a2_term = t2s * p3 - p4 * p2;
    let sp6_den = (u_pole - u_zero) * t1s * t2s;
    let sp6 = if sp6_den.abs() > 1e-30 {
        let sp6_num = a1_term * a1_term * t2s * u_zero
            - (t1s * t2s * (1.0 - s2 * t3s) * (t1s - t2s) * u_zero + a2_term * a2_term * t1s)
                * u_pole;
        (sp6_num / sp6_den).max(0.0)
    } else {
        0.0
    };
    let sp5 = if (t1s * u_pole).abs() > 1e-30 {
        ((sp6 * t1s - (t1s - p4).powi(2) * u_pole + a1_term * a1_term) / (t1s * u_pole)).max(0.0)
    } else {
        0.0
    };
    let sq5 = sp5.sqrt();
    let sq6 = sp6.sqrt();
    let big_d = (1.0 + p4) + sq5;
    if !big_d.is_finite() || big_d.abs() < 1e-30 {
        return PASSTHROUGH;
    }
    let inv_d = 1.0 / big_d;
    let b0 = (p2 * p4 + p3 + sq6) * inv_d;
    let b1 = -2.0 * (p3 - p2 * p4) * inv_d;
    let b2 = (p3 - sq6 + p2 * p4) * inv_d;
    let a1 = -2.0 * (1.0 - p4) * inv_d;
    let a2 = (1.0 + p4 - sq5) * inv_d;
    [1.0, a1, a2, b0, b1, b2]
}

/// Bell bucket-B (slopes 3..9) per-section synthesis given the captured
/// 6-field analog quadratic + per-section sub-frequencies.
///
/// This is the algorithmic core for slope ≥ 3.  It takes the captured-from-
/// Pro-Q analog quadratic coefficients
///   `(b2z, b1z, b0z, b2p, b1p, b0p)`
/// (struct offsets 0x60..0x88), the per-section sub-frequencies
///   `(w_pole, w_zero, w_third, w_eval)`,
/// and the section-specific `sec_gain_ref`, and emits the digital biquad.
///
/// Validated bit-exact (≤ 1e-9 at LF, see
/// `tools/proq4_probe/lookup_capture/bell_bucketB_synth_v3.py`).
#[must_use]
pub fn bell_bucket_b_section_from_analog(
    b2z: f64,
    b1z: f64,
    b0z: f64,
    b2p: f64,
    b1p: f64,
    b0p: f64,
    omega0: f64,
    w_pole: f64,
    w_zero: f64,
    w_third: f64,
    w_eval: f64,
    sec_gain_ref: f64,
) -> Coeffs {
    // Map digital ω → normalized analog Ω = tan(ω/2) / tan(ω₀/2).
    let t0 = (omega0 * 0.5).tan();
    let h_sq = |w: f64| -> f64 {
        let tw = (w * 0.5).tan();
        let om = tw / t0;
        let om2 = om * om;
        let num = (b0z - b2z * om2).powi(2) + (b1z * om).powi(2);
        let den = (b0p - b2p * om2).powi(2) + (b1p * om).powi(2);
        if den > 1e-300 {
            num / den
        } else {
            0.0
        }
    };
    let u_pole = h_sq(w_pole);
    let u_zero = h_sq(w_zero);
    let u_third = h_sq(w_third);
    let u_eval = h_sq(w_eval);
    lagrange3pt_synth_kernel(
        w_pole,
        w_zero,
        w_third,
        w_eval,
        u_pole,
        u_zero,
        u_third,
        u_eval,
        sec_gain_ref,
    )
}

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
pub(crate) fn lagrange_synth_alt_path(
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
        let den = cap_d * w4 + cap_e * w2 + cap_f;
        if den.abs() < 1e-300 {
            0.0
        } else {
            (cap_a * w4 + cap_b * w2 + cap_c) / den
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
    let sp6 = ((t1s - s_val).powi(2) * u_pole / t1s - t1s * u_eval + 2.0 * p2 * p3 * s_val
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
