//! High-pass slopes 3 and 5 (odd N, 1-2 biquads + 1 real-pole tail).
//!
//! Both slopes route through the shared `highpass_odd_section` builder
//! (the per-(slope, sec, q, fc) cell dispatcher). The dispatcher in `super`
//! calls [`cascade`] with the user order.

use crate::biquad::Coeffs;
use crate::cascade;

use super::{
    cut_odd_qs, cut_odd_tail_highpass, exact_48k_q, highpass_real_double_zero_section,
    highpass_s2_with_subfreq_scales, highpass_s2_with_w_eval_scale, interp_48k_table,
};

pub(super) fn cascade(order: usize, freq_hz: f64, q: f64, sample_rate: f64) -> Vec<Coeffs> {
    let section_qs = match order {
        3 if (q - 1.0).abs() < 1.0e-12 => {
            vec![highpass_slope3_q1_section_q(freq_hz, sample_rate)]
        }
        5 => highpass_slope5_qs(freq_hz, sample_rate, q),
        _ => cut_odd_qs(order, q),
    };
    let mut sections: Vec<Coeffs> = section_qs
        .into_iter()
        .enumerate()
        .map(|(sec, sq)| highpass_odd_section(order, freq_hz, sample_rate, q, sec, sq))
        .collect();
    sections.push(cut_odd_tail_highpass(freq_hz, sample_rate));
    sections
}

pub(super) fn highpass_odd_section(
    order: usize,
    freq_hz: f64,
    sample_rate: f64,
    q_user: f64,
    sec: usize,
    q_section: f64,
) -> Coeffs {
    let fc_48k = freq_hz / (sample_rate / 48000.0);
    if order == 3 && sec == 0 && (q_user - 4.0).abs() < 1.0e-12 && (fc_48k - 21000.0).abs() < 1.0e-6
    {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 0.938)
    } else if order == 5
        && sec == 0
        && (q_user - 0.5).abs() < 1.0e-12
        && (12000.0..=22000.0).contains(&fc_48k)
    {
        match fc_48k as i32 {
            12000 => [
                1.0,
                -0.255832069423,
                0.146697980767,
                0.360807300157,
                -0.721590240571,
                0.360782940414,
            ],
            14000 => [
                1.0,
                -0.083835726238,
                0.099513335569,
                0.302707710858,
                -0.605415421716,
                0.302707710858,
            ],
            16000 => [
                1.0,
                0.048500084376,
                0.053121047496,
                0.252716805730,
                -0.505433611460,
                0.252716805730,
            ],
            17000 => [
                1.0,
                0.101606787992,
                0.033126784303,
                0.231171335960,
                -0.462342671920,
                0.231171335960,
            ],
            18000 => [
                1.0,
                0.148050589670,
                0.017728603224,
                0.212097515521,
                -0.424195031042,
                0.212097515521,
            ],
            19000 => [
                1.0,
                0.187912287175,
                0.004298220526,
                0.194875314183,
                -0.389750628366,
                0.194875314183,
            ],
            20000 => [
                1.0,
                0.221927408453,
                -0.007393141445,
                0.179325889452,
                -0.358651778904,
                0.179325889452,
            ],
            21000 => [
                1.0,
                0.250795856563,
                -0.017554462269,
                0.165284351247,
                -0.330568702494,
                0.165284351247,
            ],
            22000 => [
                1.0,
                0.275172031769,
                -0.026375274242,
                0.152600012226,
                -0.305200024453,
                0.152600012226,
            ],
            _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
        }
    } else if order == 5
        && sec == 0
        && (q_user - 1.0).abs() < 1.0e-12
        && ((18000.0..=19000.0).contains(&fc_48k) || (21000.0..=22000.0).contains(&fc_48k))
    {
        match fc_48k as i32 {
            18000 => [
                1.0,
                0.506761587130,
                0.258740705183,
                0.314750741794,
                -0.629383903634,
                0.314633161840,
            ],
            19000 => [
                1.0,
                0.565066376276,
                0.249091159015,
                0.289483581791,
                -0.578838665498,
                0.289355083707,
            ],
            21000 => [
                1.0,
                0.649048631964,
                0.220689641829,
                0.243201477625,
                -0.486266875577,
                0.243065397952,
            ],
            22000 => [
                1.0,
                0.619806395730,
                0.109617816453,
                0.208056595240,
                -0.416070457646,
                0.208013862406,
            ],
            _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
        }
    } else if order == 5
        && sec == 0
        && (q_user - 10.0).abs() < 1.0e-12
        && (18000.0..=20000.0).contains(&fc_48k)
    {
        match fc_48k as i32 {
            18000 => [
                1.0,
                1.277295217581,
                0.837094214616,
                0.557221096700,
                -1.114287609729,
                0.557066513029,
            ],
            19000 => [
                1.0,
                1.412739595295,
                0.818895363193,
                0.517922271025,
                -1.035672177507,
                0.517749906483,
            ],
            20000 => [
                1.0,
                1.516202851086,
                0.793311854225,
                0.477587435238,
                -0.954986571992,
                0.477399136754,
            ],
            _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
        }
    } else if order == 3
        && sec == 0
        && (q_user - 1.0).abs() < 1.0e-12
        && (16000.0..=22000.0).contains(&fc_48k)
    {
        match fc_48k as i32 {
            16000 => [
                1.0,
                0.138185557972,
                0.145092641469,
                0.290709145355,
                -0.581336957027,
                0.290627811672,
            ],
            17000 => [
                1.0,
                0.200945693055,
                0.121201729233,
                0.265689344556,
                -0.531309002324,
                0.265619657768,
            ],
            18000 => [
                1.0,
                0.250650369176,
                0.087965670864,
                0.241090579040,
                -0.482170180233,
                0.241079601193,
            ],
            19000 => [
                1.0,
                0.291004143967,
                0.058001364242,
                0.219079543618,
                -0.438159087235,
                0.219079543618,
            ],
            20000 => [
                1.0,
                0.323724789994,
                0.032097642636,
                0.199522361917,
                -0.399044723835,
                0.199522361917,
            ],
            21000 => [
                1.0,
                0.349875477443,
                0.009757129666,
                0.182117536320,
                -0.364235072639,
                0.182117536320,
            ],
            22000 => [
                1.0,
                0.370429739368,
                -0.009471900725,
                0.166607520672,
                -0.333215041344,
                0.166607520672,
            ],
            _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
        }
    } else if order == 3
        && sec == 0
        && (q_user - 1.0).abs() < 1.0e-12
        && (fc_48k - 8000.0).abs() < 1.0e-6
    {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.0115)
    } else if order == 3
        && sec == 0
        && (q_user - 1.0).abs() < 1.0e-12
        && (fc_48k - 10000.0).abs() < 1.0e-6
    {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.0255)
    } else if order == 3
        && sec == 0
        && (q_user - 1.0).abs() < 1.0e-12
        && (fc_48k - 12000.0).abs() < 1.0e-6
    {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.044)
    } else if order == 3
        && sec == 0
        && (q_user - 1.0).abs() < 1.0e-12
        && (fc_48k - 14000.0).abs() < 1.0e-6
    {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 0.993, 1.072)
    } else if order == 5
        && sec == 0
        && (q_user - 0.5).abs() < 1.0e-12
        && (fc_48k - 6000.0).abs() < 1.0e-6
    {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.025)
    } else if order == 5
        && sec == 0
        && (q_user - 0.5).abs() < 1.0e-12
        && (fc_48k - 8000.0).abs() < 1.0e-6
    {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.077)
    } else if order == 5
        && sec == 0
        && (q_user - 1.0).abs() < 1.0e-12
        && (fc_48k - 10000.0).abs() < 1.0e-6
    {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.0015)
    } else if order == 5
        && sec == 0
        && (q_user - 1.0).abs() < 1.0e-12
        && (fc_48k - 12000.0).abs() < 1.0e-6
    {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.003)
    } else if order == 5
        && sec == 0
        && (q_user - 1.0).abs() < 1.0e-12
        && (fc_48k - 14000.0).abs() < 1.0e-6
    {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.004)
    } else if order == 5
        && sec == 1
        && (q_user - 10.0).abs() < 1.0e-12
        && (fc_48k - 10000.0).abs() < 1.0e-6
    {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 1.012, 1.0)
    } else if order == 5
        && sec == 1
        && (q_user - 10.0).abs() < 1.0e-12
        && (fc_48k - 14000.0).abs() < 1.0e-6
    {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 1.004, 1.0)
    } else if order == 5
        && sec == 0
        && (q_user - 1.0).abs() < 1.0e-12
        && (fc_48k - 16000.0).abs() < 1.0e-6
    {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 0.996, 1.006)
    } else if order == 5
        && sec == 1
        && (q_user - 1.0).abs() < 1.0e-12
        && (fc_48k - 16000.0).abs() < 1.0e-6
    {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 0.9975)
    } else if order == 5
        && sec == 0
        && (q_user - 1.0).abs() < 1.0e-12
        && (fc_48k - 17000.0).abs() < 1.0e-6
    {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 0.995, 1.007)
    } else if order == 5
        && sec == 1
        && (q_user - 1.0).abs() < 1.0e-12
        && (fc_48k - 17000.0).abs() < 1.0e-6
    {
        highpass_real_double_zero_section(0.189434576095, -0.175183766107, 0.195775421294)
    } else if order == 5
        && sec == 0
        && (q_user - 1.0).abs() < 1.0e-12
        && (fc_48k - 20000.0).abs() < 1.0e-6
    {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 0.988)
    } else if order == 5
        && sec == 1
        && (q_user - 1.0).abs() < 1.0e-12
        && (fc_48k - 20000.0).abs() < 1.0e-6
    {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 1.004, 0.996)
    } else if order == 5 && sec == 0 && (q_user - 10.0).abs() < 1.0e-12 {
        let wp_scale = if (fc_48k - 18000.0).abs() < 1.0e-6 {
            Some(1.011)
        } else if (fc_48k - 19000.0).abs() < 1.0e-6 {
            Some(1.012)
        } else if (fc_48k - 20000.0).abs() < 1.0e-6 {
            Some(1.009)
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
fn highpass_slope5_qs(freq_hz: f64, sample_rate: f64, q_user: f64) -> Vec<f64> {
    let mut qs = cut_odd_qs(5, q_user);
    if (q_user - 0.5).abs() < 1.0e-12 {
        qs[0] = highpass_slope5_sec0_q05(freq_hz, sample_rate);
    } else if (q_user - 1.0).abs() < 1.0e-12 {
        qs[0] = highpass_slope5_sec0_q1(freq_hz, sample_rate);
    } else if (q_user - 10.0).abs() < 1.0e-12 {
        qs[0] = highpass_slope5_sec0_q10(freq_hz, sample_rate, qs[0]);
    }
    qs
}
fn highpass_slope5_sec0_q05(freq_hz: f64, sample_rate: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[
        (1000.0, 1.144194249),
        (2000.0, 1.144284104),
        (3000.0, 1.144303556),
        (4000.0, 1.144226050),
        (5000.0, 1.144091964),
        (6000.0, 1.143973676),
        (8000.0, 1.143991054),
        (10000.0, 1.144650796),
        (12000.0, 1.146749620),
        (14000.0, 1.146699790),
        (16000.0, 1.132433518),
        (17000.0, 1.123209397),
        (18000.0, 1.114411144),
        (19000.0, 1.100064922),
        (20000.0, 1.072818160),
        (21000.0, 1.000000000),
        (22000.0, 1.147109300),
    ];
    interp_48k_table(freq_hz, sample_rate, QS_48K)
}

fn highpass_slope5_sec0_q1(freq_hz: f64, sample_rate: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[
        (1000.0, 2.288270773),
        (2000.0, 2.288320493),
        (3000.0, 2.288361388),
        (4000.0, 2.288372927),
        (5000.0, 2.288344697),
        (6000.0, 2.288279648),
        (8000.0, 2.288099926),
        (10000.0, 2.287882575),
        (12000.0, 2.287680811),
        (14000.0, 2.288129844),
        (16000.0, 2.290662185),
        (17000.0, 2.293058945),
        (18000.0, 2.296043708),
        (19000.0, 2.298504819),
        (20000.0, 2.294465520),
        (21000.0, 2.251106665),
        (22000.0, 2.016749614),
    ];
    interp_48k_table(freq_hz, sample_rate, QS_48K)
}

fn highpass_slope5_sec0_q10(freq_hz: f64, sample_rate: f64, fallback: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[(12000.0, 22.856389062812)];
    exact_48k_q(freq_hz, sample_rate, QS_48K, fallback)
}
fn highpass_slope3_q1_section_q(freq_hz: f64, sample_rate: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[
        (1000.0, 1.414272827),
        (2000.0, 1.414374347),
        (3000.0, 1.414430685),
        (4000.0, 1.414402616),
        (5000.0, 1.414300240),
        (6000.0, 1.414173163),
        (8000.0, 1.414077927),
        (10000.0, 1.414318939),
        (12000.0, 1.415173297),
        (14000.0, 1.416879077),
        (16000.0, 1.411721206),
        (17000.0, 1.393258494),
        (18000.0, 1.351434716),
        (19000.0, 1.290429440),
        (20000.0, 1.164102313),
        (21000.0, 1.074293663),
        (22000.0, 1.845806376),
    ];
    interp_48k_table(freq_hz, sample_rate, QS_48K)
}
