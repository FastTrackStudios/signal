//! Decoded coefficient tables, and the pre-warp they feed.

use super::*;

/// Pro-Q 4 Bandpass-specific cascade values (`a1_sec`, `a2_sec`) per Q per
/// section. Extracted from probe `LAG_PROTO_DETAIL` at fc=10 (matched-Z
/// near-bit-exact). Pro-Q's actual BP analog cascade differs from
/// `notch_inner_pair` at Q≠1 due to floating-point arithmetic order.
pub(crate) fn bp_cascade_for_q(slope: usize, q: f64) -> Vec<(f64, f64)> {
    if matches!(slope, 3 | 5 | 7 | 9) {
        use std::f64::consts::SQRT_2;
        let q_user = q.max(1e-6);
        let c_quartic = 2.0 + 2.0 / (q_user * q_user);
        let (angles, real_count) = lp_atoms_for_slope(slope);
        let mut sections = Vec::with_capacity(angles.len() * 2 + real_count);
        for &theta in angles {
            let b = -2.0 * SQRT_2 * theta.cos() / q_user;
            let (a1i, a2i) = notch_inner_pair(b, c_quartic);
            sections.push((a1i, a2i));
            sections.push((a1i / a2i, 1.0 / a2i));
        }
        for _ in 0..real_count {
            sections.push((SQRT_2 / q_user, 1.0));
        }
        return sections;
    }
    if slope == 4 {
        return notch_analog_sections(6, q);
    }
    if slope == 6 {
        use std::f64::consts::SQRT_2;
        let q_user = q.max(1e-6);
        let c_quartic = 2.0 + 2.0 / (q_user * q_user);
        let theta = 120.0_f64 * PI / 180.0;
        let b = -2.0 * SQRT_2 * theta.cos() / q_user;
        let (a1i, a2i) = notch_inner_pair(b, c_quartic);
        return vec![(a1i, a2i), (a1i / a2i, 1.0 / a2i), (SQRT_2 / q_user, 1.0)];
    }
    if slope != 8 {
        return notch_analog_sections(slope, q);
    }
    let q05: Vec<(f64, f64)> = vec![
        (0.136_632_085_431_080_4, 0.102_927_775_183_326_71),
        (1.327_455_929_050_467_3, 9.715_550_522_867_913),
        (0.427_699_803_334_397_3, 0.119_727_970_383_810_43),
        (3.572_263_038_981_829_3, 8.352_267_200_340_18),
        (0.746_428_135_875_452_8, 0.158_221_244_052_665_94),
        (4.717_622_720_922_321, 6.320_263_792_560_857),
    ];
    let q10: Vec<(f64, f64)> = vec![
        (0.157_968_903_048_767_57, 0.275_167_890_247_755_6),
        (0.574_081_891_991_593_9, 3.634_144_954_557_088),
        (0.514_131_726_881_778_2, 0.346_014_345_973_210_24),
        (1.485_868_238_889_679_8, 2.890_053_581_990_569),
        (1.041_465_571_915_082_5, 0.616_038_504_746_829_3),
        (1.690_585_188_896_737_4, 1.623_275_156_170_579_6),
    ];
    let q40: Vec<(f64, f64)> = vec![
        (0.076_090_173_887_941_88, 0.711_615_600_837_688_5),
        (0.106_925_949_625_576_74, 1.405_253_059_127_478),
        (0.218_757_319_001_314_68, 0.777_798_189_568_640_3),
        (0.281_252_029_041_919_26, 1.285_680_544_659_778_8),
        (0.325_671_823_251_275_2, 0.911_343_216_434_378_7),
        (0.357_353_648_305_477_44, 1.097_281_443_441_791_5),
    ];
    let q100: Vec<(f64, f64)> = vec![
        (0.034_108_918_559_306_506, 0.872_385_813_635_398_5),
        (0.039_098_433_314_909_74, 1.146_281_822_067_703_3),
        (0.095_002_807_786_505_8, 0.904_759_374_280_998_1),
        (0.105_003_397_021_425_67, 1.105_266_249_155_681_5),
        (0.134_101_194_217_655_35, 0.963_977_549_097_682_1),
        (0.139_112_362_464_492_4, 1.037_368_557_946_226),
    ];
    let (lo_q, lo_v, hi_q, hi_v): (f64, &Vec<(f64, f64)>, f64, &Vec<(f64, f64)>) = if q <= 0.5 {
        (0.5, &q05, 0.5, &q05)
    } else if q <= 1.0 {
        (0.5, &q05, 1.0, &q10)
    } else if q <= 4.0 {
        (1.0, &q10, 4.0, &q40)
    } else if q <= 10.0 {
        (4.0, &q40, 10.0, &q100)
    } else {
        (10.0, &q100, 10.0, &q100)
    };
    if (lo_q - hi_q).abs() < 1e-9 {
        return lo_v.clone();
    }
    let alpha = (q - lo_q) / (hi_q - lo_q);
    lo_v.iter()
        .zip(hi_v.iter())
        .map(|(&(a1l, a2l), &(a1h, a2h))| (a1l + alpha * (a1h - a1l), a2l + alpha * (a2h - a2l)))
        .collect()
}

/// Apply Pro-Q 4's effective fc-prewarp to a captured "analog-form" biquad
/// section to produce the final digital biquad.
///
/// **The captured form at ZPK2BQ output offset +0x60..+0x88 is NOT the
/// analog form** — it's a post-BLT digital biquad whose effective frequency
/// scale is normalized by `Q_user · ω / 2`, not by `tan(ω/2)`.
///
/// Per RE (`apply_proq4_prewarp_decoded.md`, closes #86), Pro-Q 4 does
/// NOT have a dedicated `apply_proq4_prewarp` helper.  The fc-prewarp is
/// implicit in `precompute_filter_omega_and_q`, which writes
/// `Q_pre = (Q · 0.5 · ω) / tan(min(ω, π−0.01)/2)` into `band[+0x125c]`.
/// `setup_eq_band_filter` then passes `param_8 = 1/Q_pre` as the analog-pole
/// pre-scaling factor into `bilinear_transform_zpk`.  Algebraically this is
/// equivalent to BLT-prewarping with `t_eff = 1/Q_pre` instead of `tan(ω/2)`.
///
/// Inputs: `(b0, b1, b2, a0, a1, a2)` captured "analog-like" form
/// (a0 = 1, the section's denominator z² coefficient), plus user fc / Q /
/// sr to compute `Q_pre` exactly as the binary does.
///
/// Output: final digital biquad in `[a0, a1, a2, b0, b1, b2]` order.
#[must_use]
pub fn apply_proq4_prewarp(
    captured: [f64; 6],
    freq_hz: f64,
    q_user: f64,
    sample_rate: f64,
) -> Coeffs {
    // Captured layout (per path-A): b0, b1, b2, a0, a1, a2 — all real,
    // a0 = 1 (already normalized at BLT output).
    let (b0, b1, b2, a0_in, a1, a2) = (
        captured[0],
        captured[1],
        captured[2],
        captured[3],
        captured[4],
        captured[5],
    );
    let _ = a0_in; // expected = 1

    // Pro-Q 4 fc-prewarp via Q_pre (decoded from precompute_filter_omega_and_q
    // @ 0x180111b40).  Constants verified bit-exact:
    //   ω clamp = π − 0.01  (DAT_180231c64 = 3.13159275f)
    //   Q_pre   = (Q · 0.5 · ω) / tan(ω_clamped / 2)
    // setup_eq_band_filter then passes param_8 = 1/Q_pre as the analog-pole
    // pre-scaling factor into the BLT, which is algebraically equivalent to
    // running standard BLT with t_eff = 1/Q_pre.
    let omega = 2.0 * PI * freq_hz / sample_rate;
    const OMEGA_CLAMP: f64 = std::f64::consts::PI - 0.01;
    let omega_c = omega.min(OMEGA_CLAMP);
    let q = q_user.max(1e-6);
    let q_pre = (q * 0.5 * omega) / (omega_c * 0.5).tan();
    let t = 1.0 / q_pre;

    let t2 = t * t;
    let d_a = 1.0 + a1 * t + a2 * t2;
    let inv_d = 1.0 / d_a;
    let two_t2m1 = 2.0 * (a2 * t2 - 1.0);

    let a1_new = two_t2m1 * inv_d;
    let a2_new = (1.0 - a1 * t + a2 * t2) * inv_d;
    let b0_new = (b0 + b1 * t + b2 * t2) * inv_d;
    let b1_new = 2.0 * (b2 * t2 - b0) * inv_d;
    let b2_new = (b0 - b1 * t + b2 * t2) * inv_d;

    [1.0, a1_new, a2_new, b0_new, b1_new, b2_new]
}

/// LP-prototype atoms per Pro-Q 4 slope index, for Bell / Notch / Bandpass at slope ≥ 4.
/// Returns (`complex_angles_radians`, `real_pole_count`).
///
/// Each "atom" produces two biquad sections via reciprocal-magnitude
/// doubling: pole-pair_high uses `gain_lin^(+1/(2·N))`, pole-pair_low uses
/// gain_lin^(-1/(2·N)), where N = total atom count (complex + real).
///
/// Captured from Pro-Q 4 BLT hook at gain≈0 dB / fc=1000 / Q=1.
/// See `docs/reports/proq4/re/complete_pipeline.md` §4 and
/// `bell_lp_prototype_captures.txt`.
#[must_use]
pub fn lp_atoms_for_slope(slope: usize) -> (&'static [f64], usize) {
    use std::f64::consts::PI;
    const A105: f64 = 105.0 * PI / 180.0;
    const A112_5: f64 = 112.5 * PI / 180.0;
    const A120: f64 = 120.0 * PI / 180.0;
    const A126: f64 = 126.0 * PI / 180.0;
    const A135: f64 = 135.0 * PI / 180.0;
    const A150: f64 = 150.0 * PI / 180.0;
    const A157_5: f64 = 157.5 * PI / 180.0;
    const A165: f64 = 165.0 * PI / 180.0;

    static A_S3: [f64; 1] = [A150];
    static A_S4: [f64; 1] = [A135];
    static A_S5: [f64; 1] = [A126];
    static A_S6: [f64; 1] = [A120];
    static A_S7: [f64; 2] = [A112_5, A157_5];
    static A_S8: [f64; 3] = [A105, A135, A165];
    static A_S9: [f64; 4] = [
        101.25 * PI / 180.0,
        123.75 * PI / 180.0,
        146.25 * PI / 180.0,
        168.75 * PI / 180.0,
    ];

    match slope {
        1 | 2 => (&[], 1),
        3 => (&A_S3, 0),
        5 => (&A_S5, 1),
        6 => (&A_S6, 1),
        7 => (&A_S7, 0),
        8 => (&A_S8, 0),
        9 => (&A_S9, 0),
        _ => (&A_S4, 0),
    }
}
