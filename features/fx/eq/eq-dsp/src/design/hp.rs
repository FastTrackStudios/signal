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
        // Six sections is twelve poles — 72 dB/oct. Order 8 (48 dB/oct) used
        // to land here too and came out a whole slope step too steep; it
        // belongs on the plain four-section Butterworth cascade below.
        _ if n >= 6 => s8::cascade(freq_hz, q, sample_rate),
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
        (10.0, 0.998_691_855_911_958, -0.877_290_201_631_075),
        (20.0, 0.997_385_430_252_582, -0.768_768_291_838_255),
        (50.0, 0.993_476_387_154_816, -0.585_711_220_668_981),
        (60.0, 0.992_176_781_489_861, -0.585_711_422_188_186),
        (100.0, 0.986_995_331_630_348, -0.585_712_594_645_768),
        (120.0, 0.984_414_765_375_856, -0.585_713_400_690_640),
        (200.0, 0.974_159_783_037_736, -0.585_718_090_106_843),
        (250.0, 0.967_804_741_085_148, -0.585_722_211_233_331),
        (500.0, 0.936_645_991_140_099, -0.585_756_538_354_349),
        (1000.0, 0.877_305_483_721_184, -0.585_893_568_732_927),
        (2000.0, 0.769_662_287_597_341, -0.586_437_276_207_160),
        (3000.0, 0.675_217_229_618_949, -0.587_328_038_598_633),
        (4000.0, 0.592_338_825_613_973, -0.588_543_696_387_856),
        (5000.0, 0.519_589_457_990_445, -0.590_055_064_315_925),
        (6000.0, 0.455_702_719_318_892, -0.591_827_544_206_347),
        (8000.0, 0.350_194_365_473_022, -0.596_001_093_719_496),
        (10000.0, 0.268_426_516_182_656, -0.600_746_732_477_198),
        (12000.0, 0.204_727_930_305_518, -0.605_766_227_216_505),
        (14000.0, 0.154_777_254_914_140, -0.610_812_853_713_298),
        (16000.0, 0.115_302_682_706_168, -0.615_704_432_146_813),
        (17000.0, 0.098_697_760_293_060, -0.618_052_497_155_838),
        (18000.0, 0.083_839_425_829_454, -0.620_321_289_327_010),
        (19000.0, 0.070_513_740_268_875, -0.622_503_903_863_575),
        (20000.0, 0.058_535_061_121_769, -0.624_595_900_758_898),
        (21000.0, 0.047_742_394_741_058, -0.626_594_864_880_914),
        (22000.0, 0.037_996_222_971_866, -0.628_499_987_709_256),
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
    const Z1: f64 = -0.091_036_004_267_1;
    const Z2: f64 = -0.667_163_858_960_4;
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
            0.998_691_859_509_884,
            -0.877_290_518_632_687,
            -0.877_290_486_395_658,
            0.999_345_804_482_390,
        ),
        (
            20.0,
            0.997_385_430_252_582,
            -0.768_768_291_838_243,
            -0.768_768_209_592_626,
            0.998_692_192_161_009,
        ),
        (
            50.0,
            0.993_476_387_154_816,
            -0.585_711_220_727_027,
            -0.585_710_762_724_846,
            0.996_734_928_619_579,
        ),
        (
            60.0,
            0.992_176_781_489_861,
            -0.585_711_422_198_450,
            -0.585_710_762_677_068,
            0.996_083_692_281_813,
        ),
        (
            100.0,
            0.986_995_331_630_348,
            -0.585_712_594_626_105,
            -0.585_710_762_647_127,
            0.993_484_648_627_242,
        ),
        (
            120.0,
            0.984_414_765_375_856,
            -0.585_713_400_700_806,
            -0.585_710_762_676_955,
            0.992_188_662_442_326,
        ),
        (
            200.0,
            0.974_159_783_037_736,
            -0.585_718_090_110_956,
            -0.585_710_762_670_907,
            0.987_028_162_046_818,
        ),
        (
            250.0,
            0.967_804_741_085_148,
            -0.585_722_211_235_148,
            -0.585_710_762_668_641,
            0.983_821_806_860_326,
        ),
        (
            500.0,
            0.936_645_991_140_100,
            -0.585_756_538_354_761,
            -0.585_710_762_667_145,
            0.968_005_955_617_735,
        ),
        (
            1000.0,
            0.877_305_483_721_186,
            -0.585_893_568_732_804,
            -0.585_710_762_666_668,
            0.937_425_175_095_089,
        ),
        (
            2000.0,
            0.769_662_287_597_345,
            -0.586_437_276_208_027,
            -0.585_710_762_666_840,
            0.880_228_409_567_937,
        ),
        (
            3000.0,
            0.675_217_229_618_976,
            -0.587_328_038_601_746,
            -0.585_710_762_666_805,
            0.827_895_943_166_514,
        ),
        (
            4000.0,
            0.592_338_825_614_059,
            -0.588_543_696_388_543,
            -0.585_710_762_666_810,
            0.779_966_495_452_376,
        ),
        (
            5000.0,
            0.519_589_457_990_627,
            -0.590_055_064_311_697,
            -0.585_710_762_666_797,
            0.736_024_312_486_891,
        ),
        (
            6000.0,
            0.455_702_719_318_784,
            -0.591_827_544_211_335,
            -0.585_710_762_666_788,
            0.695_694_017_828_252,
        ),
        (
            8000.0,
            0.350_194_365_472_012,
            -0.596_001_093_739_233,
            -0.585_710_762_666_792,
            0.624_545_512_739_474,
        ),
        (
            10000.0,
            0.268_426_516_181_077,
            -0.600_746_732_497_784,
            -0.585_710_762_666_802,
            0.564_180_473_826_580,
        ),
        (
            12000.0,
            0.204_727_930_307_357,
            -0.605_766_227_213_136,
            -0.585_710_762_666_789,
            0.512_689_169_432_332,
        ),
        (
            14000.0,
            0.154_777_254_928_486,
            -0.610_812_853_648_994,
            -0.585_710_762_666_801,
            0.468_517_340_423_049,
        ),
        (
            16000.0,
            0.115_302_682_712_088,
            -0.615_704_432_126_017,
            -0.585_710_762_666_792,
            0.430_401_787_064_580,
        ),
        (
            17000.0,
            0.098_697_760_278_387,
            -0.618_052_497_214_342,
            -0.585_710_762_666_805,
            0.413_286_693_063_422,
        ),
        (
            18000.0,
            0.083_839_425_803_767,
            -0.620_321_289_416_559,
            -0.585_710_762_666_793,
            0.397_317_307_079_040,
        ),
        (
            19000.0,
            0.070_513_740_286_545,
            -0.622_503_903_808_894,
            -0.585_710_762_666_815,
            0.382_395_269_966_219,
        ),
        (
            20000.0,
            0.058_535_061_135_979,
            -0.624_595_900_723_045,
            -0.585_710_762_666_801,
            0.368_431_691_354_322,
        ),
        (
            21000.0,
            0.047_742_394_739_838,
            -0.626_594_864_893_093,
            -0.585_710_762_666_792,
            0.355_346_392_134_530,
        ),
        (
            22000.0,
            0.037_996_222_999_245,
            -0.628_499_987_653_795,
            -0.585_710_762_666_799,
            0.343_067_273_735_296,
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
