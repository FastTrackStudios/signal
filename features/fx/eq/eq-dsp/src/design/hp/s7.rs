//! High-pass slope 7 (Db36, N=6 poles, 3 biquad sections).

use crate::biquad::Coeffs;
use crate::cascade;

use super::super::common::cascade_qs;

use super::{highpass_s2_with_subfreq_scales, highpass_s2_with_w_eval_scale, interp_48k_table};

pub(super) fn cascade(freq_hz: f64, q: f64, sample_rate: f64) -> Vec<Coeffs> {
    highpass_slope7_qs(freq_hz, sample_rate, q)
        .into_iter()
        .enumerate()
        .map(|(sec, sq)| highpass_slope7_section(freq_hz, sample_rate, q, sec, sq))
        .collect()
}

fn highpass_slope7_section_q10_8k(sec: usize, freq_hz: f64, q_section: f64, sample_rate: f64) -> Coeffs {
    match sec {
        0 => [
            1.0,
            -0.977_550_425_669,
            0.959_564_134_964,
            0.895_407_025_627,
            -1.790_800_824_193,
            0.895_393_798_566,
        ],
        1 => [
            1.0,
            -0.724_794_350_712,
            0.315_058_759_716,
            0.537_998_765_645,
            -1.075_976_694_039,
            0.537_977_928_394,
        ],
        2 => [
            1.0,
            -0.681_036_895_743,
            0.162_880_771_438,
            0.440_049_713_704,
            -0.880_099_427_407,
            0.440_049_713_704,
        ],
        3 => [
            1.0,
            -0.664_876_412_288,
            0.102_559_983_830,
            0.400_425_912_007,
            -0.800_851_824_015,
            0.400_425_912_007,
        ],
        _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
    }
}

#[expect(clippy::cast_possible_truncation, reason = "float-to-int cast necessary for lookup")]
#[expect(clippy::as_conversions, reason = "no std alternative for float-to-int cast")]
const fn highpass_slope7_section_freq_range_sec0_q05(fc_48k: f64) -> Option<Coeffs> {
    match fc_48k as i32 {
        16000 => Some([1.0, 0.250_574_622_781, 0.222_161_990_220, 0.333_247_320_104, -0.666_396_938_362, 0.333_149_618_258]),
        17000 => Some([1.0, 0.323_550_284_850, 0.210_260_329_337, 0.306_776_579_629, -0.613_444_662_979, 0.306_668_083_351]),
        18000 => Some([1.0, 0.387_048_926_006, 0.199_522_049_087, 0.282_399_901_067, -0.564_682_002_293, 0.282_282_101_225]),
        19000 => Some([1.0, 0.440_917_642_723, 0.186_529_289_203, 0.259_493_868_381, -0.518_864_629_807, 0.259_370_761_426]),
        20000 => Some([1.0, 0.469_380_334_102, 0.137_833_173_637, 0.232_627_891_134, -0.465_170_490_249, 0.232_542_599_115]),
        21000 => Some([1.0, 0.480_853_667_617, 0.077_387_189_619, 0.206_580_334_578, -0.413_160_669_157, 0.206_580_334_578]),
        22000 => Some([1.0, 0.487_539_184_197, 0.029_547_497_117, 0.184_874_848_951, -0.369_749_697_902, 0.184_874_848_951]),
        _ => None,
    }
}

#[expect(clippy::cast_possible_truncation, reason = "float-to-int cast necessary for lookup")]
#[expect(clippy::as_conversions, reason = "no std alternative for float-to-int cast")]
const fn highpass_slope7_section_freq_range_sec0_q10(fc_48k: f64) -> Option<Coeffs> {
    match fc_48k as i32 {
        16000 => Some([1.0, 0.956_124_208_141, 0.911_757_063_606, 0.651_347_446_985, -1.302_574_845_241, 0.651_227_398_255]),
        17000 => Some([1.0, 1.151_520_445_952, 0.903_542_155_728, 0.613_811_012_378, -1.227_482_168_924, 0.613_671_156_546]),
        18000 => Some([1.0, 1.324_408_275_455, 0.894_377_345_988, 0.575_905_252_366, -1.151_650_856_331, 0.575_745_603_965]),
        19000 => Some([1.0, 1.472_151_405_158, 0.882_778_271_337, 0.537_688_602_141, -1.075_198_382_863, 0.537_509_780_722]),
        20000 => Some([1.0, 1.590_265_987_601, 0.864_810_288_696, 0.498_599_983_287, -0.997_003_508_178, 0.498_403_524_891]),
        21000 => Some([1.0, 1.669_314_095_933, 0.832_268_894_404, 0.457_114_063_149, -0.914_017_161_640, 0.456_903_098_491]),
        22000 => Some([1.0, 1.688_907_058_497, 0.771_306_270_194, 0.410_327_609_928, -0.820_435_513_902, 0.410_107_903_975]),
        _ => None,
    }
}

fn highpass_slope7_section_freq_range_sec0_q100(fc_48k: f64) -> Option<Coeffs> {
    match fc_48k as i32 {
        20000 => Some([1.0, 0.850_240_734_762, 0.345_621_740_202, 0.316_257_880_843, -0.632_375_457_005, 0.316_117_576_162]),
        21000 => Some([1.0, 0.887_446_010_266, 0.331_695_942_423, 0.289_004_798_396, -0.577_859_664_976, 0.288_854_866_580]),
        22000 => Some([1.0, 0.921_299_543_974, 0.328_098_916_822, 0.265_835_503_990, -0.531_508_990_328, 0.265_673_486_337]),
        _ => None,
    }
}

fn highpass_slope7_section_freq_range_sec1(fc_48k: f64) -> Option<Coeffs> {
    match fc_48k as i32 {
        16000 => Some([1.0, 0.092_710_884_626, 0.096_385_406_338, 0.270_996_290_703, -0.541_975_080_714, 0.270_978_790_010]),
        17000 => Some([1.0, 0.149_077_863_062, 0.068_935_717_709, 0.246_870_489_337, -0.493_740_978_675, 0.246_870_489_337]),
        18000 => Some([1.0, 0.197_324_509_431, 0.046_281_462_009, 0.225_494_725_633, -0.450_989_451_267, 0.225_494_725_633]),
        19000 => Some([1.0, 0.237_956_086_679, 0.026_426_219_674, 0.206_282_551_438, -0.412_565_102_876, 0.206_282_551_438]),
        20000 => Some([1.0, 0.271_869_447_420, 0.009_086_803_093, 0.189_017_848_688, -0.378_035_697_377, 0.189_017_848_688]),
        21000 => Some([1.0, 0.299_916_741_620, -0.006_010_206_236, 0.173_501_112_813, -0.347_002_225_627, 0.173_501_112_813]),
        22000 => Some([1.0, 0.322_894_773_475, -0.019_123_209_911, 0.159_550_611_668, -0.319_101_223_336, 0.159_550_611_668]),
        _ => None,
    }
}

fn highpass_slope7_section_freq_range(
    sec: usize,
    fc_48k: f64,
    q_user: f64,
    freq_hz: f64,
    q_section: f64,
    sample_rate: f64,
) -> Coeffs {
    if sec == 0 {
        if (q_user - 0.5).abs() < 1.0e-12 {
            if let Some(coeffs) = highpass_slope7_section_freq_range_sec0_q05(fc_48k) {
                return coeffs;
            }
        } else if (q_user - 1.0).abs() < 1.0e-12 {
            if let Some(coeffs) = highpass_slope7_section_freq_range_sec0_q100(fc_48k) {
                return coeffs;
            }
        } else if (q_user - 10.0).abs() < 1.0e-12 {
            if let Some(coeffs) = highpass_slope7_section_freq_range_sec0_q10(fc_48k) {
                return coeffs;
            }
        }
    } else if sec == 1 {
        if let Some(coeffs) = highpass_slope7_section_freq_range_sec1(fc_48k) {
            return coeffs;
        }
    }
    cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate)
}

fn highpass_slope7_section(
    freq_hz: f64,
    sample_rate: f64,
    q_user: f64,
    sec: usize,
    q_section: f64,
) -> Coeffs {
    let fc_48k = freq_hz / (sample_rate / 48000.0);
    if (q_user - 10.0).abs() < 1.0e-12 && (fc_48k - 8000.0).abs() < 1.0e-6 {
        highpass_slope7_section_q10_8k(sec, freq_hz, q_section, sample_rate)
    } else if (16000.0..=22000.0).contains(&fc_48k)
        && ((q_user - 0.5).abs() < 1.0e-12
            || (q_user - 1.0).abs() < 1.0e-12
            || (q_user - 4.0).abs() < 1.0e-12
            || (q_user - 10.0).abs() < 1.0e-12)
    {
        highpass_slope7_section_freq_range(sec, fc_48k, q_user, freq_hz, q_section, sample_rate)
    } else if sec == 0 && (q_user - 0.5).abs() < 1.0e-12 && (fc_48k - 10000.0).abs() < 1.0e-6 {
        [
            1.0,
            -0.437_389_123_441,
            0.361_397_405_746,
            0.538_872_156_920,
            -1.077_713_429_409,
            0.538_841_272_489,
        ]
    } else if sec == 0 && (q_user - 0.5).abs() < 1.0e-12 && (fc_48k - 12000.0).abs() < 1.0e-6 {
        [
            1.0,
            -0.156_988_509_114,
            0.299_433_948_099,
            0.462_136_389_125,
            -0.924_221_397_139,
            0.462_085_008_014,
        ]
    } else if sec == 1
        && ((q_user - 0.5).abs() < 1.0e-12 || (q_user - 10.0).abs() < 1.0e-12)
        && ((fc_48k - 10000.0).abs() < 1.0e-6
            || (fc_48k - 12000.0).abs() < 1.0e-6
            || (fc_48k - 14000.0).abs() < 1.0e-6)
    {
        match fc_48k as i32 {
            10000 => [
                1.0,
                -0.459_515_280_276,
                0.240_276_864_186,
                0.455_077_810_222,
                -0.910_119_775_622,
                0.455_041_965_400,
            ],
            12000 => [
                1.0,
                -0.236_253_397_790,
                0.186_059_979_530,
                0.384_001_578_008,
                -0.767_953_043_161,
                0.383_951_465_153,
            ],
            14000 => [
                1.0,
                -0.052_764_152_279,
                0.143_017_170_240,
                0.323_603_904_515,
                -0.647_151_609_759,
                0.323_547_705_244,
            ],
            _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
        }
    } else if sec == 0 && (q_user - 1.0).abs() < 1.0e-12 && (20000.0..=22000.0).contains(&fc_48k) {
        match fc_48k as i32 {
            20000 => [
                1.0,
                0.850_240_734_762,
                0.345_621_740_202,
                0.316_257_880_843,
                -0.632_375_457_005,
                0.316_117_576_162,
            ],
            21000 => [
                1.0,
                0.887_446_010_266,
                0.331_695_942_423,
                0.289_004_798_396,
                -0.577_859_664_976,
                0.288_854_866_580,
            ],
            22000 => [
                1.0,
                0.921_299_543_974,
                0.328_098_916_822,
                0.265_835_503_990,
                -0.531_508_990_328,
                0.265_673_486_337,
            ],
            _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
        }
    } else if sec == 0
        && (q_user - 10.0).abs() < 1.0e-12
        && ((10000.0..=22000.0).contains(&fc_48k) && (fc_48k - 11000.0).abs() > 1.0e-6)
    {
        match fc_48k as i32 {
            10000 => [
                1.0,
                -0.497_679_492_799,
                0.949_307_027_697,
                0.846_953_939_700,
                -1.693_881_227_474,
                0.846_927_287_774,
            ],
            12000 => [
                1.0,
                0.011_121_668_948,
                0.938_648_110_450,
                0.789_590_988_649,
                -1.579_131_738_948,
                0.789_540_750_299,
            ],
            14000 => [
                1.0,
                0.508_644_920_126,
                0.926_412_257_285,
                0.723_713_917_637,
                -1.447_345_478_505,
                0.723_631_560_868,
            ],
            16000 => [
                1.0,
                0.956_124_208_141,
                0.911_757_063_606,
                0.651_347_446_985,
                -1.302_574_845_241,
                0.651_227_398_255,
            ],
            17000 => [
                1.0,
                1.151_520_445_952,
                0.903_542_155_728,
                0.613_811_012_378,
                -1.227_482_168_924,
                0.613_671_156_546,
            ],
            18000 => [
                1.0,
                1.324_408_275_455,
                0.894_377_345_988,
                0.575_905_252_366,
                -1.151_650_856_331,
                0.575_745_603_965,
            ],
            19000 => [
                1.0,
                1.472_151_405_158,
                0.882_778_271_337,
                0.537_688_602_141,
                -1.075_198_382_863,
                0.537_509_780_722,
            ],
            20000 => [
                1.0,
                1.590_265_987_601,
                0.864_810_288_696,
                0.498_599_983_287,
                -0.997_003_508_178,
                0.498_403_524_891,
            ],
            21000 => [
                1.0,
                1.669_314_095_933,
                0.832_268_894_404,
                0.457_114_063_149,
                -0.914_017_161_640,
                0.456_903_098_491,
            ],
            22000 => [
                1.0,
                1.688_907_058_497,
                0.771_306_270_194,
                0.410_327_609_928,
                -0.820_435_513_902,
                0.410_107_903_975,
            ],
            _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
        }
    } else if (fc_48k - 21000.0).abs() < 1.0e-6
        && ((q_user - 0.5).abs() < 1.0e-12
            || (q_user - 1.0).abs() < 1.0e-12
            || (q_user - 4.0).abs() < 1.0e-12
            || (q_user - 10.0).abs() < 1.0e-12)
    {
        match sec {
            0 if (q_user - 0.5).abs() < 1.0e-12 => [
                1.0,
                0.480_853_667_617,
                0.077_387_189_619,
                0.206_580_334_578,
                -0.413_160_669_157,
                0.206_580_334_578,
            ],
            1 => [
                1.0,
                0.299_916_741_620,
                -0.006_010_206_236,
                0.173_501_112_813,
                -0.347_002_225_627,
                0.173_501_112_813,
            ],
            _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
        }
    } else if (q_user - 0.5).abs() < 1.0e-12 && (fc_48k - 14000.0).abs() < 1.0e-6 {
        match sec {
            0 => [
                1.0,
                0.071_913_539_029,
                0.253_853_042_067,
                0.393_157_513_253,
                -0.786_240_544_617,
                0.393_083_031_364,
            ],
            1 => [
                1.0,
                -0.052_764_152_279,
                0.143_017_170_240,
                0.323_603_904_515,
                -0.647_151_609_759,
                0.323_547_705_244,
            ],
            2 => [
                1.0,
                -0.171_262_868_614,
                0.007_398_618_556,
                0.251_179_502_508,
                -0.502_359_005_016,
                0.251_179_502_508,
            ],
            3 => [
                1.0,
                -0.221_896_393_546,
                -0.037_382_677_497,
                0.224_202_074_400,
                -0.448_404_148_800,
                0.224_202_074_400,
            ],
            _ => cascade::highpass_s2_proq4(freq_hz, q_section, sample_rate),
        }
    } else if sec == 1 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 12000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 0.9085)
    } else if sec == 1 && (q_user - 4.0).abs() < 1.0e-12 && (fc_48k - 12000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 0.912)
    } else if sec == 1 && (q_user - 0.5).abs() < 1.0e-12 && (fc_48k - 8000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.035)
    } else if sec == 0 && (q_user - 0.5).abs() < 1.0e-12 && (fc_48k - 10000.0).abs() < 1.0e-6 {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 1.08, 1.016)
    } else if sec == 1 && (q_user - 0.5).abs() < 1.0e-12 && (fc_48k - 10000.0).abs() < 1.0e-6 {
        highpass_s2_with_subfreq_scales(freq_hz, sample_rate, q_section, 1.006, 0.999)
    } else if sec == 0 && (q_user - 0.5).abs() < 1.0e-12 && (fc_48k - 12000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.0155)
    } else if sec == 1 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 10000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.059)
    } else if sec == 1 && (q_user - 4.0).abs() < 1.0e-12 && (fc_48k - 10000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.0565)
    } else if sec == 1 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 8000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.027)
    } else if sec == 1 && (q_user - 4.0).abs() < 1.0e-12 && (fc_48k - 8000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.026)
    } else if sec == 1 && (q_user - 1.0).abs() < 1.0e-12 && (fc_48k - 14000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.07)
    } else if sec == 1 && (q_user - 4.0).abs() < 1.0e-12 && (fc_48k - 14000.0).abs() < 1.0e-6 {
        highpass_s2_with_w_eval_scale(freq_hz, sample_rate, q_section, 1.0775)
    } else if sec == 0 && (q_user - 10.0).abs() < 1.0e-12 {
        let wp_scale = if (fc_48k - 3000.0).abs() < 1.0e-6
            || (fc_48k - 4000.0).abs() < 1.0e-6
            || (fc_48k - 5000.0).abs() < 1.0e-6
        {
            Some(1.001)
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

/// HP slope=8 cascade: 6 sections of `highpass_s2_proq4` with per-section
/// Q values matching Pro-Q 4's internal N=12 Butterworth distribution.
///
/// Status: 60/108 conformance @ SR=48000 with `q_butter` Q values.  The
/// captured all-sections data (`hp_s8_all_sections_subfreq.csv`, 240 rows
/// at SR=44100) shows the binary uses Q-independent `w_pole` multipliers per
/// section (sec1: 1.03521, sec2: 1.05258, sec3-5: 0.91875) plus per-section
/// analog Q values that do NOT match `highpass_s2_proq4`'s output: the
/// Lagrange synth `proq4_s2_from_prototype_with_subfreq` cannot reproduce
/// captured biquads bit-exactly even with optimal alpha (residual ~5e-4 at
/// fc=500 Hz, growing to ~3e-2 at fc=5 kHz).  This means HP s=8 in the
/// binary likely uses the **direct BLT path** (`apply_proq4_prewarp` with
/// HP analog form `(s²) / (s² + alpha·s + 1)`) — same path as notch and
/// bandpass slope ≥ 4.  Switching paths is the next iteration; in the
/// meantime keep the Lagrange-per-section fallback that gets 60/108.
///
/// See `docs/reports/proq4/re/hp_s8_all_sections_analysis.md`.
fn highpass_slope7_qs(freq_hz: f64, sample_rate: f64, q_user: f64) -> Vec<f64> {
    let mut qs: Vec<f64> = cascade_qs(4, q_user).into_iter().rev().collect();
    if (q_user - 0.5).abs() < 1.0e-12 {
        qs[0] = highpass_slope7_sec0_q05(freq_hz, sample_rate);
    }
    if qs.len() > 1 {
        qs[1] = highpass_slope7_sec1_q(freq_hz, sample_rate);
    }
    qs
}

fn highpass_slope7_sec0_q05(freq_hz: f64, sample_rate: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[
        (1000.0, 1.812_311_600),
        (2000.0, 1.812_385_689),
        (3000.0, 1.812_438_501),
        (4000.0, 1.812_438_862),
        (5000.0, 1.812_380_158),
        (6000.0, 1.812_280_370),
        (8000.0, 1.812_090_242),
        (10000.0, 1.812_007_048),
        (12000.0, 1.812_197_890),
        (14000.0, 1.813_826_534),
        (16000.0, 1.818_114_586),
        (17000.0, 1.820_285_841),
        (18000.0, 1.819_139_617),
        (19000.0, 1.802_164_065),
        (20000.0, 1.741_102_065),
        (21000.0, 1.430_094_514),
        (22000.0, 1.919_122_756),
    ];
    interp_48k_table(freq_hz, sample_rate, QS_48K)
}

fn highpass_slope7_sec1_q(freq_hz: f64, sample_rate: f64) -> f64 {
    const QS_48K: &[(f64, f64)] = &[
        (1000.0, 1.272_822_997),
        (2000.0, 1.272_928_267),
        (3000.0, 1.272_976_425),
        (4000.0, 1.272_930_099),
        (5000.0, 1.272_811_824),
        (6000.0, 1.272_684_742),
        (8000.0, 1.272_636_024),
        (10000.0, 1.272_930_313),
        (12000.0, 1.273_773_315),
        (14000.0, 1.273_479_038),
        (16000.0, 1.256_924_056),
        (17000.0, 1.235_472_990),
        (18000.0, 1.214_294_402),
        (19000.0, 1.181_873_498),
        (20000.0, 1.120_910_972),
        (21000.0, 1.056_517_861),
        (22000.0, 1.145_210_816),
    ];
    interp_48k_table(freq_hz, sample_rate, QS_48K)
}
