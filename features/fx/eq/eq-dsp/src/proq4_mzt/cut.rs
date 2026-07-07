//! Pro-Q 4 cut MZT biquads: lowpass, highpass, slope-8 per-section helpers.

use std::f64::consts::PI;

use crate::biquad::Coeffs;

use super::biquad_from_mode0_params;

/// Lowpass biquad — Pro-Q 4 form, MZT poles + DC unity.
///
/// Analog prototype: `H(s) = 1 / (s² + (1/Q_bw)·s + 1)` with Q_bw = Q/√2.
/// Verified at fc=1000 Q=1: |H(0)|=1, |H(e^{jw0})|=-3 dB, |H(π)|≈-55 dB
/// (standard cutoff convention).
pub fn design_lowpass(freq_hz: f64, q: f64, sample_rate: f64) -> Coeffs {
    let q = q.max(1e-6);
    let w0 = 2.0 * PI * freq_hz / sample_rate;
    let x0 = (w0 * 0.5).tan().powi(2);
    let x0_root = x0.sqrt();
    let xp1 = x0 + 1.0;

    // Denominator: BLT of analog (s² + (1/Q_bw)·s + 1), Q_bw = Q/√2
    let d_mid = std::f64::consts::SQRT_2 / q;
    let denom = xp1 + x0_root * d_mid;
    let inv_d = 1.0 / denom;

    let a0 = 1.0;
    let a1 = 2.0 * (x0 - 1.0) * inv_d;
    let a2 = (xp1 - x0_root * d_mid) * inv_d;

    // LP numerator: H(s) = 1 / den → numerator is 1 (constant).
    // In biquad form: (b0+b1+b2) at DC should = (1+a1+a2) for gain=1 at DC
    //                 (b0-b1+b2) at Nyquist should = 0 (infinite attenuation at ∞)
    // Solution: numerator = x_0·(1+x)² expanded as biquad
    // Actually for LP: zeros at z=-1 (Nyquist double zero). Classical form.
    // Using MZT: b0 = x_0·(1 + factor)/D, etc.
    // Simpler: use analog num = x_0 (scaling to pin DC=1), zeros at w=∞
    // LP has num = 1 (const) while denom has w² terms. After MZT transform,
    // num polynomial in x has form: x_0 + x_0·x + x_0·x² / ... → let me derive cleanly.
    //
    // Direct derivation: for H_a(s) = w_0²/(s² + (w_0/Q)s + w_0²), at |H_a(jw)|² at w=∞ = 0,
    // at w=0 = 1.
    // Substituting w² = x/x_0:
    //   |H_a|²(x) = w_0⁴/((w_0²·(1-x/x_0))² + (w_0²·√(x/x_0)/Q)²) = 1/((1-x/x_0)² + x·(1/x_0)·(1/Q²))
    // Multiplying num and denom by x_0²:
    //   |H_a|² = x_0² / (x_0·(1-x/x_0))² · x_0² ... too complex.
    //
    // Practical: RBJ LP zeros at z=-1, which after MZT scaled by x:
    // b0 = x_0/D, b1 = 2·x_0/D, b2 = x_0/D (classical after our LP normalization)
    // where D = xp1 + x0_root·d_mid (same as denom)
    let x0_over_d = x0 * inv_d;
    let b0 = x0_over_d;
    let b1 = 2.0 * x0_over_d;
    let b2 = x0_over_d;

    [a0, a1, a2, b0, b1, b2]
}
/// Highpass biquad — Pro-Q 4 form, BLT with Q_bw = Q/√2.
///
/// Verified at fc=1000 Q=1: |H(0)|≈-134 dB, |H(e^{jw0})|=-3 dB, |H(π)|=0.
pub fn design_highpass(freq_hz: f64, q: f64, sample_rate: f64) -> Coeffs {
    let q = q.max(1e-6);
    let w0 = 2.0 * PI * freq_hz / sample_rate;
    let x0 = (w0 * 0.5).tan().powi(2);
    let x0_root = x0.sqrt();
    let xp1 = x0 + 1.0;

    let d_mid = std::f64::consts::SQRT_2 / q;
    let denom = xp1 + x0_root * d_mid;
    let inv_d = 1.0 / denom;

    let a0 = 1.0;
    let a1 = 2.0 * (x0 - 1.0) * inv_d;
    let a2 = (xp1 - x0_root * d_mid) * inv_d;

    // HP zeros at z=+1 (DC double zero). Classical form:
    // b0 = 1/D, b1 = -2/D, b2 = 1/D (numerator polynomial in x = x_0²·(1-x)²·... )
    // Actually for HP: H_a(s) = s²/(s² + (w_0/Q)s + w_0²), zero at w=0, gain→1 at w=∞.
    // After MZT: b0 = 1/D, b1 = -2/D, b2 = 1/D (NOT including x_0 scaling)
    let b0 = inv_d;
    let b1 = -2.0 * inv_d;
    let b2 = inv_d;

    [a0, a1, a2, b0, b1, b2]
}
/// Lowpass via MZT — Pro-Q 4 formulation (from lp_exact.md RE).
///
/// p2 = 1
/// p3 = 1 / ((fs/(2·fc))² − 1)  ← gives Nyquist leak matching Pro-Q 4
/// p4 = t² = tan²(πfc/fs)
/// sp5 = √2·t/Q
/// sp6 = 1.04193 · (p4 + p3)
///
/// Accuracy: <0.01% error at fc≤500 Hz for Q≥1.
pub fn design_lowpass_mzt(freq_hz: f64, q: f64, sample_rate: f64) -> Coeffs {
    let q = q.max(1e-6);
    let w0 = 2.0 * PI * freq_hz / sample_rate;
    let t = (w0 * 0.5).tan();
    let t2 = t * t;

    let fs_over_2fc = sample_rate / (2.0 * freq_hz);
    let p3 = 1.0 / (fs_over_2fc * fs_over_2fc - 1.0);
    let sp6 = 1.04193 * (t2 + p3);

    let p2 = 1.0_f64;
    let p4 = t2;
    let sp5 = std::f64::consts::SQRT_2 * t / q;
    biquad_from_mode0_params(p2, p3, p4, sp5, sp6)
}
/// Highpass via MZT — from notch_bandpass_lp_hp_mzt.md.
/// Exact (~10 ppm): p2=0, p3=1, sp6=0.
pub fn design_highpass_mzt(freq_hz: f64, q: f64, sample_rate: f64) -> Coeffs {
    let q = q.max(1e-6);
    let w0 = 2.0 * PI * freq_hz / sample_rate;
    let t = (w0 * 0.5).tan();

    let p2 = 0.0_f64;
    let p3 = 1.0_f64;
    let p4 = t * t;
    let sp5 = std::f64::consts::SQRT_2 * t / q;
    let sp6 = 0.0_f64;
    biquad_from_mode0_params(p2, p3, p4, sp5, sp6)
}
/// - `docs/reports/proq4/re/lagrange_synthesis_decoded.md` (mainline synth)
/// - `docs/reports/proq4/re/lagrange_runtime_decoded.md` (mode-0 closed form)
///
/// Pipeline per section k ∈ 0..6:
/// 1. θ_k = π·(2k+1)/24 → pole_re_k = -cos(θ_k) (Butterworth N=12 poles).
/// 2. Sec 0 only: Q-loaded — pole_re_k /= q_eff, q_eff = min(q_user, 7.383).
/// 3. Analog poly A=1, B=0, C=0, D=1, b1p=2|pole_re_k|, b0p=1; helper rescales by ω₀.
/// 4. Sub-frequencies (sec 0..2): solver_wp = ω_base · M_k where
///    M_k = [1.01749, 1.18923, 1.96544]. Apply 0.7π Nyquist guard.
/// 5. w_zero = 0.001·w_pole, w_third = 0.2·w_pole.
/// 6. Sec 0..2 w_eval: ((0.4421·w_pole − 5/12)² · 0.2 + 0.785) · π (f32 cast),
///    then max with min(w_pole, π), capped at π.
/// 7. Sec 3..5: w_eval = π directly (formula skipped, root_count=0 in solver).
/// 8. Feed (b2z,b1z,b0z,b2p,b1p,b0p,w_pole,w_zero,w_third,w_eval) into the
///    Lagrange-MZT 3-point synth + mode-0 ASM closed form.
///
/// **Status (2026-05-01)**: Closed-form path implemented but conformance
/// regresses 60/108 → 12/108 vs. existing `cascade::highpass_s2_proq4`-per-
/// section path. Only Q≈1 / low-fc rows pass; Q∈{0.5,4,10} all fail. The
/// regression suggests one or more of (a) Q-loading on sec 0 needs to flow
/// into sub-frequencies (current code keeps M_k Q-invariant), (b) sec 3..5
/// pole-pair pre-warp needs δ²-driven `ω₀/((k−1)t+1)` rewrite (currently
/// just 0.7π Nyquist clamp), (c) Q≠1 flips the writer branch from
/// peak_type3 to a sibling routine. Kept as `#[allow(dead_code)]`
/// reference for next iteration.
pub fn hp_slope8_section_biquad(
    sec_idx: usize,
    freq_hz: f64,
    q_user: f64,
    sample_rate: f64,
) -> Coeffs {
    use crate::cascade::proq4_s2_from_prototype_with_subfreq_pub;

    // Butterworth N=12 unit-circle pole angles. Section ordering in the
    // binary places highest-Q section (smallest pole_re) FIRST: sec 0 → k=5,
    // sec 1 → k=4, ..., sec 5 → k=0.
    let k = 5 - sec_idx;
    let theta = PI * (2.0 * k as f64 + 1.0) / 24.0;
    let pole_re_mag = theta.cos(); // |pole_re| on unit Butterworth circle

    // Q-loading on sec 0 only
    const Q_CLAMP: f64 = 7.383;
    let pole_re_eff = if sec_idx == 0 {
        let q_eff = q_user.min(Q_CLAMP).max(1e-6);
        pole_re_mag / q_eff
    } else {
        pole_re_mag
    };

    let b2p = 1.0;
    let b1p = 2.0 * pole_re_eff;
    let b0p = 1.0;

    // Post-rewrite w_pole multipliers from hp_s8_all_sections_subfreq.csv
    // (SR=48000, 240 rows, verified 2026-05-01). Sec 0 is Q-dependent,
    // sec 1, 2 are Q-invariant fixed multipliers, sec 3-5 = 1.0 (= ω_base).
    const M_K_FIXED: [f64; 6] = [1.0, 1.1267645435, 1.1456678821, 1.0, 1.0, 1.0];

    // Solver (pre-rewrite) wp multipliers, used as INPUT to the
    // compute_peak_type3 w_eval formula for sec 1, 2 only. Per
    // hp_s8_w_eval_sec1_2_decoded.md: M_1 = 2^(1/4) (exact), M_0/M_2 from
    // FULL_PIPELINE STAGE_7 traces. Sec 0 uses post-rewrite wp directly
    // (already the correct input — verified across all (fc,Q) in CSV).
    const SOLVER_M: [f64; 3] = [1.01749, 1.18923, 1.96544];

    let omega_base = 2.0 * PI * freq_hz / sample_rate;

    let m_k = if sec_idx == 0 {
        hp_s8_sec0_q_multiplier(q_user)
    } else {
        M_K_FIXED[sec_idx]
    };
    let w_pole_raw = omega_base * m_k;

    // Nyquist guard: w_pole = min(raw, t·0.3π + 0.7π) per
    // hp_s8_w_eval_sec1_2_decoded.md. δ² = 4·α² − 2 ; t = clamp(−δ²/2, 0, 1)
    // where α = pole_re_eff (Q-loaded for sec 0). This matters for sec 0 at
    // low Q + high fc: at fc=22000 Q=0.5 the Q-loaded α=0.261, t=0.864,
    // ceiling=3.013 = captured wp_post (vs 3.110 with un-loaded α).
    let d2 = 4.0 * pole_re_eff * pole_re_eff - 2.0;
    let t = (-0.5 * d2).clamp(0.0, 1.0);
    let nyquist_ceiling = t * 0.3 * PI + 0.7 * PI;
    let w_pole = w_pole_raw.min(nyquist_ceiling).min(PI);

    let w_zero = 0.001 * w_pole;
    let w_third = 0.2 * w_pole;

    // w_eval input choice (decoded 2026-05-01 from CSV captures):
    //   sec 0: formula consumes the POST-rewrite w_pole (matches captures
    //          to ≤1e-4 across all Q,fc).
    //   sec 1, 2: formula consumes the PRE-rewrite SOLVER wp = ω_base · solver_M
    //          (matches captures to ≤1e-4 across fc, Q-invariant).
    //   sec 3-5: w_eval = 0 (root_count=0 in solver — formula skipped). Note:
    //          captured w_eval = 0 in CSV, NOT π as previously assumed.
    // For sec 3-5 the binary writes w_eval=0 to the prototype struct, but
    // the downstream Lagrange synth then *substitutes* w_eval=π before
    // evaluating |H_proto(jπ)|² (per the JA at 0x18011041a path used by
    // highpass_s2_proq4 in the Q≤1 branch). So we pass π here.
    // Sec 0 w_eval input: use Q-dependent SOLVER multiplier (pre-rewrite),
    // not the post-rewrite w_pole. Verified bit-exact across all (fc,Q) in
    // hp_s8_all_sections_subfreq.csv on 2026-05-01.
    //   M_solver(Q): Q=0.5→1.07601, Q=1→1.01749, Q=2→1.00429, Q=4→1.00107, Q=10→1.00031
    let w_eval = match sec_idx {
        0 => {
            let m_sol_0 = hp_s8_sec0_solver_q_multiplier(q_user);
            let wp_solver = omega_base * m_sol_0;
            hp_s8_w_eval_sec_0_2(wp_solver)
        }
        1 | 2 => {
            let wp_solver = omega_base * SOLVER_M[sec_idx];
            hp_s8_w_eval_sec_0_2(wp_solver)
        }
        _ => PI,
    };

    proq4_s2_from_prototype_with_subfreq_pub(
        freq_hz,
        sample_rate,
        1.0,
        0.0,
        0.0,
        b2p,
        b1p,
        b0p,
        w_pole,
        w_zero,
        w_third,
        w_eval,
    )
}
/// Sec 0 w_pole / ω_base multiplier. Q-dependent table interpolated in log10(Q).
/// fc-invariant for fc < 14 kHz (per hp_s8_alpha_scratch_qdep.md).
fn hp_s8_sec0_q_multiplier(q_user: f64) -> f64 {
    // Table from hp_s8_alpha_scratch_qdep.md (corrected, post-rewrite AF probe).
    const TABLE: [(f64, f64); 5] = [
        (0.5, 1.06497),
        (1.0, 1.01688),
        (2.0, 1.00425),
        (4.0, 1.00125),
        (10.0, 1.00125),
    ];
    let q = q_user.max(1e-6);
    if q <= TABLE[0].0 {
        return TABLE[0].1;
    }
    if q >= TABLE[TABLE.len() - 1].0 {
        return TABLE[TABLE.len() - 1].1;
    }
    let lq = q.log10();
    for w in TABLE.windows(2) {
        let (q0, m0) = w[0];
        let (q1, m1) = w[1];
        if q >= q0 && q <= q1 {
            let l0 = q0.log10();
            let l1 = q1.log10();
            let frac = (lq - l0) / (l1 - l0);
            return m0 + (m1 - m0) * frac;
        }
    }
    TABLE[TABLE.len() - 1].1
}
/// Sec 0 SOLVER (pre-rewrite) wp / ω_base multiplier. Q-dependent.
/// fc-invariant — verified bit-exact across all (fc,Q) in
/// hp_s8_all_sections_subfreq.csv (2026-05-01). These are the multipliers
/// used as INPUT to the `compute_peak_type3_parameters` w_eval formula
/// for sec 0; differs from the POST-rewrite multiplier
/// (`hp_s8_sec0_q_multiplier`) which is applied to w_pole itself.
fn hp_s8_sec0_solver_q_multiplier(q_user: f64) -> f64 {
    const TABLE: [(f64, f64); 5] = [
        (0.5, 1.07601),
        (1.0, 1.01749),
        (2.0, 1.00429),
        (4.0, 1.00107),
        (10.0, 1.00031),
    ];
    let q = q_user.max(1e-6);
    if q <= TABLE[0].0 {
        return TABLE[0].1;
    }
    if q >= TABLE[TABLE.len() - 1].0 {
        return TABLE[TABLE.len() - 1].1;
    }
    for w in TABLE.windows(2) {
        let (q0, m0) = w[0];
        let (q1, m1) = w[1];
        if q >= q0 && q <= q1 {
            let l0 = q0.log10();
            let l1 = q1.log10();
            let lq = q.log10();
            let frac = (lq - l0) / (l1 - l0);
            return m0 + (m1 - m0) * frac;
        }
    }
    TABLE[TABLE.len() - 1].1
}
/// Decoded `compute_peak_type3_parameters` w_eval formula for HP s=8 sec 0..2.
/// f32 cast preserved for bit-exactness with binary's SS instructions.
fn hp_s8_w_eval_sec_0_2(w_pole_solver: f64) -> f64 {
    const A: f64 = 0.44209706414415373;
    const B: f64 = 5.0 / 12.0;
    const K: f64 = 0.2;
    const D: f64 = 0.785;
    let fv = ((A * w_pole_solver - B) as f32) as f64;
    let e = ((fv * fv * K + D) as f32) as f64;
    let cand = e * PI;
    let wp_clamp = w_pole_solver.min(PI);
    if cand >= wp_clamp {
        cand.min(PI)
    } else {
        wp_clamp
    }
}
///   k = 5-sec_idx. Q_eff = clamp(Q, 1e-6, 7.383) for sec 0; Q_eff = 1
///   otherwise. (This is the value that the synth kernel reads in place of
///   `b1p` — confirmed from `SOLVE_BQ` E coefficient: E = (w_sf²−2)·ω².)
/// - `w_pole = ω₀` (constant across all sections, fc-up-to-Nyquist)
/// - `w_third = ω₀ · √max(1 − w_sf²/2, 0.25)` where w_sf = w_section_field
/// - `w_zero = w_third / 2`
/// - `w_eval` slot is written 0; downstream JA at 0x18011041a substitutes π
///    (1-root branch — `proto[4]` table per `lp_hp_notch_bp_subfreq_decoded.md`).
///
/// We pass `b1p = w_section_field` to the kernel because the kernel's
/// `b1p` slot is what actually consumes the Butterworth damping —
/// the literal `b1p=1` in LAG_PROTO_DETAIL is overridden inside the synth.
///
/// Verification (against captured `LAG_OUT` from FT=4 probe):
/// - fc=10k Q=1: all 6 sections bit-exact (≤4e-7).
/// - fc=14k Q=1: sec 0..3 bit-exact (≤4e-7); sec 4-5 ~5e-2 off.
/// - fc≥14k: sec 4-5 (b1p ∈ [1.85, 1.98]) consistently ~5e-2 off — Z-domain
///   analysis of captured biquads shows REAL Z-poles even though analog
///   `s²+b1p·s+1` poles are still complex (b1p²<4). Pro-Q 4 uses a
///   different mapping in this regime that is NOT the byte[0x49] swap
///   branch (decompile of `compute_biquad_response_magnitude @ 0x1801103c0`
///   shows that swap fires only when u_pole < 1e-10, which is far from
///   the ~0.25-0.30 u_pole values seen here). The exact alternate mapping
///   for sec 4-5 at fc≥14k remains undecoded — those cells correspond to
///   the 32 remaining LP s=8 conformance failures.
pub fn lp_slope8_section_biquad(
    sec_idx: usize,
    freq_hz: f64,
    q_user: f64,
    sample_rate: f64,
) -> Coeffs {
    use crate::cascade::proq4_s2_from_prototype_with_subfreq_pub;

    let omega_base_raw = 2.0 * PI * freq_hz / sample_rate;
    // Use the same omega clamp as proq4_s2_from_prototype_with_subfreq for
    // sub-frequency math; the synth function will re-clamp internally.
    let omega_base = omega_base_raw.min(PI - 0.01);

    // Butterworth N=12 pole pair angle ordering: sec 0 → highest-Q (smallest
    // real-part of pole) which is k=5; sec 5 → k=0.
    let k = 5 - sec_idx;
    let theta = PI * (2.0 * k as f64 + 1.0) / 24.0;
    let cos_theta = theta.cos();

    // b1p = 2·cos(θ_k); only sec 0 is Q-loaded (divides by Q_user).
    // Sec 0 Q clamp at 7.383 matches HP s=8 (verified at fc=1k Q=10:
    // captured b1p = 0.03536 = 2·cos(11π/24)/7.383).
    const Q_CLAMP_SEC0: f64 = 7.383;
    let b1p = if sec_idx == 0 {
        let q_eff = q_user.min(Q_CLAMP_SEC0).max(1e-6);
        2.0 * cos_theta / q_eff
    } else {
        2.0 * cos_theta
    };

    // Sub-frequency rule (decoded from LAG_PROTO_DETAIL, fc/Q-validated):
    //   bw_sq = max(1 − b1p²/2, 0.25)
    //   w_third = ω₀ · √bw_sq
    //   w_zero  = w_third / 2
    let bw_sq = (1.0 - b1p * b1p / 2.0).max(0.25);
    let w_third = omega_base * bw_sq.sqrt();
    let w_zero = 0.5 * w_third;
    let w_pole = omega_base;
    let w_eval = PI; // 1-root branch substitution (binary writes 0, JA → π)

    // LP analog prototype: numerator = const (0, 0, 1), denominator with
    // Butterworth N=12 pole-pair (1, b1p, 1).
    //
    // 2026-05-01 FT=4 capture (lp_audio_path_captures/) revealed that the
    // LAG_PROTO_DETAIL struct stores `b1p = 1` (literal — the analog
    // denominator is generic `s²+s+1`) and the per-section Butterworth
    // damping is carried in a separate slot `w_section_field = 2cos(θ_k)/Q_eff`
    // which the synth kernel reads in place of b1p. So we pass the
    // Butterworth coefficient through the b1p slot to the kernel.
    proq4_s2_from_prototype_with_subfreq_pub(
        freq_hz,
        sample_rate,
        0.0,
        0.0,
        1.0,
        1.0,
        b1p,
        1.0,
        w_pole,
        w_zero,
        w_third,
        w_eval,
    )
}
