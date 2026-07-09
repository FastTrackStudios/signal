//! Notch cascade builders for Pro-Q 4 (s=2 through s=8).

use std::f64::consts::PI;

use crate::biquad::Coeffs;

use super::*;

/// Solve for the inner-section `(a1, a2)` of the LP→BS factorisation
/// (`notch_formula.md` / `bandpass_formula.md`).
///
/// The palindromic quartic `s⁴ + B·s³ + C·s² + B·s + 1` factors via
/// `u = s + 1/s` into `u² + B·u + (C − 2) = 0`, giving two `u` values;
/// the inner pole pair is the smaller-|s| roots of `s² − u·s + 1 = 0`.
pub fn notch_inner_pair(b_quartic: f64, c_quartic: f64) -> (f64, f64) {
    let half_b = 0.5 * b_quartic;
    let disc = half_b * half_b - (c_quartic - 2.0);
    let (u_re, u_im) = if disc >= 0.0 {
        (-half_b + disc.sqrt(), 0.0)
    } else {
        (-half_b, (-disc).sqrt())
    };
    let u2_re = u_re * u_re - u_im * u_im;
    let u2_im = 2.0 * u_re * u_im;
    let d_re = u2_re - 4.0;
    let d_im = u2_im;
    let d_mag = (d_re * d_re + d_im * d_im).sqrt();
    let sqrt_re = ((d_mag + d_re) * 0.5).max(0.0).sqrt();
    let sqrt_im = ((d_mag - d_re) * 0.5).max(0.0).sqrt() * d_im.signum();
    let sp_re = 0.5 * (u_re + sqrt_re);
    let sp_im = 0.5 * (u_im + sqrt_im);
    let sm_re = 0.5 * (u_re - sqrt_re);
    let sm_im = 0.5 * (u_im - sqrt_im);
    let mag_p = sp_re * sp_re + sp_im * sp_im;
    let mag_m = sm_re * sm_re + sm_im * sm_im;
    let (s_re, mag2) = if mag_p < mag_m {
        (sp_re, mag_p)
    } else {
        (sm_re, mag_m)
    };
    (-2.0 * s_re, mag2)
}
pub fn notch_analog_sections(slope: usize, q: f64) -> Vec<(f64, f64)> {
    use std::f64::consts::SQRT_2;
    let q_user = q.max(1e-6);
    // C = 2 + 2/Q² is θ-independent for the LP→BS-derived quartic
    // (`u₁·u₂ = (√2/Q)²` regardless of LP-prototype angle).
    let c_quartic = 2.0 + 2.0 / (q_user * q_user);
    match slope {
        2 => vec![(SQRT_2 / q_user, 1.0)],
        4 => {
            let b = 2.0 / q_user;
            vec![notch_inner_pair(b, c_quartic)]
        }
        6 => {
            let b = 2.0 / q_user;
            let (a1i, a2i) = notch_inner_pair(b, c_quartic);
            vec![(a1i, a2i), (a1i / a2i, 1.0 / a2i)]
        }
        8 => {
            // slope=8 LP prototype = Butterworth N=6 (per
            // `lp_atoms_for_slope`: A_S8 = [105°, 135°, 165°]).  Each LP
            // conjugate pair → one quartic via LP→BS → reciprocal pair
            // of analog notch sections.  Total: 6 sections.
            //
            // Note: `notch_formula.md` documents 3 sections from path-A
            // captures, but conformance-wise the full Butterworth N=6
            // LP→BS cascade matches Pro-Q 4 better at low fc — likely
            // the path-A captures are reduced to inner-only forms while
            // the audio-path uses both inner and outer.
            let mut sections = Vec::with_capacity(6);
            for theta_deg in [105.0_f64, 135.0, 165.0] {
                let theta = theta_deg * PI / 180.0;
                let b = -2.0 * SQRT_2 * theta.cos() / q_user;
                let (a1i, a2i) = notch_inner_pair(b, c_quartic);
                sections.push((a1i, a2i));
                sections.push((a1i / a2i, 1.0 / a2i));
            }
            sections
        }
        _ => vec![(SQRT_2 / q_user, 1.0)],
    }
}
/// Pro-Q 4 Notch cascade for slope ∈ {2, 4, 6, 8}.
///
/// Per `notch_formula.md`: numerator universally `s² + 1`, per-slope
/// denominator from a Butterworth LP prototype mapped through LP→BS with
/// `α = √2/Q`, then prewarped BLT.  Per-section DC-gain compensation
/// multiplies the numerator by `a2` so each section has unity DC and
/// Nyquist gain (cancels across reciprocal pairs).
pub fn notch_cascade_proq4(freq_hz: f64, q: f64, sample_rate: f64, slope: usize) -> Vec<Coeffs> {
    let q_user = q.max(1e-6);
    let _omega0 = (2.0 * PI * freq_hz / sample_rate).min(PI - 0.01);

    // Slope-2 single-section path: matched-Z transform of analog
    //   H(s) = (s² + ω₀²) / (s² + (√2/Q)·ω₀·s + ω₀²).
    // Verified bit-exact (≤6e-6 error) against captured Pro-Q 4
    // LAG_OUT at fc ≤ 5 kHz across Q ∈ {0.5, 1, 4, 10}. Mild drift
    // near Nyquist (the binary applies a small fc-prewarp correction
    // that is not yet captured); BLT/Lagrange variants do worse there.
    if slope == 2 {
        return vec![notch_s2_alt_path_synth(freq_hz, q_user, sample_rate)];
    }

    // Higher-slope cascade. Per `notch_formula.md`, notch numerator
    // is universally `s² + 1` (zeros pinned at s = ±j in the shared-g
    // convention) and denominator = BP analog `s² + a1·s + a2`.
    //
    // **Per-section template (slope ≥ 4)**: each Butterworth section's
    // analog `(b2p, b1p, b0p) = (1, a1, a2)` is fed to the same
    // Lagrange-MZT synth used by the s=2 single-section path. Sub-
    // frequencies follow the s=2 successful pattern but anchored on
    // the section's local center ω_sec = ω₀·√a2 with effective
    // Q_section = √a2 / a1. This generalises the bit-exact s=2 formula
    // (which is the slope=2 single-section special case where
    // a1 = √2/Q, a2 = 1, ω_sec = ω₀, Q_section = Q).
    notch_analog_sections(slope, q)
        .into_iter()
        .enumerate()
        .map(|(idx, (a1, a2))| {
            // iVar2 alternates 0/1 across reciprocal pair members
            // (inner=0, outer=1). Inner sections come first per pair.
            let ivar2 = (idx % 2) as u8;
            notch_s8_section_biquad(freq_hz, q_user, sample_rate, a1, a2, ivar2)
        })
        .collect()
}
/// Pro-Q 4 Notch lagrange alt-path synth, parameterized.
/// Used by both s=2 (single section) and s≥4 (per-section after applying
/// compute_band_shelf_parameters transform).
fn notch_alt_path_kernel(
    omega0: f64,
    a1_sec: f64,
    a2_sec: f64,
    g_ref: f64,
    w_pole: f64,
    w_zero: f64,
    w_third: f64,
) -> Coeffs {
    // Notch numerator (s²+1) → (A,B,C) = (1, -2g², g⁴)
    let g = omega0;
    let g2 = g * g;
    let g4 = g2 * g2;
    alt_path_kernel_generic(
        omega0,
        a1_sec,
        a2_sec,
        g_ref,
        w_pole,
        w_zero,
        w_third,
        1.0,
        -2.0 * g2,
        g4,
    )
}
/// Generic Pro-Q lagrange-MZT alt-path kernel.
/// `(cap_a, cap_b, cap_c)` are the numerator polynomial coefficients of
/// |H_a(jω)|² for the analog section: N(ω) = A·ω⁴ + B·ω² + C.
/// `(a1_sec, a2_sec)` define the denominator (1, a1, a2) → (1, E, F·g⁴).
fn alt_path_kernel_generic(
    _omega0: f64,
    a1_sec: f64,
    a2_sec: f64,
    g_ref: f64,
    w_pole: f64,
    w_zero: f64,
    w_third: f64,
    cap_a: f64,
    cap_b: f64,
    cap_c: f64,
) -> Coeffs {
    let g = _omega0;
    let g2 = g * g;
    let g4 = g2 * g2;
    let cap_e = (a1_sec * a1_sec - 2.0 * a2_sec) * g2;
    let cap_f = a2_sec * a2_sec * g4;
    let h_sq = |w: f64| -> f64 {
        let w2 = w * w;
        let w4 = w2 * w2;
        let num = cap_a * w4 + cap_b * w2 + cap_c;
        let den = w4 + cap_e * w2 + cap_f;
        if den.abs() > 1e-300 {
            (num / den).max(0.0)
        } else {
            0.0
        }
    };
    let u_zero = h_sq(w_zero);
    let u_third = h_sq(w_third);
    let u_eval = h_sq(PI);

    let t1 = (w_pole * 0.5).tan();
    let t2 = (w_zero * 0.5).tan();
    let t3 = (w_third * 0.5).tan();
    let t1s = t1 * t1;
    let t2s = t2 * t2;
    let t3s = t3 * t3;

    let p2 = g_ref.sqrt();

    let term1 = t1s * t1s * (t2s - t3s) * u_third * u_zero;
    let term2 = g_ref * u_zero * t2s * (t1s - t3s).powi(2);
    let term3 = g_ref * u_third * t3s * (t1s - t2s).powi(2);
    let discr = term1 - term2 + term3;
    let sqrt_discr = discr.abs().sqrt();

    let p3_alt = if sqrt_discr > 1e-300 {
        let inner = (t2s - t3s) * t2s * u_third * u_zero;
        let p3_numer = inner.max(0.0).sqrt() * p2 * t3;
        let candidate = p3_numer / sqrt_discr;
        let cap = 2.0 * u_eval.sqrt();
        candidate.min(cap)
    } else {
        2.0 * u_eval.sqrt()
    };

    let p4_alt = if p2.abs() > 1e-300 {
        t1s * p3_alt / p2
    } else {
        0.0
    };

    let denom_sp5 = t2s * u_zero;
    let sp5_alt = if denom_sp5 > 1e-300 {
        let term_a = ((t2s - t1s) * p3_alt).powi(2);
        let inner_b = t2s - t1s * p3_alt / p2;
        let term_b = inner_b * inner_b * u_zero;
        ((term_a - term_b) / denom_sp5).abs()
    } else {
        0.0
    };

    let p3 = p3_alt;
    let p4 = p4_alt;
    let sp5 = sp5_alt;
    let sp6: f64 = 0.0;
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
/// Pro-Q 4 Notch s=8 per-section synthesis — full pipeline:
///   solve_biquad → compute_band_shelf transform (per iVar2/root_count) → alt-path synth
fn notch_s8_section_biquad(
    fc: f64,
    q_user: f64,
    sample_rate: f64,
    a1_sec: f64,
    a2_sec: f64,
    ivar2: u8,
) -> Coeffs {
    let omega0 = (2.0 * PI * fc / sample_rate).min(PI - 1e-9);
    let g = omega0;
    let g2 = g * g;
    let g4 = g2 * g2;

    // Compute solve_biquad output per section
    let cap_b = -2.0 * g2;
    let cap_c = g4;
    let cap_e = (a1_sec * a1_sec - 2.0 * a2_sec) * g2;
    let cap_f = a2_sec * a2_sec * g4;
    let delta = cap_e - cap_b;
    let gamma = 2.0 * (cap_f - cap_c);
    let disc = gamma * gamma - 4.0 * delta * (cap_f * cap_b - cap_c * cap_e);

    if disc < 0.0 || delta.abs() < 1e-30 {
        return PASSTHROUGH;
    }
    let sq = disc.sqrt();
    let r1 = (-gamma + sq) / (2.0 * delta);
    let r2 = (-gamma - sq) / (2.0 * delta);
    let (root_count, solve_w_pole, solve_w_third) = match (r1 > 0.0, r2 > 0.0) {
        (true, true) => {
            let lo = r1.min(r2);
            let hi = r1.max(r2);
            (2u8, lo.sqrt(), hi.sqrt())
        }
        (true, false) => (1u8, r1.sqrt(), r2),
        (false, true) => (1u8, r2.sqrt(), r1),
        (false, false) => return PASSTHROUGH,
    };

    // S_blend from proto[10] = √2 (constant for Notch)
    let alpha = std::f64::consts::SQRT_2 / q_user;
    let a_shelf = (0.5_f64.powf(alpha * 0.5)).max(0.01);
    let sqrt_t = ((omega0 / PI).sqrt() as f32) as f64;
    let s_blend = (a_shelf - 0.99) * sqrt_t + 0.99;

    let g_ref = 1.0 / (a2_sec * a2_sec);

    // Alt-path formula (used by root_count=1 always, and root_count=2 outer
    // when w_zero clamps to π at high fc — verified via probe at fc=20k Q=1).
    let alt_path = || {
        let beta = (omega0 / PI - 0.8).clamp(0.0, 0.2);
        let b1p_f32 = ((beta * beta * 25.0) as f32) as f64;
        let mut wz = s_blend.sqrt() * omega0 * (1.0 - b1p_f32 * 0.05);
        let wt = (1.0 - b1p_f32 * 0.2) * wz * s_blend;
        if omega0 < 0.0314 {
            wz = 2.0 * omega0;
        }
        (omega0, wz, wt)
    };

    // Apply compute_band_shelf per-section transform
    let (w_pole, w_zero, w_third) = match (root_count, ivar2) {
        (2, 0) => {
            // inner pair: swap, w_third = solve_w_pole · S_blend
            (solve_w_third, solve_w_pole, solve_w_pole * s_blend)
        }
        (2, 1) => {
            // outer pair: at high fc the analog |H(jω)|² evaluation in
            // compute_band_shelf falls below threshold, triggering fallback
            // path that produces alt-path-equivalent output.
            // Threshold 0.98π separates midpoint from alt-path:
            //   fc=8k Q=1 sec 3:   solve_w_third=3.026 (0.963π) → MIDPOINT
            //   fc=18k Q=4 sec 3:  solve_w_third=3.029 (0.964π) → MIDPOINT
            //   fc=12k Q=1 sec 1:  solve_w_third=3.119 (0.993π) → ALT-PATH
            //   fc=20k Q=1 sec 1:  solve_w_third=5.199 (clamp π) → ALT-PATH
            // For midpoint case, w_zero gets clamped to 2.9845130209103035
            // (constant param_1[0xf] set inside compute_band_shelf, used by
            // prepare_band's post-clamp). This matters when solve_w_third
            // is close to π.
            const W_ZERO_CLAMP: f64 = 2.9845130209103035;
            if solve_w_third > 0.98 * PI {
                alt_path()
            } else {
                let w_zero_clamped = solve_w_third.min(W_ZERO_CLAMP);
                let midpoint = ((solve_w_pole + w_zero_clamped) * 0.5).min(solve_w_pole * 1.01);
                (solve_w_pole, w_zero_clamped, midpoint)
            }
        }
        (1, _) => alt_path(),
        _ => return PASSTHROUGH,
    };

    let w_pole = w_pole.min(PI - 0.01);
    let w_zero = w_zero.min(PI - 0.01);
    let w_third = w_third.min(PI - 0.01);

    notch_alt_path_kernel(omega0, a1_sec, a2_sec, g_ref, w_pole, w_zero, w_third)
}
/// Pro-Q 4 Notch s=2 synthesis — decoded alt-path inside lagrange-MZT
/// (`compute_audio_biquad_lagrange_mzt @ 0x1801103c0`, branch at 0x180110593).
///
/// Trigger: byte[0x49]=1 AND |u_pole| < 1e-10. For Notch this fires
/// because the analog form (s²+1)/(s²+α·s+1) has |H(jω₀)|² = 0 exactly.
///
/// Pipeline:
///   1. Sub-frequencies from compute_band_shelf_parameters @ 0x18010d780.
///   2. Alt-path computes p3, p4, sp5, sp6 via different formulas that
///      avoid the u_pole=0 singularity.
///   3. Mode-0 ASM emits the biquad.
fn notch_s2_alt_path_synth(freq_hz: f64, q_user: f64, sample_rate: f64) -> Coeffs {
    let alpha = std::f64::consts::SQRT_2 / q_user;
    let omega_d = (2.0 * PI * freq_hz / sample_rate).min(PI - 1e-9);

    // Sub-frequencies from compute_band_shelf_parameters bVar1 path.
    let w_pole = omega_d;
    let omega_over_pi = w_pole / PI;
    let beta = (omega_over_pi - 0.8).clamp(0.0, 0.2);
    let b1p_f32 = ((beta * beta * 25.0) as f32) as f64;
    let a_shelf = (0.5_f64.powf(alpha * 0.5)).max(0.01);
    let sqrt_t = (omega_over_pi.sqrt() as f32) as f64;
    let s_blend = (a_shelf - 0.99) * sqrt_t + 0.99;
    let mut w_zero = s_blend.sqrt() * w_pole * (1.0 - b1p_f32 * 0.05);
    let w_third = (1.0 - b1p_f32 * 0.2) * w_zero * s_blend;
    if w_pole < 0.0314_f64 {
        w_zero = (2.0 * w_pole).min(PI - 0.01);
    }
    let w_zero = w_zero.min(PI - 0.01);
    let w_third = w_third.min(PI - 0.01);

    // Compute |H_proto|² at sub-frequencies. Analog form: (s²+1)/(s²+α·s+1)
    // squared magnitude polynomial: num = (1-x)², den = (1-x)² + α²·x where x = ω².
    // Normalized to corner ω₀: substitute ω → ω/ω₀ → x = ω²/ω₀².
    // Pro-Q's evaluator scales by ω₀² so num = (ω²-ω₀²)², den = (ω²-ω₀²)² + α²·ω²·ω₀².
    let g = omega_d;
    let g2 = g * g;
    let h_sq = |w: f64| -> f64 {
        let w2 = w * w;
        let num = (w2 - g2) * (w2 - g2);
        let den = num + alpha * alpha * w2 * g2;
        if den.abs() > 1e-300 {
            (num / den).max(0.0)
        } else {
            0.0
        }
    };
    let u_zero = h_sq(w_zero);
    let u_third = h_sq(w_third);
    let u_eval = h_sq(PI);

    // Pre-warped tan values
    let t1 = (w_pole * 0.5).tan();
    let t2 = (w_zero * 0.5).tan();
    let t3 = (w_third * 0.5).tan();
    let t1s = t1 * t1;
    let t2s = t2 * t2;
    let t3s = t3 * t3;

    let g_ref = 1.0_f64;
    let p2 = g_ref.sqrt();

    // Alt-path discriminant
    let term1 = t1s * t1s * (t2s - t3s) * u_third * u_zero;
    let term2 = g_ref * u_zero * t2s * (t1s - t3s).powi(2);
    let term3 = g_ref * u_third * t3s * (t1s - t2s).powi(2);
    let discr = term1 - term2 + term3;
    let sqrt_discr = discr.abs().sqrt();

    // Alt-path p3
    let p3_alt = if sqrt_discr > 1e-300 {
        let inner = (t2s - t3s) * t2s * u_third * u_zero;
        let p3_numer = inner.max(0.0).sqrt() * p2 * t3;
        let candidate = p3_numer / sqrt_discr;
        let cap = 2.0 * u_eval.sqrt();
        candidate.min(cap)
    } else {
        2.0 * u_eval.sqrt()
    };

    // Alt-path p4 = t1²·p3/p2
    let p4_alt = if p2.abs() > 1e-300 {
        t1s * p3_alt / p2
    } else {
        0.0
    };

    // Alt-path sp5
    let denom_sp5 = t2s * u_zero;
    let sp5_alt = if denom_sp5 > 1e-300 {
        let term_a = ((t2s - t1s) * p3_alt).powi(2);
        let inner_b = t2s - t1s * p3_alt / p2;
        let term_b = inner_b * inner_b * u_zero;
        ((term_a - term_b) / denom_sp5).abs()
    } else {
        0.0
    };

    let sp6_alt = 0.0_f64;

    // Mode-0 ASM closed form (with internal sqrts)
    let p3 = p3_alt;
    let p4 = p4_alt;
    let sp5 = sp5_alt;
    let sp6 = sp6_alt;
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
/// Pro-Q 4 Notch slope-2 — prewarped bilinear transform.
///
/// Analog prototype: H(s) = (s² + ω₀²)/(s² + α·ω₀·s + ω₀²) with α = √2/Q.
/// Discretized via prewarped BLT s ← (1/t)·(z−1)/(z+1) where t = tan(π·fc/sr).
///
/// Verified against PROBE_HOOK_AUDIO_BIQUAD captures: matches Pro-Q 4
/// to ≤5e-3 across (fc, Q) grid; near-exact at low fc, small drift at
/// high fc (likely a slight pole-prewarp variant — TBD).
pub fn notch_s2_proq4(freq_hz: f64, q: f64, sample_rate: f64) -> Coeffs {
    use std::f64::consts::SQRT_2;
    let q_user = q.max(1e-6);
    let alpha = SQRT_2 / q_user;
    let t = (PI * freq_hz / sample_rate).tan();
    let t2 = t * t;
    let alpha_t = alpha * t;
    let da0 = 1.0 + alpha_t + t2;
    let inv_d = 1.0 / da0;
    let nb0 = 1.0 + t2;
    let nb1 = -2.0 + 2.0 * t2;
    let da1 = -2.0 + 2.0 * t2;
    let da2 = 1.0 - alpha_t + t2;
    [
        1.0,
        da1 * inv_d,
        da2 * inv_d,
        nb0 * inv_d,
        nb1 * inv_d,
        nb0 * inv_d,
    ]
}
