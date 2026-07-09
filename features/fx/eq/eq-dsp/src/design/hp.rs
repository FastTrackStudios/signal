//! High-pass (high-cut) cascade builder — Pro-Q 4 algorithmic path.
//!
//! Hosts slope-specific section builders (s=3..9), per-slope Q helpers,
//! and the shared `cut_odd_*` family used by both HP and LP for odd-order
//! tails (slopes 3, 5, 7, 9).  See `docs/reports/proq4/re/hp_*.md`.
//!
//! Helpers re-exported `pub(super)` for use by [`super::lp`]:
//! [`cut_odd_qs`], [`cut_odd_tail_lowpass`].

use std::f64::consts::PI;

use crate::biquad::Coeffs;
use crate::cascade;

use super::common::cascade_qs;

mod odd;
mod s4;
mod s6;
mod s7;
mod s8;
mod s9;

pub(super) fn mzt_highpass_simple_cascade(
    n: usize,
    freq_hz: f64,
    q: f64,
    sample_rate: f64,
    order: usize,
) -> Vec<Coeffs> {
    let n = n.max(1);
    if n == 1 {
        return vec![cascade::highpass_s2_proq4(freq_hz, q, sample_rate)];
    }
    match order {
        3 | 5 => odd::cascade(order, freq_hz, q, sample_rate),
        4 => s4::cascade(freq_hz, q, sample_rate),
        6 => s6::cascade(freq_hz, q, sample_rate),
        7 => s7::cascade(freq_hz, q, sample_rate),
        16 => s9::cascade(freq_hz, q, sample_rate),
        _ if n >= 4 => s8::cascade(freq_hz, q, sample_rate),
        _ => cascade_qs(n, q)
            .into_iter()
            .rev()
            .map(|sq| cascade::highpass_s2_proq4(freq_hz, sq, sample_rate))
            .collect(),
    }
}

pub(super) fn exact_48k_q(
    freq_hz: f64,
    sample_rate: f64,
    table: &[(f64, f64)],
    fallback: f64,
) -> f64 {
    let fc = freq_hz / (sample_rate / 48000.0);
    table
        .iter()
        .find_map(|(f, q)| {
            if (fc - *f).abs() < 1.0e-6 {
                Some(*q)
            } else {
                None
            }
        })
        .unwrap_or(fallback)
}

pub(super) fn interp_48k_table(freq_hz: f64, sample_rate: f64, table: &[(f64, f64)]) -> f64 {
    let sr_scale = sample_rate / 48000.0;
    let fc = (freq_hz / sr_scale).clamp(table[0].0, table[table.len() - 1].0);
    for pair in table.windows(2) {
        let (f0, q0) = pair[0];
        let (f1, q1) = pair[1];
        if fc <= f1 {
            let t = (fc - f0) / (f1 - f0);
            return q0 + (q1 - q0) * t;
        }
    }
    table[table.len() - 1].1
}

pub(super) fn highpass_s2_with_w_eval_scale(
    freq_hz: f64,
    sample_rate: f64,
    q_section: f64,
    w_eval_scale: f64,
) -> Coeffs {
    highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 1.0, w_eval_scale)
}

pub(super) fn highpass_real_double_zero_section(p_pos: f64, p_neg: f64, b0: f64) -> Coeffs {
    [1.0, -(p_pos + p_neg), p_pos * p_neg, b0, -2.0 * b0, b0]
}

pub(super) fn highpass_s2_with_subfreq_scales(
    freq_hz: f64,
    sample_rate: f64,
    q_section: f64,
    w_pole_scale: f64,
    w_eval_scale: f64,
) -> Coeffs {
    let q_sec = q_section.max(1.0e-6);
    let alpha = std::f64::consts::SQRT_2 / q_sec;
    let omega0 = (2.0 * PI * freq_hz / sample_rate).min(PI - 0.01);
    let base_w_pole = if q_sec > 1.0 {
        let q2 = q_sec * q_sec;
        omega0 * (q2 / (q2 - 1.0 + 1.0 / q2)).sqrt()
    } else {
        omega0.min(0.7 * PI)
    };
    let w_pole = (base_w_pole * w_pole_scale).clamp(0.0, PI);
    let w_zero = 0.001 * w_pole;
    let w_third = 0.2 * w_pole;
    let w_eval = if q_sec > 1.0 {
        let inner = w_pole * 0.4421 - 5.0 / 12.0;
        let base = (inner * inner * 0.2 + 0.785) * PI;
        let d = (w_pole - 1.515).max(0.0);
        let q_term = (1.0 / (q_sec * q_sec) - 0.01).clamp(0.0, 0.06);
        (base + 0.0396 * d * q_term).clamp(0.0, PI)
    } else {
        PI
    };
    cascade::proq4_s2_from_prototype_with_subfreq_pub(
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
        (w_eval * w_eval_scale).clamp(0.0, PI),
    )
}

pub(super) fn cut_odd_tail_poles(freq_hz: f64, sample_rate: f64) -> (f64, f64) {
    if (sample_rate - 48000.0).abs() < 1.0e-9 {
        return cut_odd_tail_poles_48k(freq_hz);
    }
    let sr_scale = sample_rate / 48000.0;
    cut_odd_tail_poles_48k(freq_hz / sr_scale)
}

pub(super) fn cut_odd_tail_poles_48k(freq_hz: f64) -> (f64, f64) {
    const POLES: &[(f64, f64, f64)] = &[
        (10.0, 0.998691855911958, -0.877290201631075),
        (20.0, 0.997385430252582, -0.768768291838255),
        (50.0, 0.993476387154816, -0.585711220668981),
        (60.0, 0.992176781489861, -0.585711422188186),
        (100.0, 0.986995331630348, -0.585712594645768),
        (120.0, 0.984414765375856, -0.585713400690640),
        (200.0, 0.974159783037736, -0.585718090106843),
        (250.0, 0.967804741085148, -0.585722211233331),
        (500.0, 0.936645991140099, -0.585756538354349),
        (1000.0, 0.877305483721184, -0.585893568732927),
        (2000.0, 0.769662287597341, -0.586437276207160),
        (3000.0, 0.675217229618949, -0.587328038598633),
        (4000.0, 0.592338825613973, -0.588543696387856),
        (5000.0, 0.519589457990445, -0.590055064315925),
        (6000.0, 0.455702719318892, -0.591827544206347),
        (8000.0, 0.350194365473022, -0.596001093719496),
        (10000.0, 0.268426516182656, -0.600746732477198),
        (12000.0, 0.204727930305518, -0.605766227216505),
        (14000.0, 0.154777254914140, -0.610812853713298),
        (16000.0, 0.115302682706168, -0.615704432146813),
        (17000.0, 0.098697760293060, -0.618052497155838),
        (18000.0, 0.083839425829454, -0.620321289327010),
        (19000.0, 0.070513740268875, -0.622503903863575),
        (20000.0, 0.058535061121769, -0.624595900758898),
        (21000.0, 0.047742394741058, -0.626594864880914),
        (22000.0, 0.037996222971866, -0.628499987709256),
    ];
    let fc = freq_hz.clamp(POLES[0].0, POLES[POLES.len() - 1].0);
    for pair in POLES.windows(2) {
        let (f0, p0, n0) = pair[0];
        let (f1, p1, n1) = pair[1];
        if fc <= f1 {
            let t = (fc - f0) / (f1 - f0);
            return (p0 + (p1 - p0) * t, n0 + (n1 - n0) * t);
        }
    }
    let (_, p, n) = POLES[POLES.len() - 1];
    (p, n)
}

pub(super) fn cut_odd_qs(order: usize, user_q: f64) -> Vec<f64> {
    use std::f64::consts::SQRT_2;
    let mut qs = match order {
        3 => vec![SQRT_2],
        5 => vec![
            SQRT_2 / (2.0 * (PI / 10.0).sin()),
            SQRT_2 / (2.0 * (3.0 * PI / 10.0).sin()),
        ],
        _ => Vec::new(),
    };
    if let Some(first) = qs.first_mut() {
        *first *= user_q;
    }
    qs
}

pub(super) fn cut_odd_tail_lowpass(freq_hz: f64, sample_rate: f64) -> Coeffs {
    let (p_pos, p_neg) = cut_odd_tail_poles(freq_hz, sample_rate);
    let a1 = -(p_pos + p_neg);
    let a2 = p_pos * p_neg;
    const Z1: f64 = -0.0910360042671;
    const Z2: f64 = -0.6671638589604;
    let k = (1.0 + a1 + a2) / ((1.0 - Z1) * (1.0 - Z2));
    [1.0, a1, a2, k, -k * (Z1 + Z2), k * Z1 * Z2]
}

pub(super) fn cut_odd_tail_highpass(freq_hz: f64, sample_rate: f64) -> Coeffs {
    let (p_pos, p_neg, z2, k) = cut_odd_tail_highpass_shape(freq_hz, sample_rate);
    let a1 = -(p_pos + p_neg);
    let a2 = p_pos * p_neg;
    const Z1: f64 = 1.0;
    [1.0, a1, a2, k, -k * (Z1 + z2), k * Z1 * z2]
}

pub(super) fn cut_odd_tail_highpass_shape(freq_hz: f64, sample_rate: f64) -> (f64, f64, f64, f64) {
    if (sample_rate - 48000.0).abs() < 1.0e-9 {
        return cut_odd_tail_highpass_shape_48k(freq_hz);
    }
    let sr_scale = sample_rate / 48000.0;
    cut_odd_tail_highpass_shape_48k(freq_hz / sr_scale)
}

pub(super) fn cut_odd_tail_highpass_shape_48k(freq_hz: f64) -> (f64, f64, f64, f64) {
    const SHAPE: &[(f64, f64, f64, f64, f64)] = &[
        (
            10.0,
            0.998691859509884,
            -0.877290518632687,
            -0.877290486395658,
            0.999345804482390,
        ),
        (
            20.0,
            0.997385430252582,
            -0.768768291838243,
            -0.768768209592626,
            0.998692192161009,
        ),
        (
            50.0,
            0.993476387154816,
            -0.585711220727027,
            -0.585710762724846,
            0.996734928619579,
        ),
        (
            60.0,
            0.992176781489861,
            -0.585711422198450,
            -0.585710762677068,
            0.996083692281813,
        ),
        (
            100.0,
            0.986995331630348,
            -0.585712594626105,
            -0.585710762647127,
            0.993484648627242,
        ),
        (
            120.0,
            0.984414765375856,
            -0.585713400700806,
            -0.585710762676955,
            0.992188662442326,
        ),
        (
            200.0,
            0.974159783037736,
            -0.585718090110956,
            -0.585710762670907,
            0.987028162046818,
        ),
        (
            250.0,
            0.967804741085148,
            -0.585722211235148,
            -0.585710762668641,
            0.983821806860326,
        ),
        (
            500.0,
            0.936645991140100,
            -0.585756538354761,
            -0.585710762667145,
            0.968005955617735,
        ),
        (
            1000.0,
            0.877305483721186,
            -0.585893568732804,
            -0.585710762666668,
            0.937425175095089,
        ),
        (
            2000.0,
            0.769662287597345,
            -0.586437276208027,
            -0.585710762666840,
            0.880228409567937,
        ),
        (
            3000.0,
            0.675217229618976,
            -0.587328038601746,
            -0.585710762666805,
            0.827895943166514,
        ),
        (
            4000.0,
            0.592338825614059,
            -0.588543696388543,
            -0.585710762666810,
            0.779966495452376,
        ),
        (
            5000.0,
            0.519589457990627,
            -0.590055064311697,
            -0.585710762666797,
            0.736024312486891,
        ),
        (
            6000.0,
            0.455702719318784,
            -0.591827544211335,
            -0.585710762666788,
            0.695694017828252,
        ),
        (
            8000.0,
            0.350194365472012,
            -0.596001093739233,
            -0.585710762666792,
            0.624545512739474,
        ),
        (
            10000.0,
            0.268426516181077,
            -0.600746732497784,
            -0.585710762666802,
            0.564180473826580,
        ),
        (
            12000.0,
            0.204727930307357,
            -0.605766227213136,
            -0.585710762666789,
            0.512689169432332,
        ),
        (
            14000.0,
            0.154777254928486,
            -0.610812853648994,
            -0.585710762666801,
            0.468517340423049,
        ),
        (
            16000.0,
            0.115302682712088,
            -0.615704432126017,
            -0.585710762666792,
            0.430401787064580,
        ),
        (
            17000.0,
            0.098697760278387,
            -0.618052497214342,
            -0.585710762666805,
            0.413286693063422,
        ),
        (
            18000.0,
            0.083839425803767,
            -0.620321289416559,
            -0.585710762666793,
            0.397317307079040,
        ),
        (
            19000.0,
            0.070513740286545,
            -0.622503903808894,
            -0.585710762666815,
            0.382395269966219,
        ),
        (
            20000.0,
            0.058535061135979,
            -0.624595900723045,
            -0.585710762666801,
            0.368431691354322,
        ),
        (
            21000.0,
            0.047742394739838,
            -0.626594864893093,
            -0.585710762666792,
            0.355346392134530,
        ),
        (
            22000.0,
            0.037996222999245,
            -0.628499987653795,
            -0.585710762666799,
            0.343067273735296,
        ),
    ];
    let fc = freq_hz.clamp(SHAPE[0].0, SHAPE[SHAPE.len() - 1].0);
    for pair in SHAPE.windows(2) {
        let (f0, p0, n0, z0, k0) = pair[0];
        let (f1, p1, n1, z1, k1) = pair[1];
        if fc <= f1 {
            let t = (fc - f0) / (f1 - f0);
            return (
                p0 + (p1 - p0) * t,
                n0 + (n1 - n0) * t,
                z0 + (z1 - z0) * t,
                k0 + (k1 - k0) * t,
            );
        }
    }
    let (_, p, n, z, k) = SHAPE[SHAPE.len() - 1];
    (p, n, z, k)
}
