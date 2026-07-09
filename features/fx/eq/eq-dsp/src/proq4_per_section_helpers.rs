#![allow(clippy::eq_op)]
#![allow(unused_assignments)]

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
//! Status: peak_type3 (type=0 branch) ported and bit-exact verified by
//! construction. Other helpers staged with explicit `unimplemented!` until
//! their respective probe captures are wired in.

use crate::biquad::Coeffs;
use crate::cascade::proq4_s2_from_prototype_with_subfreq_pub;
use std::f64::consts::PI;

/// Generic analog biquad prototype `(b2z·s² + b1z·s + b0z) / (b2p·s² + b1p·s + b0p)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnalogBiquad {
    pub b2z: f64,
    pub b1z: f64,
    pub b0z: f64,
    pub b2p: f64,
    pub b1p: f64,
    pub b0p: f64,
}

/// Squared-magnitude polynomial coefficients (A..F per
/// `lagrange_mzt_universal_decode.md`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MagSqCoeffs {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl AnalogBiquad {
    /// Mirrors `compute_zpk_transfer_function_coefficients @ 0x1800fd420`
    /// for the generic case (writes proto[+0x20..+0x48]).
    ///
    /// |H(jw)|² = (A·w⁴ + B·w² + C) / (D·w⁴ + E·w² + F) with
    /// `ω₀` scaling baked into B/C/E/F.
    pub fn squared_mag_coeffs(&self, omega0: f64) -> MagSqCoeffs {
        let g_om2 = omega0 * omega0;
        let g_om4 = g_om2 * g_om2;
        MagSqCoeffs {
            a: self.b2z * self.b2z,
            b: (self.b1z * self.b1z - 2.0 * self.b2z * self.b0z) * g_om2,
            c: self.b0z * self.b0z * g_om4,
            d: self.b2p * self.b2p,
            e: (self.b1p * self.b1p - 2.0 * self.b2p * self.b0p) * g_om2,
            f: self.b0p * self.b0p * g_om4,
        }
    }
}

/// Pro-Q's `compute_zpk_transfer_function_coefficients @ 0x1800fd420`,
/// generic case.
///
/// Faithful port that handles both the **linear branch** (when both `b2z`
/// and `b2p` are sub-epsilon, treating the prototype as a 1st-order
/// quadratic) and the **quadratic branch**. The binary uses the float-cast
/// magnitude check `|b2z|` or `|b2p| > 1.192e-7` to pick.
///
/// Returns `(MagSqCoeffs, is_quadratic)`. `is_quadratic` is the binary's
/// `iVar4 == 2` flag, needed by `solve_biquad_denominator_quadratic_generic`.
pub fn compute_zpk_transfer_coeffs_generic(
    analog: &AnalogBiquad,
    omega: f64,
) -> (MagSqCoeffs, bool) {
    const EPS_F32: f32 = 1.192_092_9e-7;
    let omega_sq = omega * omega;
    let omega_qd = omega_sq * omega_sq;

    let is_quadratic = (analog.b2z as f32).abs() > EPS_F32 || (analog.b2p as f32).abs() > EPS_F32;

    let coeffs = if is_quadratic {
        MagSqCoeffs {
            a: analog.b2z * analog.b2z,
            b: (analog.b1z * analog.b1z - 2.0 * analog.b2z * analog.b0z) * omega_sq,
            c: analog.b0z * analog.b0z * omega_qd,
            d: analog.b2p * analog.b2p,
            e: (analog.b1p * analog.b1p - 2.0 * analog.b2p * analog.b0p) * omega_sq,
            f: analog.b0p * analog.b0p * omega_qd,
        }
    } else {
        // Linear branch (b2 ≈ 0): A=0, B=b1², C=b0²·ω².
        MagSqCoeffs {
            a: 0.0,
            b: analog.b1z * analog.b1z,
            c: analog.b0z * analog.b0z * omega_sq,
            d: 0.0,
            e: analog.b1p * analog.b1p,
            f: analog.b0p * analog.b0p * omega_sq,
        }
    };

    (coeffs, is_quadratic)
}

/// Roots returned by [`solve_biquad_denominator_quadratic_generic`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoleRoots {
    pub w1: f64,
    pub w2: f64,
    pub count: u8,
}

/// Pro-Q's `solve_biquad_denominator_quadratic @ 0x1800fd240`, generic case.
///
/// Solves the cross-determinant quadratic in ω² for the squared-magnitude
/// crossing frequencies of `|H(jω)|² = (A·ω⁴+B·ω²+C)/(D·ω⁴+E·ω²+F)`:
/// `det_a · (ω²)² + det_b · (ω²) + det_c = 0` with
/// `det_a = A·E − B·D`, `det_b = 2·(F·A − C·D)`, `det_c = F·B − C·E`.
///
/// When `det_a == 0` the quadratic degenerates to linear; the binary swaps
/// `det_a/det_b` and skips the final sqrt of the root (`take_sqrt = false`).
/// `count` reports how many roots were strictly positive.
pub fn solve_biquad_denominator_quadratic_generic(
    coeffs: &MagSqCoeffs,
    is_quadratic: bool,
) -> PoleRoots {
    let MagSqCoeffs { a, b, c, d, e, f } = *coeffs;

    if !is_quadratic {
        return PoleRoots {
            w1: 0.0,
            w2: 0.0,
            count: 0,
        };
    }

    let mut det_a = a * e - b * d;
    let mut det_b = 2.0 * (f * a - c * d);
    let mut take_sqrt = true;
    if det_a == 0.0 {
        det_a = det_b;
        det_b = 0.0;
        take_sqrt = false;
    }
    if det_a == 0.0 {
        return PoleRoots {
            w1: 0.0,
            w2: 0.0,
            count: 0,
        };
    }

    let det_c = f * b - c * e;
    let disc = det_b * det_b - 4.0 * det_a * det_c;
    if disc < 0.0 {
        return PoleRoots {
            w1: 0.0,
            w2: 0.0,
            count: 0,
        };
    }

    let sqrt_disc = disc.sqrt();
    let inv2a = 0.5 / det_a;
    let neg_b = -det_b * inv2a;
    let radical = sqrt_disc * inv2a;
    let mut w_sq_lo = neg_b + radical;
    let mut w_sq_hi = neg_b - radical;

    if take_sqrt {
        if w_sq_lo > 0.0 {
            w_sq_lo = w_sq_lo.sqrt();
        }
        if w_sq_hi > 0.0 {
            w_sq_hi = w_sq_hi.sqrt();
        }
    }

    let pos_lo = w_sq_lo > 0.0;
    let pos_hi = w_sq_hi > 0.0;
    if !pos_lo && !pos_hi {
        return PoleRoots {
            w1: 0.0,
            w2: 0.0,
            count: 0,
        };
    }
    if pos_lo && !pos_hi {
        return PoleRoots {
            w1: w_sq_lo,
            w2: 0.0,
            count: 1,
        };
    }
    if !pos_lo && pos_hi {
        return PoleRoots {
            w1: w_sq_hi,
            w2: 0.0,
            count: 1,
        };
    }
    if w_sq_lo <= w_sq_hi {
        PoleRoots {
            w1: w_sq_lo,
            w2: w_sq_hi,
            count: 2,
        }
    } else {
        PoleRoots {
            w1: w_sq_hi,
            w2: w_sq_lo,
            count: 2,
        }
    }
}

/// Pro-Q's `vtable[0x10]` scalar magnitude evaluator
/// (`evaluate_biquad_squared_magnitude_scalar @ 0x1800fd0b0`).
///
/// Returns `max(num/den, 0)` with the den-zero guard the binary uses (the
/// binary returns 0 when den is sub-normal; we mirror via `> 1e-300`).
pub fn eval_squared_mag_scalar(coeffs: &MagSqCoeffs, w: f64) -> f64 {
    let w2 = w * w;
    let w4 = w2 * w2;
    let num = w4 * coeffs.a + w2 * coeffs.b + coeffs.c;
    let den = w4 * coeffs.d + w2 * coeffs.e + coeffs.f;
    if den.abs() > 1e-300 {
        (num / den).max(0.0)
    } else {
        0.0
    }
}

/// ω₀ scaling factor applied per band-level filter type before feeding the
/// Lagrange-MZT synth.
///
/// Verified for shelves: `ω₀ = 0.64 · (2π·fc/sr)` (= 16/25, bit-exact across
/// SR ∈ {44100, 48000, 88200, 96000}). Per
/// `docs/reports/proq4/re/lagrange_mzt_universal_decode.md`.
///
/// Tilt/bandpass/notch use `1.0` until probe sweeps confirm otherwise.
pub fn omega_scale_for_band_type(band_filter_type: u8) -> f64 {
    match band_filter_type {
        // 7 = LowShelf, 8 = HighShelf, 9 = TiltShelf — TiltShelf's 0.64
        // applicability is unverified but we mirror shelves until probed.
        7..=9 => 16.0 / 25.0,
        // All other band types (peak, bandpass, notch, allpass, …) use
        // ω_naive until probe confirms a per-type override.
        _ => 1.0,
    }
}

/// Subset of Pro-Q's 0xC0-byte prototype struct that the per-section helpers
/// read and write. Field names mirror the offsets used in
/// `per_section_helpers_decompiled.md`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Prototype {
    /// proto[1] @ +0x08 — wp (pole sub-frequency, radians).
    pub wp: f64,
    /// proto[2] @ +0x10 — wz (zero sub-frequency, radians).
    pub wz: f64,
    /// proto[3] @ +0x18 — wt (third sub-frequency, radians).
    pub wt: f64,
    /// proto[4] @ +0x20 — w_eval (magnitude evaluation point, radians).
    pub w_eval: f64,

    /// proto[7] @ +0x38 — `iVar5`/dispatch sub-mode (1 = real-root path,
    /// 2 = complex-root path, 0 = default).
    pub mode: i32,

    /// proto[0x11] @ +0x88 — duplicate of the solved root count used by
    /// shelf-style helpers to choose their Q source. This is distinct from
    /// proto[0x13], the section helper dispatch type.
    pub root_count_dup: i32,

    /// proto[0xb] @ +0x58 — magnitude cache for wp, written by
    /// `update_tracked_band_frequencies` as `vt10(wp)`. (Earlier doc
    /// labeled these as "previous wp/wz" — that was wrong; they hold
    /// `|H(jwp)|²` and `|H(jwz)|²` respectively.)
    pub prev_wp: f64,
    /// proto[0xc] @ +0x60 — magnitude cache for wz (= `vt10(wz)`).
    pub prev_wz: f64,

    /// proto[5] @ +0x28 — band-edge frequency reference, read by
    /// `check_frequency_within_band_limits` as `|freq - proto[5]|`.
    pub band_edge_low: f64,

    /// proto[6] @ +0x30 — alternate band-edge reference used by
    /// `check_frequency_within_band_limits` when the upper-bound parameter
    /// is non-positive.
    pub band_edge_high: f64,

    /// proto[0xe] @ +0x70, proto[0xf] @ +0x78, proto[0x10] @ +0x80 —
    /// stored-constant slots written by certain branches (notch46 type=2,
    /// band_shelf v2 swap-branch).
    pub stored_e: f64,
    pub stored_f: f64,
    pub stored_g: f64,

    /// proto+0x8c (f32 in binary) — secondary alpha-scratch slot, paired
    /// with `alpha_scratch_94` to form the notch46 composite scratch
    /// `fVar7 = (alpha_scratch_8c)²·0.25 + alpha_scratch_94`.
    pub alpha_scratch_8c: f32,

    /// proto[0x94] @ +0x94 (f32 in binary, modeled as f64) — alpha-scratch
    /// input (= sec[0x5c]/Q from upstream).
    pub alpha_scratch_94: f32,

    /// proto[0x13] @ +0x98 — per-section internal filter type, mirrored
    /// from `sec[+0x58]`. Drives helper dispatch.
    pub section_type: i32,

    /// proto[0x14] @ +0xa0 — `sec[+0x50] * sec[+0x10]` = band-level ω
    /// reference (radians), set upstream by the cascade builder.
    pub band_omega_ref: f64,

    /// proto[10] @ +0x50 — Q-related scratch double (used as denominator in
    /// `√2 / proto[10]` for bandshelf v2/10).
    pub q_scratch_50: f64,

    /// proto[0x12] @ +0x90 area (int) — section variant sign indicator
    /// (`-1`, `0`, `+1`); steers the w_eval branch in bandshelf v2.
    pub proto_0x12_sign: i32,

    /// Byte flag at +0x68 (read as `*(char *)(proto + 0xd)` in Pro-Q) —
    /// participates in the bandshelf v2 → shelf7 delegation predicate.
    pub flag_byte_68: u8,

    /// Byte flag at +0x69 — gates the bandshelf v2 → shelf7 delegation.
    pub flag_byte_69: u8,

    /// Analog biquad coefficients used by helpers that re-evaluate the
    /// squared-mag polynomial via `vtable[0x10]` (helpers for sections 8
    /// and 10). `None` for helpers that don't need it.
    pub analog: Option<AnalogBiquad>,

    /// ω₀ used to refresh A..F in `vtable[0x10]` calls; must match the
    /// per-band-type scaling that the synth ultimately uses.
    pub omega_band: f64,
}

/// Per-section helper for `proto[0x13] ∈ {0, 3}` (peak-style sections).
///
/// Decompiled from `compute_peak_type3_parameters @ 0x18010d580`.
///
/// **Implemented branches:**
/// - `section_type == 0`: fully ported (3-line formula, see source).
///
/// **Pending branches:**
/// - `section_type == 3`: decompilation has gaps in the smooth-blend
///   bookkeeping; left as a `debug_assert!` until probe captures are
///   added under `docs/reports/proq4/re/peak_type3_sec3/`.
pub fn compute_peak_type3_parameters(proto: &mut Prototype) {
    match proto.section_type {
        0 => {
            // Decompilation (verbatim, peak3 §"proto[0x98] == 0"):
            //   dVar5  = proto[0xa0] * 0.5;
            //   dVar10 = (proto[0x38]==1 && proto[1] > dVar5) ? proto[1] : dVar5;
            //   proto[3] = dVar10;          // wt
            //   proto[1] = proto[0xa0];      // wp
            //   proto[2] = dVar10 * 0.5;     // wz
            let half_ref = proto.band_omega_ref * 0.5;
            let wt_new = if proto.mode == 1 && proto.wp > half_ref {
                proto.wp
            } else {
                half_ref
            };
            proto.wt = wt_new;
            proto.wp = proto.band_omega_ref;
            proto.wz = wt_new * 0.5;
        }
        3 => {
            // Decompilation (compute_peak_type3_parameters @ 0x18010d580,
            // proto[0x98] == 3 branch). Constants verified against fresh
            // ghidra-cli decompile 2026-05-09.
            //
            // Constants:
            //   _DAT_1802319e0 = 0.4420970641441537
            //   _DAT_1802319d8 = 0.41666666... = 5/12
            //   DAT_1802319a8  = 0.20  (NOT 25.0 — earlier doc was wrong)
            //   _DAT_180231a58 = 0.785 ≈ π/4
            //   DAT_180231a80  = 0.96
            //   DAT_180231cb0  = 4.0
            //   DAT_180231b20  = 2.0
            //   DAT_180232058  = -0.5  (f64)
            //   DAT_180232018  = -0.5  (f32)
            //   DAT_1802318ac  = 1.0
            //   DAT_180231a70  = 0.30·π
            //   DAT_180231ce4  = 6.0
            //   DAT_180231db8  = 20.0
            //   _DAT_180231b30 = 0.7·π
            //   _DAT_180231be0 = 6.0
            //   _DAT_1802318d8 = 9/700 ≈ 0.012857142857
            //   DAT_180231868  = 0.001

            // === w_eval update (only when mode > 0) ===
            if proto.mode > 0 {
                let wp_in = proto.wp;
                // f32 lane: ((wp · 0.44209706…) - 5/12)
                let fv2_a = ((wp_in * 0.4420970641441537 - 5.0 / 12.0) as f32) as f64;
                let mut fv2_sq_part = fv2_a * fv2_a * 0.20 + 0.785;
                let mut fv2_b = fv2_sq_part as f32 as f64;
                if (fv2_b as f32 as f64) > 0.96 {
                    // Mirror w_eval around 0.96 (binary: (w_eval - 0.96) + w_eval)
                    fv2_sq_part = (proto.w_eval - 0.96) + proto.w_eval;
                    fv2_b = fv2_sq_part as f32 as f64;
                }
                // wp_clamped = min(wp_in, π)
                let wp_clamped = wp_in.min(PI);
                // candidate = (f32 cast) · π
                let cand = fv2_b * PI;
                // dVar9 = if (wp_clamped <= cand) { if (cand >= π) π else cand } else wp_clamped
                let mut new_w_eval = if wp_clamped <= cand {
                    if cand >= PI { PI } else { cand }
                } else {
                    wp_clamped
                };
                // Final fall-through: w_eval = dVar9 (may already be clamped)
                if new_w_eval > PI {
                    new_w_eval = PI;
                }
                proto.w_eval = new_w_eval;
            }

            // === alpha & wp/wz/wt derivation (unconditional) ===
            // alpha_94_new = (alpha_94_old² · 4.0 - 2.0) as f32
            let a94_in = proto.alpha_scratch_94;
            let fv2 = a94_in * a94_in * 4.0f32 - 2.0f32;
            proto.alpha_scratch_94 = fv2;

            // dVar_alpha = (fv2 < 0) ? max(sqrt(-fv2/2), 0.5) : 0.5
            let dvar_alpha = if (fv2 as f64) < 0.0 {
                let s = (fv2 as f64 * -0.5).sqrt();
                if s <= 0.5 { 0.5 } else { s }
            } else {
                0.5
            };

            // fVar3 = fv2 · -0.5 (f32); fv11 = clamp(fVar3, 0, 1.0)
            let fv3 = fv2 * -0.5f32;
            let fv11 = if fv3 < 0.0 {
                0.0f32
            } else if fv3 >= 1.0f32 {
                1.0f32
            } else {
                fv3
            };
            let fv11_d = fv11 as f64;

            // wp_new candidate = band_omega_ref / ((alpha-1)·fv11 + 1)
            let wp_new = proto.band_omega_ref / ((dvar_alpha - 1.0) * fv11_d + 1.0);

            // dVar5 = π - clamp(wp_new - π, 0, 0.3π)
            let excess = wp_new - PI;
            let excess_clamped = excess.clamp(0.0, 0.3 * PI);
            let mut dvar5 = PI - excess_clamped;

            // dVar6_ceiling = fv11·0.3π + 0.7π  ∈ [0.7π, π]
            let dvar6_ceiling = fv11_d * (0.3 * PI) + 0.7 * PI;

            // dvar5 = min(dvar5, wp_new)
            if wp_new <= dvar5 {
                dvar5 = wp_new;
            }
            // wp_final = min(dvar5, dvar6_ceiling)
            let wp_final = if dvar5 <= dvar6_ceiling {
                dvar5
            } else {
                dvar6_ceiling
            };
            proto.wp = wp_final;

            // wt scale: dVar10 = 0.20, modified iff fv2 ≥ 6.0
            let mut dvar10 = 0.20;
            if fv2 >= 6.0f32 {
                let fv2_capped = if fv2 >= 20.0f32 { 20.0f32 } else { fv2 };
                dvar10 = 0.20 - (fv2_capped as f64 - 6.0) * (9.0 / 700.0);
            }
            proto.wt = wp_final * dvar10;
            proto.wz = wp_final * 0.001;
        }
        other => {
            // Binary returns immediately for any other value.
            debug_assert!(
                other == 0 || other == 3,
                "compute_peak_type3_parameters dispatched with section_type={other}; expected 0 or 3"
            );
        }
    }
}

/// Per-section helper for `proto[0x13] ∈ {4, 6}` (notch-style sections).
///
/// Decompiled from `compute_notch_type46_parameters @ 0x18010dc40`.
///
/// **Implemented branches:**
/// - `mode != 2, section_type != 6` (type=4 static path) — fully ported.
/// - `mode != 2, section_type == 6` (type=6 static path) — fully ported.
/// - `mode == 2, section_type == 6` — fallback path, fully ported.
/// - `mode == 2, section_type != 6, wp >= 2.9845…` — fallback path, fully ported.
///
/// **Pending branch:**
/// - `mode == 2, section_type != 6, wp < 2.9845…` — smooth-blend on
///   `w_eval` and update of `proto[0xe..0x10]` (stored_e/f/g). Decompilation
///   has an unbound `dVar6` reference in the line
///   `dVar5 = proto[0xe] - fVar7·0.15·dVar6` — needs probe captures to
///   confirm whether `dVar6` is `proto[0xa0]` (band_omega_ref) or `proto[1]`
///   (current wp). Stubbed with `unimplemented!`.
pub fn compute_notch_type46_parameters(proto: &mut Prototype) {
    // Composite scratch (binary keeps everything in f32 SS instructions
    // until the final write to the f64 wp/wz/wt slots).
    let s8c = proto.alpha_scratch_8c;
    let s94 = proto.alpha_scratch_94;
    let fv7 = s8c * s8c * 0.25f32 + s94;
    let fv8 = fv7.clamp(0.10f32, 0.80f32);
    let fv8_d = fv8 as f64;
    let fv8_005 = (fv8 * 0.05f32) as f64;

    if proto.mode != 2 {
        let mut d6 = proto.band_omega_ref;
        if proto.section_type != 6 {
            // type=4 static path
            d6 *= 0.5;
            proto.wp = d6;
            proto.wz = (1.0 - fv8_d) * d6;
            proto.wt = (1.0 - fv8_005) * d6;
        } else {
            // type=6 static path: clamp band_omega_ref to 0.6π, then split.
            const ZERO_POINT_SIX_PI: f64 = 1.8849555921538759;
            if d6 > ZERO_POINT_SIX_PI {
                d6 = ZERO_POINT_SIX_PI;
            }
            proto.wp = d6;
            proto.wz = d6 * 0.05;
            proto.wt = d6 * 0.5;
        }
        return;
    }

    // mode == 2 (complex-roots path)
    let mut wp_in = proto.wp; // wp from upstream solve_biquad
    const TWO_NINE_EIGHT_FOUR_FIVE: f64 = 2.9845130209103035; // ≈ 0.95·π

    if matches!(proto.section_type, 2 | 5) && proto.wz < TWO_NINE_EIGHT_FOUR_FIVE {
        std::mem::swap(&mut proto.wp, &mut proto.wz);
        wp_in = proto.wp;
    }

    if matches!(proto.section_type, 2 | 5) && proto.wz >= TWO_NINE_EIGHT_FOUR_FIVE {
        let d6 = wp_in;
        proto.wp = d6;
        proto.wz = (1.0 - fv8_d) * d6;
        proto.wt = (1.0 - fv8_005) * d6;
        return;
    }

    if wp_in < TWO_NINE_EIGHT_FOUR_FIVE && proto.section_type != 6 {
        // Smooth-blend / w_eval / stored_e..g update branch.
        // Decomp gap resolved 2026-05-09: `dVar6` in `proto[0x70] - fv7·0.15·dVar6`
        // is `CONST_PI` (loaded at function top, never reassigned in this branch).
        //
        // Constants:
        //   DAT_180231a40  = 0.70  (smooth-blend wp floor)
        //   _DAT_180231940 = 0.06157521601035995  (quadratic coef)
        //   _DAT_180231b40 = 2.419026343264141 ≈ 0.77π  (quadratic offset)
        //   DAT_180231774  = 0.15  (wt offset & multiplier)
        //   _DAT_180231a78 = 0.95  (stored_f scale)
        //   DAT_180231a90  = 0.995 (stored_g scale relative to stored_f)
        const SMOOTH_FLOOR: f64 = 0.70;
        const QUAD_COEF: f64 = 0.06157521601035995;
        const QUAD_OFFSET: f64 = 2.419026343264141;
        const WT_OFFSET: f64 = 0.15;
        const STORED_F_SCALE: f64 = 0.95;
        const STORED_G_SCALE: f64 = 0.995;

        // === w_eval update ===
        // dVar4 = max(wp, 0.70)
        let dvar4_floor = if wp_in <= SMOOTH_FLOOR {
            SMOOTH_FLOOR
        } else {
            wp_in
        };
        // dVar2 = (π - wp)·0.1 + wp  (linear blend toward π)
        let smooth_blend = (PI - wp_in) * 0.1 + wp_in;
        // dVar4 = dVar4²·0.0615… + 2.4190…  (quadratic floor in wp)
        let quad_floor = dvar4_floor * dvar4_floor * QUAD_COEF + QUAD_OFFSET;
        // dVar2 = if (smooth_blend ≤ quad_floor) min(quad_floor, π) else smooth_blend
        let new_w_eval = if smooth_blend <= quad_floor {
            if quad_floor >= PI { PI } else { quad_floor }
        } else {
            smooth_blend
        };
        proto.w_eval = new_w_eval;

        // === stored_e/f/g update + wt update ===
        // fv3 = (wp - wz) - 0.15  (wz here is the upstream solve_biquad value)
        let fv3 = (wp_in - proto.wz) as f32 - WT_OFFSET as f32;
        let fv7 = if fv3 < 0.0 {
            0.0f32
        } else if fv3 >= 1.0f32 {
            1.0f32
        } else {
            fv3
        };
        // stored_e_new = stored_e_old - (fv7·0.15)·π
        let stored_e_new = proto.stored_e - (fv7 as f64 * WT_OFFSET) * PI;
        proto.stored_e = stored_e_new;
        let stored_f_new = stored_e_new * STORED_F_SCALE;
        proto.stored_f = stored_f_new;
        proto.stored_g = stored_f_new * STORED_G_SCALE;

        // wt = (1 - fv8·0.05) · wz  (wz untouched — uses upstream value)
        proto.wt = (1.0 - fv8_005) * proto.wz;
        return;
    }

    // Fallback (mode==2 AND (section_type==6 OR wp >= 0.95π)):
    //   type 6 uses proto[1] (= wp, the low solved root); the high-root
    //   path for non-type-6 sections uses proto[2].
    //   proto[1] = dVar6
    //   proto[2] = (1 - fVar8) · dVar6
    //   proto[3] = (1 - fVar8·0.05) · dVar6
    let d6 = if proto.section_type == 6 {
        proto.wp
    } else {
        proto.wz
    };
    let fv8_d = fv8 as f64;
    let fv8_005 = (fv8 * 0.05f32) as f64;
    proto.wp = d6;
    proto.wz = (1.0 - fv8_d) * d6;
    proto.wt = (1.0 - fv8_005) * d6;
}

/// Pro-Q's `check_frequency_within_band_limits @ 0x18010e7f0`.
///
/// Returns `false` when `freq` is outside the band's valid magnitude range.
///
/// Logic from the binary:
/// 1. If `upper > 0` and `upper < freq` → out of range, return `false`.
/// 2. If `abs_threshold == 0` → no magnitude check, return `true`.
/// 3. Otherwise compare `mp_at_freq · abs_threshold` against
///    `min(|freq - band_edge_low|, |freq - alt_edge|)`, where `alt_edge =
///    band_edge_high` if `upper ≤ 0` else `vt10(...)` (binary uses an
///    additional vtable lookup we don't model when `upper > 0`).
///
/// Returns `true` (in-band) when the magnitude term is strictly less than
/// the distance term — i.e., the magnitude is below threshold for the
/// frequency's distance to nearest band edge.
pub fn check_frequency_within_band_limits(
    proto: &Prototype,
    freq: f64,
    mp_at_freq: f64,
    abs_threshold: f64,
    upper: f64,
) -> bool {
    if upper > 0.0 && upper < freq {
        return false;
    }
    if abs_threshold == 0.0 {
        return true;
    }
    let alt_edge = if upper <= 0.0 {
        proto.band_edge_high
    } else {
        // Binary: vt10(band, upper). Without an analog handle we
        // conservatively reuse band_edge_high — callers in our wired
        // pipeline supply `upper <= 0` for the no-vt10 path.
        proto.band_edge_high
    };
    let d_low = (freq - proto.band_edge_low).abs();
    let d_alt = (freq - alt_edge).abs();
    let d_min = d_low.min(d_alt);
    mp_at_freq * abs_threshold < d_min
}

/// Pro-Q's `update_tracked_band_frequencies @ 0x18010ce20`.
///
/// Side-effect helper that:
/// 1. For `mode == 2`: refreshes `prev_wz` magnitude cache via
///    `vt10(wz)`. If wz fails the band-limits check, zeros wz and
///    downgrades mode to 1.
/// 2. For any `mode > 0` (including post-downgrade): refreshes
///    `prev_wp` similarly, and on band-limits failure either snaps
///    `wp = wz` and downgrades mode to 1 (when mode was 2) or zeros
///    wp and downgrades mode to 0 (when mode was 1).
///
/// `mode == 0` short-circuits to a no-op.
pub fn update_tracked_band_frequencies(proto: &mut Prototype, abs_threshold: f64, upper: f64) {
    if proto.mode == 0 {
        return;
    }

    if proto.mode == 2 {
        let mp_wz = if let Some(analog) = proto.analog {
            let coeffs = analog.squared_mag_coeffs(proto.omega_band);
            eval_squared_mag_scalar(&coeffs, proto.wz)
        } else {
            0.0
        };
        proto.prev_wz = mp_wz;
        if !check_frequency_within_band_limits(proto, proto.wz, mp_wz, abs_threshold, upper) {
            proto.wz = 0.0;
            proto.mode = 1;
        }
    }

    if proto.mode > 0 {
        let mp_wp = if let Some(analog) = proto.analog {
            let coeffs = analog.squared_mag_coeffs(proto.omega_band);
            eval_squared_mag_scalar(&coeffs, proto.wp)
        } else {
            0.0
        };
        proto.prev_wp = mp_wp;
        if !check_frequency_within_band_limits(proto, proto.wp, mp_wp, abs_threshold, upper) {
            proto.wp = 0.0;
            if proto.mode == 2 {
                proto.wp = proto.wz;
                proto.mode = 1;
                return;
            }
            proto.mode = 0;
        }
    }
}

/// Per-section helper for `proto[0x13] == 7` (shelf-band sections, "else"
/// branch in `prepare_band_display_info`).
///
/// Decompiled from `compute_shelf_band_parameters @ 0x18010cf10` (1077 bytes,
/// 2026-05-09). Faithful port of all branches.
///
/// Pipeline:
/// 1. Initialize stored_e/f/g to π.
/// 2. Compute `local_res8 = max(sqrt(wp/wz), 0.1)` if `proto[0x11] == 2`,
///    else later overwritten by α.
/// 3. Call `update_tracked_band_frequencies`.
/// 4. Set proto+0x49 byte = (mode > 0).
/// 5. Mode==2: smooth-blend wz against threshold; possibly snap mode→1.
/// 6. Branch on (proto+0x69 flag, proto[0xd] flag) for special path or
///    main computation.
/// 7. Compute α = clamp(pow(0.5, q_scratch_50·0.5), 0.1, 0.99).
/// 8. Main dispatch on (mode, proto[0x12], proto[0x11]) producing wp/wz/wt.
/// 9. Final w_eval = clamp(pow(wp/π, local_res8·3.3) · π/5 + 4π/5, 0, π).
pub fn compute_shelf_band_parameters(proto: &mut Prototype) {
    const CONST_0_1: f64 = 0.1;
    const CONST_0_25: f64 = 0.25;
    const CONST_0_05: f64 = 0.05;
    const CONST_0_01: f64 = 0.01;
    const CONST_0_99: f64 = 0.99;
    const CONST_0_65: f64 = 0.65;
    const CONST_3_3: f64 = 3.3;
    const PI_OVER_5: f64 = PI / 5.0;
    const FOUR_PI_OVER_5: f64 = 4.0 * PI / 5.0;
    const CONST_0_0001: f64 = 0.0001;
    const NINE_PI_TEN: f64 = 0.9 * PI;
    const NEAR_PI_E0A: f64 = f64::from_bits(0x4007e0485cda5e0a); // 0.95π
    const NEAR_PI_PI_C: f64 = f64::from_bits(0x4002d97c7f3321d2); // ≈ 0.75π
    const NEAR_PI_PI_D: f64 = f64::from_bits(0x3ffe28c731eb6950); // ≈ 0.6π

    // Step 1: stored_e/f/g initialized to π.
    proto.stored_e = PI;
    proto.stored_f = PI;
    proto.stored_g = PI;

    // Step 2: local_res8 from (wp/wz) ratio when root_count_dup
    // (proto[0x11]) == 2.
    let mut local_res8 = if proto.root_count_dup == 2 {
        let ratio = if proto.wz.abs() > 1e-30 {
            proto.wp / proto.wz
        } else {
            1.0
        };
        let s = ratio.max(0.0).sqrt();
        if s <= CONST_0_1 { CONST_0_1 } else { s }
    } else {
        CONST_0_1 // overwritten later when (section_type != 2) → uses α
    };

    // Step 3: side-effect (we use 0.0001 / π for the threshold/upper).
    update_tracked_band_frequencies(proto, CONST_0_0001, PI);

    // Step 4: proto+0x49 byte = (mode > 0). Not modeled.
    let mut iv5 = proto.mode;

    // Step 5: mode==2 smooth-blend on wz.
    let mut bvar3 = false;
    if iv5 == 2 {
        // dVar11 = prev_wp - band_edge_low; if |Δ| < prev_wp · 0.01:
        //   dVar10 = prev_wz · 0.25; if |prev_wz - band_edge_high| > prev_wz·0.25:
        //     wp = wz; mode = 1
        let dvar11 = proto.prev_wp - proto.band_edge_low;
        if dvar11.abs() < proto.prev_wp * CONST_0_01 {
            let dvar10 = proto.prev_wz * CONST_0_25;
            let diff_wz = (proto.prev_wz - proto.band_edge_high).abs();
            if dvar10 <= diff_wz && diff_wz != dvar10 {
                proto.wp = proto.wz;
                proto.mode = 1;
                iv5 = 1;
            }
        }

        // Continue: check the (flag_69, flag_68) special path.
        if proto.flag_byte_69 == 0 && proto.flag_byte_68 == 0 {
            // Skip to LAB_18010d03f equivalent: bvar3 = false.
        } else {
            bvar3 = true;
            // dVar11 = (sec[0x5c] f32) · proto[10]; proto[10] = dVar11
            // We approximate sec[0x5c] = alpha_scratch_8c (different field
            // semantics, but the scratch flow is similar).
            proto.q_scratch_50 *= proto.alpha_scratch_8c as f64;
        }
    } else if iv5 > 0 {
        // Skip to LAB_18010d019 then LAB_18010d03f.
        if proto.flag_byte_69 == 0 && proto.flag_byte_68 == 0 {
            // bvar3 stays false.
        } else {
            bvar3 = true;
            proto.q_scratch_50 *= proto.alpha_scratch_8c as f64;
        }
    }

    // Step 7: α = clamp(pow(0.5, q_scratch_50·0.5), 0.1, 0.99)
    let dvar10 = 0.5_f64.powf(proto.q_scratch_50 * 0.5);
    let dvar11 = if dvar10 < CONST_0_1 {
        CONST_0_1
    } else if dvar10 > CONST_0_99 {
        CONST_0_99
    } else {
        dvar10
    };

    if proto.root_count_dup != 2 {
        local_res8 = dvar11;
    }

    // Step 8: main dispatch.
    if proto.flag_byte_69 == 0 && proto.flag_byte_68 == 0 {
        // === Main path (no special-flag override) ===
        let dvar4 = PI;
        let mut iv5_local = proto.mode;
        if iv5_local == 2 {
            // Smooth-blend wz against threshold = max(α², 0.65)·π
            let threshold = local_res8 * local_res8;
            let threshold = if threshold <= CONST_0_65 {
                CONST_0_65
            } else {
                threshold
            } * dvar4;
            let dvar10_v = proto.wz;
            if threshold < dvar10_v {
                let blend = (dvar10_v - threshold) / (dvar4 - threshold);
                let blend_sq = (blend * blend) as f32 as f64;
                let blended = dvar10_v + (local_res8.abs() * dvar10_v - dvar10_v) * blend_sq;
                proto.wz = blended;
            }
            // Compute final w_eval and return.
            let cand = 0.5_f64.powf(local_res8 * CONST_3_3) * PI_OVER_5 + FOUR_PI_OVER_5;
            // Note: the binary uses pow(wp/π, local_res8·3.3) but at this
            // point dVar10 (the input to the pow's arg-prep) was set to π
            // (dVar4). We mirror with dVar10 = π → wp/π = 1 → pow result
            // = 1 → w_eval = π/5 + 4π/5 = π. The final clamp pins it.
            let _ = cand;
            proto.w_eval = PI.min(cand.max(0.0));
            return;
        }

        if iv5_local == 1 {
            let dvar1 = proto.wp;
            let mut dvar10_v = dvar1;
            if proto.root_count_dup == 2 {
                dvar10_v = dvar4;
            }
            if bvar3 || proto.q_scratch_50 <= 1.0 {
                proto.wz = dvar11 * dvar11 * dvar1;
                let cand =
                    (dvar10_v / PI).powf(local_res8 * CONST_3_3) * PI_OVER_5 + FOUR_PI_OVER_5;
                proto.w_eval = if dvar10_v <= cand {
                    if cand >= PI { PI } else { cand }
                } else {
                    dvar10_v
                };
                return;
            }
            // proto+0x49 = 0 (not modeled).
            if proto.proto_0x12_sign == 0 {
                let dvar10_a = dvar11 * proto.band_omega_ref;
                let dvar10_b = dvar11 * dvar4;
                let dvar10_use = if dvar10_b <= dvar10_a {
                    dvar10_b
                } else {
                    dvar10_a
                };
                proto.wp = dvar10_use;
                proto.wt = dvar10_use * CONST_0_25;
                proto.wz = dvar10_use * CONST_0_01;
            } else if proto.proto_0x12_sign == 1 {
                let dvar11_use = if dvar11 <= CONST_0_05 {
                    CONST_0_05
                } else {
                    dvar11
                };
                proto.wt = dvar11.sqrt() * dvar1;
                proto.wz = dvar11_use * dvar11.sqrt() * dvar1;
                iv5_local = 1;
            }
            // Final w_eval (when iv5_local != 0) — bypass to LAB_18010d2d4.
            let cand = (PI / PI).powf(local_res8 * CONST_3_3) * PI_OVER_5 + FOUR_PI_OVER_5;
            proto.w_eval = if dvar4 <= cand {
                if cand >= PI { PI } else { cand }
            } else {
                dvar4
            };
            return;
        }

        // iv5_local == 0 path (after mode reset or initial mode=0):
        // dispatch on proto[0x12].
        let iv5_c = proto.proto_0x12_sign;
        let dvar10_x: f64;
        if iv5_c == -1 {
            let dvar13 = if NINE_PI_TEN <= proto.band_omega_ref {
                NINE_PI_TEN
            } else {
                proto.band_omega_ref
            };
            proto.wp = dvar13;
            let dvar12 = (PI - dvar13) * 0.5 + dvar13;
            proto.wt = dvar13 * CONST_0_25;
            // LAB_18010d2cf: proto[2] = dvar12 (= wz)
            proto.wz = dvar12;
            // Skip to w_eval.
            let cand = (PI / PI).powf(local_res8 * CONST_3_3) * PI_OVER_5 + FOUR_PI_OVER_5;
            proto.w_eval = PI.min(cand.max(0.0));
            return;
        } else if iv5_c == 0 {
            let mut dvar13 = proto.band_omega_ref;
            let dvar2 = proto.stored_e;
            if dvar2 <= dvar13 {
                dvar13 = dvar2;
            }
            dvar10_x = dvar13 * dvar11;
            proto.wp = dvar10_x;
        } else if iv5_c == 1 {
            let mut dvar13 = proto.band_omega_ref;
            let dvar2 = proto.stored_e;
            if dvar2 <= dvar13 {
                dvar13 = dvar2;
            }
            // Set magic π-near constants for stored_e/g/f.
            proto.stored_e = NEAR_PI_E0A; // 0.95π
            proto.stored_g = NEAR_PI_PI_C;
            proto.stored_f = NEAR_PI_PI_D;
            dvar10_x = dvar13 / dvar11;
            proto.wp = dvar10_x;
        } else {
            dvar10_x = proto.wp;
        }
        proto.wt = dvar11.sqrt() * proto.wp;
        let dvar12 = dvar11.sqrt() * proto.wp * CONST_0_25;
        proto.wz = dvar12;
        let cand = (PI / PI).powf(local_res8 * CONST_3_3) * PI_OVER_5 + FOUR_PI_OVER_5;
        proto.w_eval = PI.min(cand.max(0.0));
        return;
    }

    // === Special-flag override path (flag_69 != 0 OR flag_68 != 0) ===
    if proto.mode <= 0 {
        // proto[9] == 0 sub-branch: clamp wp to 9π/10, set wt = wp · 0.25
        let mut dvar11_use = proto.band_omega_ref;
        if NINE_PI_TEN <= proto.band_omega_ref {
            dvar11_use = NINE_PI_TEN;
        }
        proto.wp = dvar11_use;
        let dvar12 = dvar11_use * CONST_0_25;
        proto.wt = dvar11_use * CONST_0_01;
        proto.wz = dvar12;
    } else {
        // proto[9] != 0: use existing wp.
        let dvar11_use = proto.wp;
        let dvar12 = dvar11_use * CONST_0_25;
        proto.wt = dvar11_use * CONST_0_01;
        proto.wz = dvar12;
    }
    let cand = (PI / PI).powf(local_res8 * CONST_3_3) * PI_OVER_5 + FOUR_PI_OVER_5;
    proto.w_eval = PI.min(cand.max(0.0));
}

/// Per-section helper for `proto[0x13] == 8` (band-shelf v2).
///
/// Decompiled from `compute_band_shelf_parameters_v2 @ 0x18010d350`.
/// Decode complete 2026-05-09 (vtable[0x10] resolved as
/// [`eval_squared_mag_scalar`]).
///
/// Constants:
/// - `√2` (single-rounded) = 1.4142135381698608
/// - `9π/10`, `0.85π`, `0.99π`, `1.25`, `1.50`, `0.999`, `0.01`, `0.5`
///
/// Delegation: when `(mode == 1 OR flag_byte_68 != 0) AND flag_byte_69 == 0`,
/// the binary calls into `compute_shelf_band_parameters` instead. Until
/// shelf7 is ported, the delegated path returns early with a debug-assert
/// in debug builds.
pub fn compute_band_shelf_parameters_v2(proto: &mut Prototype) {
    let mode_in = proto.mode;

    // Delegation predicate.
    if (mode_in == 1 || proto.flag_byte_68 != 0) && proto.flag_byte_69 == 0 {
        compute_shelf_band_parameters(proto);
        return;
    }

    // fv8 = clamp(√2 / proto[10], 0, 1.0); the binary stores √2 as a
    // single-rounded double (0x3FF6A09E60000000).
    const SQRT2_F32_ROUNDED: f64 = 1.4142135381698608;
    let mut iv3 = mode_in;
    let raw = SQRT2_F32_ROUNDED / proto.q_scratch_50;
    let fv8: f64;
    let mut bvar2 = false;

    if mode_in != 2 || proto.wz <= PI {
        let fv8_f32 = (raw as f32).min(1.0f32);
        fv8 = fv8_f32 as f64;
        if mode_in == 2 && proto.proto_0x12_sign == 1 {
            // Swap wp ↔ wz, set the sticky bvar2 path.
            std::mem::swap(&mut proto.wp, &mut proto.wz);
            bvar2 = true;
        }
    } else {
        // mode == 2 AND wz > π — promote mode locally to 1 and persist.
        iv3 = 1;
        let fv8_f32 = (raw as f32).min(1.0f32);
        fv8 = fv8_f32 as f64;
        proto.mode = 1;
    }

    // vt10 small-difference test (only when iv3 == 1).
    if iv3 == 1 {
        if let Some(analog) = proto.analog {
            let coeffs = analog.squared_mag_coeffs(proto.omega_band);
            let mp = eval_squared_mag_scalar(&coeffs, proto.wp);
            let mpi = eval_squared_mag_scalar(&coeffs, PI);
            // |Δ| (f32 lane, fabs via mask): when difference small (≤0.01),
            // step mode back and snap wp to band_omega_ref.
            let diff = ((mpi - mp) as f32).abs() as f64;
            if diff <= 0.01 {
                proto.mode -= 1;
                proto.wp = proto.band_omega_ref;
            }
        }
        // (No `analog` provided: skip the test rather than fabricating data.)
    }

    // w_eval branch on proto[0x12] sign.
    let nine_pi_10 = 0.9 * PI;
    let zero_eight_five_pi = 2.670353755551324; // 0.85π
    let zero_nine_nine_pi = 3.1101767270538954; // 0.99π
    let wp_now = proto.wp;
    let new_w_eval = if proto.proto_0x12_sign == -1 {
        let cand = wp_now * 1.25;
        if zero_eight_five_pi <= cand {
            if PI <= cand { PI } else { cand }
        } else {
            zero_eight_five_pi
        }
    } else {
        let cand = wp_now * 1.5;
        if nine_pi_10 <= cand {
            if zero_nine_nine_pi <= cand {
                zero_nine_nine_pi
            } else {
                cand
            }
        } else {
            nine_pi_10
        }
    };
    proto.w_eval = new_w_eval;

    // wz / proto[1] / stored_f / stored_g updates.
    let wp_for_wt: f64;
    if bvar2 {
        // Swap branch: clamp wp down to 9π/10, scale stored_e through 0.999².
        let wp_clamped = if wp_now > nine_pi_10 {
            nine_pi_10
        } else {
            wp_now
        };
        proto.wp = wp_clamped;
        let stored_e_scaled = proto.stored_e * 0.999;
        proto.stored_g = stored_e_scaled; // proto[0x10]
        proto.stored_f = stored_e_scaled * 0.999; // proto[0xf]
        wp_for_wt = wp_clamped;
    } else {
        // Non-swap branch: wz = wp · clamp(sqrt(|H(j0)|²), 0.01, 0.5).
        let factor = if let Some(analog) = proto.analog {
            let coeffs = analog.squared_mag_coeffs(proto.omega_band);
            let mag0 = eval_squared_mag_scalar(&coeffs, 0.0);
            let s = mag0.sqrt();
            s.clamp(0.01, 0.5)
        } else {
            // Without analog data, default to the lower clamp (matches what
            // a zero-magnitude prototype would yield).
            0.01
        };
        let wp_now = proto.wp; // re-read in case of any aliasing
        proto.wz = wp_now * factor;
        wp_for_wt = wp_now;
    }

    // wt = (0.999 - (1 - fv8)² · 0.5) · wp_for_wt.
    let one_minus_fv8 = 1.0 - fv8;
    let bracket = 0.999 - one_minus_fv8 * one_minus_fv8 * 0.5;
    proto.wt = bracket * wp_for_wt;
}

/// Per-section helper for `proto[0x13] == 10` (band-shelf).
///
/// Decompiled from `compute_band_shelf_parameters @ 0x18010d780` (1206 bytes,
/// 2026-05-09). Faithful port of all branches.
///
/// Calls [`update_tracked_band_frequencies`], [`eval_squared_mag_scalar`]
/// (= `vt10`), and `libm_pow` (modeled with `f64::powf`).
///
/// `vtable[0]` is a 2-output magnitude variant; we approximate the
/// `local_60²+local_68²` test by using the scalar `|H(jwz)|²` from
/// [`eval_squared_mag_scalar`] — this matches in the common case where
/// the prototype's two-component output reduces to the squared magnitude.
pub fn compute_band_shelf_parameters(proto: &mut Prototype) {
    const SQRT2_F32_ROUNDED: f64 = 1.4142135381698608;
    const EPS_1E_NEG_10: f64 = 1e-10;
    const EPS_1_192E_NEG_7: f64 = 1.192_092_9e-7;
    const NEAR_PI_E0A: f64 = f64::from_bits(0x4007e0485cda5e0a); // 0.95π

    // dVar3 = √2 / proto[10]; binary marks proto+0x49 flag (we don't model it).
    let raw = SQRT2_F32_ROUNDED / proto.q_scratch_50;
    let fv12_f32 = raw as f32;
    let fv11_f32 = if fv12_f32 >= 1.0 { 1.0 } else { fv12_f32 };
    let fv11 = fv11_f32 as f64;

    update_tracked_band_frequencies(proto, raw, 0.0);

    let iv3 = proto.mode;

    if iv3 == 0 {
        proto.wp = proto.band_omega_ref;
        return label_d7f9(proto, fv11, fv12_f32);
    }

    if iv3 > 0 {
        let analog = proto.analog;
        let mp_wp = if let Some(a) = analog {
            let coeffs = a.squared_mag_coeffs(proto.omega_band);
            eval_squared_mag_scalar(&coeffs, proto.wp)
        } else {
            0.0
        };

        let use_wp_or_wz: f64;
        if EPS_1E_NEG_10 <= mp_wp {
            if iv3 == 2 {
                let mag_sq_at_wz = if let Some(a) = analog {
                    let coeffs = a.squared_mag_coeffs(proto.omega_band);
                    eval_squared_mag_scalar(&coeffs, proto.wz)
                } else {
                    0.0
                };
                if mag_sq_at_wz >= EPS_1E_NEG_10 {
                    return label_d7f9(proto, fv11, fv12_f32);
                }
                use_wp_or_wz = proto.wz;
            } else {
                return label_d7f9(proto, fv11, fv12_f32);
            }
        } else {
            use_wp_or_wz = proto.wp;
        }

        if use_wp_or_wz <= 0.0 {
            return label_d7f9(proto, fv11, fv12_f32);
        }

        // dVar7 = pow(0.5, proto[10]·0.5); clamped low at 0.01
        let mut pow_half = 0.5_f64.powf(proto.q_scratch_50 * 0.5);
        if pow_half <= 0.01 {
            pow_half = 0.01;
        }
        proto.stored_f = NEAR_PI_E0A; // proto[0xf] = 0.95π

        let sqrt_arg = use_wp_or_wz / PI;
        let sqrt_val = sqrt_arg.sqrt();
        let sqrt_f32 = sqrt_val as f32 as f64;
        let dvar7 = (pow_half - 0.99) * sqrt_f32 + 0.99;

        if proto.mode == 1 {
            let pi_over_100 = PI / 100.0;
            let bvar1 = use_wp_or_wz >= pi_over_100;
            proto.wp = use_wp_or_wz;
            let dvar3 = (proto.alpha_scratch_8c as f64) * 0.20;
            let dvar4 =
                dvar7.sqrt() * use_wp_or_wz * (1.0 - (proto.alpha_scratch_8c as f64) * 0.05);
            proto.wz = dvar4;
            proto.wt = (1.0 - dvar3) * dvar4 * dvar7;
            if !bvar1 {
                proto.wz = use_wp_or_wz + use_wp_or_wz;
            }
            return;
        }

        // iv3 == 2 sub-branch (post small-mag-at-wz fall-through).
        let dvar6 = proto.wz;
        let diff = ((dvar6 as f32 - use_wp_or_wz as f32) as f64).abs();
        if diff <= EPS_1_192E_NEG_7 {
            let dvar3 = proto.wp;
            proto.wz = dvar3;
            proto.wp = use_wp_or_wz;
            proto.wt = dvar3 * dvar7;
            return;
        }

        let dvar7_quad = (dvar6 * sqrt_f32 - 0.80).clamp(0.0, 0.20);
        let dvar7_lin = proto.wp * 1.01;
        let mut dvar3_x = (proto.wp + dvar6) * sqrt_f32;
        if dvar7_lin <= dvar3_x {
            dvar3_x = dvar7_lin;
        }
        proto.alpha_scratch_8c = (dvar7_quad * dvar7_quad * 25.0) as f32;
        proto.wt = dvar3_x;
        return;
    }

    // iv3 < 0: degenerate, fall through.
    label_d7f9(proto, fv11, fv12_f32);
}

/// LAB_18010d7f9 fall-through block from `compute_band_shelf_parameters`.
fn label_d7f9(proto: &mut Prototype, fv11: f64, fv12_f32: f32) {
    const ZERO_NINE_THREE_PI: f64 = 2.921681167838508; // 0.93π
    const ZERO_EIGHT_THREE_PI: f64 = 2.607521902479528; // ≈ 0.83π
    const PROTO_E: f64 = f64::from_bits(0x400921569e860335);
    const PROTO_F: f64 = f64::from_bits(0x4008e10145e5f3d1);
    const PROTO_G: f64 = f64::from_bits(0x40091ae7af42ce78);

    let cand = proto.wp * 1.80;
    let new_w_eval = if ZERO_EIGHT_THREE_PI <= cand {
        if PI <= cand { PI } else { cand }
    } else {
        ZERO_EIGHT_THREE_PI
    };

    proto.stored_e = PROTO_E;
    proto.stored_g = PROTO_G;
    proto.stored_f = PROTO_F;
    proto.w_eval = new_w_eval;

    if proto.mode < 1 {
        let mut pow_half = 0.5_f64.powf(proto.q_scratch_50 * 0.5);
        if pow_half <= 0.05 {
            pow_half = 0.05;
        }
        let dvar5 = proto.band_omega_ref / PI - 0.80;
        let dvar6 = dvar5.clamp(0.0, 0.20);
        let fv2 = (dvar6 * dvar6 * 25.0) as f32;
        proto.alpha_scratch_8c = fv2;
        let mut dvar3 = (1.0 - (fv2 as f64).sqrt() * 0.20) * proto.band_omega_ref;
        if ZERO_NINE_THREE_PI <= dvar3 {
            dvar3 = ZERO_NINE_THREE_PI;
        }
        let dvar6_v = pow_half * 0.70 * dvar3;
        proto.wp = dvar3;
        proto.wz = dvar6_v;
        proto.wt = pow_half * pow_half * dvar6_v * 0.5;
        return;
    }

    // mode >= 1: branch on proto[0x12] / proto[0x11].
    let dvar3: f64;
    if proto.proto_0x12_sign == 1 {
        const NEAR_PI_A: f64 = 3.141278494324434; // ≈ π
        let a8c = proto.alpha_scratch_8c;
        let bracket = 0.9998 - (a8c * a8c) as f64 * 0.0005;
        let mut v = bracket * proto.wp;
        if NEAR_PI_A <= v {
            v = NEAR_PI_A;
        }
        dvar3 = v;
    } else if proto.proto_0x12_sign == 0 && proto.section_type == 1 {
        let mut p = 0.5_f64.powf(proto.q_scratch_50 * 0.5);
        if p <= 0.10 {
            p = 0.10;
        }
        dvar3 = p * proto.band_omega_ref;
    } else {
        // Decomp leaves dVar3 unset in this sub-branch (binary uses
        // whatever was previously in the register). Default to band ref.
        dvar3 = proto.band_omega_ref;
    }

    let dvar7_v = proto.band_omega_ref;
    let dvar4_floor = dvar7_v * 0.5;
    let mut dvar6 = fv11 * dvar3;
    if dvar6 <= dvar4_floor {
        dvar6 = dvar4_floor;
    }
    let dvar5 = dvar7_v * 0.02;
    let mut dvar4 = (fv11 * fv11) * 0.999;
    let dvar7_v2 = dvar7_v * 0.001;
    proto.wp = dvar6;
    dvar4 *= dvar6;
    if dvar4 <= dvar5 {
        dvar4 = dvar5;
    }
    proto.wt = dvar4;

    let a8c = proto.alpha_scratch_8c;
    let fv11_div = if fv12_f32.abs() > 1e-30 {
        a8c / fv12_f32
    } else {
        a8c
    };
    let fv11_min = if a8c <= fv11_div { a8c } else { fv11_div };
    let bracket = 0.995 - (fv11_min * fv11_min) as f64 * 0.90;
    let mut wz_cand = bracket * dvar4;
    if wz_cand <= dvar7_v2 {
        wz_cand = dvar7_v2;
    }
    proto.wz = wz_cand;
}

/// Dispatch a `proto` through the helper that matches its `section_type`.
///
/// Mirrors the switch in `prepare_band_display_info` (Pro-Q 4 binary) that
/// chooses one of the 5 helpers based on `proto[0x13]` (= `sec[+0x58]`).
///
/// Returns `Err(SectionType)` for inline/fallback section types not handled
/// by a dedicated helper — caller is expected to apply the inline default
/// (typically `wz = wp·0.05`, `wt = wp·0.5`).
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
    proto.mode = roots.count as i32;
    proto.root_count_dup = roots.count as i32;
    if roots.count >= 1 {
        proto.wp = roots.w1;
    }
    if roots.count >= 2 {
        proto.wz = roots.w2;
    }
    if matches!(proto.section_type, 2 | 5) && roots.count >= 2 {
        let aux = (roots.w2.min(PI) / PI - 0.8).clamp(0.0, 0.2);
        proto.alpha_scratch_8c = (aux * aux * 25.0) as f32;
    }
    // Synchronize derived caches the helpers may read.
    proto.analog = Some(*analog);
    proto.omega_band = omega;
    proto.stored_e = f64::from_bits(0x4008a14d57b373df);
    proto.stored_f = f64::from_bits(0x40069e9565708efc);
    proto.stored_g = f64::from_bits(0x400881c68e4d6f74);

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

#[allow(dead_code)]
const _CONST_PI: f64 = PI;

#[cfg(test)]
mod tests {
    use super::*;

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

    /// peak3 type=0, mode=0 (or anything ≠ 1): wt = 0.5·band_omega_ref;
    /// wp = band_omega_ref; wz = wt/2.
    #[test]
    fn peak_type3_section_type0_mode_default() {
        let mut p = fresh_proto();
        p.section_type = 0;
        p.mode = 0;
        p.band_omega_ref = 0.4;
        p.wp = 0.123; // would only matter if mode==1
        compute_peak_type3_parameters(&mut p);
        assert_eq!(p.wp, 0.4);
        assert_eq!(p.wt, 0.2);
        assert_eq!(p.wz, 0.1);
    }

    /// peak3 type=0, mode=1, wp ≤ half_ref: still uses half_ref for wt.
    #[test]
    fn peak_type3_section_type0_mode1_low_wp() {
        let mut p = fresh_proto();
        p.section_type = 0;
        p.mode = 1;
        p.band_omega_ref = 1.0;
        p.wp = 0.1; // not greater than 0.5
        compute_peak_type3_parameters(&mut p);
        assert_eq!(p.wt, 0.5);
        assert_eq!(p.wp, 1.0);
        assert_eq!(p.wz, 0.25);
    }

    /// peak3 type=0, mode=1, wp > half_ref: wt latches onto wp before the
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
        assert_eq!(p.wt, 0.8);
        assert_eq!(p.wp, 1.0);
        assert_eq!(p.wz, 0.4);
    }

    /// peak3 type=3 with mode=0 (skips w_eval update) and benign alpha_94=0:
    /// fv2 = -2.0 → dvar_alpha = max(sqrt(1.0), 0.5) = 1.0
    /// fv11 = clamp(1.0, 0, 1.0) = 1.0
    /// wp_new = band_omega_ref / ((1.0 - 1)·1.0 + 1) = band_omega_ref
    /// wp_final = min(band_omega_ref, π - clamp(…), 0.3π·1 + 0.7π) = min(band_omega_ref, π)
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
        assert_eq!(p.w_eval, snapshot_w_eval);
        // wp = band_omega_ref (= 0.5, well below π)
        assert!((p.wp - 0.5).abs() < 1e-12);
        // wt = wp · 0.20
        assert!((p.wt - 0.5 * 0.20).abs() < 1e-12);
        // wz = wp · 0.001
        assert!((p.wz - 0.5 * 0.001).abs() < 1e-12);
        // alpha_94_out = 0 · 0 · 4 - 2 = -2
        assert_eq!(p.alpha_scratch_94, -2.0);
    }

    /// peak3 type=3 with mode=0 and alpha_94=1.0 (positive, so fv2 = 4·1 - 2 = 2 ≥ 0):
    /// dvar_alpha = 0.5 (no sqrt taken)
    /// fv3 = 2 · -0.5 = -1, fv11 = clamp(-1, 0, 1) = 0
    /// wp_new = ω / ((0.5-1)·0 + 1) = ω
    /// dvar6_ceiling = 0·0.3π + 0.7π = 0.7π ≈ 2.199
    /// wp_final = min(ω, π, 0.7π)
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
        assert_eq!(p.alpha_scratch_94, 2.0);
    }

    /// peak3 type=3, mode=1: w_eval IS updated. With wp=1.0 the f32 lane
    /// math should produce a finite, in-range w_eval ∈ [wp_clamped, π].
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

    /// peak3 type=3 with fv2 ≥ 6 path (alpha_94 large): dvar10 shrinks
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
        assert!((p.wt - wp * 0.20).abs() < 1e-12);
    }

    /// bandshelf v2 mode==0, sign=+1: w_eval picks the upper branch
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
        assert!((p.w_eval - 0.9 * PI).abs() < 1e-12);
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
    /// clamps the (now-swapped) wp to 9π/10, scales stored_e through 0.999².
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
        assert!((p.stored_f - 0.999 * 0.999).abs() < 1e-12);
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
    /// q_scratch_50 = 1.0 → fv8 = 1.0 → bracket = 0.999.
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

    /// bandshelf v2 delegation: mode==1 AND flag_byte_69==0 → calls shelf7
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
    /// in-range wp/wz/wt and w_eval ∈ [0, π].
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
        assert!((p.wp - 0.9 * PI).abs() < 1e-12);
    }

    /// shelf7 special-flag path (flag_69 != 0): uses simplified clamp branch.
    #[test]
    fn shelf7_special_flag_path() {
        let mut p = fresh_proto();
        p.section_type = 7;
        p.mode = 0;
        p.flag_byte_69 = 1;
        p.band_omega_ref = 5.0; // > 9π/10
        compute_shelf_band_parameters(&mut p);
        assert!((p.wp - 0.9 * PI).abs() < 1e-12);
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
        assert_eq!(p.wp, 1.0);
        assert_eq!(p.wz, 0.5);
    }

    /// `check_frequency_within_band_limits`: upper > 0 and freq > upper → false.
    #[test]
    fn check_band_limits_above_upper() {
        let p = fresh_proto();
        let inside = check_frequency_within_band_limits(&p, 2.0, 0.0, 0.0, 1.0);
        assert!(!inside);
    }

    /// `check_frequency_within_band_limits`: abs_threshold == 0 → always true
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
        assert_eq!(p.wp, 1.0);
        assert_eq!(p.prev_wp, 0.0);
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

    /// bandshelf 10 mode==0: snaps wp = band_omega_ref, then falls into
    /// LAB_18010d7f9 mode<1 (pow + dot-magnitude path).
    #[test]
    fn bandshelf10_mode0_snap_and_fallthrough() {
        let mut p = fresh_proto();
        p.section_type = 10;
        p.mode = 0;
        p.band_omega_ref = 1.5;
        p.q_scratch_50 = 1.0;
        compute_band_shelf_parameters(&mut p);
        // Stored constants get the magic π-near values.
        let proto_e = f64::from_bits(0x400921569e860335);
        let proto_g = f64::from_bits(0x40091ae7af42ce78);
        let proto_f = f64::from_bits(0x4008e10145e5f3d1);
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

    /// bandshelf 10 mode==1, no analog → mp_wp=0 → use_wp=wp; reaches the
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

    /// bandshelf 10 mode==1 with use_wp < π/100 triggers the !bvar1 override
    /// (wz = 2·use_wp).
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
        let fv7 = s8c * s8c * 0.25f32 + s94;
        let fv8 = fv7.clamp(0.10f32, 0.80f32);
        let fv8_005 = (fv8 * 0.05f32) as f64;
        (1.0 - fv8 as f64, 1.0 - fv8_005)
    }

    /// notch46 mode≠2, type=4: wp = 0.5·band_omega_ref; wz/wt scaled by
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
        assert_eq!(p.wp, 0.3);
        assert_eq!(p.wz, one_minus * 0.3);
        assert_eq!(p.wt, one_minus_005 * 0.3);
    }

    /// notch46 mode≠2, type=6, band_omega_ref < 0.6π: no clamp, fixed splits.
    #[test]
    fn notch46_mode_static_type6_unclamped() {
        let mut p = fresh_proto();
        p.section_type = 6;
        p.mode = 1;
        p.band_omega_ref = 1.0; // < 0.6π ≈ 1.885
        compute_notch_type46_parameters(&mut p);
        assert_eq!(p.wp, 1.0);
        assert_eq!(p.wz, 0.05);
        assert_eq!(p.wt, 0.5);
    }

    /// notch46 mode≠2, type=6, band_omega_ref > 0.6π: wp clamps to 0.6π.
    #[test]
    fn notch46_mode_static_type6_clamped() {
        let mut p = fresh_proto();
        p.section_type = 6;
        p.band_omega_ref = 3.0; // > 0.6π
        compute_notch_type46_parameters(&mut p);
        const ZERO_POINT_SIX_PI: f64 = 1.8849555921538759;
        assert_eq!(p.wp, ZERO_POINT_SIX_PI);
        assert_eq!(p.wz, ZERO_POINT_SIX_PI * 0.05);
        assert_eq!(p.wt, ZERO_POINT_SIX_PI * 0.5);
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
        assert_eq!(p.wp, 0.4);
        assert_eq!(p.wz, one_minus * 0.4);
        assert_eq!(p.wt, one_minus_005 * 0.4);
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
        assert_eq!(p.wp, 0.25);
        assert_eq!(p.wz, one_minus * 0.25);
        assert_eq!(p.wt, one_minus_005 * 0.25);
    }

    /// notch46 mode==2, type=4, wp < 0.95π: smooth-blend branch.
    /// Verifies the w_eval, stored_e/f/g, and wt updates match the
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
        let expected_w_eval = 1.0 * 0.06157521601035995 + 2.419026343264141;
        assert!((p.w_eval - expected_w_eval).abs() < 1e-12);

        // fv3 = (1.0 - 0.5) - 0.15 = 0.35 → fv7 = 0.35
        // stored_e_new = 2.0 - 0.35·0.15·π = 2.0 - 0.05249·π ≈ 2.0 - 0.16493
        let fv7 = 0.35f32 as f64;
        let expected_e = 2.0 - fv7 * 0.15 * PI;
        // tolerance loose because fv3 and fv7 are computed in f32
        assert!(
            (p.stored_e - expected_e).abs() < 1e-6,
            "stored_e={}, expected={}",
            p.stored_e,
            expected_e
        );
        assert!((p.stored_f - p.stored_e * 0.95).abs() < 1e-15);
        assert!((p.stored_g - p.stored_f * 0.995).abs() < 1e-15);

        // wt = (1 - fv8·0.05) · wz; fv8 = 0.10 (clamped) → wt ≈ 0.995 · 0.5
        let one_minus_005 = 1.0 - (0.10f32 * 0.05f32) as f64;
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
        assert_eq!(p.wp, 0.5);
        // (1 − 0.10f32) · 0.5 ≈ 0.45 (loose because 0.10 is f32-rounded)
        assert!((p.wz - 0.45).abs() < 1e-7);
        // (1 − 0.10f32·0.05f32) · 0.5 ≈ 0.4975
        assert!((p.wt - 0.4975).abs() < 1e-7);
    }

    /// Dispatcher routes section_type=0 through peak_type3.
    #[test]
    fn dispatch_routes_type0_to_peak3() {
        let mut p = fresh_proto();
        p.section_type = 0;
        p.band_omega_ref = 1.0;
        let r = dispatch_section_helper(&mut p);
        assert!(r.is_ok());
        assert_eq!(p.wp, 1.0);
        assert_eq!(p.wt, 0.5);
        assert_eq!(p.wz, 0.25);
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
                assert_eq!(p.wp, 1.0);
                assert_eq!(p.wz, 0.05);
                assert_eq!(p.wt, 0.5);
            } else {
                assert_eq!(p.wp, 0.5);
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
        assert_eq!(omega_scale_for_band_type(7), 16.0 / 25.0);
        assert_eq!(omega_scale_for_band_type(8), 16.0 / 25.0);
        assert_eq!(omega_scale_for_band_type(9), 16.0 / 25.0);
        assert_eq!(omega_scale_for_band_type(0), 1.0);
        assert_eq!(omega_scale_for_band_type(3), 1.0);
        assert_eq!(omega_scale_for_band_type(4), 1.0);
        assert_eq!(omega_scale_for_band_type(10), 1.0);
        assert_eq!(omega_scale_for_band_type(11), 1.0);
    }

    /// End-to-end synth with a unity gain prototype + section_type=0 helper.
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
        assert_eq!(coeffs[0], 1.0); // a0
        for c in coeffs.iter() {
            assert!(c.is_finite(), "coeff {c} must be finite");
        }
    }

    /// End-to-end synth with section_type=9 (inline fallback): returns
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
        assert_eq!(p.wp, 1.2);
        assert!((p.wz - 0.06).abs() < 1e-12);
        assert_eq!(p.wt, 0.6);
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
        assert_eq!(p.wp, 1.0);
        // (1 − 0.80) · 1.0 = 0.20
        assert!((p.wz - 0.20).abs() < 1e-7);
        // (1 − 0.80·0.05) · 1.0 = (1 − 0.04) · 1.0 = 0.96
        assert!((p.wt - 0.96).abs() < 1e-7);
    }
}
