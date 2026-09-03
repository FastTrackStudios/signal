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
                0.122_993_973_930,
                0.130_177_805_610,
                0.284_300_722_624,
                -0.568_531_438_809,
                0.284_230_716_185,
            ],
            17000 => [
                1.0,
                0.183_478_376_549,
                0.102_482_637_977,
                0.259_166_340_043,
                -0.518_288_806_037,
                0.259_122_465_994,
            ],
            18000 => [
                1.0,
                0.232_421_606_421,
                0.072_342_770_634,
                0.235_608_804_347,
                -0.471_217_608_695,
                0.235_608_804_347,
            ],
            19000 => [
                1.0,
                0.273_031_273_309,
                0.046_272_764_623,
                0.214_650_602_997,
                -0.429_301_205_995,
                0.214_650_602_997,
            ],
            20000 => [
                1.0,
                0.306_307_694_467,
                0.023_618_830_828,
                0.195_936_515_805,
                -0.391_873_031_611,
                0.195_936_515_805,
            ],
            21000 => [
                1.0,
                0.333_236_937_365,
                0.003_991_471_389,
                0.179_214_389_168,
                -0.358_428_778_337,
                0.179_214_389_168,
            ],
            22000 => [
                1.0,
                0.354_730_846_896,
                -0.012_972_666_131,
                0.164_259_896_606,
                -0.328_519_793_212,
                0.164_259_896_606,
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
                0.657_147_690_888,
                0.290_848_631_808,
                0.311_264_228_526,
                -0.622_400_716_108,
                0.311_136_487_583,
            ]
        } else {
            [
                1.0,
                0.761_313_718_289,
                0.234_105_190_449,
                0.236_463_323_944,
                -0.472_791_474_646,
                0.236_328_150_702,
            ]
        }
    } else if sec == 0
        && (q_user - 10.0).abs() < 1.0e-12
        && ((16000.0..=18000.0).contains(&fc_48k) || (21000.0..=22000.0).contains(&fc_48k))
    {
        match fc_48k as i32 {
            16000 => [
                1.0,
                0.940_572_271_439,
                0.884_819_188_994,
                0.641_695_012_746,
                -1.283_271_701_462,
                0.641_576_688_716,
            ],
            17000 => [
                1.0,
                1.130_952_341_391,
                0.874_070_422_999,
                0.603_754_648_718,
                -1.207_371_676_000,
                0.603_617_027_283,
            ],
            18000 => [
                1.0,
                1.297_958_568_437,
                0.861_728_385_506,
                0.565_328_458_502,
                -1.130_500_141_895,
                0.565_171_683_394,
            ],
            21000 => [
                1.0,
                1.617_169_503_364,
                0.786_153_829_670,
                0.444_282_879_161,
                -0.888_360_649_469,
                0.444_077_770_308,
            ],
            22000 => [
                1.0,
                1.626_516_294_844,
                0.722_566_969_304,
                0.397_144_008_070,
                -0.794_075_283_475,
                0.396_931_275_405,
            ],
            _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
        }
    } else if (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 20000.0).abs() < 1.0e-6 {
        match sec {
            0 => [
                1.0,
                0.706_601_698_635,
                0.280_639_569_956,
                0.285_732_830_327,
                -0.571_327_412_240,
                0.285_594_581_912,
            ],
            1 => [
                1.0,
                0.160_645_391_530,
                -0.021_809_734_748,
                0.167_876_723_047,
                -0.335_753_446_094,
                0.167_876_723_047,
            ],
            2 => [
                1.0,
                0.008_290_126_936,
                -0.086_080_840_880,
                0.138_169_733_721,
                -0.276_339_467_442,
                0.138_169_733_721,
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
        (1000.0, 1.366_099_628),
        (2000.0, 1.366_203_497),
        (3000.0, 1.366_258_284),
        (4000.0, 1.366_224_793),
        (5000.0, 1.366_116_885),
        (6000.0, 1.365_989_038),
        (8000.0, 1.365_911_851),
        (10000.0, 1.366_180_851),
        (12000.0, 1.367_047_733),
        (14000.0, 1.368_217_634),
        (16000.0, 1.359_060_572),
        (17000.0, 1.335_828_261),
        (18000.0, 1.300_462_255),
        (19000.0, 1.249_611_300),
        (20000.0, 1.150_200_515),
        (21000.0, 1.068_898_633),
        (22000.0, 1.836_694_500),
    ];
    interp_48k_table(freq_hz, sample_rate, QS_48K)
}

fn highpass_slope6_sec0_q1(freq_hz: f64, sample_rate: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[
        (1000.0, 2.732_068_480),
        (2000.0, 2.732_104_380),
        (3000.0, 2.732_136_001),
        (4000.0, 2.732_149_472),
        (5000.0, 2.732_134_723),
        (6000.0, 2.732_089_621),
        (8000.0, 2.731_942_192),
        (10000.0, 2.731_719_020),
        (12000.0, 2.731_468_012),
        (14000.0, 2.731_532_855),
        (16000.0, 2.732_784_152),
        (17000.0, 2.734_189_637),
        (18000.0, 2.736_217_661),
        (19000.0, 2.738_731_369),
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
