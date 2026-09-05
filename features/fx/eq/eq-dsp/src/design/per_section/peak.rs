//! Peak / bell section parameters (Pro-Q band type 3).

use dsp_core::num;

use super::{Prototype, PI};

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
                let fv2_a = f64::from(num::narrow(
                    wp_in.mul_add(0.442_097_064_144_153_7, -(5.0 / 12.0)),
                ));
                let mut fv2_sq_part = (fv2_a * fv2_a).mul_add(0.20, 0.785);
                let mut fv2_b = f64::from(num::narrow(fv2_sq_part));
                if f64::from(num::narrow(fv2_b)) > 0.96 {
                    // Mirror w_eval around 0.96 (binary: (w_eval - 0.96) + w_eval)
                    fv2_sq_part = (proto.w_eval - 0.96) + proto.w_eval;
                    fv2_b = f64::from(num::narrow(fv2_sq_part));
                }
                // wp_clamped = min(wp_in, π)
                let wp_clamped = wp_in.min(PI);
                // candidate = (f32 cast) · π
                let cand = fv2_b * PI;
                // dVar9 = if (wp_clamped <= cand) { if (cand >= π) π else cand } else wp_clamped
                let mut new_w_eval = if wp_clamped <= cand {
                    if cand >= PI {
                        PI
                    } else {
                        cand
                    }
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
            let fv2 = (a94_in * a94_in).mul_add(4.0f32, -2.0f32);
            proto.alpha_scratch_94 = fv2;

            // dVar_alpha = (fv2 < 0) ? max(sqrt(-fv2/2), 0.5) : 0.5
            let dvar_alpha = if f64::from(fv2) < 0.0 {
                let s = (f64::from(fv2) * -0.5).sqrt();
                if s <= 0.5 {
                    0.5
                } else {
                    s
                }
            } else {
                0.5
            };

            // fVar3 = fv2 · -0.5 (f32); fv11 = clamp(fVar3, 0, 1.0)
            let fv3 = fv2 * -0.5f32;
            let fv11 = fv3.clamp(0.0f32, 1.0f32);
            let fv11_d = f64::from(fv11);

            // wp_new candidate = band_omega_ref / ((alpha-1)·fv11 + 1)
            let wp_new = proto.band_omega_ref / ((dvar_alpha - 1.0).mul_add(fv11_d, 1.0));

            // dVar5 = π - clamp(wp_new - π, 0, 0.3π)
            let excess = wp_new - PI;
            let excess_clamped = excess.clamp(0.0, 0.3 * PI);
            let mut dvar5 = PI - excess_clamped;

            // dVar6_ceiling = fv11·0.3π + 0.7π  ∈ [0.7π, π]
            let dvar6_ceiling = fv11_d.mul_add(0.3 * PI, 0.7 * PI);

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
            let dvar10 = if fv2 >= 6.0f32 {
                let fv2_capped = if fv2 >= 20.0f32 { 20.0f32 } else { fv2 };
                (f64::from(fv2_capped) - 6.0).mul_add(-(9.0 / 700.0), 0.20)
            } else {
                0.20
            };
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
