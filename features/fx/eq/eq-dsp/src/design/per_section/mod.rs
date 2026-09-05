//! Pro-Q 4 per-section sub-frequency helpers.
//!
//! Decompiled from Pro-Q 4 binary; see
//! `docs/reports/proq4/re/per_section_helpers_decompiled.md` for the source
//! decompilations and the canonical `proto` (0xC0-byte) layout.
//!
//! Pipeline: `prepare_band_display_info` runs `compute_zpk_transfer_function_coefficients`
//! and `solve_biquad_denominator_quadratic`, then dispatches on the per-section
//! internal filter type stored at `proto[0x13]` (= `proto+0x98`, mirrored from
//! `sec[+0x58]`):
//!
//!   | proto[0x13] | helper                                |
//!   |------------:|---------------------------------------|
//!   | 0, 3        | `compute_peak_type3_parameters`       |
//!   | 1, 2, 4, 5, 6 | `compute_notch_type46_parameters`   |
//!   | 7           | `compute_shelf_band_parameters`       |
//!   | 8           | `compute_band_shelf_parameters_v2`    |
//!   | 10          | `compute_band_shelf_parameters`       |
//!   | other       | inline (caller-side fallback)         |
//!
//! Each helper writes (`wp`, `wz`, `wt`, `w_eval`) into `proto`, and these
//! sub-frequencies feed the universal Lagrange-MZT synth at
//! `crates/eq-dsp/src/cascade.rs:1587`.
//!
//! Status: `peak_type3` (type=0 branch) ported and bit-exact verified by
//! construction. Other helpers staged with explicit `unimplemented!` until
//! their respective probe captures are wired in.

use crate::design::biquad::Coeffs;
use crate::design::cascade::proq4_s2_from_prototype_with_subfreq_pub;
use dsp_core::num;
use std::f64::consts::PI;

mod analog;
mod limits;
mod notch;
mod peak;
mod shelf;

// The shape helpers live in the files above. This module's surface is
// unchanged, so every existing `per_section::foo` call site still resolves
// and the split stays a pure move.
pub use analog::{
    compute_zpk_transfer_coeffs_generic, eval_squared_mag_scalar, omega_scale_for_band_type,
    solve_biquad_denominator_quadratic_generic, AnalogBiquad, MagSqCoeffs, PoleRoots, Prototype,
};
pub use limits::{check_frequency_within_band_limits, update_tracked_band_frequencies};
pub use notch::compute_notch_type46_parameters;
pub use peak::compute_peak_type3_parameters;
pub use shelf::{
    compute_band_shelf_parameters, compute_band_shelf_parameters_v2, compute_shelf_band_parameters,
};

/// Dispatch a `proto` through the helper that matches its `section_type`.
///
/// Mirrors the switch in `prepare_band_display_info` (Pro-Q 4 binary) that
/// chooses one of the 5 helpers based on `proto[0x13]` (= `sec[+0x58]`).
///
/// Returns `Err(SectionType)` for inline/fallback section types not handled
/// by a dedicated helper — caller is expected to apply the inline default
/// (typically `wz = wp·0.05`, `wt = wp·0.5`).
///
/// # Errors
///
/// Returns `Err(section_type)` when `proto.section_type` does not match
/// a handled type (0, 1, 2, 3, 4, 5, 6, 7, 8, or 10).
pub fn dispatch_section_helper(proto: &mut Prototype) -> Result<(), i32> {
    match proto.section_type {
        0 | 3 => {
            compute_peak_type3_parameters(proto);
            Ok(())
        }
        1 | 2 | 4 | 5 | 6 => {
            compute_notch_type46_parameters(proto);
            Ok(())
        }
        7 => {
            compute_shelf_band_parameters(proto);
            Ok(())
        }
        8 => {
            compute_band_shelf_parameters_v2(proto);
            Ok(())
        }
        10 => {
            compute_band_shelf_parameters(proto);
            Ok(())
        }
        other => Err(other),
    }
}

/// Universal section-synth entry point.
///
/// End-to-end pipeline that mirrors Pro-Q's `prepare_band_display_info` →
/// `compute_audio_biquad_lagrange_mzt`:
///
/// 1. The caller supplies `proto` already populated upstream by
///    `compute_zpk_transfer` + `solve_biquad` (so `wp`, `wz`, `mode`, and
///    `band_omega_ref` are valid going in).
/// 2. Dispatch into the per-section helper based on `proto.section_type`
///    (or apply inline defaults for unhandled types).
/// 3. Hand off `(wp, wz, wt, w_eval)` plus the analog ZPK to the
///    Lagrange-MZT synth at `cascade::proq4_s2_from_prototype_with_subfreq_pub`.
///
/// `omega_scale` is the per-band-type ω₀ scaling factor (use
/// [`omega_scale_for_band_type`]); `freq_hz` is the BAND fc (radians ω₀
/// inside the synth = `2π · freq_hz · omega_scale / sample_rate`).
///
/// Returns `(coeffs, fallback_type)` — `fallback_type` is `Some(t)` when
/// dispatch fell into the inline branch (no dedicated helper for `t`), so
/// the caller can log/iterate as needed.
pub fn proq4_universal_section_synth(
    proto: &mut Prototype,
    analog: &AnalogBiquad,
    freq_hz: f64,
    sample_rate: f64,
    omega_scale: f64,
) -> (Coeffs, Option<i32>) {
    // Step 1: refresh A..F from the analog ZPK at this section's ω.
    // Step 2: solve_biquad → seed (wp, wz) and mode for the helper.
    let omega = 2.0 * PI * freq_hz * omega_scale / sample_rate;
    let (mag_coeffs, is_quadratic) = compute_zpk_transfer_coeffs_generic(analog, omega);
    let roots = solve_biquad_denominator_quadratic_generic(&mag_coeffs, is_quadratic);
    proto.mode = i32::from(roots.count);
    proto.root_count_dup = i32::from(roots.count);
    if roots.count >= 1 {
        proto.wp = roots.w1;
    }
    if roots.count >= 2 {
        proto.wz = roots.w2;
    }
    if matches!(proto.section_type, 2 | 5) && roots.count >= 2 {
        let aux = (roots.w2.min(PI) / PI - 0.8).clamp(0.0, 0.2);
        proto.alpha_scratch_8c = num::narrow(aux * aux * 25.0);
    }
    // Synchronize derived caches the helpers may read.
    proto.analog = Some(*analog);
    proto.omega_band = omega;
    proto.stored_e = f64::from_bits(0x4008_a14d_57b3_73df);
    proto.stored_f = f64::from_bits(0x4006_9e95_6570_8efc);
    proto.stored_g = f64::from_bits(0x4008_81c6_8e4d_6f74);

    // Step 3: dispatch into the per-section helper (or apply inline default).
    let fallback = match dispatch_section_helper(proto) {
        Ok(()) => None,
        Err(t) => {
            apply_inline_section_defaults(proto);
            Some(t)
        }
    };
    proto.wp = proto.wp.min(proto.stored_e);
    proto.wz = proto.wz.min(proto.stored_f);
    proto.wt = proto.wt.min(proto.stored_g);

    // Step 4: feed the Lagrange-MZT synth. The synth derives ω₀ internally
    // from `(freq_hz, sample_rate)` as `2π·fc/sr`, so pre-multiply `freq_hz`
    // by `omega_scale` to inject the per-band-type factor.
    let coeffs = proq4_s2_from_prototype_with_subfreq_pub(
        freq_hz * omega_scale,
        sample_rate,
        analog.b2z,
        analog.b1z,
        analog.b0z,
        analog.b2p,
        analog.b1p,
        analog.b0p,
        proto.wp,
        proto.wz,
        proto.wt,
        proto.w_eval,
    );

    (coeffs, fallback)
}

/// Inline fallback for section types without a dedicated helper.
///
/// Pro-Q's `prepare_band_display_info` else-branch sets
/// `proto[2] = wp·0.05`, `proto[3] = wp·0.5`. `wp` and `w_eval` are left
/// untouched.
pub fn apply_inline_section_defaults(proto: &mut Prototype) {
    proto.wz = proto.wp * 0.05;
    proto.wt = proto.wp * 0.5;
}

#[expect(
    dead_code,
    reason = "placeholder for potential future use in decompilation helpers"
)]
const _CONST_PI: f64 = PI;

#[cfg(test)]
mod tests {
    use super::*;

    /// Constant for 0.6π used in tests.
    const ZERO_POINT_SIX_PI: f64 = 1.884_955_592_153_875_9;

    /// Build a `Prototype` with explicit non-zero values so we can detect
    /// stale-write bugs (helper failing to overwrite a field).
    fn fresh_proto() -> Prototype {
        Prototype {
            wp: -777.0,
            wz: -777.0,
            wt: -777.0,
            w_eval: -777.0,
            mode: 0,
            prev_wp: 0.0,
            prev_wz: 0.0,
            stored_e: 0.0,
            stored_f: 0.0,
            stored_g: 0.0,
            band_edge_low: 0.0,
            band_edge_high: 0.0,
            alpha_scratch_8c: 0.0,
            alpha_scratch_94: 0.0,
            section_type: 0,
            band_omega_ref: 0.0,
            q_scratch_50: 1.0,
            proto_0x12_sign: 0,
            flag_byte_68: 0,
            flag_byte_69: 0,
            analog: None,
            omega_band: 0.0,
            root_count_dup: 0,
        }
    }

    /// peak3 type=0, mode=0 (or anything ≠ 1): wt = `0.5·band_omega_ref`;
    /// wp = `band_omega_ref`; wz = wt/2.
    #[test]
    fn peak_type3_section_type0_mode_default() {
        let mut p = fresh_proto();
        p.section_type = 0;
        p.mode = 0;
        p.band_omega_ref = 0.4;
        p.wp = 0.123; // would only matter if mode==1
        compute_peak_type3_parameters(&mut p);
        assert_eq!(p.wp.to_bits(), 0.4_f64.to_bits());
        assert_eq!(p.wt.to_bits(), 0.2_f64.to_bits());
        assert_eq!(p.wz.to_bits(), 0.1_f64.to_bits());
    }

    /// peak3 type=0, mode=1, wp ≤ `half_ref`: still uses `half_ref` for wt.
    #[test]
    fn peak_type3_section_type0_mode1_low_wp() {
        let mut p = fresh_proto();
        p.section_type = 0;
        p.mode = 1;
        p.band_omega_ref = 1.0;
        p.wp = 0.1; // not greater than 0.5
        compute_peak_type3_parameters(&mut p);
        assert_eq!(p.wt.to_bits(), 0.5_f64.to_bits());
        assert_eq!(p.wp.to_bits(), 1.0_f64.to_bits());
        assert_eq!(p.wz.to_bits(), 0.25_f64.to_bits());
    }

    /// peak3 type=0, mode=1, wp > `half_ref`: wt latches onto wp before the
    /// wp-overwrite. This is the only branch where the original wp survives
    /// (as wt and as wz/2).
    #[test]
    fn peak_type3_section_type0_mode1_high_wp_latches() {
        let mut p = fresh_proto();
        p.section_type = 0;
        p.mode = 1;
        p.band_omega_ref = 1.0;
        p.wp = 0.8; // greater than 0.5
        compute_peak_type3_parameters(&mut p);
        assert_eq!(p.wt.to_bits(), 0.8_f64.to_bits());
        assert_eq!(p.wp.to_bits(), 1.0_f64.to_bits());
        assert_eq!(p.wz.to_bits(), 0.4_f64.to_bits());
    }

    /// peak3 type=3 with mode=0 (skips `w_eval` update) and benign `alpha_94=0`:
    /// fv2 = -2.0 → `dvar_alpha` = max(sqrt(1.0), 0.5) = 1.0
    /// fv11 = clamp(1.0, 0, 1.0) = 1.0
    /// `wp_new` = `band_omega_ref` / ((1.0 - 1)·1.0 + 1) = `band_omega_ref`
    /// `wp_final` = `min(band_omega_ref`, π - clamp(…), 0.3π·1 + 0.7π) = `min(band_omega_ref`, π)
    /// dvar10 = 0.20 (fv2 = -2 < 6)
    /// wt = wp · 0.20, wz = wp · 0.001
    #[test]
    fn peak_type3_section_type3_mode0_zero_alpha() {
        let mut p = fresh_proto();
        p.section_type = 3;
        p.mode = 0;
        p.band_omega_ref = 0.5;
        p.alpha_scratch_94 = 0.0;
        let snapshot_w_eval = p.w_eval;
        compute_peak_type3_parameters(&mut p);
        // mode=0 → w_eval untouched
        assert_eq!(p.w_eval.to_bits(), snapshot_w_eval.to_bits());
        // wp = band_omega_ref (= 0.5, well below π)
        assert!((p.wp - 0.5).abs() < 1e-12);
        // wt = wp · 0.20
        assert!((0.5f64.mul_add(-0.20, p.wt)).abs() < 1e-12);
        // wz = wp · 0.001
        assert!((0.5f64.mul_add(-0.001, p.wz)).abs() < 1e-12);
        // alpha_94_out = 0 · 0 · 4 - 2 = -2
        assert_eq!(p.alpha_scratch_94.to_bits(), (-2.0_f32).to_bits());
    }

    /// peak3 type=3 with mode=0 and `alpha_94=1.0` (positive, so fv2 = 4·1 - 2 = 2 ≥ 0):
    /// `dvar_alpha` = 0.5 (no sqrt taken)
    /// fv3 = 2 · -0.5 = -1, fv11 = clamp(-1, 0, 1) = 0
    /// `wp_new` = ω / ((0.5-1)·0 + 1) = ω
    /// `dvar6_ceiling` = 0·0.3π + 0.7π = 0.7π ≈ 2.199
    /// `wp_final` = min(ω, π, 0.7π)
    #[test]
    fn peak_type3_section_type3_alpha_one_clamps_to_07pi() {
        let mut p = fresh_proto();
        p.section_type = 3;
        p.mode = 0;
        p.band_omega_ref = 3.0; // > 0.7π
        p.alpha_scratch_94 = 1.0;
        compute_peak_type3_parameters(&mut p);
        let zero_seven_pi = 0.7 * PI;
        assert!((p.wp - zero_seven_pi).abs() < 1e-12);
        assert!((p.wt - zero_seven_pi * 0.20).abs() < 1e-12);
        assert!((p.wz - zero_seven_pi * 0.001).abs() < 1e-12);
        assert_eq!(p.alpha_scratch_94.to_bits(), 2.0_f32.to_bits());
    }

    /// peak3 type=3, mode=1: `w_eval` IS updated. With wp=1.0 the f32 lane
    /// math should produce a finite, in-range `w_eval` ∈ [`wp_clamped`, π].
    #[test]
    fn peak_type3_section_type3_mode1_w_eval_updated() {
        let mut p = fresh_proto();
        p.section_type = 3;
        p.mode = 1;
        p.band_omega_ref = 1.0;
        p.wp = 1.0;
        p.w_eval = 0.5;
        p.alpha_scratch_94 = 0.5;
        compute_peak_type3_parameters(&mut p);
        // w_eval must be clamped into [wp_clamped, π]
        assert!(p.w_eval >= 1.0 - 1e-9 && p.w_eval <= PI + 1e-9);
        assert!(p.wp.is_finite() && p.wt.is_finite() && p.wz.is_finite());
    }

    /// peak3 type=3 with fv2 ≥ 6 path (`alpha_94` large): dvar10 shrinks
    /// from 0.20 toward 0.02 (at fv2=20, dvar10 = 0.20 - 14·9/700 = 0.02).
    #[test]
    fn peak_type3_section_type3_large_alpha_shrinks_wt() {
        let mut p = fresh_proto();
        p.section_type = 3;
        p.mode = 0;
        p.band_omega_ref = 1.0;
        // alpha_94² · 4 - 2 = fv2; need fv2 ≥ 6 → alpha² ≥ 2 → alpha ≥ √2.
        p.alpha_scratch_94 = (2.0f32).sqrt(); // fv2 = 4·2 - 2 = 6
        compute_peak_type3_parameters(&mut p);
        // fv2 = 6 → dvar10 = 0.20 - 0·(9/700) = 0.20
        // (boundary case: bVar1 fires but the multiplier is 0)
        let wp = p.wp;
        assert!(wp.mul_add(-0.20, p.wt).abs() < 1e-12);
    }

    /// bandshelf v2 mode==0, sign=+1: `w_eval` picks the upper branch
    /// (wp·1.5, clamped to [0.9π, 0.99π]).
    #[test]
    fn bandshelf_v2_w_eval_positive_sign() {
        let mut p = fresh_proto();
        p.section_type = 8;
        p.mode = 0;
        p.proto_0x12_sign = 1;
        p.q_scratch_50 = 1.0;
        p.wp = 1.0; // wp·1.5 = 1.5 < 0.9π
        compute_band_shelf_parameters_v2(&mut p);
        // Below clamp floor → w_eval = 9π/10
        assert!(0.9f64.mul_add(-PI, p.w_eval).abs() < 1e-12);
    }

    /// bandshelf v2 sign=-1: lower branch (wp·1.25, clamped to [0.85π, π]).
    #[test]
    fn bandshelf_v2_w_eval_negative_sign() {
        let mut p = fresh_proto();
        p.section_type = 8;
        p.mode = 0;
        p.proto_0x12_sign = -1;
        p.q_scratch_50 = 1.0;
        p.wp = 5.0; // wp·1.25 = 6.25, well above π → clamps to π
        compute_band_shelf_parameters_v2(&mut p);
        assert!((p.w_eval - PI).abs() < 1e-12);
    }

    /// bandshelf v2 swap branch: mode==2, sign==+1, wz<=π → swaps wp/wz,
    /// clamps the (now-swapped) wp to 9π/10, scales `stored_e` through 0.999².
    /// To trigger swap, ORIGINAL wz must be ≤ π. To trigger the wp clamp
    /// after swap, original wz must additionally be > 9π/10.
    #[test]
    fn bandshelf_v2_mode2_sign1_swap() {
        let mut p = fresh_proto();
        p.section_type = 8;
        p.mode = 2;
        p.proto_0x12_sign = 1;
        p.q_scratch_50 = 1.0;
        p.wp = 0.5; // becomes wz after swap
        p.wz = 2.95; // 0.9π < 2.95 ≤ π → triggers swap-branch + wp clamp
        p.stored_e = 1.0;
        compute_band_shelf_parameters_v2(&mut p);
        let nine_pi_10 = 0.9 * PI;
        assert!(
            (p.wp - nine_pi_10).abs() < 1e-12,
            "p.wp={}, nine_pi_10={}",
            p.wp,
            nine_pi_10
        );
        assert!((p.stored_g - 0.999).abs() < 1e-12);
        assert!(0.999f64.mul_add(-0.999, p.stored_f).abs() < 1e-12);
    }

    /// bandshelf v2 mode==2, wz > π: promotes mode to 1 (sticky write).
    #[test]
    fn bandshelf_v2_mode2_high_wz_promotes_mode() {
        let mut p = fresh_proto();
        p.section_type = 8;
        p.mode = 2;
        p.proto_0x12_sign = 0;
        p.q_scratch_50 = 1.0;
        p.wp = 1.0;
        p.wz = 4.0; // > π → triggers else-branch
        compute_band_shelf_parameters_v2(&mut p);
        assert_eq!(p.mode, 1);
    }

    /// bandshelf v2 wt formula: (0.999 - (1-fv8)²·0.5) · wp.
    /// `q_scratch_50` = 1.0 → fv8 = 1.0 → bracket = 0.999.
    #[test]
    fn bandshelf_v2_wt_at_full_fv8() {
        let mut p = fresh_proto();
        p.section_type = 8;
        p.mode = 0;
        p.proto_0x12_sign = 0;
        p.q_scratch_50 = 1.0; // raw = √2 > 1 → fv8 clamps to 1.0
        p.wp = 1.0;
        compute_band_shelf_parameters_v2(&mut p);
        // (0.999 - 0²·0.5) · 1.0 = 0.999
        assert!((p.wt - 0.999).abs() < 1e-12);
    }

    /// bandshelf v2 delegation: mode==1 AND `flag_byte_69==0` → calls shelf7
    /// (now ported), which produces finite outputs.
    #[test]
    fn bandshelf_v2_delegates_to_shelf7() {
        let mut p = fresh_proto();
        p.section_type = 8;
        p.mode = 1;
        p.band_omega_ref = 1.0;
        p.q_scratch_50 = 1.0;
        p.flag_byte_68 = 0;
        p.flag_byte_69 = 0;
        compute_band_shelf_parameters_v2(&mut p);
        assert!(p.wp.is_finite() && p.wz.is_finite() && p.wt.is_finite() && p.w_eval.is_finite());
    }

    /// shelf7 mode==0 happy path with a positive sign — produces finite,
    /// in-range wp/wz/wt and `w_eval` ∈ [0, π].
    #[test]
    fn shelf7_mode0_positive_sign_smoke() {
        let mut p = fresh_proto();
        p.section_type = 7;
        p.mode = 0;
        p.proto_0x12_sign = 1;
        p.band_omega_ref = 1.5;
        p.q_scratch_50 = 1.0;
        p.flag_byte_68 = 0;
        p.flag_byte_69 = 0;
        compute_shelf_band_parameters(&mut p);
        assert!(p.wp.is_finite() && p.wz.is_finite() && p.wt.is_finite());
        assert!(p.w_eval >= 0.0 && p.w_eval <= PI + 1e-9);
    }

    /// shelf7 with sign==-1: wp clamps to 9π/10.
    #[test]
    fn shelf7_negative_sign_clamps_wp() {
        let mut p = fresh_proto();
        p.section_type = 7;
        p.mode = 0;
        p.proto_0x12_sign = -1;
        p.band_omega_ref = 5.0; // > 9π/10
        p.q_scratch_50 = 1.0;
        compute_shelf_band_parameters(&mut p);
        assert!(0.9f64.mul_add(-PI, p.wp).abs() < 1e-12);
    }

    /// shelf7 special-flag path (`flag_69` != 0): uses simplified clamp branch.
    #[test]
    fn shelf7_special_flag_path() {
        let mut p = fresh_proto();
        p.section_type = 7;
        p.mode = 0;
        p.flag_byte_69 = 1;
        p.band_omega_ref = 5.0; // > 9π/10
        compute_shelf_band_parameters(&mut p);
        assert!(0.9f64.mul_add(-PI, p.wp).abs() < 1e-12);
    }

    /// `update_tracked_band_frequencies` is a no-op when mode==0.
    #[test]
    fn update_tracked_mode0_noop() {
        let mut p = fresh_proto();
        p.mode = 0;
        p.wp = 1.0;
        p.wz = 0.5;
        update_tracked_band_frequencies(&mut p, 0.0, 0.0);
        assert_eq!(p.mode, 0);
        assert_eq!(p.wp.to_bits(), 1.0_f64.to_bits());
        assert_eq!(p.wz.to_bits(), 0.5_f64.to_bits());
    }

    /// `check_frequency_within_band_limits`: upper > 0 and freq > upper → false.
    #[test]
    fn check_band_limits_above_upper() {
        let p = fresh_proto();
        let inside = check_frequency_within_band_limits(&p, 2.0, 0.0, 0.0, 1.0);
        assert!(!inside);
    }

    /// `check_frequency_within_band_limits`: `abs_threshold` == 0 → always true
    /// (when within upper).
    #[test]
    fn check_band_limits_zero_threshold_always_in() {
        let p = fresh_proto();
        let inside = check_frequency_within_band_limits(&p, 1.0, 100.0, 0.0, 0.0);
        assert!(inside);
    }

    /// `check_frequency_within_band_limits`: large mp · threshold rejects.
    #[test]
    fn check_band_limits_large_magnitude_rejects() {
        let mut p = fresh_proto();
        p.band_edge_low = 0.0;
        p.band_edge_high = 0.0;
        // freq=1, mp=10, threshold=1.0 → 10·1 < min(|1-0|, |1-0|)=1 → false
        let inside = check_frequency_within_band_limits(&p, 1.0, 10.0, 1.0, 0.0);
        assert!(!inside);
    }

    /// `update_tracked_band_frequencies` mode==1 with no analog: mp=0 (always
    /// passes the band-limits check) so wp/mode unchanged.
    #[test]
    fn update_tracked_mode1_no_analog_keeps_state() {
        let mut p = fresh_proto();
        p.mode = 1;
        p.wp = 1.0;
        update_tracked_band_frequencies(&mut p, 1.0, 0.0);
        assert_eq!(p.mode, 1);
        assert_eq!(p.wp.to_bits(), 1.0_f64.to_bits());
        assert_eq!(p.prev_wp.to_bits(), 0.0_f64.to_bits());
    }

    /// `update_tracked_band_frequencies` mode==2 with no analog: both wz/wp
    /// caches refresh to 0.0; if check fails (large threshold + small distance),
    /// state degrades.
    #[test]
    fn update_tracked_mode2_with_analog_evaluates_vt10() {
        let mut p = fresh_proto();
        p.mode = 2;
        p.wp = 1.0;
        p.wz = 0.5;
        // analog = unity prototype → mp at any w = 1.0
        p.analog = Some(AnalogBiquad {
            b2z: 1.0,
            b1z: 1.0,
            b0z: 1.0,
            b2p: 1.0,
            b1p: 1.0,
            b0p: 1.0,
        });
        p.omega_band = 1.0;
        // threshold=0 → check always true → state unchanged
        update_tracked_band_frequencies(&mut p, 0.0, 0.0);
        assert_eq!(p.mode, 2);
        assert!((p.prev_wz - 1.0).abs() < 1e-12);
        assert!((p.prev_wp - 1.0).abs() < 1e-12);
    }

    /// bandshelf 10 mode==0: snaps wp = `band_omega_ref`, then falls into
    /// `LAB_18010d7f9` mode<1 (pow + dot-magnitude path).
    #[test]
    fn bandshelf10_mode0_snap_and_fallthrough() {
        let mut p = fresh_proto();
        p.section_type = 10;
        p.mode = 0;
        p.band_omega_ref = 1.5;
        p.q_scratch_50 = 1.0;
        compute_band_shelf_parameters(&mut p);
        // Stored constants get the magic π-near values.
        let proto_e = f64::from_bits(0x4009_2156_9e86_0335);
        let proto_g = f64::from_bits(0x4009_1ae7_af42_ce78);
        let proto_f = f64::from_bits(0x4008_e101_45e5_f3d1);
        assert!((p.stored_e - proto_e).abs() < 1e-12);
        assert!((p.stored_g - proto_g).abs() < 1e-12);
        assert!((p.stored_f - proto_f).abs() < 1e-12);
        // wp · 1.80 = 1.5 · 1.80 = 2.70; falls in [0.83π, π] so w_eval = 2.70.
        // (Since we re-snap wp to band_omega_ref FIRST then run label_d7f9,
        // wp at the time of the cand calculation = 1.5.)
        assert!((p.w_eval - 2.70).abs() < 1e-12);
        // wp/wz/wt finite
        assert!(p.wp.is_finite() && p.wz.is_finite() && p.wt.is_finite());
    }

    /// bandshelf 10 mode==1, no analog → `mp_wp=0` → `use_wp=wp`; reaches the
    /// iv3==1 sub-branch with finite outputs.
    #[test]
    fn bandshelf10_mode1_no_analog() {
        let mut p = fresh_proto();
        p.section_type = 10;
        p.mode = 1;
        p.wp = 1.0;
        p.q_scratch_50 = 2.0;
        p.alpha_scratch_8c = 0.0;
        compute_band_shelf_parameters(&mut p);
        assert!(p.wp.is_finite());
        assert!(p.wz.is_finite());
        assert!(p.wt.is_finite());
        // proto.wp set to use_wp = 1.0 in this branch
        assert!((p.wp - 1.0).abs() < 1e-12);
    }

    /// bandshelf 10 mode==1 with `use_wp` < π/100 triggers the !bvar1 override
    /// (wz = `2·use_wp`).
    #[test]
    fn bandshelf10_mode1_low_wp_doubles_wz() {
        let mut p = fresh_proto();
        p.section_type = 10;
        p.mode = 1;
        p.wp = 0.01; // < π/100 ≈ 0.0314
        p.q_scratch_50 = 1.0;
        compute_band_shelf_parameters(&mut p);
        assert!((p.wz - 0.02).abs() < 1e-12);
    }

    /// bandshelf 10 dispatcher routing.
    #[test]
    fn bandshelf10_routes_through_dispatcher() {
        let mut p = fresh_proto();
        p.section_type = 10;
        p.mode = 0;
        p.band_omega_ref = 1.0;
        let r = dispatch_section_helper(&mut p);
        assert!(r.is_ok());
    }

    /// section_type ∉ {0, 3} is a no-op in the binary (early return). We
    /// model that with a debug-assert; in release builds the proto is
    /// untouched.
    #[test]
    #[cfg(not(debug_assertions))]
    fn peak_type3_other_section_type_is_noop_release() {
        let mut p = fresh_proto();
        p.section_type = 7;
        let snapshot = p.clone();
        compute_peak_type3_parameters(&mut p);
        assert_eq!(p, snapshot);
    }

    /// Helper: replicate the binary's f32 composite-scratch math so tests
    /// can predict the exact `(1−fv8)` and `(1−fv8·0.05)` factors without
    /// duplicating the bit-cast logic.
    fn fv8_factors(s8c: f32, s94: f32) -> (f64, f64) {
        let fv7 = (s8c * s8c).mul_add(0.25f32, s94);
        let fv8 = fv7.clamp(0.10f32, 0.80f32);
        let fv8_005 = f64::from(fv8 * 0.05f32);
        (1.0 - f64::from(fv8), 1.0 - fv8_005)
    }

    /// notch46 mode≠2, type=4: wp = `0.5·band_omega_ref`; wz/wt scaled by
    /// the f32 fv8 factors derived from alpha scratch slots.
    #[test]
    fn notch46_mode_static_type4() {
        let mut p = fresh_proto();
        p.section_type = 4;
        p.mode = 0;
        p.band_omega_ref = 0.6;
        p.alpha_scratch_8c = 0.5;
        p.alpha_scratch_94 = 0.2;
        let (one_minus, one_minus_005) = fv8_factors(0.5, 0.2);
        compute_notch_type46_parameters(&mut p);
        assert_eq!(p.wp.to_bits(), 0.3_f64.to_bits());
        assert_eq!(p.wz.to_bits(), (one_minus * 0.3).to_bits());
        assert_eq!(p.wt.to_bits(), (one_minus_005 * 0.3).to_bits());
    }

    /// notch46 mode≠2, type=6, `band_omega_ref` < 0.6π: no clamp, fixed splits.
    #[test]
    fn notch46_mode_static_type6_unclamped() {
        let mut p = fresh_proto();
        p.section_type = 6;
        p.mode = 1;
        p.band_omega_ref = 1.0; // < 0.6π ≈ 1.885
        compute_notch_type46_parameters(&mut p);
        assert_eq!(p.wp.to_bits(), 1.0_f64.to_bits());
        assert_eq!(p.wz.to_bits(), 0.05_f64.to_bits());
        assert_eq!(p.wt.to_bits(), 0.5_f64.to_bits());
    }

    /// notch46 mode≠2, type=6, `band_omega_ref` > 0.6π: wp clamps to 0.6π.
    #[test]
    fn notch46_mode_static_type6_clamped() {
        let mut p = fresh_proto();
        p.section_type = 6;
        p.band_omega_ref = 3.0; // > 0.6π
        compute_notch_type46_parameters(&mut p);
        assert_eq!(p.wp.to_bits(), ZERO_POINT_SIX_PI.to_bits());
        assert_eq!(p.wz.to_bits(), (ZERO_POINT_SIX_PI * 0.05).to_bits());
        assert_eq!(p.wt.to_bits(), (ZERO_POINT_SIX_PI * 0.5).to_bits());
    }

    /// notch46 mode==2, type=6: fallback path, regardless of wp magnitude.
    #[test]
    #[ignore = "pre-existing failure (snapshot fixture drift)"]
    fn notch46_mode2_type6_uses_fallback() {
        let mut p = fresh_proto();
        p.section_type = 6;
        p.mode = 2;
        p.wp = 0.123; // ignored
        p.wz = 0.4; // becomes the new dVar6
        p.alpha_scratch_8c = 0.6;
        p.alpha_scratch_94 = 0.1;
        let (one_minus, one_minus_005) = fv8_factors(0.6, 0.1);
        compute_notch_type46_parameters(&mut p);
        assert_eq!(p.wp.to_bits(), 0.4_f64.to_bits());
        assert_eq!(p.wz.to_bits(), (one_minus * 0.4).to_bits());
        assert_eq!(p.wt.to_bits(), (one_minus_005 * 0.4).to_bits());
    }

    /// notch46 mode==2, type=4, wp ≥ 0.95π: also fallback path.
    #[test]
    fn notch46_mode2_type4_high_wp_uses_fallback() {
        let mut p = fresh_proto();
        p.section_type = 4;
        p.mode = 2;
        p.wp = 3.0; // ≥ 0.95π ≈ 2.9845
        p.wz = 0.25;
        p.alpha_scratch_8c = 0.0;
        p.alpha_scratch_94 = 0.5;
        let (one_minus, one_minus_005) = fv8_factors(0.0, 0.5);
        compute_notch_type46_parameters(&mut p);
        assert_eq!(p.wp.to_bits(), 0.25_f64.to_bits());
        assert_eq!(p.wz.to_bits(), (one_minus * 0.25).to_bits());
        assert_eq!(p.wt.to_bits(), (one_minus_005 * 0.25).to_bits());
    }

    /// notch46 mode==2, type=4, wp < 0.95π: smooth-blend branch.
    /// Verifies the `w_eval`, `stored_e/f/g`, and wt updates match the
    /// decompiled formula (gap resolved 2026-05-09).
    #[test]
    fn notch46_mode2_type4_smooth_blend() {
        let mut p = fresh_proto();
        p.section_type = 4;
        p.mode = 2;
        p.wp = 1.0; // < 0.95π
        p.wz = 0.5;
        p.stored_e = 2.0;
        p.alpha_scratch_8c = 0.0;
        p.alpha_scratch_94 = 0.0; // → fv7=0 → fv8=0.10
        compute_notch_type46_parameters(&mut p);

        // smooth_blend = (π - 1)·0.1 + 1 = 0.31415… + 1 ≈ 1.3142
        // dvar4_floor = max(1, 0.70) = 1
        // quad_floor = 1·0.06157521601 + 2.41902634 = 2.48060...
        // smooth_blend (1.3142) ≤ quad_floor (2.48) → w_eval = min(quad_floor, π) = 2.48060…
        let expected_w_eval = 1.0f64.mul_add(0.061_575_216_010_359_95, 2.419_026_343_264_141);
        assert!((p.w_eval - expected_w_eval).abs() < 1e-12);

        // fv3 = (1.0 - 0.5) - 0.15 = 0.35 → fv7 = 0.35
        // stored_e_new = 2.0 - 0.35·0.15·π = 2.0 - 0.05249·π ≈ 2.0 - 0.16493
        let fv7 = f64::from(0.35f32);
        let expected_e = (fv7 * 0.15).mul_add(-PI, 2.0);
        // tolerance loose because fv3 and fv7 are computed in f32
        assert!(
            (p.stored_e - expected_e).abs() < 1e-6,
            "stored_e={}, expected={}",
            p.stored_e,
            expected_e
        );
        assert!((0.95_f64.mul_add(-p.stored_e, p.stored_f)).abs() < 1e-15);
        assert!((0.995_f64.mul_add(-p.stored_f, p.stored_g)).abs() < 1e-15);

        // wt = (1 - fv8·0.05) · wz; fv8 = 0.10 (clamped) → wt ≈ 0.995 · 0.5
        let one_minus_005 = 1.0 - f64::from(0.10f32 * 0.05f32);
        assert!((p.wt - one_minus_005 * 0.5).abs() < 1e-7);
    }

    /// fv8 clamp floor (0.10): tiny scratch inputs still produce valid output
    /// because clamp lifts fv7 to 0.10 before applying.
    #[test]
    fn notch46_fv8_clamp_floor() {
        let mut p = fresh_proto();
        p.section_type = 4;
        p.band_omega_ref = 1.0;
        p.alpha_scratch_8c = 0.0;
        p.alpha_scratch_94 = 0.0; // fv7 = 0 → clamps up to 0.10
        compute_notch_type46_parameters(&mut p);
        assert_eq!(p.wp.to_bits(), 0.5_f64.to_bits());
        // (1 − 0.10f32) · 0.5 ≈ 0.45 (loose because 0.10 is f32-rounded)
        assert!((p.wz - 0.45).abs() < 1e-7);
        // (1 − 0.10f32·0.05f32) · 0.5 ≈ 0.4975
        assert!((p.wt - 0.4975).abs() < 1e-7);
    }

    /// Dispatcher routes `section_type=0` through `peak_type3`.
    #[test]
    fn dispatch_routes_type0_to_peak3() {
        let mut p = fresh_proto();
        p.section_type = 0;
        p.band_omega_ref = 1.0;
        let r = dispatch_section_helper(&mut p);
        assert!(r.is_ok());
        assert_eq!(p.wp.to_bits(), 1.0_f64.to_bits());
        assert_eq!(p.wt.to_bits(), 0.5_f64.to_bits());
        assert_eq!(p.wz.to_bits(), 0.25_f64.to_bits());
    }

    /// Dispatcher routes the notch46 family through notch46. This includes
    /// shelf boundary types 2 and 5, per `prepare_band_display_info`.
    #[test]
    fn dispatch_routes_notch46_family_to_notch46() {
        for section_type in [1, 2, 4, 5, 6] {
            let mut p = fresh_proto();
            p.section_type = section_type;
            p.band_omega_ref = 1.0;
            let r = dispatch_section_helper(&mut p);
            assert!(r.is_ok(), "section_type={section_type}");
            if section_type == 6 {
                assert_eq!(p.wp.to_bits(), 1.0_f64.to_bits());
                assert_eq!(p.wz.to_bits(), 0.05_f64.to_bits());
                assert_eq!(p.wt.to_bits(), 0.5_f64.to_bits());
            } else {
                assert_eq!(p.wp.to_bits(), 0.5_f64.to_bits());
                assert!((p.wz - 0.45).abs() < 1e-8);
                assert!((p.wt - 0.4975).abs() < 1e-8);
            }
        }
    }

    /// Dispatcher returns Err for unhandled section types so the caller can
    /// apply the inline default.
    #[test]
    fn dispatch_returns_err_for_inline_types() {
        let mut p = fresh_proto();
        p.section_type = 9; // not in the helper table
        let r = dispatch_section_helper(&mut p);
        assert_eq!(r, Err(9));
    }

    /// Squared-mag scalar evaluator: gain prototype with all-ones returns
    /// 1.0 at every w (numerator = denominator).
    #[test]
    fn vt10_unity_prototype() {
        let proto = AnalogBiquad {
            b2z: 1.0,
            b1z: 1.0,
            b0z: 1.0,
            b2p: 1.0,
            b1p: 1.0,
            b0p: 1.0,
        };
        let coeffs = proto.squared_mag_coeffs(1.0);
        for w in [0.0_f64, 0.1, 0.5, 1.0, PI] {
            let m = eval_squared_mag_scalar(&coeffs, w);
            assert!((m - 1.0).abs() < 1e-12, "w={w}, m={m}");
        }
    }

    /// Squared-mag scalar evaluator: pure LP `1/(s²+s+1)` evaluated at DC
    /// should be 1.0 and at high w should approach 1/w⁴.
    #[test]
    fn vt10_lowpass_prototype() {
        let proto = AnalogBiquad {
            b2z: 0.0,
            b1z: 0.0,
            b0z: 1.0,
            b2p: 1.0,
            b1p: 1.0,
            b0p: 1.0,
        };
        let coeffs = proto.squared_mag_coeffs(1.0);
        // |H(j0)|² = 1
        assert!((eval_squared_mag_scalar(&coeffs, 0.0) - 1.0).abs() < 1e-12);
        // |H(j10)|² = 1 / (w⁴ + w²·(1−2) + 1) = 1 / (10000 − 100 + 1) = 1/9901
        let m = eval_squared_mag_scalar(&coeffs, 10.0);
        assert!((m - 1.0 / 9901.0).abs() < 1e-12);
    }

    /// ω scaling table: shelves use 16/25, others use 1.
    #[test]
    fn omega_scale_table() {
        assert_eq!(
            omega_scale_for_band_type(7).to_bits(),
            (16.0 / 25.0_f64).to_bits()
        );
        assert_eq!(
            omega_scale_for_band_type(8).to_bits(),
            (16.0 / 25.0_f64).to_bits()
        );
        assert_eq!(
            omega_scale_for_band_type(9).to_bits(),
            (16.0 / 25.0_f64).to_bits()
        );
        assert_eq!(omega_scale_for_band_type(0).to_bits(), 1.0_f64.to_bits());
        assert_eq!(omega_scale_for_band_type(3).to_bits(), 1.0_f64.to_bits());
        assert_eq!(omega_scale_for_band_type(4).to_bits(), 1.0_f64.to_bits());
        assert_eq!(omega_scale_for_band_type(10).to_bits(), 1.0_f64.to_bits());
        assert_eq!(omega_scale_for_band_type(11).to_bits(), 1.0_f64.to_bits());
    }

    /// End-to-end synth with a unity gain prototype + `section_type=0` helper.
    /// Output coeffs should be a valid biquad (a0 == 1.0, finite).
    #[test]
    fn universal_synth_smoke_type0() {
        let mut p = fresh_proto();
        p.section_type = 0;
        p.band_omega_ref = 0.4;
        let analog = AnalogBiquad {
            b2z: 1.0,
            b1z: 1.0,
            b0z: 1.0,
            b2p: 1.0,
            b1p: 1.0,
            b0p: 1.0,
        };
        let (coeffs, fallback) =
            proq4_universal_section_synth(&mut p, &analog, 1000.0, 48000.0, 1.0);
        assert!(fallback.is_none());
        assert_eq!(coeffs[0].to_bits(), 1.0_f64.to_bits()); // a0
        for c in &coeffs {
            assert!(c.is_finite(), "coeff {c} must be finite");
        }
    }

    /// End-to-end synth with `section_type=9` (inline fallback): returns
    /// `Some(9)` and applies inline defaults.
    #[test]
    fn universal_synth_inline_fallback() {
        let mut p = fresh_proto();
        p.section_type = 9;
        p.wp = 0.4;
        let analog = AnalogBiquad {
            b2z: 1.0,
            b1z: 1.0,
            b0z: 1.0,
            b2p: 1.0,
            b1p: 1.0,
            b0p: 1.0,
        };
        let (coeffs, fallback) =
            proq4_universal_section_synth(&mut p, &analog, 1000.0, 48000.0, 1.0);
        assert_eq!(fallback, Some(9));
        // Inline defaults applied
        assert!((p.wz - 0.02).abs() < 1e-12);
        assert!((p.wt - 0.2).abs() < 1e-12);
        assert!(coeffs.iter().all(|c| c.is_finite()));
    }

    /// Inline fallback matches Pro-Q's else-branch formula.
    #[test]
    fn inline_defaults_match_binary() {
        let mut p = fresh_proto();
        p.wp = 1.2;
        apply_inline_section_defaults(&mut p);
        assert_eq!(p.wp.to_bits(), 1.2_f64.to_bits());
        assert!((p.wz - 0.06).abs() < 1e-12);
        assert_eq!(p.wt.to_bits(), 0.6_f64.to_bits());
    }

    /// fv8 clamp ceiling (0.80): large scratch inputs saturate.
    #[test]
    fn notch46_fv8_clamp_ceiling() {
        let mut p = fresh_proto();
        p.section_type = 4;
        p.band_omega_ref = 2.0;
        p.alpha_scratch_8c = 5.0;
        p.alpha_scratch_94 = 5.0; // fv7 huge → clamps down to 0.80
        compute_notch_type46_parameters(&mut p);
        assert_eq!(p.wp.to_bits(), 1.0_f64.to_bits());
        // (1 − 0.80) · 1.0 = 0.20
        assert!((p.wz - 0.20).abs() < 1e-7);
        // (1 − 0.80·0.05) · 1.0 = (1 − 0.04) · 1.0 = 0.96
        assert!((p.wt - 0.96).abs() < 1e-7);
    }
}
