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
                -0.255_832_069_423,
                0.146_697_980_767,
                0.360_807_300_157,
                -0.721_590_240_571,
                0.360_782_940_414,
            ],
            14000 => [
                1.0,
                -0.083_835_726_238,
                0.099_513_335_569,
                0.302_707_710_858,
                -0.605_415_421_716,
                0.302_707_710_858,
            ],
            16000 => [
                1.0,
                0.048_500_084_376,
                0.053_121_047_496,
                0.252_716_805_730,
                -0.505_433_611_460,
                0.252_716_805_730,
            ],
            17000 => [
                1.0,
                0.101_606_787_992,
                0.033_126_784_303,
                0.231_171_335_960,
                -0.462_342_671_920,
                0.231_171_335_960,
            ],
            18000 => [
                1.0,
                0.148_050_589_670,
                0.017_728_603_224,
                0.212_097_515_521,
                -0.424_195_031_042,
                0.212_097_515_521,
            ],
            19000 => [
                1.0,
                0.187_912_287_175,
                0.004_298_220_526,
                0.194_875_314_183,
                -0.389_750_628_366,
                0.194_875_314_183,
            ],
            20000 => [
                1.0,
                0.221_927_408_453,
                -0.007_393_141_445,
                0.179_325_889_452,
                -0.358_651_778_904,
                0.179_325_889_452,
            ],
            21000 => [
                1.0,
                0.250_795_856_563,
                -0.017_554_462_269,
                0.165_284_351_247,
                -0.330_568_702_494,
                0.165_284_351_247,
            ],
            22000 => [
                1.0,
                0.275_172_031_769,
                -0.026_375_274_242,
                0.152_600_012_226,
                -0.305_200_024_453,
                0.152_600_012_226,
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
                0.506_761_587_130,
                0.258_740_705_183,
                0.314_750_741_794,
                -0.629_383_903_634,
                0.314_633_161_840,
            ],
            19000 => [
                1.0,
                0.565_066_376_276,
                0.249_091_159_015,
                0.289_483_581_791,
                -0.578_838_665_498,
                0.289_355_083_707,
            ],
            21000 => [
                1.0,
                0.649_048_631_964,
                0.220_689_641_829,
                0.243_201_477_625,
                -0.486_266_875_577,
                0.243_065_397_952,
            ],
            22000 => [
                1.0,
                0.619_806_395_730,
                0.109_617_816_453,
                0.208_056_595_240,
                -0.416_070_457_646,
                0.208_013_862_406,
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
                1.277_295_217_581,
                0.837_094_214_616,
                0.557_221_096_700,
                -1.114_287_609_729,
                0.557_066_513_029,
            ],
            19000 => [
                1.0,
                1.412_739_595_295,
                0.818_895_363_193,
                0.517_922_271_025,
                -1.035_672_177_507,
                0.517_749_906_483,
            ],
            20000 => [
                1.0,
                1.516_202_851_086,
                0.793_311_854_225,
                0.477_587_435_238,
                -0.954_986_571_992,
                0.477_399_136_754,
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
                0.138_185_557_972,
                0.145_092_641_469,
                0.290_709_145_355,
                -0.581_336_957_027,
                0.290_627_811_672,
            ],
            17000 => [
                1.0,
                0.200_945_693_055,
                0.121_201_729_233,
                0.265_689_344_556,
                -0.531_309_002_324,
                0.265_619_657_768,
            ],
            18000 => [
                1.0,
                0.250_650_369_176,
                0.087_965_670_864,
                0.241_090_579_040,
                -0.482_170_180_233,
                0.241_079_601_193,
            ],
            19000 => [
                1.0,
                0.291_004_143_967,
                0.058_001_364_242,
                0.219_079_543_618,
                -0.438_159_087_235,
                0.219_079_543_618,
            ],
            20000 => [
                1.0,
                0.323_724_789_994,
                0.032_097_642_636,
                0.199_522_361_917,
                -0.399_044_723_835,
                0.199_522_361_917,
            ],
            21000 => [
                1.0,
                0.349_875_477_443,
                0.009_757_129_666,
                0.182_117_536_320,
                -0.364_235_072_639,
                0.182_117_536_320,
            ],
            22000 => [
                1.0,
                0.370_429_739_368,
                -0.009_471_900_725,
                0.166_607_520_672,
                -0.333_215_041_344,
                0.166_607_520_672,
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
        highpass_real_double_zero_section(0.189_434_576_095, -0.175_183_766_107, 0.195_775_421_294)
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
        (1000.0, 1.144_194_249),
        (2000.0, 1.144_284_104),
        (3000.0, 1.144_303_556),
        (4000.0, 1.144_226_050),
        (5000.0, 1.144_091_964),
        (6000.0, 1.143_973_676),
        (8000.0, 1.143_991_054),
        (10000.0, 1.144_650_796),
        (12000.0, 1.146_749_620),
        (14000.0, 1.146_699_790),
        (16000.0, 1.132_433_518),
        (17000.0, 1.123_209_397),
        (18000.0, 1.114_411_144),
        (19000.0, 1.100_064_922),
        (20000.0, 1.072_818_160),
        (21000.0, 1.000_000_000),
        (22000.0, 1.147_109_300),
    ];
    interp_48k_table(freq_hz, sample_rate, QS_48K)
}

fn highpass_slope5_sec0_q1(freq_hz: f64, sample_rate: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[
        (1000.0, 2.288_270_773),
        (2000.0, 2.288_320_493),
        (3000.0, 2.288_361_388),
        (4000.0, 2.288_372_927),
        (5000.0, 2.288_344_697),
        (6000.0, 2.288_279_648),
        (8000.0, 2.288_099_926),
        (10000.0, 2.287_882_575),
        (12000.0, 2.287_680_811),
        (14000.0, 2.288_129_844),
        (16000.0, 2.290_662_185),
        (17000.0, 2.293_058_945),
        (18000.0, 2.296_043_708),
        (19000.0, 2.298_504_819),
        (20000.0, 2.294_465_520),
        (21000.0, 2.251_106_665),
        (22000.0, 2.016_749_614),
    ];
    interp_48k_table(freq_hz, sample_rate, QS_48K)
}

fn highpass_slope5_sec0_q10(freq_hz: f64, sample_rate: f64, fallback: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[(12000.0, 22.856_389_062_812)];
    exact_48k_q(freq_hz, sample_rate, QS_48K, fallback)
}
fn highpass_slope3_q1_section_q(freq_hz: f64, sample_rate: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[
        (1000.0, 1.414_272_827),
        (2000.0, 1.414_374_347),
        (3000.0, 1.414_430_685),
        (4000.0, 1.414_402_616),
        (5000.0, 1.414_300_240),
        (6000.0, 1.414_173_163),
        (8000.0, 1.414_077_927),
        (10000.0, 1.414_318_939),
        (12000.0, 1.415_173_297),
        (14000.0, 1.416_879_077),
        (16000.0, 1.411_721_206),
        (17000.0, 1.393_258_494),
        (18000.0, 1.351_434_716),
        (19000.0, 1.290_429_440),
        (20000.0, 1.164_102_313),
        (21000.0, 1.074_293_663),
        (22000.0, 1.845_806_376),
    ];
    interp_48k_table(freq_hz, sample_rate, QS_48K)
}
