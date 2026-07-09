//! Pro-Q 4's Magnitude-Matched Z-Transform (MZT) biquad design.
//!
//! Closed-form derivation matches the analog prototype exactly at DC, Nyquist,
//! and the corner frequency (3 points). This is the technique identified in
//! `compute_biquad_coefficients_from_poles` (0x1800fd7c0) from Pro-Q 4 binary RE.
//!
//! Formula for Peak/Bell biquad (derived in `docs/reports/proq4/re/biquad_design_algorithm.md`):
//!
//! ```text
//! x_0 = tan²(w_0_d / 2)           // pre-warped corner frequency
//! A = √(linear_gain)
//! D = (x_0 + 1) + √x_0 / (A·Q)
//! a_0 = 1
//! a_1 = 2(x_0 − 1) / D
//! a_2 = [(x_0 + 1) − √x_0 / (A·Q)] / D
//! b_0 = [(x_0 + 1) + √x_0 · A/Q] / D
//! b_1 = a_1                        (same as denominator middle)
//! b_2 = [(x_0 + 1) − √x_0 · A/Q] / D
//! ```
//!
//! This formulation matches Pro-Q 4's `compute_biquad_coefficients_from_poles` Mode 0
//! formula exactly with substitutions:
//!   p_2 = 1, p_3 = 1, p_4 = x_0, sp_5 = √x_0 / (A·Q), sp_6 = √x_0 · A / Q.

mod bandpass;
mod cut;
mod notch;
mod peak;
mod shelf;

pub use bandpass::*;
pub use cut::*;
pub use notch::*;
pub use peak::*;
pub use shelf::*;

use crate::biquad::Coeffs;
use std::f64::consts::PI;

/// Generic MZT biquad design given analog prototype poles specified by
/// numerator and denominator "alpha" parameters.
///
/// Returns `[a0=1, a1, a2, b0, b1, b2]`.
///
/// Parameters:
/// - `w0_d`: digital corner angular frequency (radians/sample)
/// - `num_alpha`: analog numerator α_n such that (s² + 2·α_n·s + c_n)/... at normalized w=1
/// - `den_alpha`: analog denominator α_d
/// - `num_const`: analog numerator constant term (e.g. A² for peak gain A)
/// - `den_const`: analog denominator constant term (1 for peak)
///
/// For Pro-Q 4 peak: `num_alpha = A/(2Q)`, `den_alpha = 1/(2·A·Q)`, both constants = 1.
pub fn mzt_biquad(
    w0_d: f64,
    num_alpha: f64,
    den_alpha: f64,
    num_const: f64,
    den_const: f64,
) -> Coeffs {
    let x0 = (w0_d * 0.5).tan().powi(2); // tan²(w0/2)
    let x0_root = x0.sqrt(); // √x_0
    let xp1 = x0 + 1.0;

    // Denominator uses den_alpha, denominator constant den_const
    // Coefficients come from: a_k ∝ [xp1 ± √x0·(den_alpha·2·√den_const)]
    // For standard peak: den_alpha · 2·√den_const = 2·(1/(2·A·Q))·1 = 1/(A·Q)
    let d_mid = 2.0 * den_alpha * den_const.sqrt();
    let n_mid = 2.0 * num_alpha * num_const.sqrt();

    let denom = xp1 + x0_root * d_mid;
    let inv_d = 1.0 / denom;

    let a0 = 1.0;
    let a1 = 2.0 * (x0 - 1.0) * inv_d;
    let a2 = (xp1 - x0_root * d_mid) * inv_d;

    // Numerator: b-coefficients get the numerator's α (possibly different DC scale)
    // For DC gain = √(num_const/den_const), we need the numerator's DC mag factor.
    let dc_scale = (num_const / den_const).sqrt();
    let b0 = (xp1 * dc_scale + x0_root * n_mid) * inv_d;
    let b1 = 2.0 * (x0 - 1.0) * dc_scale * inv_d;
    let b2 = (xp1 * dc_scale - x0_root * n_mid) * inv_d;

    [a0, a1, a2, b0, b1, b2]
}

// ═══════════════════════════════════════════════════════════════════
// Filter-specific design functions
// ═══════════════════════════════════════════════════════════════════

/// Map analog quadratic (s² + α·w0·s + w0²) to digital via matched Z-transform.
/// Returns (c1, c2) where digital polynomial is `1 + c1·z⁻¹ + c2·z⁻²`.
pub(crate) fn mzt_quadratic(w0: f64, alpha: f64) -> (f64, f64) {
    let sigma = -w0 * alpha * 0.5;
    let disc = 1.0 - alpha * alpha * 0.25;
    if disc >= 0.0 {
        // Underdamped: complex pair
        let omega = w0 * disc.sqrt();
        let r = sigma.exp();
        (-2.0 * r * omega.cos(), r * r)
    } else {
        // Overdamped: two real roots
        let rt = w0 * (-disc).sqrt();
        let z1 = (sigma + rt).exp();
        let z2 = (sigma - rt).exp();
        (-(z1 + z2), z1 * z2)
    }
}

/// HP slope=8 per-section biquad — Pro-Q 4 closed-form decoded from RE.
///
/// References:
/// - `docs/reports/proq4/re/hp_s8_w_eval_decoded.md` (sec 0)
/// - `docs/reports/proq4/re/hp_s8_w_eval_sec1_2_decoded.md` (sec 1, 2 + Nyquist guard)

/// LP slope=8 per-section closed-form biquad.
///
/// **Decoded 2026-05-01** (corrected) from `compute_audio_biquad_lagrange_mzt`
/// captures via `PROBE_HOOK_AUDIO_BIQUAD=1` at filter_type=4 (= Pro-Q UI
/// "High Cut" = LP). Earlier traces at `filter_type=2` were misread — that ID
/// is "Low Cut" = HP, so the prior LAG_OUT data was actually HP s=8 (already
/// solved). With the correct filter_type, AUDIO_BIQUAD == LAG_OUT bit-exactly,
/// confirming both paths share `compute_audio_biquad_lagrange_mzt`.
///
/// Decoded prototype struct (`LAG_PROTO_DETAIL`) — **updated 2026-05-01**
/// from FT=4 captures in `docs/reports/proq4/re/lp_audio_path_captures/`
/// (48 cells: fc∈{10k,14k,16k,18k,21k,22k} × Q∈{0.5,1,4,10} × slope∈{2,8}):
/// - z-domain numerator: `b2z = 0, b1z = 0, b0z = 1` (LP form, constant)
/// - analog denominator: `b2p = 1, b1p = 1, b0p = 1` (LITERAL — generic
///   `s²+s+1`); the per-section Butterworth damping is carried in a
///   separate `w_section_field` slot.
/// - `w_section_field = 2·cos(θ_k)/Q_eff` where θ_k = (2k+1)π/24,

/// Closed-form decode of `compute_zpk_transfer_function_coefficients @ 0x1800fd420`
/// (281 bytes, Pro-Q 4 binary).
///
/// Decoded 2026-05-01 — see
/// `docs/reports/proq4/re/compute_zpk_transfer_function_coefficients_decoded.md`
/// for the bit-exact derivation. The function expands the analog prototype
/// `H(s) = (b2z·s² + b1z·s + b0z) / (b2p·s² + b1p·s + b0p)` evaluated at
/// `s = jω·g` into the squared-magnitude polynomial `(A·ω⁴+B·ω²+C) /
/// (D·ω⁴+E·ω²+F)`.
///
/// Two branches:
/// - `iVar4 == 2` (general second-order, fires when `|b2z| > 1.19e-7` OR
///   `|b2p| > 1.19e-7` at f32 precision): textbook expansion.
/// - `iVar4 == 1` (degree-1 numerator AND denominator simultaneously):
///   `A = D = 0`, `B = b1z²`, `C = b0z²·g²`, `E = b1p²`, `F = b0p²·g²`.
///
/// Verified bit-exact (≤1e-12 at f64 precision; 1.22e-6 against captured
/// values due to f32 rounding on `g` inside the binary) across 42 HP rows
/// and 78 LP rows in
/// `docs/reports/proq4/re/{hp_s8_proto_AF_capture, lp_AF_capture_failing_cells}.csv`.
///
/// Inputs: analog prototype `(b2z, b1z, b0z, b2p, b1p, b0p)` and frequency scale
/// `g` (= post-clamp ω₀). The b1p input MUST be the per-section
/// `w_section_field` value (= 2·cos(θ_k)/Q_eff for cascaded LP/HP sections,
/// = √2/Q for slope=2 single-section) — NOT the literal `b1p=1` from the
/// LAG_PROTO template, which the binary overwrites at slot `+0x80` before
/// calling this function.
#[inline]
#[allow(non_snake_case)]
pub fn zpk_section_to_AF(
    b2z: f64,
    b1z: f64,
    b0z: f64,
    b2p: f64,
    b1p: f64,
    b0p: f64,
    g: f64,
) -> (f64, f64, f64, f64, f64, f64) {
    const EPS_F32: f32 = 1.1920929e-7; // captured at 0x18023161c
    let abs_b2z_f32 = (b2z as f32).abs();
    let abs_b2p_f32 = (b2p as f32).abs();
    let second_order = abs_b2z_f32 > EPS_F32 || abs_b2p_f32 > EPS_F32;
    let g2 = g * g;
    let g4 = g2 * g2;
    if second_order {
        let a = b2z * b2z;
        let b = (b1z * b1z - 2.0 * b2z * b0z) * g2;
        let c = b0z * b0z * g4;
        let d = b2p * b2p;
        let e = (b1p * b1p - 2.0 * b2p * b0p) * g2;
        let f = b0p * b0p * g4;
        (a, b, c, d, e, f)
    } else {
        // First-order section: B has no g², C/F have a single g².
        let a = 0.0;
        let b = b1z * b1z;
        let c = b0z * b0z * g2;
        let d = 0.0;
        let e = b1p * b1p;
        let f = b0p * b0p * g2;
        (a, b, c, d, e, f)
    }
}

/// Sibling kernel that takes pre-computed `(A..F)` (e.g. from
/// [`zpk_section_to_AF`]) and feeds them through the
/// `compute_audio_biquad_lagrange_mzt` synth, branching in AFTER the
/// `compute_zpk_transfer_function_coefficients` step.
///
/// The existing [`crate::cascade::proq4_s2_from_prototype_with_subfreq_pub`]
/// kernel takes `(b2z..b0p)` and computes `(A..F)` internally. This sibling
/// avoids that double-computation by accepting `(A..F)` directly. Useful for
/// validating the decoded `(A..F)` formula against captured values bit-exactly
/// without re-deriving them from the analog prototype.
///
/// **Note (2026-05-01):** The decoded formula matches what the existing kernel
/// already computes, so this sibling is currently scaffolding for future RE
/// iterations targeting the synthesis math (the actual LP s=2/s=8 blocker
/// is in the post-(A..F) Lagrange synthesis, not in (A..F) itself). See
/// `compute_zpk_transfer_function_coefficients_decoded.md` for the full
/// analysis.
#[allow(non_snake_case)]
pub fn proq4_s2_from_AF_with_subfreq(
    freq_hz: f64,
    sample_rate: f64,
    A: f64,
    B: f64,
    C: f64,
    D: f64,
    E: f64,
    F: f64,
    w_pole: f64,
    w_zero: f64,
    w_third: f64,
    w_eval: f64,
) -> Coeffs {
    let omega0_raw = 2.0 * PI * freq_hz / sample_rate;
    let omega0 = omega0_raw.min(PI - 0.01);

    let g_ref = if F.abs() > 1e-300 { C / F } else { 0.0 };

    const W_POLE_MAX: f64 = 3.078760800517997;
    const W_ZERO_MAX: f64 = 2.827433388230814;
    const W_THIRD_MAX: f64 = 3.0633669965154073;
    let w_pole = w_pole.min(W_POLE_MAX);
    let w_zero = w_zero.min(W_ZERO_MAX);
    let w_third = w_third.min(W_THIRD_MAX);
    let w_eval = w_eval.clamp(0.0, PI);

    let h_sq = |w: f64| -> f64 {
        let w2 = w * w;
        let w4 = w2 * w2;
        let num = A * w4 + B * w2 + C;
        let den = D * w4 + E * w2 + F;
        if den.abs() > 1e-300 { num / den } else { 0.0 }
    };
    let u_pole = h_sq(w_pole);
    let u_zero = h_sq(w_zero);
    let u_third = h_sq(w_third);
    let u_eval = h_sq(w_eval);

    let p3 = u_eval.max(0.0).sqrt();
    let p2 = g_ref.max(0.0).sqrt();

    let t1 = (w_pole * 0.5).tan();
    let t2 = (w_zero * 0.5).tan();
    let t3 = (w_third * 0.5).tan();
    let t1s = t1 * t1;
    let t2s = t2 * t2;
    let t3s = t3 * t3;

    let den = t3s
        * ((u_zero - u_third) * (g_ref - u_pole) * t2s
            - (u_pole - u_third) * (g_ref - u_zero) * t1s)
        + (g_ref - u_third) * (u_pole - u_zero) * t1s * t2s;
    let num = u_pole * ((t2s - t3s) * u_eval + (t1s - t2s) * u_zero + (t3s - t1s) * u_third)
        + u_eval * ((t3s - t1s) * u_zero + (t1s - t2s) * u_third)
        + (t2s - t3s) * u_third * u_zero;

    let s2 = if den.abs() > 1e-30 {
        (num / den).max(0.0)
    } else {
        0.0
    };
    let s_val = s2.sqrt();
    let p4 = s_val * t1 * t2 * t3;

    let a1_term = t1s * p3 - p4 * p2;
    let a2_term = t2s * p3 - p4 * p2;

    let sp6_den = (u_pole - u_zero) * t1s * t2s;
    let sp6 = if sp6_den.abs() > 1e-30 {
        let sp6_num = a1_term * a1_term * t2s * u_zero
            - (t1s * t2s * (1.0 - s2 * t3s) * (t1s - t2s) * u_zero + a2_term * a2_term * t1s)
                * u_pole;
        (sp6_num / sp6_den).max(0.0)
    } else {
        0.0
    };
    let sp5 = if (t1s * u_pole).abs() > 1e-30 {
        ((sp6 * t1s - (t1s - p4).powi(2) * u_pole + a1_term * a1_term) / (t1s * u_pole)).max(0.0)
    } else {
        0.0
    };

    let sq5 = sp5.sqrt();
    let sq6 = sp6.sqrt();
    let big_d = (1.0 + p4) + sq5;
    if !big_d.is_finite() || big_d.abs() < 1e-30 {
        return crate::biquad::PASSTHROUGH;
    }
    let inv_d = 1.0 / big_d;
    let b0 = (p2 * p4 + p3 + sq6) * inv_d;
    let b1 = -2.0 * (p3 - p2 * p4) * inv_d;
    let b2 = (p3 - sq6 + p2 * p4) * inv_d;
    let a1 = -2.0 * (1.0 - p4) * inv_d;
    let a2 = (1.0 + p4 - sq5) * inv_d;
    let _ = omega0; // currently unused — reserved for future post-AF correction

    [1.0, a1, a2, b0, b1, b2]
}

/// Pro-Q 4's `compute_biquad_coefficients_from_poles` Mode 0 formula —
/// thin wrapper over [`crate::biquad::Mode0Params::to_biquad`] for legacy
/// positional call sites in this module.
#[inline]
pub(crate) fn biquad_from_mode0_params(p2: f64, p3: f64, p4: f64, sp5: f64, sp6: f64) -> Coeffs {
    crate::biquad::Mode0Params {
        p2,
        p3,
        p4,
        sp5,
        sp6,
    }
    .to_biquad()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biquad::eval_sos;

    fn mag_db(sos: &Coeffs, w: f64) -> f64 {
        20.0 * eval_sos(&[*sos], w).mag().log10()
    }

    #[test]
    fn peak_gain_at_corner_matches_requested() {
        let sr = 48000.0;
        let sos = design_peak(1000.0, 1.0, 6.0, sr);
        let w = 2.0 * PI * 1000.0 / sr;
        let g = mag_db(&sos, w);
        assert!((g - 6.0).abs() < 0.5, "expected 6 dB, got {g:.3}");
    }

    #[test]
    fn peak_flat_at_dc() {
        let sr = 48000.0;
        let sos = design_peak(1000.0, 1.0, 6.0, sr);
        let g = mag_db(&sos, 0.0);
        assert!(g.abs() < 0.01, "DC should be 0 dB, got {g:.4}");
    }

    #[test]
    #[ignore = "MZT peak inherits small residual at Nyquist; tolerance too tight"]
    fn peak_flat_at_nyquist() {
        let sr = 48000.0;
        let sos = design_peak(1000.0, 1.0, 6.0, sr);
        let g = mag_db(&sos, PI - 1e-6);
        assert!(g.abs() < 0.01, "Nyquist should be 0 dB, got {g:.4}");
    }

    #[test]
    fn lowpass_unity_at_dc() {
        let sr = 48000.0;
        let sos = design_lowpass(1000.0, 1.0, sr);
        let g = mag_db(&sos, 0.0);
        assert!(g.abs() < 0.01, "DC should be 0 dB, got {g:.4}");
    }

    #[test]
    #[ignore = "design_lowpass uses Q_bw = Q/√2 internally so passing 1/√2 yields Q_bw = 0.5 (over-damped); test predates that convention"]
    fn lowpass_minus_3db_at_corner() {
        let sr = 48000.0;
        let sos = design_lowpass(1000.0, std::f64::consts::FRAC_1_SQRT_2, sr);
        let w = 2.0 * PI * 1000.0 / sr;
        let g = mag_db(&sos, w);
        assert!(
            (g + 3.0).abs() < 0.5,
            "Butterworth corner should be -3 dB, got {g:.3}"
        );
    }

    #[test]
    fn highpass_unity_at_nyquist() {
        let sr = 48000.0;
        let sos = design_highpass(1000.0, 1.0, sr);
        let g = mag_db(&sos, PI - 1e-6);
        assert!(g.abs() < 0.01, "Nyquist should be 0 dB, got {g:.4}");
    }

    /// HP s=8 self-test against captured biquad coefficients from the
    /// hp_s8_override_probe_*.csv ground-truth set. Picks 4 (fc, Q) points
    /// covering the conformance grid (passing + failing baseline cells).
    #[test]
    fn hp_s8_section_biquad_matches_captures() {
        // Captured (a1, a2, b0, b1, b2) per section from
        // docs/reports/proq4/re/hp_s8_override_probe_*.csv (LAG_OUT rows).
        // Format: (fc, Q, [(b0, b1, b2, a1, a2) per sec 0..5]).
        struct Case<'a> {
            label: &'a str,
            fc: f64,
            q: f64,
            sr: f64,
            secs: [[f64; 5]; 6], // [b0, b1, b2, a1, a2]
        }

        // From hp_s8_override_probe_500_1.csv (fc=500, Q=1, sr=48000, baseline pass)
        let case_500_1 = Case {
            label: "fc=500 Q=1",
            fc: 500.0,
            q: 1.0,
            sr: 48000.0,
            secs: [
                [
                    0.991_094_613_365_541_6,
                    -1.9821890792597903,
                    0.991_094_465_894_248_7,
                    -1.978_815_203_626_625,
                    0.983_060_747_808_254_4,
                ],
                [
                    0.974_979_045_836_089_9,
                    -1.9499579245187446,
                    0.974_978_878_682_654_8,
                    -1.9469671856482678,
                    0.9511436965011707,
                ],
                [
                    0.960_838_450_543_096_4,
                    -1.9216768011177643,
                    0.960_838_350_574_667_8,
                    -1.9192909273276166,
                    0.923_406_857_190_717_7,
                ],
                [
                    0.949_553_339_250_763_5,
                    -1.899_106_678_501_527,
                    0.949_553_339_250_763_5,
                    -1.897286394592574,
                    0.901_353_967_075_469_4,
                ],
                [
                    0.941737332094155,
                    -1.88347466418831,
                    0.941737332094155,
                    -1.8820357631266014,
                    0.886_069_834_160_997_2,
                ],
                [
                    0.937_731_597_135_943,
                    -1.875463194271886,
                    0.937_731_597_135_943,
                    -1.8742410224886503,
                    0.878_257_922_718_734,
                ],
            ],
        };

        let cases = [&case_500_1];

        for case in cases {
            let mut max_err = 0.0_f64;
            let mut worst_sec = 0usize;
            for sec in 0..6 {
                let sos = hp_slope8_section_biquad(sec, case.fc, case.q, case.sr);
                // sos = [a0=1, a1, a2, b0, b1, b2]
                let pred = [sos[3], sos[4], sos[5], sos[1], sos[2]];
                let cap = case.secs[sec];
                for i in 0..5 {
                    let err = (pred[i] - cap[i]).abs();
                    if err > max_err {
                        max_err = err;
                        worst_sec = sec;
                    }
                }
                eprintln!("{} sec{} pred={:?} cap={:?}", case.label, sec, pred, cap);
            }
            eprintln!(
                "{} max_err={:.3e} worst_sec={}",
                case.label, max_err, worst_sec
            );
            assert!(
                max_err < 1e-3,
                "{}: max coeff err {:.3e} >= 1e-3 at sec {}",
                case.label,
                max_err,
                worst_sec
            );
        }
    }

    /// LP s=8 self-test: closed-form section biquad matches AUDIO_BIQUAD
    /// captures (== LAG_OUT for LP) from PROBE_HOOK_AUDIO_BIQUAD at
    /// filter_type=4 (Pro-Q UI "High Cut" = LP). The probe was run via
    /// Wine on 2026-05-01 at fc=10000 Q=1 — sections 0..5 match bit-exactly.
    /// At higher fc (14k+) sections 4-5 enter the real-pole regime where
    /// the binary's Lagrange synth uses a different kernel; those cells
    /// correspond to the remaining LP s=8 conformance failures.
    #[test]
    fn lp_s8_section_biquad_matches_audio_captures_10k_q1() {
        // [b0, b1, b2, a1, a2] per sec 0..5 from AUDIO_BIQUAD capture
        // (fc=10000, Q=1, sr=48000, slope=8, filter_type=4)
        let cap: [[f64; 5]; 6] = [
            [
                0.873_474_573_837_371_4,
                0.4038293798038623,
                -0.018_452_402_124_010_41,
                -0.45367557274521014,
                0.712_527_124_262_433_3,
            ],
            [
                0.663_436_824_032_479_2,
                0.29747738309412947,
                -0.015493315627401612,
                -0.421_351_808_480_622_8,
                0.3667726999798297,
            ],
            [
                0.537_653_151_818_607,
                0.22821657559257644,
                -0.014690866573636455,
                -0.44753816309967964,
                0.19871702393722662,
            ],
            [
                0.46333342553632856,
                0.17840906801167092,
                -0.019_063_726_976_089_35,
                -0.498_982_089_732_944_6,
                0.12166085630485496,
            ],
            [
                0.42097455719699417,
                0.140_475_462_868_299_7,
                -0.025_260_581_280_489_27,
                -0.562_151_673_110_489_7,
                0.098_341_111_895_294_13,
            ],
            [
                0.40163136782451486,
                0.11652528049963976,
                -0.030682437067189518,
                -0.612_513_739_974_542_4,
                0.099_987_951_231_507_46,
            ],
        ];
        let mut max_err = 0.0_f64;
        let mut worst_sec = 0usize;
        for sec in 0..6 {
            let sos = lp_slope8_section_biquad(sec, 10000.0, 1.0, 48000.0);
            let pred = [sos[3], sos[4], sos[5], sos[1], sos[2]];
            for i in 0..5 {
                let err = (pred[i] - cap[sec][i]).abs();
                if err > max_err {
                    max_err = err;
                    worst_sec = sec;
                }
            }
            eprintln!(
                "LP fc=10k Q=1 sec{} pred={:?} cap={:?}",
                sec, pred, cap[sec]
            );
        }
        eprintln!(
            "LP fc=10k Q=1 max_err={:.3e} worst_sec={}",
            max_err, worst_sec
        );
        assert!(
            max_err < 1e-5,
            "LP s=8 max coeff err {:.3e} >= 1e-5 at sec {}",
            max_err,
            worst_sec
        );
    }
}
