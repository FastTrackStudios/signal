//! High-pass slope 4 (Db24, N=4 poles, 2 sections).

use crate::biquad::Coeffs;
use crate::cascade;

use super::super::common::cascade_qs;

use super::{highpass_s2_with_w_eval_scale, interp_48k_table};

pub(super) fn cascade(freq_hz: f64, q: f64, sample_rate: f64) -> Vec<Coeffs> {
    highpass_slope4_qs(freq_hz, sample_rate, q)
        .into_iter()
        .enumerate()
        .map(|(sec, sq)| highpass_slope4_section(freq_hz, sample_rate, q, sec, sq))
        .collect()
}

fn highpass_slope4_section(
    freq_hz: f64,
    sample_rate: f64,
    q_user: f64,
    sec: usize,
    q_section: f64,
) -> Coeffs {
    let fc_48k = freq_hz / (sample_rate / 48000.0);
    if sec == 0 && (q_user - 1.0).abs() < 1.0e-12 && (16000.0..=22000.0).contains(&fc_48k) {
        #[expect(
            clippy::as_conversions,
            clippy::cast_possible_truncation,
            reason = "float-to-int cast with explicit rounding"
        )]
        match fc_48k.round() as i32 {
            16000 => [
                1.0,
                0.259_481_815_917,
                0.227_385_227_777,
                0.336_487_734_960,
                -0.672_877_994_805,
                0.336_390_259_846,
            ],
            17000 => [
                1.0,
                0.333_070_397_146,
                0.215_404_105_496,
                0.309_746_072_206,
                -0.619_383_640_563,
                0.309_637_568_357,
            ],
            18000 => [
                1.0,
                0.397_072_155_695,
                0.204_937_689_642,
                0.285_161_755_792,
                -0.570_205_203_185,
                0.285_043_447_393,
            ],
            19000 => [
                1.0,
                0.451_735_910_412,
                0.193_283_078_345,
                0.262_235_949_315,
                -0.524_346_885_532,
                0.262_110_936_218,
            ],
            20000 => [
                1.0,
                0.483_332_239_770,
                0.151_021_899_130,
                0.236_112_221_618,
                -0.472_127_892_223,
                0.236_015_670_605,
            ],
            21000 => [
                1.0,
                0.492_600_950_675,
                0.085_618_068_225,
                0.208_948_913_422,
                -0.417_897_826_844,
                0.208_948_913_422,
            ],
            22000 => [
                1.0,
                0.497_329_054_611,
                0.034_049_664_095,
                0.186_481_069_165,
                -0.372_962_138_330,
                0.186_481_069_165,
            ],
            _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
        }
    } else if sec == 0 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 10000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.005)
    } else if sec == 0 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 12000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.0085)
    } else if sec == 0 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 14000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.013)
    } else {
        cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate)
    }
}
fn highpass_slope4_qs(freq_hz: f64, sample_rate: f64, q_user: f64) -> Vec<f64> {
    let mut qs: Vec<f64> = cascade_qs(2, q_user).into_iter().rev().collect();
    if (q_user - 1.0).abs() < 1.0e-12 {
        if let Some(q) = qs.get_mut(0) {
            *q = highpass_slope4_sec0_q1(freq_hz, sample_rate);
        }
    }
    qs
}
fn highpass_slope4_sec0_q1(freq_hz: f64, sample_rate: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[
        (1000.0, 1.847_797_502),
        (2000.0, 1.847_869_332),
        (3000.0, 1.847_921_287),
        (4000.0, 1.847_923_115),
        (5000.0, 1.847_867_540),
        (6000.0, 1.847_770_770),
        (8000.0, 1.847_578_575),
        (10000.0, 1.847_476_030),
        (12000.0, 1.847_615_618),
        (14000.0, 1.849_124_792),
        (16000.0, 1.853_365_605),
        (17000.0, 1.855_806_739),
        (18000.0, 1.855_733_776),
        (19000.0, 1.842_962_319),
        (20000.0, 1.761_479_432),
        (21000.0, 1.439_270_993),
        (22000.0, 1.925_770_802),
    ];
    interp_48k_table(freq_hz, sample_rate, QS_48K)
}
