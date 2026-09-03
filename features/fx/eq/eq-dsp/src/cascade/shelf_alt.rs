//! Shelf-alt and flat-tilt cascade builders for Pro-Q 4.

use crate::biquad::Coeffs;

use super::*;

/// Compute cascade biquads for the shelf-alt filter (type 12 / 0xc).
///
/// From `compute_cascade_coefficients` @ 0x1800fec20:
/// - Always exactly 3 sections (`param_3` == 0xc path)
/// - Uses gain^(1/4) scaling on pole/zero positions
/// - Hardcoded frequency ladder: base constants * `32^section_index`
/// - Real poles and zeros (no imaginary component)
/// - Produces z-domain poles/zeros directly (transform type 0)
///
/// Constants from binary:
///   `DAT_180232030` = -0.01313900648833929 (base pole/zero 1)
///   `DAT_180232038` = -0.07432544468767008 (base pole/zero 2)
///   `DAT_180231c58` = 32.0 (section spacing)
///   `DAT_180231bd8` = 5.656854249492381 = 4*sqrt(2) (inter-section gain scaling)
#[must_use]
pub fn compute_cascade_shelf_alt(
    _freq_hz: f64,
    _q: f64,
    gain_db: f64,
    _sample_rate: f64,
    _order: usize,
) -> Vec<Coeffs> {
    if gain_db.abs() < 0.001 {
        return vec![PASSTHROUGH; 3];
    }

    // Convert dB to linear gain
    let gain_linear = 10.0_f64.powf(gain_db / 20.0);

    // Binary: param_4 = SQRT(param_4); dVar24 = sqrt(param_4)
    // So gain_sqrt = gain^(1/2), gain_quarter = gain^(1/4)
    let gain_sqrt = gain_linear.sqrt();
    let gain_quarter = gain_sqrt.sqrt();
    let inv_gain_quarter = 1.0 / gain_quarter;

    // Hardcoded constants from binary
    const BASE_1: f64 = -0.013_139_006_488_339_29; // DAT_180232030
    const BASE_2: f64 = -0.074_325_444_687_670_08; // DAT_180232038
    const SECTION_SPACING: f64 = 32.0; // DAT_180231c58
    const INTER_GAIN: f64 = 5.656_854_249_492_381; // DAT_180231bd8 = 4*sqrt(2)

    // Build 3 sections, each with 2 real poles and 2 real zeros
    // Section k uses frequencies: base * SECTION_SPACING^k
    let mut sections = Vec::with_capacity(3);
    let mut freq_1 = BASE_1;
    let mut freq_2 = BASE_2;
    let mut section_gain = gain_sqrt; // Binary: param_1[0x11] = param_4 (= sqrt(gain))

    for _ in 0..3 {
        // Zeros scaled by gain^(1/4), poles scaled by 1/gain^(1/4)
        let z1 = freq_1 * gain_quarter;
        let z2 = freq_2 * gain_quarter;
        let p1 = freq_1 * inv_gain_quarter;
        let p2 = freq_2 * inv_gain_quarter;

        // Convert 2-real-pole, 2-real-zero ZPK section to biquad
        // a0=1, a1=-(p1+p2), a2=p1*p2, b0=section_gain, b1=-(z1+z2)*section_gain, b2=z1*z2*section_gain
        let a0 = 1.0;
        let a1 = -(p1 + p2);
        let a2 = p1 * p2;
        let b0 = section_gain;
        let b1 = -(z1 + z2) * section_gain;
        let b2 = z1 * z2 * section_gain;

        sections.push([a0, a1, a2, b0, b1, b2]);

        // Step frequencies for next section
        freq_1 *= SECTION_SPACING;
        freq_2 *= SECTION_SPACING;
        // Step gain: binary multiplies by DAT_180231bd8 between sections
        section_gain *= INTER_GAIN;
    }

    sections
}
/// Compute cascade biquads for the Flat Tilt filter (UI type 8).
///
/// **RE-decoded structure** (see
/// `docs/reports/proq4/re/flattilt_proq4_pipeline.md` and
/// `flattilt_formula.md`).  Pro-Q 4's `FlatTilt` always emits **exactly 3
/// distinct sections** regardless of slope/Q — slope and Q are ignored
/// entirely at this stage.  Probe captures across (fc, Q, slope) confirm:
///
///   - The 3 digital biquads are emitted post-prewarped-bilinear from the
///     analog-form 3-section ladder (R = 32, K ≈ 2.79886).
///   - `(a1, a2, b1/b0, b2/b0)` per section depend on **`gain_dB` only**
///     (fc-, Q-, slope-independent — bit-identical across the captured
///     fc/Q/slope grid to 13+ decimals).
///   - `ln(b0)` per section is **odd** in g (`b0(g) · b0(−g) = 1` exactly),
///     and decomposes as `g · (C_sec + F(ln fc))` for mid-range
///     fc ∈ [50, 10000] Hz.
///
/// Polynomial coefficients fitted from `tools/proq4_probe/sweep_audio_biquad.sh`
/// at `filter_type=8` (`FlatTilt`) over (fc, gain) sweeps — see
/// `flattilt_proq4_pipeline.md`.  Max abs err vs probe captures:
/// sec0/sec1 < 1e-9, sec2 < 5e-6 on the (a1, a2, b1/b0, b2/b0) fit;
/// b0 fit < 1e-3 in ln(b0) on |g| ≤ 12 (≈ 0.01 dB).  At extreme fc / |g|
/// ≥ 18 Pro-Q applies a soft clamp not captured by this smooth model.
#[must_use]
pub fn compute_cascade_flat_tilt(
    freq_hz: f64,
    _q: f64,
    gain_db: f64,
    _sample_rate: f64,
    _order: usize,
) -> Vec<Coeffs> {
    if gain_db.abs() < 0.001 {
        return vec![PASSTHROUGH; 3];
    }

    let g = gain_db;

    fn poly6(g: f64, c: [f64; 7]) -> f64 {
        let mut s = c[6];
        for k in (0..6).rev() {
            s = s * g + c[k];
        }
        s
    }

    // ── Section 0 (low-band) ──
    let a1_s0 = poly6(
        g,
        [
            -1.992_692_536_860_356_3,
            0.000_209_753_084_677_340_4,
            3.002_154_431_867_738_5e-06,
            2.848_799_821_073_583_4e-08,
            2.004_609_984_838_383e-10,
            1.109_699_998_613_083_7e-12,
            4.870_319_575_536_682e-15,
        ],
    );
    let a2_s0 = poly6(
        g,
        [
            0.992_699_365_709_348_4,
            -0.000_209_360_698_763_802_34,
            -2.990_891_713_536_880_3e-06,
            -2.827_294_471_277_730_2e-08,
            -1.973_843_189_594_546_2e-10,
            -1.073_241_694_180_157_9e-12,
            -4.528_179_605_329_451e-15,
        ],
    );
    let b1b0_s0 = poly6(
        g,
        [
            -1.992_692_536_872_318_7,
            -0.000_209_753_089_671_891_95,
            3.002_154_723_420_854e-06,
            -2.848_789_398_922_205_8e-08,
            2.004_568_266_785_799_3e-10,
            -1.110_172_168_493_570_7e-12,
            4.890_361_326_473_601_4e-15,
        ],
    );
    let b2b0_s0 = poly6(
        g,
        [
            0.992_699_365_721_423_5,
            0.000_209_360_703_752_087_54,
            -2.990_892_006_044_58e-06,
            2.827_284_056_604_082_4e-08,
            -1.973_801_457_669_405_8e-10,
            1.073_713_520_405_974_3e-12,
            -4.548_207_471_750_509e-15,
        ],
    );

    // ── Section 1 (mid-band) ──
    let a1_s1 = poly6(
        g,
        [
            -1.784_443_454_831_328,
            0.005_696_718_213_032_868,
            6.849_350_401_132_077e-05,
            4.260_506_503_616_277e-07,
            3.301_197_418_839_762_5e-10,
            -2.203_934_637_650_939_2e-11,
            -1.509_350_710_351_175_6e-13,
        ],
    );
    let a2_s1 = poly6(
        g,
        [
            0.790_706_166_022_757_9,
            -0.005_356_314_173_722_063,
            -5.951_546_344_610_963_6e-05,
            -2.756_822_002_724_874_6e-07,
            1.408_081_471_392_087_6e-09,
            3.575_881_334_689_599e-11,
            2.143_673_949_117_008_4e-13,
        ],
    );
    let b1b0_s1 = poly6(
        g,
        [
            -1.784_443_453_044_233_5,
            -0.005_696_718_247_663_22,
            6.849_347_493_716_487e-05,
            -4.260_502_564_743_535_5e-07,
            3.302_979_336_144_011_7e-10,
            2.203_756_756_230_279_8e-11,
            -1.511_383_230_359_411_1e-13,
        ],
    );
    let b2b0_s1 = poly6(
        g,
        [
            0.790_706_164_345_764_8,
            0.005_356_314_203_811_757,
            -5.951_543_631_240_163e-05,
            2.756_818_595_550_316_4e-07,
            1.407_915_947_277_809_8e-09,
            -3.575_728_696_492_522e-11,
            2.145_566_827_731_940_8e-13,
        ],
    );

    // ── Section 2 (high-band) ──
    let a1_s2 = poly6(
        g,
        [
            -0.005_632_379_286_931_274,
            0.013_979_874_567_800_341,
            2.515_291_082_980_285_6e-05,
            -6.206_904_094_847_639e-07,
            4.793_840_229_294_326e-08,
            8.072_123_114_726_494e-12,
            7.184_872_605_790_285e-11,
        ],
    );
    let a2_s2 = poly6(
        g,
        [
            -0.100_592_946_530_763_38,
            0.002_445_972_584_531_427_4,
            2.675_604_198_872_257_7e-05,
            -1.019_316_567_786_714_4e-07,
            -5.369_047_738_247_244e-09,
            1.321_190_604_758_911_4e-09,
            -1.517_690_410_580_43e-11,
        ],
    );
    let b1b0_s2 = poly6(
        g,
        [
            -0.005_632_374_944_741_661_5,
            -0.013_979_874_582_119_857,
            2.515_284_211_512_645e-05,
            6.206_906_650_560_4e-07,
            4.793_881_315_307_385e-08,
            -8.073_809_031_896_64e-12,
            7.184_825_335_182_062e-11,
        ],
    );
    let b2b0_s2 = poly6(
        g,
        [
            -0.100_592_945_767_278_33,
            -0.002_445_972_597_720_834_4,
            2.675_602_966_318_711e-05,
            1.019_317_912_711_003_6e-07,
            -5.368_974_412_107_201e-09,
            -1.321_191_132_922_681_1e-09,
            -1.517_697_373_844_341_3e-11,
        ],
    );

    // b0 per section: ln(b0) = g · (F_sec(ln fc) + g²·G_sec(ln fc)).
    // Decomposition is exactly odd in g (per reciprocity b0(g)·b0(-g) = 1
    // verified to 1e-9 across captures).  Higher-order term is required
    // because the linear-in-g portion has small g²-symmetric residual at
    // mid fc (per `flattilt_proq4_pipeline.md`).  Fits restricted to
    // |g| ≤ 12 (test grid range); at extreme |g| ≥ 18 Pro-Q applies a
    // soft clamp not modeled here.
    let lf = freq_hz.ln();
    fn poly7(x: f64, c: [f64; 8]) -> f64 {
        let mut s = c[7];
        for k in (0..7).rev() {
            s = s * x + c[k];
        }
        s
    }
    fn poly4(x: f64, c: [f64; 5]) -> f64 {
        let mut s = c[4];
        for k in (0..4).rev() {
            s = s * x + c[k];
        }
        s
    }

    // Section 0
    let f_s0 = poly7(
        lf,
        [
            0.128_904_826_771_966_44,
            -0.011_076_617_826_249_566,
            2.202_551_352_231_801e-06,
            -7.232_733_657_254_718e-07,
            1.352_596_406_529_664_5e-07,
            -1.448_010_708_861_024_6e-08,
            8.260_241_090_275_101e-10,
            -1.946_713_840_011_496_6e-11,
        ],
    );
    let g_coef_s0 = poly4(
        lf,
        [
            -2.933_443_218_908_337_2e-08,
            -2.569_616_074_071_628_2e-11,
            1.418_383_101_442_965_3e-11,
            -2.087_564_966_600_914e-12,
            1.084_112_099_745_412_9e-13,
        ],
    );
    let b0_s0 = (g * (f_s0 + g * g * g_coef_s0)).exp();

    // Section 1
    let f_s1 = poly7(
        lf,
        [
            0.122_694_287_124_831_78,
            -0.011_076_621_546_169_398,
            2.204_775_171_713_488_8e-06,
            -7.239_779_292_698_593e-07,
            1.353_880_212_371_941_8e-07,
            -1.449_360_380_221_411_8e-08,
            8.267_848_786_166_165e-10,
            -1.948_494_086_704_327_5e-11,
        ],
    );
    let g_coef_s1 = poly4(
        lf,
        [
            -7.657_806_177_514_374e-07,
            -2.568_655_757_651_314_4e-11,
            1.418_110_699_391_359_8e-11,
            -2.087_247_775_544_410_4e-12,
            1.083_982_243_407_473_1e-13,
        ],
    );
    let b0_s1 = (g * (f_s1 + g * g * g_coef_s1)).exp();

    // Section 2
    let f_s2 = poly7(
        lf,
        [
            0.050_738_784_465_770_105,
            -0.011_076_600_274_579_78,
            2.192_032_326_303_338_7e-06,
            -7.199_306_763_939_582e-07,
            1.346_490_952_525_294_6e-07,
            -1.441_576_290_942_335_8e-08,
            8.223_882_502_550_958e-10,
            -1.938_187_248_283_264_5e-11,
        ],
    );
    let g_coef_s2 = poly4(
        lf,
        [
            5.131_158_124_039_21e-07,
            -2.569_710_512_284_468_5e-11,
            1.418_416_845_944_977_2e-11,
            -2.087_611_256_295_646_7e-12,
            1.084_133_470_229_396_7e-13,
        ],
    );
    let b0_s2 = (g * (f_s2 + g * g * g_coef_s2)).exp();

    vec![
        [1.0, a1_s0, a2_s0, b0_s0, b1b0_s0 * b0_s0, b2b0_s0 * b0_s0],
        [1.0, a1_s1, a2_s1, b0_s1, b1b0_s1 * b0_s1, b2b0_s1 * b0_s1],
        [1.0, a1_s2, a2_s2, b0_s2, b1b0_s2 * b0_s2, b2b0_s2 * b0_s2],
    ]
}
