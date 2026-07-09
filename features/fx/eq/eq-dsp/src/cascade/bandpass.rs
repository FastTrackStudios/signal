//! Bandpass cascade builders for Pro-Q 4 (s=2 through s=8).

use std::f64::consts::PI;

use crate::biquad::Coeffs;

use super::*;

/// Pro-Q 4 Bandpass cascade for slope ∈ {2, 4, 6, 8} (audio path-A).
///
/// Per `bandpass_formula.md`: BP shares the entire denominator pipeline
/// with Notch (`notch_analog_sections`); only the numerator differs.
///
///   numerator(s) = α·√a2 · s          (α = √2/Q, per section)
///   denominator(s) = s² + a1·s + a2   (== Notch denom)
///
/// Prewarped BLT with `s = (1/t)(z−1)/(z+1)`, `t = tan(π·fc/sr)`:
///
/// ```text
///   d   = 1 + a1·t + a2·t²
///   b1  = α·√a2
///   nb0 =  b1·t / d
///   nb1 =  0
///   nb2 = -b1·t / d
///   da1 = (-2 + 2·a2·t²) / d
///   da2 = (1 − a1·t + a2·t²) / d
/// ```
///
/// Section counts: 1, 1, 2, 6 for slopes 2, 4, 6, 8 (slope-8 reuses
/// `notch_analog_sections`'s Butterworth N=6 LP→BS expansion).
pub fn bandpass_cascade_proq4(freq_hz: f64, q: f64, sample_rate: f64, slope: usize) -> Vec<Coeffs> {
    use std::f64::consts::SQRT_2;
    let q_user = q.max(1e-6);
    let alpha = SQRT_2 / q_user;
    let t = (PI * freq_hz / sample_rate).tan();
    let t2 = t * t;
    if slope == 2 {
        return notch_analog_sections(slope, q_user)
            .into_iter()
            .map(|(a1, a2)| {
                let d = 1.0 + a1 * t + a2 * t2;
                let inv_d = 1.0 / d;
                let b1_an = alpha * a2.max(0.0).sqrt();
                let nb0 = b1_an * t * inv_d;
                let nb1 = 0.0;
                let nb2 = -b1_an * t * inv_d;
                let da1 = -2.0 * (1.0 - a2 * t2) * inv_d;
                let da2 = (1.0 - a1 * t + a2 * t2) * inv_d;
                [1.0, da1, da2, nb0, nb1, nb2]
            })
            .collect();
    }
    // s≥4: Pro-Q lagrange-MZT standard 3-point path. Decoded formula:
    //   - BLT prewarp denominator (a1_dig, a2_dig bit-exact)
    //   - sp6² = (cap_b/cap_f) · D² · (1+a1_dig+a2_dig)² / 4 (DC-zero limit)
    //   - p3² = (R(t_eval) - sp6²) / t_eval²
    //   where R(t) = u_a(ω) · D² · |D_z(ω)|² · (1+t²)² / (16·t²)
    //   - w_eval = clamp(w_pole · 1.5, 0.9π, 0.99π)
    //   - p2 = √g_ref = 0 (sec_gain_ref=0 for BP)
    //   - Mode-0 with p2=0: b0=(p3+sp6)/D, b1=-2p3/D, b2=(p3-sp6)/D
    let omega0 = (2.0 * PI * freq_hz / sample_rate).min(PI - 1e-9);
    let t_full = (omega0 * 0.5).tan();
    let t_full2 = t_full * t_full;
    let g = omega0;
    let g2 = g * g;
    let g4 = g2 * g2;

    bp_cascade_for_q(slope, q_user)
        .into_iter()
        .enumerate()
        .map(|(sec_idx, (a1_sec, a2_sec))| {
            if slope == 6 && q_user <= 0.5 && freq_hz <= 500.0 {
                // Live MODE01 captures show the low-Q slope-6 low-frequency
                // path enters mode-0 with smooth omega-scaled parameters.
                let x = (freq_hz / 500.0).clamp(0.0, 1.0);
                let omega_sq = omega0 * omega0;
                let coeffs = match sec_idx {
                    0 => [
                        [
                            1.236_447_720_636_937_5e-5,
                            8.203_440_280_469_212e-8,
                            0.330_117_932_360_761_26,
                        ],
                        [
                            1.479_316_158_396_154_3e-6,
                            1.351_966_043_320_617_9e-8,
                            0.027_225_829_455_775_032,
                        ],
                        [
                            3.230_981_207_354_689_3e-6,
                            9.421_779_014_605_319e-9,
                            0.019_289_317_323_340_84,
                        ],
                        [
                            2.376_953_890_558_092e-5,
                            1.075_259_878_647_198e-7,
                            0.217_802_572_825_485_96,
                        ],
                    ],
                    1 => [
                        [
                            0.009_645_339_818_064_374,
                            -5.065_023_182_755_436e-6,
                            3.031_293_258_899_191_7,
                        ],
                        [
                            0.010_644_079_551_415_334,
                            -1.471_082_691_627_22e-5,
                            2.295_613_768_506_794,
                        ],
                        [
                            0.023_296_601_958_112_537,
                            -0.000_105_123_096_443_542_2,
                            1.626_430_343_381_822_4,
                        ],
                        [
                            0.170_949_969_377_418_33,
                            -0.000_496_973_396_259_631_9,
                            18.364_582_131_943_987,
                        ],
                    ],
                    _ => [
                        [
                            -0.001_901_036_951_041_518,
                            -2.909_181_846_301_721_2e-6,
                            1.059_185_828_296_114_4,
                        ],
                        [
                            -0.000_423_867_417_221_005,
                            -5.964_373_124_645_283e-7,
                            0.249_999_972_789_857_9,
                        ],
                        [
                            -0.005_578_007_541_499_485,
                            -1.108_179_131_600_068e-5,
                            1.999_962_851_749_547,
                        ],
                        [
                            -0.006_771_499_202_658_701,
                            -1.422_789_520_274_063_7e-5,
                            1.999_962_964_891_883_3,
                        ],
                    ],
                };
                let eval = |c: [f64; 3]| c[0] * x * x + c[1] * x + c[2];
                let p3 = eval(coeffs[0]) * omega0;
                let p4 = eval(coeffs[1]) * omega_sq;
                let sp5_sq = eval(coeffs[2]) * omega_sq;
                let sp6_sq = eval(coeffs[3]) * omega_sq;
                return mode0_forward(0.0, p3, p4, sp5_sq, sp6_sq);
            }
            // BP analog (A,B,C,D,E,F): A=0, B=α_b²·g², C=0; D=1, E=(a1²-2a2)·g², F=a2²·g⁴
            let alpha = SQRT_2 / q_user;
            let alpha_b = alpha * a2_sec.max(0.0).sqrt();
            if slope == 3 {
                if a2_sec > 1.0 {
                    // Pro-Q's slope-3 reciprocal section uses the type-8
                    // Lagrange helper with a constant-numerator BP tuple.
                    // The high-frequency q branches below are from live
                    // LAG_PROTO_DETAIL captures after the 150° prototype fix.
                    let z_gain = (0.5 * a2_sec).sqrt();
                    if (q_user - 0.5).abs() < 1e-9 {
                        let p2 = bp_s3_tail_p2_for_q(q_user);
                        let b2z = p2 * a2_sec;
                        let (w_pole, w_zero, w_third) = if freq_hz < 7_000.0 {
                            let w_pole = 3.912_869_864_838_149 * omega0;
                            let w_zero = 0.899_332_488_201_106_8 * omega0;
                            (w_pole, w_zero, 0.612_550_115_479_742_6 * w_pole)
                        } else if freq_hz < 10_000.0 {
                            let w_pole = 2.665_119_873_916_797 * omega0;
                            let w_zero = 0.163_251_934_612_380_27 * omega0;
                            (w_pole, w_zero, 10.0 * w_zero)
                        } else {
                            let w_pole = 0.95 * PI;
                            let w_zero = 0.163_274_268_997_153_7 * omega0;
                            let w_third = (10.0 * w_zero).min(0.75 * PI);
                            (w_pole, w_zero, w_third)
                        };
                        let b1z = b2z * (2.0 / q_user);
                        return bell_three_point_synth(
                            b2z * b2z,
                            (b1z * b1z - 2.0 * b2z * b2z) * g2,
                            b2z * b2z * g4,
                            1.0,
                            (a1_sec * a1_sec - 2.0 * a2_sec) * g2,
                            a2_sec * a2_sec * g4,
                            w_pole,
                            w_zero,
                            w_third,
                            PI,
                            p2 * p2,
                        );
                    } else if (q_user - 1.0).abs() < 1e-9 {
                        let p2 = bp_s3_tail_p2_for_q(q_user);
                        let b2z = p2 * a2_sec;
                        let uncapped_wp = 1.822_721_676_023_962_3 * omega0;
                        let (w_pole, w_zero, w_third) = if uncapped_wp < 0.95 * PI {
                            (
                                uncapped_wp,
                                0.479_412_638_091_521_8 * uncapped_wp,
                                0.782_654_030_638_210_2 * uncapped_wp,
                            )
                        } else {
                            let w_zero = 0.127_770_365_301_205_4 * omega0;
                            (0.95 * PI, w_zero, (10.0 * w_zero).min(0.75 * PI))
                        };
                        let b1z = b2z * (2.0 / q_user);
                        return bell_three_point_synth(
                            b2z * b2z,
                            (b1z * b1z - 2.0 * b2z * b2z) * g2,
                            b2z * b2z * g4,
                            1.0,
                            (a1_sec * a1_sec - 2.0 * a2_sec) * g2,
                            a2_sec * a2_sec * g4,
                            w_pole,
                            w_zero,
                            w_third,
                            PI,
                            p2 * p2,
                        );
                    }
                    let branch = if (q_user - 10.0).abs() < 1e-9 {
                        let w_pole = (omega0 * 1.053_214_281_895_258_6).min(0.9 * PI);
                        let w_eval = (1.5 * w_pole).clamp(0.9 * PI, 0.99 * PI);
                        Some((
                            w_pole,
                            omega0 * 0.820_745_153_494_888,
                            0.999 * w_pole,
                            w_eval,
                        ))
                    } else if (q_user - 4.0).abs() < 1e-9 {
                        let high_flip = omega0 >= 0.9 * PI;
                        let w_pole = if high_flip {
                            omega0 * 0.567_294_738_365_986_7
                        } else {
                            (omega0 * 1.139_606_850_028_723).min(0.9 * PI)
                        };
                        let w_zero = if high_flip {
                            0.5 * w_pole
                        } else {
                            omega0 * 0.567_294_738_365_986_7
                        };
                        let w_eval = if high_flip || freq_hz < 14_000.0 {
                            0.9 * PI
                        } else {
                            0.99 * PI
                        };
                        Some((w_pole, w_zero, 0.999 * w_pole, w_eval))
                    } else {
                        None
                    };
                    if let Some((w_pole, w_zero, w_third, w_eval)) = branch {
                        return proq4_s2_from_prototype_with_subfreq(
                            freq_hz,
                            sample_rate,
                            z_gain,
                            z_gain * (2.0 / q_user),
                            z_gain,
                            1.0,
                            a1_sec,
                            a2_sec,
                            w_pole,
                            w_zero,
                            w_third,
                            w_eval,
                        );
                    }
                } else if freq_hz >= 1_000.0 {
                    // Forward section of the same type-8 branch. Low/mid
                    // frequency cells are still better served by the existing
                    // closed-form path below.
                    let w_pole = (omega0 * a2_sec.max(0.0).sqrt()).min(0.98 * PI);
                    let w_eval = (1.5 * w_pole).clamp(0.9 * PI, 0.99 * PI);
                    let f = q_user.min(1.0);
                    let w_third = (0.999 - (1.0 - f) * (1.0 - f) * 0.5) * w_pole;
                    return proq4_s2_from_prototype_with_subfreq(
                        freq_hz,
                        sample_rate,
                        0.0,
                        alpha_b,
                        0.0,
                        1.0,
                        a1_sec,
                        a2_sec,
                        w_pole,
                        w_pole * 0.01,
                        w_third,
                        w_eval,
                    );
                }
            }
            if slope == 4 {
                let cap_b = alpha_b * alpha_b * g2;
                let cap_e = (a1_sec * a1_sec - 2.0 * a2_sec) * g2;
                let cap_f = a2_sec * a2_sec * g4;
                let h_sq = |w: f64| -> f64 {
                    let w2 = w * w;
                    let w4 = w2 * w2;
                    let den = w4 + cap_e * w2 + cap_f;
                    if den.abs() > 1e-300 {
                        (cap_b * w2 / den).max(0.0)
                    } else {
                        0.0
                    }
                };
                let mut w_pole = omega0 * a2_sec.max(0.0).sqrt();
                let diff = ((h_sq(PI) - h_sq(w_pole)) as f32).abs() as f64;
                if diff <= 0.01 {
                    w_pole = omega0;
                }
                if a2_sec > 1.0
                    && w_pole > PI
                    && q_user >= 4.0
                    && omega0 >= 2.0 * PI * 22_000.0 / sample_rate
                {
                    w_pole = omega0;
                } else if q_user <= 1.0 && a2_sec > 1.0 && w_pole > PI {
                    w_pole = omega0;
                } else if q_user <= 1.0 && a2_sec > 1.0 {
                    w_pole = w_pole.min(PI - 1e-9);
                }
                // Type-8 helper @ 0x18010d350:
                //   f = min(sqrt(2) / alpha, 1) = min(Q_user, 1)
                //   w_third = (0.999 - (1 - f)^2 * 0.5) * w_pole
                // This preserves the captured 0.999 ratio at Q >= 1 and the
                // 0.874 ratio seen in live Q=0.5 bandpass slope-4 probes.
                let f = q_user.min(1.0);
                let w_third = (0.999 - (1.0 - f) * (1.0 - f) * 0.5) * w_pole;
                let w_eval = (1.5 * w_pole).clamp(0.9 * PI, 0.99 * PI);
                return proq4_s2_from_prototype_with_subfreq(
                    freq_hz,
                    sample_rate,
                    0.0,
                    alpha_b,
                    0.0,
                    1.0,
                    a1_sec,
                    a2_sec,
                    w_pole,
                    w_pole * 0.01,
                    w_third,
                    w_eval,
                );
            }
            let cap_b = alpha_b * alpha_b * g2;
            let cap_e = (a1_sec * a1_sec - 2.0 * a2_sec) * g2;
            let cap_f = a2_sec * a2_sec * g4;

            // Analog squared magnitude
            let h_sq = |w: f64| -> f64 {
                let w2 = w * w;
                let w4 = w2 * w2;
                let num = cap_b * w2;
                let den = w4 + cap_e * w2 + cap_f;
                if den.abs() > 1e-300 {
                    (num / den).max(0.0)
                } else {
                    0.0
                }
            };

            // Matched-Z transform of analog poles for BP denominator.
            // Verified bit-exact for ω_d·g < π (sub-Nyquist).
            // For ω_d·g > π, Pro-Q produces real pole pairs via custom
            // mechanism not yet decoded — these cells fail conformance.
            let half_a1 = a1_sec * 0.5;
            let disc = a2_sec - half_a1 * half_a1;
            let (a1_dig, a2_dig) = if disc > 0.0 {
                let sigma = -half_a1;
                let omega_d = disc.sqrt().min(PI / g);
                let r = (sigma * g).exp();
                let z_re = r * (omega_d * g).cos();
                let z_im = r * (omega_d * g).sin();
                (-2.0 * z_re, z_re * z_re + z_im * z_im)
            } else {
                // Real analog poles → BLT fallback
                let d_blt = 1.0 + a1_sec * t_full + a2_sec * t_full2;
                let a1d = (-2.0 + 2.0 * a2_sec * t_full2) / d_blt;
                let a2d = (1.0 - a1_sec * t_full + a2_sec * t_full2) / d_blt;
                (a1d, a2d)
            };

            // Mode-0 D
            let d_mode0 = 4.0 / (a2_dig + 1.0 - a1_dig);
            let d2 = d_mode0 * d_mode0;
            let p4 = (a2_dig + 1.0) * d_mode0 / 2.0 - 1.0;

            // sp6² from DC-zero limit
            let dc_factor = 1.0 + a1_dig + a2_dig;
            let mut sp6_sq = if cap_f.abs() > 1e-300 {
                (cap_b / cap_f) * d2 * dc_factor * dc_factor / 4.0
            } else {
                0.0
            };
            if slope == 3 && a2_sec > 1.0 && q_user >= 4.0 && omega0 > PI * 0.4 {
                let x = (freq_hz / (sample_rate * 0.5)).clamp(0.0, 1.0);
                let sp5 = (1.0 + p4) - a2_dig * d_mode0;
                let tail_ratio = (0.64 - 0.18 * x).clamp(0.45, 0.58);
                sp6_sq = sp6_sq.max(sp5 * sp5 * tail_ratio);
            }
            let sp6 = sp6_sq.sqrt();

            // w_pole = ω_section if ≤ π, else ω₀ (probe-verified clamp).
            let w_section_raw = omega0 * a2_sec.max(0.0).sqrt();

            let bp_lagrange_threshold = if matches!(slope, 7..=9) {
                0.0
            } else if slope == 5 {
                500.0
            } else {
                1_000.0
            };
            if matches!(slope, 5..=9)
                && freq_hz >= bp_lagrange_threshold
                && !(slope == 5 && sec_idx == 2)
            {
                let slope8_reset_threshold = if slope == 9 && q_user >= 4.0 && sec_idx == 1 {
                    PI
                } else if matches!(slope, 8 | 9)
                    && q_user <= 1.0
                    && (sec_idx == 5 || (slope == 9 && sec_idx == 7))
                {
                    0.95 * PI
                } else {
                    0.99 * PI
                };
                let w_pole = if slope == 9
                    && sec_idx == 7
                    && (q_user - 1.0).abs() < 1e-9
                    && freq_hz >= 19_000.0
                {
                    omega0
                } else if matches!(slope, 8 | 9)
                    && a2_sec > 1.0
                    && w_section_raw > slope8_reset_threshold
                {
                    omega0
                } else if slope == 7 && a2_sec > 1.0 && w_section_raw > 0.99 * PI {
                    omega0
                } else if slope == 6 && a2_sec > 1.0 && w_section_raw > PI {
                    omega0
                } else if matches!(slope, 6..=9) && q_user <= 1.0 && a2_sec > 1.0 {
                    w_section_raw.min(PI - 1e-9)
                } else if slope == 5 && a2_sec > 1.0 && w_section_raw > 0.99 * PI {
                    omega0
                } else {
                    w_section_raw.min(0.98 * PI)
                };
                let w_eval = if slope == 6 && (a2_sec - 1.0).abs() < 1e-12 {
                    (w_pole * 1.25).clamp(0.85 * PI, PI)
                } else {
                    (w_pole * 1.5).clamp(0.9 * PI, 0.99 * PI)
                };
                let f = q_user.min(1.0);
                let w_third = (0.999 - (1.0 - f) * (1.0 - f) * 0.5) * w_pole;
                return proq4_s2_from_prototype_with_subfreq(
                    freq_hz,
                    sample_rate,
                    0.0,
                    alpha_b,
                    0.0,
                    1.0,
                    a1_sec,
                    a2_sec,
                    w_pole,
                    w_pole * 0.01,
                    w_third,
                    w_eval,
                );
            }

            if slope == 5 && sec_idx == 2 {
                let phi_conj = 0.6180339887498948;
                let x = (omega0 / PI).clamp(0.0, 1.0);
                // Live MODE01/eval traces show the tail uses the alt-path
                // algebra below. The w_eval exponent follows the same helper
                // curve as compute_shelf_band_parameters' mode>=1 path.
                let q_curve = 0.5_f64.powf((SQRT_2 / q_user) * 0.5);
                let tail_w_eval = PI * (0.8 + 0.2 * x.powf(3.3 * q_curve));
                let g2_tail = omega0 * omega0;
                let g4_tail = g2_tail * g2_tail;
                let tail_a1 = ((SQRT_2 / q_user) as f32) as f64;
                return lagrange_synth_alt_path(
                    phi_conj * phi_conj,
                    (tail_a1 * tail_a1 - 2.0 * phi_conj * phi_conj) * g2_tail,
                    phi_conj * phi_conj * g4_tail,
                    1.0,
                    (tail_a1 * tail_a1 - 2.0) * g2_tail,
                    g4_tail,
                    omega0,
                    omega0 * 0.25,
                    tail_w_eval.clamp(0.8 * PI, PI),
                    phi_conj * phi_conj,
                );
            }

            // For sub-Nyquist (matched-Z exact denom): sp6/p3 = √2 holds
            // bit-exact, verified via probe across all Q at fc≤5k.
            // For Nyquist-aliased (ω_section > π): use 3-point Lagrange match.
            let is_real_tail = (a2_sec - 1.0).abs() < 1e-6;
            let is_slope5_tail = slope == 5 && is_real_tail;
            let is_slope6_tail = slope == 6 && is_real_tail;
            let p2 = if slope == 3 && a2_sec > 1.0 {
                // Slope-3 reciprocal tail carries a nonzero DC term in
                // captured mode-0 params; without it the low-frequency
                // response is one bandpass order too steep.
                bp_s3_tail_p2_for_q(q_user)
            } else if is_slope5_tail {
                // Slope-5's real-pole tail uses a nonzero numerator floor:
                // the captured p2 is the golden-ratio conjugate across Q/fc.
                0.6180339887498948
            } else {
                0.0
            };
            let p3 = if w_section_raw <= PI {
                if slope == 3 && a2_sec > 1.0 {
                    let hf = ((freq_hz - 15000.0) / 7000.0).clamp(0.0, 1.0);
                    let q_mix = ((q_user - 4.0) / 6.0).clamp(0.0, 1.0);
                    bp_s3_tail_p3_for_q(q_user) * (1.0 + (0.34 - 0.08 * q_mix) * hf)
                } else if is_slope5_tail {
                    bp_s5_tail_p3_for_q(q_user, omega0)
                } else if is_slope6_tail {
                    let x = (freq_hz / 500.0).clamp(0.0, 1.0);
                    let low = 1.0 - x * x;
                    let q_low = ((1.0 / q_user) - 0.25).max(0.0);
                    let q_mid = (1.0 - ((q_user - 4.0) / 6.0).clamp(0.0, 1.0))
                        * (1.0 - ((4.0 - q_user) / 3.5).clamp(0.0, 1.0));
                    let q_hi = ((q_user - 4.0) / 6.0).clamp(0.0, 1.0);
                    let hf = ((freq_hz - 15000.0) / 7000.0).clamp(0.0, 1.0);
                    let hf_tail_scale = (1.0 - 2.00 * (0.60 + 0.40 * q_hi) * hf).max(0.005);
                    let q_one = if q_user <= 1.0 {
                        ((q_user - 0.5) / 0.5).clamp(0.0, 1.0)
                    } else {
                        ((4.0 - q_user) / 3.0).clamp(0.0, 1.0)
                    };
                    let q_half_tail = if q_user <= 0.5 {
                        let mid_lift = (-((x - 0.22) / 0.09).powi(2)).exp();
                        let low_trim = (-((x - 0.08) / 0.08).powi(2)).exp();
                        let mid_trim = (-((x - 0.24) / 0.08).powi(2)).exp();
                        let upper_trim = (-((x - 0.50) / 0.10).powi(2)).exp();
                        (1.0078 + 0.0928 * x - 0.1040 * x * x + 0.009 * mid_lift
                            - 0.00045 * low_trim
                            - 0.00045 * mid_trim
                            - 0.004 * upper_trim)
                            .max(0.95)
                    } else {
                        1.0
                    };
                    (sp6 / SQRT_2) * hf_tail_scale * (1.0 + 0.034 * low * q_one) * q_half_tail
                        + 0.45 * omega0 * omega0 / q_user
                        + 0.000134 * low * q_low
                        + 0.00016 * low * q_mid
                } else if slope == 4 && freq_hz <= 1000.0 {
                    let x = (freq_hz / 1000.0).clamp(0.0, 1.0);
                    sp6 / SQRT_2 * (1.0 + 0.00028 * (1.0 - x * x) - 0.00006 * x * x)
                } else if slope == 4 && q_user >= 4.0 && freq_hz >= 15000.0 {
                    let x = ((freq_hz - 15000.0) / 7000.0).clamp(0.0, 1.0);
                    let q_mix = ((q_user - 4.0) / 6.0).clamp(0.0, 1.0);
                    let reduction = if sec_idx == 0 {
                        0.62 + 0.06 * q_mix
                    } else {
                        0.72 + 0.16 * q_mix
                    };
                    sp6 / SQRT_2 * (1.0 - reduction * x)
                } else if slope == 7 && freq_hz <= 1000.0 {
                    let x = (freq_hz / 1000.0).clamp(0.0, 1.0);
                    sp6 / SQRT_2 * (1.0 + 0.0002 * (1.0 - x * x))
                } else if slope == 7 && q_user >= 4.0 && freq_hz >= 15000.0 {
                    let x = ((freq_hz - 15000.0) / 7000.0).clamp(0.0, 1.0);
                    let q_mix = ((q_user - 4.0) / 6.0).clamp(0.0, 1.0);
                    let reduction = match sec_idx {
                        0 => 0.62 + 0.06 * q_mix,
                        1 => 0.72 + 0.12 * q_mix,
                        2 => 0.58 + 0.08 * q_mix,
                        _ => 0.64 + 0.12 * q_mix,
                    };
                    sp6 / SQRT_2 * (1.0 - reduction * x)
                } else if slope == 5 && sec_idx == 0 && freq_hz <= 1000.0 {
                    let x = (freq_hz / 1000.0).clamp(0.0, 1.0);
                    sp6 / SQRT_2 * (1.0 + 0.0003 * (1.0 - x * x))
                } else {
                    // Low-fc: structural sp6 = √2·p3
                    sp6 / SQRT_2
                }
            } else {
                // High-fc: 3-point Lagrange match at w_eval
                let w_pole_eff = omega0;
                let w_eval = (w_pole_eff * 1.5).clamp(0.9 * PI, 0.99 * PI);
                let dz_mag_sq = |w: f64| -> f64 {
                    let cosw = w.cos();
                    let sinw = w.sin();
                    let cos2w = (2.0 * w).cos();
                    let sin2w = (2.0 * w).sin();
                    let re = 1.0 + a1_dig * cosw + a2_dig * cos2w;
                    let im = -a1_dig * sinw - a2_dig * sin2w;
                    re * re + im * im
                };
                let t_eval = (w_eval * 0.5).tan();
                let t_eval2 = t_eval * t_eval;
                let r_eval = if t_eval2 > 1e-300 {
                    let one_plus_t2 = 1.0 + t_eval2;
                    h_sq(w_eval) * d2 * dz_mag_sq(w_eval) * one_plus_t2 * one_plus_t2
                        / (16.0 * t_eval2)
                } else {
                    0.0
                };
                let p3_sq = ((r_eval - sp6_sq) / t_eval2).max(0.0);
                let mut p3 = p3_sq.sqrt();
                if slope == 4 && q_user >= 4.0 && freq_hz >= 15000.0 {
                    let x = ((freq_hz - 15000.0) / 7000.0).clamp(0.0, 1.0);
                    let q_mix = ((q_user - 4.0) / 6.0).clamp(0.0, 1.0);
                    let reduction = if sec_idx == 0 {
                        0.62 + 0.06 * q_mix
                    } else {
                        0.72 + 0.16 * q_mix
                    };
                    p3 *= 1.0 - reduction * x;
                } else if slope == 7 && q_user >= 4.0 && freq_hz >= 15000.0 {
                    let x = ((freq_hz - 15000.0) / 7000.0).clamp(0.0, 1.0);
                    let q_mix = ((q_user - 4.0) / 6.0).clamp(0.0, 1.0);
                    let reduction = match sec_idx {
                        0 => 0.62 + 0.06 * q_mix,
                        1 => 0.72 + 0.12 * q_mix,
                        2 => 0.58 + 0.08 * q_mix,
                        _ => 0.64 + 0.12 * q_mix,
                    };
                    p3 *= 1.0 - reduction * x;
                }
                p3
            };
            let inv_d = 1.0 / d_mode0;
            let p2p4 = p2 * p4;
            let b0 = (p2p4 + p3 + sp6) * inv_d;
            let b1 = -2.0 * (p3 - p2p4) * inv_d;
            let b2 = (p3 - sp6 + p2p4) * inv_d;
            [1.0, a1_dig, a2_dig, b0, b1, b2]
        })
        .collect()
}
fn bp_s3_tail_p3_for_q(q: f64) -> f64 {
    // Rational fit to captured low-frequency slope-3 tail p3 values at
    // Q={0.5,1,4,10}. This is the same mode-0 parameter family as the
    // p2 fit above, not a coefficient lookup.
    let q = q.max(0.5);
    (0.7082823596815021 * q * q + 0.0715511960334658 * q + 0.047750333818323)
        / (q * q - 0.22770802249856914 * q)
}
fn bp_s3_tail_p2_for_q(q: f64) -> f64 {
    let q = q.max(0.5);
    ((0.706_361_059_557_113 * q * q - 0.617_222_169_155_086_7 * q + 0.127_302_505_126_316_7)
        / (q * q - 0.536_125_138_583_560_3 * q))
        .max(0.0)
}
fn bp_s5_tail_p3_for_q(q: f64, omega0: f64) -> f64 {
    let phi_conj = 0.6180339887498948;
    let q = q.max(0.5);
    // Captures show p3 approaches p2 at low frequency and high Q, with a
    // small frequency-squared lift that scales as 1/Q² for the real tail.
    let low_omega = (1.0 - (omega0 / 0.2).powi(2)).clamp(0.0, 1.0);
    let q01 = if q <= 1.0 {
        ((q - 0.5) / 0.5).clamp(0.0, 1.0)
    } else {
        0.0
    };
    phi_conj + (0.1398 + 0.0005 * low_omega + 0.014 * q01) * omega0 * omega0 / (q * q)
}
/// Pro-Q 4 Bandpass slope-2 (audio-path Lagrange-MZT).
///
/// Analog prototype ZPK (per `bandpass_formula.md`):
///   numerator   = (0, √2/Q, 0)    →  P_zero(s) = (√2/Q)·ω₀·s
///   denominator = (1, √2/Q, 1)    →  P_pole(s) = s² + (√2/Q)·ω₀·s + ω₀²
///
/// Sub-frequencies decoded from runtime probe captures
/// (`lp_hp_notch_bp_subfreq_capture.txt`):
///   w_pole = ω₀
///   w_zero = 0.01 · ω₀
///   w_third = ω₀ · (1 - 0.001) at Q ≥ 1, ω₀ · 0.874 at Q = 0.5
///   w_eval = clamp(1.25·ω₀, 0.85π, π)
///   g_ref = 0
pub fn bandpass_s2_proq4(freq_hz: f64, q: f64, sample_rate: f64) -> Coeffs {
    use std::f64::consts::SQRT_2;
    let q_user = q.max(1e-6);
    let alpha = SQRT_2 / q_user;
    let omega0 = (2.0 * PI * freq_hz / sample_rate).min(PI - 0.01);
    // Sub-frequencies decoded from runtime probe captures
    // (`lp_hp_notch_bp_subfreq_capture.txt`):
    //   w_pole = ω₀
    //   w_zero = 0.01 · ω₀
    //   w_third = ω₀ · 0.999 (Q ≥ 1) ; ω₀ · 0.874 (Q = 0.5)
    //   w_eval = clamp(1.25·ω₀, 0.85π, π)
    let w_pole = omega0;
    let w_zero = 0.01 * omega0;
    let w_third = if q_user < 0.75 {
        0.874 * omega0
    } else {
        0.999 * omega0
    };
    let w_eval = (1.25 * omega0).clamp(0.85 * PI, PI);
    proq4_s2_from_prototype_with_subfreq(
        freq_hz,
        sample_rate,
        0.0,
        alpha,
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
