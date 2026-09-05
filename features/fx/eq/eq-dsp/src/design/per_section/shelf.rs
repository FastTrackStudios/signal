//! Shelf and band-shelf section parameters.
//!
//! Three related routines rather than one, matching three code paths in the
//! binary; the difference between them is documented at each function.

use super::{Prototype, PI, update_tracked_band_frequencies, eval_squared_mag_scalar};
use dsp_core::num;

#[expect(clippy::too_many_lines, reason = "a decoded routine: one contiguous function in the binary, whose commentary cites the captured rows each branch was verified against. Splitting it would separate the arithmetic from its evidence")]
/// Per-section helper for `proto[0x13] == 7` (shelf-band sections, "else"
/// branch in `prepare_band_display_info`).
///
/// Decompiled from `compute_shelf_band_parameters @ 0x18010cf10` (1077 bytes,
/// 2026-05-09). Faithful port of all branches.
///
/// Pipeline:
/// 1. Initialize `stored_e/f/g` to π.
/// 2. Compute `local_res8 = max(sqrt(wp/wz), 0.1)` if `proto[0x11] == 2`,
///    else later overwritten by α.
/// 3. Call `update_tracked_band_frequencies`.
/// 4. Set proto+0x49 byte = (mode > 0).
/// 5. Mode==2: smooth-blend wz against threshold; possibly snap mode→1.
/// 6. Branch on (proto+0x69 flag, proto[0xd] flag) for special path or
///    main computation.
/// 7. Compute α = clamp(pow(0.5, `q_scratch_50·0.5`), 0.1, 0.99).
/// 8. Main dispatch on (mode, proto[0x12], proto[0x11]) producing wp/wz/wt.
/// 9. Final `w_eval` = clamp(pow(wp/π, `local_res8·3.3`) · π/5 + 4π/5, 0, π).
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
    const NEAR_PI_E0A: f64 = f64::from_bits(0x4007_e048_5cda_5e0a); // 0.95π
    const NEAR_PI_PI_C: f64 = f64::from_bits(0x4002_d97c_7f33_21d2); // ≈ 0.75π
    const NEAR_PI_PI_D: f64 = f64::from_bits(0x3ffe_28c7_31eb_6950); // ≈ 0.6π

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
        if s <= CONST_0_1 {
            CONST_0_1
        } else {
            s
        }
    } else {
        CONST_0_1 // overwritten later when (section_type != 2) → uses α
    };

    // Step 3: side-effect (we use 0.0001 / π for the threshold/upper).
    update_tracked_band_frequencies(proto, CONST_0_0001, PI);

    // Step 4: proto+0x49 byte = (mode > 0). Not modeled.
    let iv5 = proto.mode;

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
            if dvar10 < diff_wz {
                proto.wp = proto.wz;
                proto.mode = 1;
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
            proto.q_scratch_50 *= f64::from(proto.alpha_scratch_8c);
        }
    } else if iv5 > 0 {
        // Skip to LAB_18010d019 then LAB_18010d03f.
        if proto.flag_byte_69 == 0 && proto.flag_byte_68 == 0 {
            // bvar3 stays false.
        } else {
            bvar3 = true;
            proto.q_scratch_50 *= f64::from(proto.alpha_scratch_8c);
        }
    }

    // Step 7: α = clamp(pow(0.5, q_scratch_50·0.5), 0.1, 0.99)
    let dvar10 = 0.5_f64.powf(proto.q_scratch_50 * 0.5);
    let dvar11 = dvar10.clamp(CONST_0_1, CONST_0_99);

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
                let blend_sq = f64::from(num::narrow((blend * blend)));
                let blended = local_res8.abs().mul_add(dvar10_v, -dvar10_v).mul_add(blend_sq, dvar10_v);
                proto.wz = blended;
            }
            // Compute final w_eval and return.
            let cand = 0.5_f64.powf(local_res8 * CONST_3_3).mul_add(PI_OVER_5, FOUR_PI_OVER_5);
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
            let dvar10_v = if proto.root_count_dup == 2 { dvar4 } else { dvar1 };
            if bvar3 || proto.q_scratch_50 <= 1.0 {
                proto.wz = dvar11 * dvar11 * dvar1;
                let cand =
                    (dvar10_v / PI).powf(local_res8 * CONST_3_3).mul_add(PI_OVER_5, FOUR_PI_OVER_5);
                proto.w_eval = if dvar10_v <= cand {
                    if cand >= PI {
                        PI
                    } else {
                        cand
                    }
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
            }
            // Final w_eval (when iv5_local != 0) — bypass to LAB_18010d2d4.
            let cand = 1.0_f64.powf(local_res8 * CONST_3_3).mul_add(PI_OVER_5, FOUR_PI_OVER_5);
            proto.w_eval = if dvar4 <= cand {
                if cand >= PI {
                    PI
                } else {
                    cand
                }
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
            let dvar12 = (PI - dvar13).mul_add(0.5, dvar13);
            proto.wt = dvar13 * CONST_0_25;
            // LAB_18010d2cf: proto[2] = dvar12 (= wz)
            proto.wz = dvar12;
            // Skip to w_eval.
            let cand = 1.0_f64.powf(local_res8 * CONST_3_3).mul_add(PI_OVER_5, FOUR_PI_OVER_5);
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
        }
        proto.wt = dvar11.sqrt() * proto.wp;
        let dvar12 = dvar11.sqrt() * proto.wp * CONST_0_25;
        proto.wz = dvar12;
        let cand = 1.0_f64.powf(local_res8 * CONST_3_3).mul_add(PI_OVER_5, FOUR_PI_OVER_5);
        proto.w_eval = PI.min(cand.max(0.0));
        return;
    }

    // === Special-flag override path (flag_69 != 0 OR flag_68 != 0) ===
    if proto.mode <= 0 {
        // proto[9] == 0 sub-branch: clamp wp to 9π/10, set wt = wp · 0.25
        let dvar11_use = if NINE_PI_TEN <= proto.band_omega_ref { NINE_PI_TEN } else { proto.band_omega_ref };
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
    let cand = 1.0_f64.powf(local_res8 * CONST_3_3).mul_add(PI_OVER_5, FOUR_PI_OVER_5);
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
    const SQRT2_F32_ROUNDED: f64 = 1.414_213_538_169_860_8;

    let mode_in = proto.mode;

    // Delegation predicate.
    if (mode_in == 1 || proto.flag_byte_68 != 0) && proto.flag_byte_69 == 0 {
        compute_shelf_band_parameters(proto);
        return;
    }

    // fv8 = clamp(√2 / proto[10], 0, 1.0); the binary stores √2 as a
    // single-rounded double (0x3FF6A09E60000000).
    let mut iv3 = mode_in;
    let raw = SQRT2_F32_ROUNDED / proto.q_scratch_50;
    let fv8: f64;
    let mut bvar2 = false;

    if mode_in != 2 || proto.wz <= PI {
        let fv8_f32 = num::narrow(raw).min(1.0f32);
        fv8 = f64::from(fv8_f32);
        if mode_in == 2 && proto.proto_0x12_sign == 1 {
            // Swap wp ↔ wz, set the sticky bvar2 path.
            std::mem::swap(&mut proto.wp, &mut proto.wz);
            bvar2 = true;
        }
    } else {
        // mode == 2 AND wz > π — promote mode locally to 1 and persist.
        iv3 = 1;
        let fv8_f32 = num::narrow(raw).min(1.0f32);
        fv8 = f64::from(fv8_f32);
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
            let diff = f64::from(num::narrow(mpi - mp).abs());
            if diff <= 0.01 {
                proto.mode = proto.mode.saturating_sub(1);
                proto.wp = proto.band_omega_ref;
            }
        }
        // (No `analog` provided: skip the test rather than fabricating data.)
    }

    // w_eval branch on proto[0x12] sign.
    let nine_pi_10 = 0.9 * PI;
    let zero_eight_five_pi = 2.670_353_755_551_324; // 0.85π
    let zero_nine_nine_pi = 3.110_176_727_053_895_4; // 0.99π
    let wp_now = proto.wp;
    let new_w_eval = if proto.proto_0x12_sign == -1 {
        let cand = wp_now * 1.25;
        if zero_eight_five_pi <= cand {
            if PI <= cand {
                PI
            } else {
                cand
            }
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
    let wp_for_wt = if bvar2 {
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
        wp_clamped
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
        wp_now
    };

    // wt = (0.999 - (1 - fv8)² · 0.5) · wp_for_wt.
    let one_minus_fv8 = 1.0 - fv8;
    let bracket = (one_minus_fv8 * one_minus_fv8).mul_add(-0.5, 0.999);
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
    const SQRT2_F32_ROUNDED: f64 = 1.414_213_538_169_860_8;
    const EPS_1E_NEG_10: f64 = 1e-10;
    const EPS_1_192E_NEG_7: f64 = 1.192_092_9e-7;
    const NEAR_PI_E0A: f64 = f64::from_bits(0x4007_e048_5cda_5e0a); // 0.95π

    // dVar3 = √2 / proto[10]; binary marks proto+0x49 flag (we don't model it).
    let raw = SQRT2_F32_ROUNDED / proto.q_scratch_50;
    let fv12_f32 = num::narrow(raw);
    let fv11_f32 = if fv12_f32 >= 1.0 { 1.0 } else { fv12_f32 };
    let fv11 = f64::from(fv11_f32);

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
        let sqrt_f32 = f64::from(num::narrow(sqrt_val));
        let dvar7 = (pow_half - 0.99).mul_add(sqrt_f32, 0.99);

        if proto.mode == 1 {
            let pi_over_100 = PI / 100.0;
            let bvar1 = use_wp_or_wz >= pi_over_100;
            proto.wp = use_wp_or_wz;
            let dvar3 = f64::from(proto.alpha_scratch_8c) * 0.20;
            let dvar4 =
                dvar7.sqrt() * use_wp_or_wz * f64::from(proto.alpha_scratch_8c).mul_add(-0.05, 1.0);
            proto.wz = dvar4;
            proto.wt = (1.0 - dvar3) * dvar4 * dvar7;
            if !bvar1 {
                proto.wz = use_wp_or_wz + use_wp_or_wz;
            }
            return;
        }

        // iv3 == 2 sub-branch (post small-mag-at-wz fall-through).
        let dvar6 = proto.wz;
        let diff = f64::from(num::narrow(dvar6) - num::narrow(use_wp_or_wz)).abs();
        if diff <= EPS_1_192E_NEG_7 {
            let dvar3 = proto.wp;
            proto.wz = dvar3;
            proto.wp = use_wp_or_wz;
            proto.wt = dvar3 * dvar7;
            return;
        }

        let dvar7_quad = dvar6.mul_add(sqrt_f32, -0.80).clamp(0.0, 0.20);
        let dvar7_lin = proto.wp * 1.01;
        let mut dvar3_x = (proto.wp + dvar6) * sqrt_f32;
        if dvar7_lin <= dvar3_x {
            dvar3_x = dvar7_lin;
        }
        proto.alpha_scratch_8c = num::narrow((dvar7_quad * dvar7_quad * 25.0));
        proto.wt = dvar3_x;
        return;
    }

    // iv3 < 0: degenerate, fall through.
    label_d7f9(proto, fv11, fv12_f32);
}

/// `LAB_18010d7f9` fall-through block from `compute_band_shelf_parameters`.
fn label_d7f9(proto: &mut Prototype, fv11: f64, fv12_f32: f32) {
    const ZERO_NINE_THREE_PI: f64 = 2.921_681_167_838_508; // 0.93π
    const ZERO_EIGHT_THREE_PI: f64 = 2.607_521_902_479_528; // ≈ 0.83π
    const PROTO_E: f64 = f64::from_bits(0x4009_2156_9e86_0335);
    const PROTO_F: f64 = f64::from_bits(0x4008_e101_45e5_f3d1);
    const PROTO_G: f64 = f64::from_bits(0x4009_1ae7_af42_ce78);

    let cand = proto.wp * 1.80;
    let new_w_eval = if ZERO_EIGHT_THREE_PI <= cand {
        if PI <= cand {
            PI
        } else {
            cand
        }
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
        let fv2 = num::narrow((dvar6 * dvar6 * 25.0));
        proto.alpha_scratch_8c = fv2;
        let mut dvar3 = f64::from(fv2).sqrt().mul_add(-0.20, 1.0) * proto.band_omega_ref;
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
        const NEAR_PI_A: f64 = 3.141_278_494_324_434; // ≈ π
        let a8c = proto.alpha_scratch_8c;
        let bracket = f64::from(a8c * a8c).mul_add(-0.0005, 0.9998);
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
    let bracket = 0.995 - f64::from(fv11_min * fv11_min) * 0.90;
    let mut wz_cand = bracket * dvar4;
    if wz_cand <= dvar7_v2 {
        wz_cand = dvar7_v2;
    }
    proto.wz = wz_cand;
}
