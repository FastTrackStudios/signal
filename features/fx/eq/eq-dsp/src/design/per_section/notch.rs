//! Notch section parameters (Pro-Q band types 4 and 6).

use super::{Prototype, PI};
use dsp_core::num;

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
///   `w_eval` and update of `proto[0xe..0x10]` (`stored_e/f/g`). Decompilation
///   has an unbound `dVar6` reference in the line
///   `dVar5 = proto[0xe] - fVar7·0.15·dVar6` — needs probe captures to
///   confirm whether `dVar6` is `proto[0xa0]` (`band_omega_ref`) or `proto[1]`
///   (current wp). Stubbed with `unimplemented!`.
pub fn compute_notch_type46_parameters(proto: &mut Prototype) {
    // mode == 2 (complex-roots path)
    const TWO_NINE_EIGHT_FOUR_FIVE: f64 = 2.984_513_020_910_303_5; // ≈ 0.95·π
                                                                   // Composite scratch (binary keeps everything in f32 SS instructions
                                                                   // until the final write to the f64 wp/wz/wt slots).
    let s8c = proto.alpha_scratch_8c;
    let s94 = proto.alpha_scratch_94;
    let fv7 = (s8c * s8c).mul_add(0.25f32, s94);
    let fv8 = fv7.clamp(0.10f32, 0.80f32);
    let fv8_d = f64::from(fv8);
    let fv8_005 = f64::from(fv8 * 0.05f32);

    if proto.mode != 2 {
        let mut d6 = proto.band_omega_ref;
        if proto.section_type == 6 {
            // type=6 static path: clamp band_omega_ref to 0.6π, then split.
            const ZERO_POINT_SIX_PI: f64 = 1.884_955_592_153_875_9;
            if d6 > ZERO_POINT_SIX_PI {
                d6 = ZERO_POINT_SIX_PI;
            }
            proto.wp = d6;
            proto.wz = d6 * 0.05;
            proto.wt = d6 * 0.5;
        } else {
            // type=4 static path
            d6 *= 0.5;
            proto.wp = d6;
            proto.wz = (1.0 - fv8_d) * d6;
            proto.wt = (1.0 - fv8_005) * d6;
        }
        return;
    }

    if matches!(proto.section_type, 2 | 5) && proto.wz < TWO_NINE_EIGHT_FOUR_FIVE {
        std::mem::swap(&mut proto.wp, &mut proto.wz);
    }
    // wp from upstream solve_biquad, after the possible swap above.
    let wp_in = proto.wp;

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
        const QUAD_COEF: f64 = 0.061_575_216_010_359_95;
        const QUAD_OFFSET: f64 = 2.419_026_343_264_141;
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
        let smooth_blend = (PI - wp_in).mul_add(0.1, wp_in);
        // dVar4 = dVar4²·0.0615… + 2.4190…  (quadratic floor in wp)
        let quad_floor = (dvar4_floor * dvar4_floor).mul_add(QUAD_COEF, QUAD_OFFSET);
        // dVar2 = if (smooth_blend ≤ quad_floor) min(quad_floor, π) else smooth_blend
        let new_w_eval = if smooth_blend <= quad_floor {
            if quad_floor >= PI {
                PI
            } else {
                quad_floor
            }
        } else {
            smooth_blend
        };
        proto.w_eval = new_w_eval;

        // === stored_e/f/g update + wt update ===
        // fv3 = (wp - wz) - 0.15  (wz here is the upstream solve_biquad value)
        let fv3 = num::narrow(wp_in - proto.wz) - num::narrow(WT_OFFSET);
        let fv7 = fv3.clamp(0.0f32, 1.0f32);
        // stored_e_new = stored_e_old - (fv7·0.15)·π
        let stored_e_new = (f64::from(fv7) * WT_OFFSET).mul_add(-PI, proto.stored_e);
        proto.stored_e = stored_e_new;
        let stored_f_val = stored_e_new * STORED_F_SCALE;
        proto.stored_f = stored_f_val;
        proto.stored_g = stored_f_val * STORED_G_SCALE;

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
    let fv8_d = f64::from(fv8);
    let fv8_005 = f64::from(fv8 * 0.05f32);
    proto.wp = d6;
    proto.wz = (1.0 - fv8_d) * d6;
    proto.wt = (1.0 - fv8_005) * d6;
}
