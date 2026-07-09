//! Shelf-alt and flat-tilt cascade builders for Pro-Q 4.

use crate::biquad::Coeffs;

use super::*;

/// Compute cascade biquads for the shelf-alt filter (type 12 / 0xc).
///
/// From compute_cascade_coefficients @ 0x1800fec20:
/// - Always exactly 3 sections (param_3 == 0xc path)
/// - Uses gain^(1/4) scaling on pole/zero positions
/// - Hardcoded frequency ladder: base constants * 32^section_index
/// - Real poles and zeros (no imaginary component)
/// - Produces z-domain poles/zeros directly (transform type 0)
///
/// Constants from binary:
///   DAT_180232030 = -0.01313900648833929 (base pole/zero 1)
///   DAT_180232038 = -0.07432544468767008 (base pole/zero 2)
///   DAT_180231c58 = 32.0 (section spacing)
///   DAT_180231bd8 = 5.656854249492381 = 4*sqrt(2) (inter-section gain scaling)
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
    // So gain_sqrt = gain^(1/2), gain_qrt = gain^(1/4)
    let gain_sqrt = gain_linear.sqrt();
    let gain_qrt = gain_sqrt.sqrt();
    let inv_gain_qrt = 1.0 / gain_qrt;

    // Hardcoded constants from binary
    const BASE_1: f64 = -0.01313900648833929; // DAT_180232030
    const BASE_2: f64 = -0.07432544468767008; // DAT_180232038
    const SECTION_SPACING: f64 = 32.0; // DAT_180231c58
    const INTER_GAIN: f64 = 5.656854249492381; // DAT_180231bd8 = 4*sqrt(2)

    // Build 3 sections, each with 2 real poles and 2 real zeros
    // Section k uses frequencies: base * SECTION_SPACING^k
    let mut sections = Vec::with_capacity(3);
    let mut freq_1 = BASE_1;
    let mut freq_2 = BASE_2;
    let mut section_gain = gain_sqrt; // Binary: param_1[0x11] = param_4 (= sqrt(gain))

    for _ in 0..3 {
        // Zeros scaled by gain^(1/4), poles scaled by 1/gain^(1/4)
        let z1 = freq_1 * gain_qrt;
        let z2 = freq_2 * gain_qrt;
        let p1 = freq_1 * inv_gain_qrt;
        let p2 = freq_2 * inv_gain_qrt;

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
/// `flattilt_formula.md`).  Pro-Q 4's FlatTilt always emits **exactly 3
/// distinct sections** regardless of slope/Q — slope and Q are ignored
/// entirely at this stage.  Probe captures across (fc, Q, slope) confirm:
///
///   - The 3 digital biquads are emitted post-prewarped-bilinear from the
///     analog-form 3-section ladder (R = 32, K ≈ 2.79886).
///   - `(a1, a2, b1/b0, b2/b0)` per section depend on **gain_dB only**
///     (fc-, Q-, slope-independent — bit-identical across the captured
///     fc/Q/slope grid to 13+ decimals).
///   - `ln(b0)` per section is **odd** in g (`b0(g) · b0(−g) = 1` exactly),
///     and decomposes as `g · (C_sec + F(ln fc))` for mid-range
///     fc ∈ [50, 10000] Hz.
///
/// Polynomial coefficients fitted from `tools/proq4_probe/sweep_audio_biquad.sh`
/// at filter_type=8 (FlatTilt) over (fc, gain) sweeps — see
/// `flattilt_proq4_pipeline.md`.  Max abs err vs probe captures:
/// sec0/sec1 < 1e-9, sec2 < 5e-6 on the (a1, a2, b1/b0, b2/b0) fit;
/// b0 fit < 1e-3 in ln(b0) on |g| ≤ 12 (≈ 0.01 dB).  At extreme fc / |g|
/// ≥ 18 Pro-Q applies a soft clamp not captured by this smooth model.
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
            -1.9926925368603563,
            0.0002097530846773404,
            3.0021544318677385e-06,
            2.8487998210735834e-08,
            2.004609984838383e-10,
            1.1096999986130837e-12,
            4.870319575536682e-15,
        ],
    );
    let a2_s0 = poly6(
        g,
        [
            0.9926993657093484,
            -0.00020936069876380234,
            -2.9908917135368803e-06,
            -2.8272944712777302e-08,
            -1.9738431895945462e-10,
            -1.0732416941801579e-12,
            -4.528179605329451e-15,
        ],
    );
    let b1b0_s0 = poly6(
        g,
        [
            -1.9926925368723187,
            -0.00020975308967189195,
            3.002154723420854e-06,
            -2.8487893989222058e-08,
            2.0045682667857993e-10,
            -1.1101721684935707e-12,
            4.8903613264736014e-15,
        ],
    );
    let b2b0_s0 = poly6(
        g,
        [
            0.9926993657214235,
            0.00020936070375208754,
            -2.99089200604458e-06,
            2.8272840566040824e-08,
            -1.9738014576694058e-10,
            1.0737135204059743e-12,
            -4.548207471750509e-15,
        ],
    );

    // ── Section 1 (mid-band) ──
    let a1_s1 = poly6(
        g,
        [
            -1.784443454831328,
            0.005696718213032868,
            6.849350401132077e-05,
            4.260506503616277e-07,
            3.3011974188397625e-10,
            -2.2039346376509392e-11,
            -1.5093507103511756e-13,
        ],
    );
    let a2_s1 = poly6(
        g,
        [
            0.7907061660227579,
            -0.005356314173722063,
            -5.9515463446109636e-05,
            -2.7568220027248746e-07,
            1.4080814713920876e-09,
            3.575881334689599e-11,
            2.1436739491170084e-13,
        ],
    );
    let b1b0_s1 = poly6(
        g,
        [
            -1.7844434530442335,
            -0.00569671824766322,
            6.849347493716487e-05,
            -4.2605025647435355e-07,
            3.3029793361440117e-10,
            2.2037567562302798e-11,
            -1.5113832303594111e-13,
        ],
    );
    let b2b0_s1 = poly6(
        g,
        [
            0.7907061643457648,
            0.005356314203811757,
            -5.951543631240163e-05,
            2.7568185955503164e-07,
            1.4079159472778098e-09,
            -3.575728696492522e-11,
            2.1455668277319408e-13,
        ],
    );

    // ── Section 2 (high-band) ──
    let a1_s2 = poly6(
        g,
        [
            -0.005632379286931274,
            0.013979874567800341,
            2.5152910829802856e-05,
            -6.206904094847639e-07,
            4.793840229294326e-08,
            8.072123114726494e-12,
            7.184872605790285e-11,
        ],
    );
    let a2_s2 = poly6(
        g,
        [
            -0.10059294653076338,
            0.0024459725845314274,
            2.6756041988722577e-05,
            -1.0193165677867144e-07,
            -5.369047738247244e-09,
            1.3211906047589114e-09,
            -1.51769041058043e-11,
        ],
    );
    let b1b0_s2 = poly6(
        g,
        [
            -0.0056323749447416615,
            -0.013979874582119857,
            2.515284211512645e-05,
            6.2069066505604e-07,
            4.793881315307385e-08,
            -8.07380903189664e-12,
            7.184825335182062e-11,
        ],
    );
    let b2b0_s2 = poly6(
        g,
        [
            -0.10059294576727833,
            -0.0024459725977208344,
            2.675602966318711e-05,
            1.0193179127110036e-07,
            -5.368974412107201e-09,
            -1.3211911329226811e-09,
            -1.5176973738443413e-11,
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
            0.12890482677196644,
            -0.011076617826249566,
            2.202551352231801e-06,
            -7.232733657254718e-07,
            1.3525964065296645e-07,
            -1.4480107088610246e-08,
            8.260241090275101e-10,
            -1.9467138400114966e-11,
        ],
    );
    let g_coef_s0 = poly4(
        lf,
        [
            -2.9334432189083372e-08,
            -2.5696160740716282e-11,
            1.4183831014429653e-11,
            -2.087564966600914e-12,
            1.0841120997454129e-13,
        ],
    );
    let b0_s0 = (g * (f_s0 + g * g * g_coef_s0)).exp();

    // Section 1
    let f_s1 = poly7(
        lf,
        [
            0.12269428712483178,
            -0.011076621546169398,
            2.2047751717134888e-06,
            -7.239779292698593e-07,
            1.3538802123719418e-07,
            -1.4493603802214118e-08,
            8.267848786166165e-10,
            -1.9484940867043275e-11,
        ],
    );
    let g_coef_s1 = poly4(
        lf,
        [
            -7.657806177514374e-07,
            -2.5686557576513144e-11,
            1.4181106993913598e-11,
            -2.0872477755444104e-12,
            1.0839822434074731e-13,
        ],
    );
    let b0_s1 = (g * (f_s1 + g * g * g_coef_s1)).exp();

    // Section 2
    let f_s2 = poly7(
        lf,
        [
            0.050738784465770105,
            -0.01107660027457978,
            2.1920323263033387e-06,
            -7.199306763939582e-07,
            1.3464909525252946e-07,
            -1.4415762909423358e-08,
            8.223882502550958e-10,
            -1.9381872482832645e-11,
        ],
    );
    let g_coef_s2 = poly4(
        lf,
        [
            5.13115812403921e-07,
            -2.5697105122844685e-11,
            1.4184168459449772e-11,
            -2.0876112562956467e-12,
            1.0841334702293967e-13,
        ],
    );
    let b0_s2 = (g * (f_s2 + g * g * g_coef_s2)).exp();

    vec![
        [1.0, a1_s0, a2_s0, b0_s0, b1b0_s0 * b0_s0, b2b0_s0 * b0_s0],
        [1.0, a1_s1, a2_s1, b0_s1, b1b0_s1 * b0_s1, b2b0_s1 * b0_s1],
        [1.0, a1_s2, a2_s2, b0_s2, b1b0_s2 * b0_s2, b2b0_s2 * b0_s2],
    ]
}
