//! Second-order (slope 2) section builders — bell, low-pass, high-pass.
//!
//! The single-biquad cases that every steeper slope is ultimately assembled
//! from, so a fault here shows up in all of them at once.

use super::{Coeffs, PI, PASSTHROUGH};

#[must_use]
pub fn bell_s2_proq4(freq_hz: f64, q: f64, gain_db: f64, sample_rate: f64) -> Coeffs {
    use std::f64::consts::SQRT_2;

    const Q_CORR_C: f64 = -1.350_719_92e-5;
    const W_POLE_MAX: f64 = 3.078_760_800_517_997;
    const W_ZERO_MAX: f64 = 2.827_433_388_230_814; // 0.9·π
    const W_THIRD_MAX: f64 = 3.063_366_996_515_407_3;

    let g_lin = 10.0_f64.powf(gain_db / 20.0);
    let big_a = g_lin.sqrt();

    let omega0_raw = 2.0 * PI * freq_hz / sample_rate;
    let omega0 = omega0_raw.min(PI - 0.01);

    // ── Low-fc fallback path (decoded but NOT applied) ──
    // Capture analysis (`docs/reports/proq4/re/low_fc_audio_biquad.csv`,
    // 60 rows at fc ∈ {10..120} Hz × Q ∈ {0.5,1,4} × g ∈ {±6,±12}) shows
    // that for omega0 < ~0.016 (fc < ~125 Hz @ 48k), Pro-Q 4 bypasses
    // `compute_audio_biquad_lagrange_mzt` and the audio dispatcher uses a
    // simpler per-section emitter at 0x1800fcdb0 that reads the band's
    // already-stored ZPK and applies a standard prewarped bilinear with
    // **Q_eff = Q/√2**.  Verified to ≤ 1e-6 max error at fc=10 across all
    // (Q, g) against the captured (display-mode) AUDIO_BIQUAD output.
    //
    // **Update (2026-04-30, `runtime_correction_decoded.md`):** the
    // earlier "runtime correction at 0x1800fcdb0" hypothesis was
    // **wrong**.  Ghidra decompilation shows 0x1800fcdb0 is
    // `compute_zpk_section_response` — a single-frequency ZPK
    // magnitude evaluator on the display path, not a per-block audio
    // coefficient transformer (only call site is the `else` branch of
    // `update_band_audio_or_display_biquads`, gated on display mode).
    // Synthesizing the IR analytically from the AUDIO_BIQUAD-captured
    // coefficients matches the plugin's actual `process()` IR to ≤1e-10
    // abs error — the runtime audio uses exactly the AUDIO_BIQUAD
    // coefficients, no further per-block transformation.
    //
    // The 49 low-fc Bell s=2 conformance failures are caused by
    // `regen_bell_refs.py` baking probe.exe's numerically-degenerate
    // N_IR=32 LS-fit into the reference CSVs (LS converges only for
    // N_IR≥256).  Our cascade matches the **real** plugin biquad to
    // 5+ decimals; the references encode LS noise.  Fix: regenerate
    // references using the AUDIO_BIQUAD hook output (mode=1) instead
    // of IR-LS.  No DSP change needed.

    let q_user = q.max(1e-6);
    let q_corr = if (q_user - 1.0).abs() < 1e-12 {
        q_user
    } else {
        Q_CORR_C.mul_add(q_user.ln(), 1.0) * q_user
    };

    let g_om = omega0;
    let g_om2 = g_om * g_om;
    let g_om4 = g_om2 * g_om2;
    let b1z = SQRT_2 * big_a / q_corr;
    let b1p = SQRT_2 / (big_a * q_corr);
    let cap_a = 1.0;
    let cap_b = (b1z * b1z - 2.0) * g_om2;
    let cap_c = g_om4;
    let cap_d = 1.0;
    let cap_e = (b1p * b1p - 2.0) * g_om2;
    let cap_f = g_om4;
    let g_ref = cap_c / cap_f;

    // High-fc δ correction decoded from `highfc_highq_sweep.csv` (90 rows,
    // fc∈{19k..23k} × Q∈{1..10}).  When ω₀ exceeds 0.8π Pro-Q 4 adds a
    // Q-independent extra to the base `1/Q` before the clamp.  Empirical
    // fit (residual ≤ 4e-6, the probe noise floor):
    //
    //   δ_eff = clamp(1/Q + 1.604204·max(0, ω₀ − 0.8π)⁴, 0.1, 0.8)
    //
    // This δ_eff feeds BOTH `w_zero = ω₀·(1 − δ)` and `w_third = ω₀·(1 − δ/20)`.
    let delta_extra = {
        let excess = 0.8f64.mul_add(-PI, omega0).max(0.0);
        1.604_204 * excess * excess * excess * excess
    };
    let delta = (1.0 / q_user + delta_extra).clamp(0.1, 0.8);
    // Sub-frequency upper-bound clamps decoded from `prepare_band_display_info`
    // (Pro-Q 4 binary @ 0x18010c8a0, end of function). Three independent caps
    // are applied to the values that flow into the Lagrange evaluator:
    //   param_1[0xe]  = 3.0788  → w_pole  ≤ 3.0788  (≈ 0.9799·π)
    //   param_1[0xf]  = 2.8274  → w_zero  ≤ 0.9·π
    //   param_1[0x10] = 3.0634  → w_third ≤ 3.0634  (≈ 0.9750·π)
    // Near Nyquist these prevent tan(w/2) from approaching infinity and
    // reduce numerical error in the synthesis. They are no-ops at low/mid fc.
    let w_pole = omega0.min(W_POLE_MAX);
    let w_zero = (omega0 * (1.0 - delta)).min(W_ZERO_MAX);
    let w_third = (omega0 * (1.0 - delta / 20.0)).min(W_THIRD_MAX);
    // w_eval rule decoded from extreme-fc oracle (lagrange_proto_extreme.csv):
    // - Default w_eval = 0.9π
    // - Once 1.2·ω₀ exceeds 0.9π (i.e. fc > 0.375·sr), w_eval rides 1.2·ω₀,
    //   clamped to π at Nyquist.
    // Equivalent: w_eval = clamp(1.2·ω₀, 0.9π, π).
    let w_eval = (1.2 * omega0).clamp(0.9 * PI, PI);

    let h_sq = |w: f64| -> f64 {
        let w2 = w * w;
        let w4 = w2 * w2;
        (cap_a * w4 + cap_b * w2 + cap_c) / (cap_d * w4 + cap_e * w2 + cap_f)
    };
    let u_pole = h_sq(w_pole);
    let u_zero = h_sq(w_zero);
    let u_third = h_sq(w_third);
    let u_eval = h_sq(w_eval);

    let p3 = u_eval.max(0.0).sqrt();
    let p2 = g_ref.max(0.0).sqrt();

    let t1 = (w_pole * 0.5).tan();
    let t2 = (w_zero * 0.5).tan();
    let t3 = (w_third * 0.5).tan();
    let t1s = t1 * t1;
    let t2s = t2 * t2;
    let t3s = t3 * t3;

    let den = t3s
        * (((u_zero - u_third) * (g_ref - u_pole)).mul_add(t2s, -((u_pole - u_third) * (g_ref - u_zero) * t1s)))
        + (g_ref - u_third) * (u_pole - u_zero) * t1s * t2s;
    let num = u_pole * ((t3s - t1s).mul_add(u_third, (t2s - t3s).mul_add(u_eval, (t1s - t2s) * u_zero)))
        + u_eval * ((t3s - t1s).mul_add(u_zero, (t1s - t2s) * u_third))
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
        let sp6_num = (a1_term * a1_term * t2s).mul_add(u_zero, -((t1s * t2s * (1.0 - s2 * t3s) * (t1s - t2s)).mul_add(u_zero, a2_term * a2_term * t1s) * u_pole));
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

/// Generic Pro-Q 4 audio-path Lagrange-MZT slope-2 synthesis.
///
/// Replicates `bell_s2_proq4` machinery exactly but takes the analog
/// prototype ZPK triples `(b2_z, b1_z, b0_z)` for the numerator and
/// `(b2_p, b1_p, b0_p)` for the denominator as inputs.  The frequency
/// scale is `g = ω₀ = 2π·fc/sr` (clamped near Nyquist), so the analog
/// prototype is effectively `b2·s² + b1·ω₀·s + b0·ω₀²`.
///
/// All sub-frequency / Lagrange / mode-0 ASM stages are identical to
/// the Bell path documented in `docs/reports/proq4/re/bell_s2_full_pipeline.md`.
///
/// **NOT YET WIRED** — see
/// `docs/reports/proq4/re/lp_audio_path_pipeline.md` (and friends).
/// Direct replication of Bell's sub-frequency selection regresses
/// LP/HP/Notch/BP conformance because Pro-Q 4 uses filter-type-specific
/// `w_zero` clamps (e.g. Notch caps at `0.95π` vs Bell's `0.9π`) and
/// the |H(jω₀)|²=0 case for Notch breaks the Bell-style Lagrange.
/// Custom sub-frequency override version.
#[doc(hidden)]
#[must_use]
pub fn proq4_s2_from_prototype_with_subfreq_pub(
    freq_hz: f64,
    sample_rate: f64,
    b2z: f64,
    b1z: f64,
    b0z: f64,
    b2p: f64,
    b1p: f64,
    b0p: f64,
    w_pole_in: f64,
    w_zero_in: f64,
    w_third_in: f64,
    w_eval_in: f64,
) -> Coeffs {
    proq4_s2_from_prototype_with_subfreq(
        freq_hz,
        sample_rate,
        b2z,
        b1z,
        b0z,
        b2p,
        b1p,
        b0p,
        w_pole_in,
        w_zero_in,
        w_third_in,
        w_eval_in,
    )
}

pub fn proq4_s2_from_prototype_with_subfreq(
    freq_hz: f64,
    sample_rate: f64,
    b2z: f64,
    b1z: f64,
    b0z: f64,
    b2p: f64,
    b1p: f64,
    b0p: f64,
    w_pole_in: f64,
    w_zero_in: f64,
    w_third_in: f64,
    w_eval_in: f64,
) -> Coeffs {
    const W_POLE_MAX: f64 = 3.078_760_800_517_997;
    const W_ZERO_MAX: f64 = 2.827_433_388_230_814;
    const W_THIRD_MAX: f64 = 3.063_366_996_515_407_3;

    let omega0_raw = 2.0 * PI * freq_hz / sample_rate;
    let omega0 = omega0_raw.min(PI - 0.01);

    let g_om = omega0;
    let g_om2 = g_om * g_om;
    let g_om4 = g_om2 * g_om2;
    let cap_a = b2z * b2z;
    let cap_b = b1z.mul_add(b1z, -(2.0 * b2z * b0z)) * g_om2;
    let cap_c = b0z * b0z * g_om4;
    let cap_d = b2p * b2p;
    let cap_e = b1p.mul_add(b1p, -(2.0 * b2p * b0p)) * g_om2;
    let cap_f = b0p * b0p * g_om4;
    let g_ref = if cap_f.abs() > 1e-300 {
        cap_c / cap_f
    } else {
        0.0
    };
    let w_pole = w_pole_in.min(W_POLE_MAX);
    let w_zero = w_zero_in.min(W_ZERO_MAX);
    let w_third = w_third_in.min(W_THIRD_MAX);
    let w_eval = if w_eval_in == 0.0 {
        PI
    } else {
        w_eval_in.clamp(0.0, PI)
    };

    let h_sq = |w: f64| -> f64 {
        let w2 = w * w;
        let w4 = w2 * w2;
        let num = cap_a.mul_add(w4, cap_b * w2) + cap_c;
        let den = cap_d.mul_add(w4, cap_e * w2) + cap_f;
        if den.abs() > 1e-300 {
            num / den
        } else {
            0.0
        }
    };
    let u_pole = h_sq(w_pole);
    let u_zero = h_sq(w_zero);
    let u_third = h_sq(w_third);
    let u_eval = h_sq(w_eval);

    let p3_main = u_eval.max(0.0).sqrt();
    let p2 = g_ref.max(0.0).sqrt();

    let t1 = (w_pole * 0.5).tan();
    let t2 = (w_zero * 0.5).tan();
    let t3 = (w_third * 0.5).tan();
    let t1s = t1 * t1;
    let t2s = t2 * t2;
    let t3s = t3 * t3;

    let den = t3s
        * (((u_zero - u_third) * (g_ref - u_pole)).mul_add(t2s, -((u_pole - u_third) * (g_ref - u_zero) * t1s)))
        + (g_ref - u_third) * (u_pole - u_zero) * t1s * t2s;
    let num = u_pole * ((t3s - t1s).mul_add(u_third, (t2s - t3s).mul_add(u_eval, (t1s - t2s) * u_zero)))
        + u_eval * ((t3s - t1s).mul_add(u_zero, (t1s - t2s) * u_third))
        + (t2s - t3s) * u_third * u_zero;

    // Pro-Q4 alt-path branch decoded from `compute_audio_biquad_lagrange_mzt`
    // 0x180110972..0x1801109ec.  When NUM/DEN < 0.0025/(t2*t3), the binary:
    //   (a) overrides s² to the threshold value 0.0025/(t2*t3); and
    //   (b) replaces p3 (originally √u_eval) with √max(N_alt/D_alt, 0)
    //       where
    //         N_alt = u_pole·[t1²(u_third−u_zero) + t2²·u_zero − t3²·u_third]
    //                + (t3²−t2²)·u_third·u_zero + 0.0025·DEN/(t2·t3)
    //         D_alt = (t2²−t3²)·u_pole + (t1²−t2²)·u_third + (t3²−t1²)·u_zero
    // This branch fires for LP near-Nyquist low-Q cases where the textbook
    // s² goes negative (NUM/DEN ≪ threshold), closing 35 LP failures while
    // leaving HP unchanged (HP ratios always exceed the threshold).
    let t2t3 = t2 * t3;
    let alt_threshold = if t2t3.abs() > 1e-30 {
        0.0025 / t2t3
    } else {
        f64::INFINITY
    };
    let raw_ratio = if den.abs() > 1e-30 {
        num / den
    } else {
        f64::NEG_INFINITY
    };
    let (s2, p3) = if raw_ratio < alt_threshold {
        let n_alt = u_pole * (t1s * (u_third - u_zero) + t2s * u_zero - t3s * u_third)
            + (t3s - t2s) * u_third * u_zero
            + 0.0025 * den / t2t3;
        let d_alt = (t3s - t1s).mul_add(u_zero, (t2s - t3s).mul_add(u_pole, (t1s - t2s) * u_third));
        let p3_alt = if d_alt.abs() > 1e-30 {
            (n_alt / d_alt).max(0.0).sqrt()
        } else {
            0.0
        };
        (alt_threshold.max(0.0), p3_alt)
    } else {
        (raw_ratio.max(0.0), p3_main)
    };
    let s_val = s2.sqrt();
    let p4 = s_val * t1 * t2 * t3;

    let a1_term = t1s * p3 - p4 * p2;
    let a2_term = t2s * p3 - p4 * p2;

    let sp6_den = (u_pole - u_zero) * t1s * t2s;
    let sp6 = if sp6_den.abs() > 1e-30 {
        let sp6_num = (a1_term * a1_term * t2s).mul_add(u_zero, -((t1s * t2s * (s2.mul_add(-t3s, 1.0)) * (t1s - t2s)).mul_add(u_zero, a2_term * a2_term * t1s) * u_pole));
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

/// Pro-Q 4 Lowpass slope-2 (audio-path Lagrange-MZT).
///
/// Sub-frequencies decoded from runtime probe captures (ft=1):
///   Q ≤ 1: `w_pole` = ω₀/2, `w_zero` = ω₀/10, `w_third` = ω₀·0.48
///   Q ≥ 2: `w_pole` shifts up; complex pattern (TBD)
///   `w_eval` = 0 at Q ≤ 1 (remapped to π−0.01), ≈ 2.45 at Q ≥ 2
#[must_use]
pub fn lowpass_s2_proq4(freq_hz: f64, q: f64, sample_rate: f64) -> Coeffs {
    use std::f64::consts::SQRT_2;
    let q_user = q.max(1e-6);
    // LP analog form: textbook Butterworth (b1_z=0, b0_z=1; b1_p=√2/Q, b0_p=1).
    // Per `solve_bq_lphpbpnotch.csv` HP rows + LP analog form: A = b2_z² = 0,
    // so AE-BD = -B·D = 0 (B=0 from b1_z=b0_z·b2_z=0).  Δ=0 → root_count=0
    // → solver fallback path → w_pole = ω₀ always (verified: captured LP
    // w_pole = ω₀ exactly at all 25 captured (fc, Q) points in lphp_subfreq_clean.csv).
    let alpha = SQRT_2 / q_user;
    let omega0 = (2.0 * PI * freq_hz / sample_rate).min(PI - 0.01);
    let w_pole = omega0;
    // w_zero, w_third bandwidth-related per LP captures:
    //   w_third = ω₀ · √max(1 - 1/Q², 0.25)
    //   w_zero  = w_third / 2
    // (factor 0.25 = floor at 0.5² when Q ≤ 1)
    let bw_sq = (1.0 - 1.0 / (q_user * q_user)).max(0.25);
    let w_third = omega0 * bw_sq.sqrt();
    let w_zero = 0.5 * w_third;
    // w_eval captured = 0 for LP across all (fc, Q); binary substitutes π
    // via the JA at 0x18011041a (per lp_hp_notch_bp_subfreq_decoded.md
    // proto[4] table). Earlier code used `π − 0.01` as a safety hedge but
    // that introduced ~0.05 dB residual at HF; using exact π closes
    // LP s=2 to 101/104 and LP s=8 (low-fc) to 76/108 (2026-05-01).
    let w_eval = PI;
    proq4_s2_from_prototype_with_subfreq(
        freq_hz,
        sample_rate,
        0.0,
        0.0,
        1.0,
        1.0,
        alpha,
        1.0,
        w_pole,
        w_zero,
        w_third,
        w_eval,
    )
}

#[must_use]
pub fn highpass_s2_proq4(freq_hz: f64, q: f64, sample_rate: f64) -> Coeffs {
    use std::f64::consts::SQRT_2;
    const W_POLE_HF_CLAMP: f64 = 0.7 * PI; // 2.199114857512855

    let q_user = q.max(1e-6);
    // HP analog form: textbook Butterworth (1, √2/Q, 1) — verified
    // bit-exact against solve_bq_lphpbpnotch.csv.
    let alpha = SQRT_2 / q_user;
    let omega0 = (2.0 * PI * freq_hz / sample_rate).min(PI - 0.01);
    // Sub-frequencies decoded from `lphp_subfreq_clean.csv` HP captures.
    // Solver yields w_pole = √(largest u-root) after the slot swap for
    // filter_type 2.  For HP analog (1,√2/Q,1):
    //   E = (b1_p² − 2)·g² = (2/Q² − 2)·g²
    //   F = g⁴
    //   Quadratic E·u² + 2F·u = 0 → u = 0 or u = -2F/E = g²·Q²/(Q²-1)
    //   w_pole_swapped = g·Q/√(Q²-1) for Q>1
    // Captured w_pole matches Q_pre² = Q²+1 substitution:
    //   w_pole = ω₀·√(1 + 1/Q²)
    //   bit-exact at Q=10, ≤25 ppm at Q=4, ~0.5% at Q=2.
    //
    // **Open: HF residual ≤ 0.015 dB** for fc ≥ 5 kHz Q≤1 and fc ≥ 16 kHz
    // for Q∈{1,4}.  Investigation 2026-05-01 (zz_dbg_hp probe) showed:
    //   - At fc=10 kHz Q=4, plugging captured (w_pole, w_zero, w_third,
    //     w_eval) into `proq4_s2_from_prototype_with_subfreq` reproduces
    //     the captured biquad to ≤6 ppm — i.e. the rational-fit code is
    //     correct.  The residual is in the *sub-frequency formulas*, not
    //     the synth.
    //   - For Q≤1 captures, w_eval=0 (1-root branch).  Setting w_eval=0
    //     here regressed conformance because the helper's `g_ref·u_third`
    //     terms then feed degenerate Lagrange weights — the Q≤1 branch in
    //     the binary likely emits via a *different* code path (NOT the
    //     2-root Lagrange synth used here) but the alt-path was not
    //     decoded in the available 60-min budget.
    //   - `lphp_subfreq_clean.csv` captures only go up to fc=10 kHz; HF
    //     extrapolation needs fresh runtime captures at fc∈{14k..22k}.
    //
    // Action items (next iteration):
    //   1. Capture HP sub-freqs at fc≥14 kHz to see if w_pole saturates or
    //      formula `ω₀·√(1+1/Q²)` continues to hold near Nyquist.
    //   2. RE the Q≤1 / 1-root code path inside `prepare_band_display_info`
    //      to determine the actual synthesis kernel (is it std BLT? a
    //      different Lagrange specialization?).
    // Improved empirical w_pole formula reduces Q=2 residual ~3x.
    // From `hp_q_prewarp_decoded.md`: ω₀·√(Q²/(Q²-1+1/Q²)).
    // Bit-exact at Q≥4, ~2e-3 at Q=2, fc-invariant.
    // High-fc clamp at 0.7π per `hp_high_fc_subfreq_analysis.md`:
    //   Q ≤ 1: w_pole = min(ω₀, 0.7π)
    //   Q > 1: same Q-prewarped formula, but w_eval freeze rule applies
    let w_pole = if q_user > 1.0 {
        let q2 = q_user * q_user;
        omega0 * (q2 / (q2 - 1.0 + 1.0 / q2)).sqrt()
    } else {
        omega0.min(W_POLE_HF_CLAMP)
    };
    // w_zero, w_third are constant ratios of w_pole per captures.
    let w_zero = 0.001 * w_pole;
    let w_third = 0.2 * w_pole;
    // w_eval: 2-root branch formula `(w_pole·0.4421 − 5/12)²·0.2 + 0.785)·π`.
    //
    // For Q ≤ 1 (1-root branch in binary), `w_eval = 0` is written to the
    // prototype struct, then the binary substitutes `w_eval = π` via the
    // `JA` at `0x18011041a` before evaluating `u_eval = |H_proto(jπ)|²`.
    // (Per `lp_hp_notch_bp_subfreq_decoded.md` proto[4] table.)  Earlier
    // ports passed `π − 0.01` here as a safety hedge — but mode-0 ASM
    // captures (`hp_mode01_capture.csv`, 2026-05-01) confirm the binary
    // uses **exactly π**: at fc=10 kHz Q=0.5 captured p3=0.6947 matches
    // √u(π,Q=0.5,ω₀)=0.6947 to <1e-5 absolute, while √u(π−0.01) yields
    // 0.6936 — a 1700 ppm error that fails conformance.
    // 2-root branch base formula `(0.4421·wp − 5/12)²·0.2π + 0.785π` matches
    // captures to ≤6e-4 for fc ≤ 14 kHz but underestimates by up to ~4e-3
    // for fc ≥ 16 kHz Q ≈ 4 (per `hp_high_fc_subfreq.csv`). Add a small
    // empirical correction `0.0396 · max(0, wp − 1.515) · q_term`, with
    // `q_term = clamp(1/Q² − 0.01, 0, 0.06)` chosen so:
    //   - Q=4 (q_term ≈ 0.053) closes the fc≥16 kHz s=2 residuals,
    //   - Q=10 (q_term ≈ 0) keeps the bit-exact base intact,
    //   - Q in (1, 2] is bounded by the 0.06 cap so the slope=8 cascade's
    //     sec1/sec2 (Q ≈ 1.16, 1.85) doesn't overshoot.
    // (Q ≤ 1 routes through the 1-root branch handled in the else-arm.)
    let w_eval = if q_user > 1.0 {
        let inner = w_pole * 0.4421 - 5.0 / 12.0;
        let base = (inner * inner).mul_add(0.2, 0.785) * PI;
        let extra = {
            let d = (w_pole - 1.515).max(0.0);
            let q_term = (1.0 / (q_user * q_user) - 0.01).clamp(0.0, 0.06);
            0.0396 * d * q_term
        };
        (base + extra).clamp(0.0, PI)
    } else {
        PI
    };
    proq4_s2_from_prototype_with_subfreq(
        freq_hz,
        sample_rate,
        1.0,
        0.0,
        0.0,
        1.0,
        alpha,
        1.0,
        w_pole,
        w_zero,
        w_third,
        w_eval,
    )
}

/// Build analog notch sections per `notch_formula.md`.  Returns `(a1, a2)`
/// for each section's denominator `s² + a1·s + a2`; numerator is the
/// universal `s² + 1`.  Section counts: 1, 1, 2, 3 for slopes 2, 4, 6, 8.
// Mode-0 forward formula (decoded from compute_biquad_coefficients_from_poles
// @ 0x180110b50). Inputs (p2, p3, p4, sp5², sp6²); outputs biquad
// [1, a1, a2, b0, b1, b2]. Used by BP s=8 lookup.
pub fn mode0_forward(p2: f64, p3: f64, p4: f64, sp5_sq: f64, sp6_sq: f64) -> Coeffs {
    let sp5 = sp5_sq.sqrt();
    let sp6 = sp6_sq.sqrt();
    let one_p_p4 = 1.0 + p4;
    let p2_p4 = p2 * p4;
    let inv_d = 1.0 / (one_p_p4 + sp5);
    let b0 = (p2_p4 + p3 + sp6) * inv_d;
    let b1 = -2.0 * (p3 - p2_p4) * inv_d;
    let b2 = (p2_p4 + p3 - sp6) * inv_d;
    let a1 = -2.0 * (1.0 - p4) * inv_d;
    let a2 = (one_p_p4 - sp5) * inv_d;
    [1.0, a1, a2, b0, b1, b2]
}
