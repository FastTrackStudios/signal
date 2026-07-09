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
        match fc_48k as i32 {
            16000 => [
                1.0,
                0.259481815917,
                0.227385227777,
                0.336487734960,
                -0.672877994805,
                0.336390259846,
            ],
            17000 => [
                1.0,
                0.333070397146,
                0.215404105496,
                0.309746072206,
                -0.619383640563,
                0.309637568357,
            ],
            18000 => [
                1.0,
                0.397072155695,
                0.204937689642,
                0.285161755792,
                -0.570205203185,
                0.285043447393,
            ],
            19000 => [
                1.0,
                0.451735910412,
                0.193283078345,
                0.262235949315,
                -0.524346885532,
                0.262110936218,
            ],
            20000 => [
                1.0,
                0.483332239770,
                0.151021899130,
                0.236112221618,
                -0.472127892223,
                0.236015670605,
            ],
            21000 => [
                1.0,
                0.492600950675,
                0.085618068225,
                0.208948913422,
                -0.417897826844,
                0.208948913422,
            ],
            22000 => [
                1.0,
                0.497329054611,
                0.034049664095,
                0.186481069165,
                -0.372962138330,
                0.186481069165,
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
        qs[0] = highpass_slope4_sec0_q1(freq_hz, sample_rate);
    }
    qs
}
fn highpass_slope4_sec0_q1(freq_hz: f64, sample_rate: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[
        (1000.0, 1.847797502),
        (2000.0, 1.847869332),
        (3000.0, 1.847921287),
        (4000.0, 1.847923115),
        (5000.0, 1.847867540),
        (6000.0, 1.847770770),
        (8000.0, 1.847578575),
        (10000.0, 1.847476030),
        (12000.0, 1.847615618),
        (14000.0, 1.849124792),
        (16000.0, 1.853365605),
        (17000.0, 1.855806739),
        (18000.0, 1.855733776),
        (19000.0, 1.842962319),
        (20000.0, 1.761479432),
        (21000.0, 1.439270993),
        (22000.0, 1.925770802),
    ];
    interp_48k_table(freq_hz, sample_rate, QS_48K)
}
