//! Decoded coefficient tables, and the pre-warp they feed.

use super::{notch_analog_sections, notch_inner_pair, PI};

/// Q-table configuration: (`lower_q`, `lower_table`, `upper_q`, `upper_table`)
type QTableConfig<'a> = (f64, &'a Vec<(f64, f64)>, f64, &'a Vec<(f64, f64)>);

/// Pro-Q 4 Bandpass-specific cascade values (`a1_sec`, `a2_sec`) per Q per
/// section. Extracted from probe `LAG_PROTO_DETAIL` at fc=10 (matched-Z
/// near-bit-exact). Pro-Q's actual BP analog cascade differs from
/// `notch_inner_pair` at Q≠1 due to floating-point arithmetic order.
pub fn bp_cascade_for_q(slope: usize, q: f64) -> Vec<(f64, f64)> {
    if matches!(slope, 3 | 5 | 7 | 9) {
        use std::f64::consts::SQRT_2;
        let q_user = q.max(1e-6);
        let c_quartic = 2.0 + 2.0 / (q_user * q_user);
        let (angles, real_count) = lp_atoms_for_slope(slope);
        let mut sections =
            Vec::with_capacity(angles.len().saturating_mul(2).saturating_add(real_count));
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
        let theta = 120.0_f64.to_radians();
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
    let (lo_q, lo_v, hi_q, hi_v): QTableConfig = if q <= 0.5 {
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
        .map(|(&(a1l, a2l), &(a1h, a2h))| {
            (alpha.mul_add(a1h - a1l, a1l), alpha.mul_add(a2h - a2l, a2l))
        })
        .collect()
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
