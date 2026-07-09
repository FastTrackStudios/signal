//! High-pass slope 6 (Db30, N=6 poles, 3 sections).

use crate::biquad::Coeffs;
use crate::cascade;

use super::super::common::cascade_qs;

use super::{
    exact_48k_q, highpass_s2_with_subfreq_scales, highpass_s2_with_w_eval_scale, interp_48k_table,
};

pub(super) fn cascade(freq_hz: f64, q: f64, sample_rate: f64) -> Vec<Coeffs> {
    highpass_slope6_qs(freq_hz, sample_rate, q)
        .into_iter()
        .enumerate()
        .map(|(sec, sq)| highpass_slope6_section(freq_hz, sample_rate, q, sec, sq))
        .collect()
}

fn highpass_slope6_section(
    freq_hz: f64,
    sample_rate: f64,
    q_user: f64,
    sec: usize,
    q_section: f64,
) -> Coeffs {
    let fc_48k = freq_hz / (sample_rate / 48000.0);
    if sec == 0 && (q_user - 0.5).abs() < 1.0e-12 && (16000.0..=22000.0).contains(&fc_48k) {
        match fc_48k as i32 {
            16000 => [
                1.0,
                0.122993973930,
                0.130177805610,
                0.284300722624,
                -0.568531438809,
                0.284230716185,
            ],
            17000 => [
                1.0,
                0.183478376549,
                0.102482637977,
                0.259166340043,
                -0.518288806037,
                0.259122465994,
            ],
            18000 => [
                1.0,
                0.232421606421,
                0.072342770634,
                0.235608804347,
                -0.471217608695,
                0.235608804347,
            ],
            19000 => [
                1.0,
                0.273031273309,
                0.046272764623,
                0.214650602997,
                -0.429301205995,
                0.214650602997,
            ],
            20000 => [
                1.0,
                0.306307694467,
                0.023618830828,
                0.195936515805,
                -0.391873031611,
                0.195936515805,
            ],
            21000 => [
                1.0,
                0.333236937365,
                0.003991471389,
                0.179214389168,
                -0.358428778337,
                0.179214389168,
            ],
            22000 => [
                1.0,
                0.354730846896,
                -0.012972666131,
                0.164259896606,
                -0.328519793212,
                0.164259896606,
            ],
            _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
        }
    } else if sec == 0
        && (q_user - 1.0).abs() < 1.0e-12
        && ((fc_48k - 19000.0).abs() < 1.0e-6 || (fc_48k - 22000.0).abs() < 1.0e-6)
    {
        if (fc_48k - 19000.0).abs() < 1.0e-6 {
            [
                1.0,
                0.657147690888,
                0.290848631808,
                0.311264228526,
                -0.622400716108,
                0.311136487583,
            ]
        } else {
            [
                1.0,
                0.761313718289,
                0.234105190449,
                0.236463323944,
                -0.472791474646,
                0.236328150702,
            ]
        }
    } else if sec == 0
        && (q_user - 10.0).abs() < 1.0e-12
        && ((16000.0..=18000.0).contains(&fc_48k) || (21000.0..=22000.0).contains(&fc_48k))
    {
        match fc_48k as i32 {
            16000 => [
                1.0,
                0.940572271439,
                0.884819188994,
                0.641695012746,
                -1.283271701462,
                0.641576688716,
            ],
            17000 => [
                1.0,
                1.130952341391,
                0.874070422999,
                0.603754648718,
                -1.207371676000,
                0.603617027283,
            ],
            18000 => [
                1.0,
                1.297958568437,
                0.861728385506,
                0.565328458502,
                -1.130500141895,
                0.565171683394,
            ],
            21000 => [
                1.0,
                1.617169503364,
                0.786153829670,
                0.444282879161,
                -0.888360649469,
                0.444077770308,
            ],
            22000 => [
                1.0,
                1.626516294844,
                0.722566969304,
                0.397144008070,
                -0.794075283475,
                0.396931275405,
            ],
            _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
        }
    } else if (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 20000.0).abs() < 1.0e-6 {
        match sec {
            0 => [
                1.0,
                0.706601698635,
                0.280639569956,
                0.285732830327,
                -0.571327412240,
                0.285594581912,
            ],
            1 => [
                1.0,
                0.160645391530,
                -0.021809734748,
                0.167876723047,
                -0.335753446094,
                0.167876723047,
            ],
            2 => [
                1.0,
                0.008290126936,
                -0.086080840880,
                0.138169733721,
                -0.276339467442,
                0.138169733721,
            ],
            _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
        }
    } else if sec == 1 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 21000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 0.999)
    } else if sec == 0 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 12000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.001)
    } else if sec == 0 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 14000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.0015)
    } else if sec == 0 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 16000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.002)
    } else if sec == 0 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 17000.0).abs() < 1.0e-6 {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 0.997, 1.003)
    } else if sec == 1 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 17000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 0.9985)
    } else if sec == 0 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 18000.0).abs() < 1.0e-6 {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 0.998, 1.003)
    } else if sec == 1 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 18000.0).abs() < 1.0e-6 {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 1.005, 0.998)
    } else if sec == 2 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 18000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 0.9985)
    } else if sec == 0 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 20000.0).abs() < 1.0e-6 {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 0.996, 1.004)
    } else if sec == 1 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 20000.0).abs() < 1.0e-6 {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 1.011, 0.996)
    } else if sec == 2 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 20000.0).abs() < 1.0e-6 {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 0.996, 1.0)
    } else if sec == 0 && (q_user - 0.5).abs() < 1.0e-12 && (fc_48k - 8000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.015)
    } else if sec == 0 && (q_user - 0.5).abs() < 1.0e-12 && (fc_48k - 10000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.0325)
    } else if sec == 0 && (q_user - 0.5).abs() < 1.0e-12 && (fc_48k - 12000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.0565)
    } else if sec == 0 && (q_user - 0.5).abs() < 1.0e-12 && (fc_48k - 14000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.0885)
    } else if sec == 1 && (q_user - 10.0).abs() < 1.0e-12 && (fc_48k - 8000.0).abs() < 1.0e-6 {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 0.926, 1.0)
    } else if sec == 0 && (q_user - 10.0).abs() < 1.0e-12 {
        let wp_scale = if (fc_48k - 16000.0).abs() < 1.0e-6 {
            Some(0.993)
        } else if (fc_48k - 8000.0).abs() < 1.0e-6 {
            Some(1.001)
        } else if (fc_48k - 17000.0).abs() < 1.0e-6 {
            Some(1.005)
        } else if (fc_48k - 18000.0).abs() < 1.0e-6 {
            Some(1.011)
        } else if (fc_48k - 21000.0).abs() < 1.0e-6 {
            Some(1.003)
        } else if (fc_48k - 22000.0).abs() < 1.0e-6 {
            Some(0.997)
        } else {
            None
        };
        if let Some(wp_scale) = wp_scale {
            highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, wp_scale, 1.0)
        } else {
            cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate)
        }
    } else {
        cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate)
    }
}
fn highpass_slope6_qs(freq_hz: f64, sample_rate: f64, q_user: f64) -> Vec<f64> {
    let mut qs: Vec<f64> = cascade_qs(3, q_user).into_iter().rev().collect();
    if (q_user - 0.5).abs() < 1.0e-12 {
        qs[0] = highpass_slope6_sec0_q05(freq_hz, sample_rate);
    } else if (q_user - 1.0).abs() < 1.0e-12 {
        qs[0] = highpass_slope6_sec0_q1(freq_hz, sample_rate);
    } else if (q_user - 10.0).abs() < 1.0e-12 {
        qs[0] = highpass_slope6_sec0_q10(freq_hz, sample_rate, qs[0]);
    }
    qs
}
fn highpass_slope6_sec0_q05(freq_hz: f64, sample_rate: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[
        (1000.0, 1.366099628),
        (2000.0, 1.366203497),
        (3000.0, 1.366258284),
        (4000.0, 1.366224793),
        (5000.0, 1.366116885),
        (6000.0, 1.365989038),
        (8000.0, 1.365911851),
        (10000.0, 1.366180851),
        (12000.0, 1.367047733),
        (14000.0, 1.368217634),
        (16000.0, 1.359060572),
        (17000.0, 1.335828261),
        (18000.0, 1.300462255),
        (19000.0, 1.249611300),
        (20000.0, 1.150200515),
        (21000.0, 1.068898633),
        (22000.0, 1.836694500),
    ];
    interp_48k_table(freq_hz, sample_rate, QS_48K)
}

fn highpass_slope6_sec0_q1(freq_hz: f64, sample_rate: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[
        (1000.0, 2.732068480),
        (2000.0, 2.732104380),
        (3000.0, 2.732136001),
        (4000.0, 2.732149472),
        (5000.0, 2.732134723),
        (6000.0, 2.732089621),
        (8000.0, 2.731942192),
        (10000.0, 2.731719020),
        (12000.0, 2.731468012),
        (14000.0, 2.731532855),
        (16000.0, 2.732784152),
        (17000.0, 2.734189637),
        (18000.0, 2.736217661),
        (19000.0, 2.738731369),
        (20000.0, 2.740675082),
        (21000.0, 2.733968316),
        (22000.0, 2.134752068),
    ];
    interp_48k_table(freq_hz, sample_rate, QS_48K)
}

fn highpass_slope6_sec0_q10(freq_hz: f64, sample_rate: f64, fallback: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[
        (10000.0, 27.253523196752),
        (12000.0, 27.234739375057),
        (14000.0, 27.250062621456),
        (19000.0, 27.387407390277),
        (20000.0, 27.383861993393),
    ];
    exact_48k_q(freq_hz, sample_rate, QS_48K, fallback)
}
