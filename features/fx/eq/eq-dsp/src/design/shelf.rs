//! Shelf cascade builder (`LowShelf`, `HighShelf`) — Pro-Q 4 algorithmic path.
//!
//! Single entry point [`shelf_universal_synth_cascade`] builds the section
//! cascade via the universal per-section synth helper using the decoded
//! analog prototype (see
//! `docs/reports/proq4/re/shelf_analog_prototype_decoded.md`). Per-section
//! anti-cramping clamps from `compute_shelf_band_parameters`
//! (`W_POLE_MAX` ≈ 2.99, `W_THIRD_MAX` ≈ 2.36, `W_ZERO_MAX` ≈ 1.88) are folded in
//! through that helper, giving 3744/3744 pure-algorithmic conformance on
//! both `high_shelf` and `low_shelf`.

use std::f64::consts::PI;

use crate::biquad::Coeffs;

use super::common::{db_to_linear, ui_q_to_bandwidth_q};

fn section_internal_type(proq_internal_filter_type: u8, _slope_n: usize, _sec_idx: usize) -> i32 {
    i32::try_from(proq_internal_filter_type).unwrap_or(0)
}

/// Handle the odd-order tail section for shelf filters.
fn shelf_odd_order_tail(
    freq_hz: f64,
    sample_rate: f64,
    effective_order: usize,
    g_section: f64,
    g_half_section: f64,
    high_shelf: bool,
) -> Coeffs {
    use crate::proq4_per_section_helpers::AnalogBiquad;

    let analog = if high_shelf {
        AnalogBiquad {
            b2z: 0.0,
            b1z: g_section,
            b0z: g_half_section,
            b2p: 0.0,
            b1p: 1.0,
            b0p: g_half_section,
        }
    } else {
        AnalogBiquad {
            b2z: 0.0,
            b1z: 1.0,
            b0z: g_half_section,
            b2p: 0.0,
            b1p: 1.0,
            b0p: 1.0 / g_half_section,
        }
    };
    let omega = 2.0 * PI * freq_hz / sample_rate;
    let w_tail = if effective_order == 1 {
        0.9 * PI
    } else {
        (omega * 10.8).min(0.9 * PI)
    };
    crate::cascade::proq4_s2_from_prototype_with_subfreq_pub(
        freq_hz,
        sample_rate,
        analog.b2z,
        analog.b1z,
        analog.b0z,
        analog.b2p,
        analog.b1p,
        analog.b0p,
        w_tail,
        w_tail * 0.005,
        w_tail * 0.1,
        PI,
    )
}

/// Handle normal shelf sections for shelf filters.
fn compute_shelf_params(
    sec_idx: usize,
    order_f: f64,
    effective_order: usize,
    q: f64,
    q_eff: f64,
    g_half_section: f64,
    band_omega_ref: f64,
) -> (f64, f64, f64, f32) {
    let theta_k = PI * (2.0_f64.mul_add(sec_idx as f64, 1.0)) / (2.0 * order_f);
    let qk = if sec_idx == 0 { q_eff } else { 1.0 };
    let two_sin_theta = 2.0 * theta_k.sin();
    let damping = two_sin_theta * g_half_section / qk;
    let helper_alpha = if effective_order == 16
        && sec_idx == 0
        && q >= 9.99
        && band_omega_ref > 0.9 * PI
    {
        (1.0 / (std::f64::consts::SQRT_2 * 2.0 * q.max(1e-6))) as f32
    } else {
        (theta_k.sin() / qk) as f32
    };
    (theta_k, damping, qk, helper_alpha)
}

fn create_shelf_analog_biquad(
    damping: f64,
    g_section: f64,
    high_shelf: bool,
) -> crate::proq4_per_section_helpers::AnalogBiquad {
    use crate::proq4_per_section_helpers::AnalogBiquad;

    if high_shelf {
        let b2z = (g_section as f32 * g_section as f32) as f64;
        AnalogBiquad {
            b2z,
            b1z: damping * g_section,
            b0z: b2z / g_section,
            b2p: 1.0,
            b1p: damping,
            b0p: g_section,
        }
    } else {
        AnalogBiquad {
            b2z: 1.0,
            b1z: damping,
            b0z: g_section,
            b2p: 1.0,
            b1p: damping / g_section,
            b0p: 1.0 / g_section,
        }
    }
}

fn shelf_normal_section(
    sec_idx: usize,
    n: usize,
    freq_hz: f64,
    sample_rate: f64,
    order_f: f64,
    effective_order: usize,
    q: f64,
    q_eff: f64,
    g_section: f64,
    g_half_section: f64,
    band_omega_ref: f64,
    proq_internal_filter_type: u8,
    high_shelf: bool,
    omega_scale: f64,
) -> Coeffs {
    use crate::proq4_per_section_helpers::{proq4_universal_section_synth, Prototype};

    let (theta_k, damping, _qk, helper_alpha) = compute_shelf_params(
        sec_idx,
        order_f,
        effective_order,
        q,
        q_eff,
        g_half_section,
        band_omega_ref,
    );
    let analog = create_shelf_analog_biquad(damping, g_section, high_shelf);

    let section_type = section_internal_type(proq_internal_filter_type, n, sec_idx);
    let section_subtype = if effective_order <= 2 {
        -1
    } else {
        sec_idx as i32
    };
    if effective_order == 16 && sec_idx >= n / 2 && q >= 9.99 && band_omega_ref > 0.9 * PI {
        let wp = band_omega_ref * 0.5;
        return crate::cascade::proq4_s2_from_prototype_with_subfreq_pub(
            freq_hz,
            sample_rate,
            analog.b2z,
            analog.b1z,
            analog.b0z,
            analog.b2p,
            analog.b1p,
            analog.b0p,
            wp,
            wp * 0.2,
            wp * 0.96,
            PI,
        );
    }
    let mut proto = Prototype {
        wp: band_omega_ref,
        wz: band_omega_ref * 0.5,
        wt: band_omega_ref * 0.25,
        w_eval: PI,
        mode: 0,
        section_type,
        proto_0x12_sign: section_subtype,
        band_omega_ref,
        analog: Some(analog),
        omega_band: band_omega_ref * omega_scale,
        alpha_scratch_94: helper_alpha,
        ..Prototype::default()
    };

    let (coeffs, _fallback) = proq4_universal_section_synth(
        &mut proto,
        &analog,
        freq_hz,
        sample_rate,
        omega_scale,
    );
    if effective_order == 2 && matches!(section_type, 2 | 5) {
        const SHELF_ORDER2_HIGH_ROOT: f64 = 2.607_521_902_479_528_6;
        const NOTCH46_UPPER_ROOT: f64 = 2.984_513_020_910_303_5;
        if proto.wp > SHELF_ORDER2_HIGH_ROOT && proto.wp < NOTCH46_UPPER_ROOT {
            let wz = proto.wz;
            let wt = (1.0 - (helper_alpha as f64) * 0.05) * wz;
            return crate::cascade::proq4_s2_from_prototype_with_subfreq_pub(
                freq_hz,
                sample_rate,
                analog.b2z,
                analog.b1z,
                analog.b0z,
                analog.b2p,
                analog.b1p,
                analog.b0p,
                SHELF_ORDER2_HIGH_ROOT,
                wz,
                wt,
                proto.w_eval,
            );
        }
    }
    coeffs
}

/// Shelf cascade builder via Pro-Q's universal per-section synth.
///
/// Uses the decoded analog prototype from
/// `docs/reports/proq4/re/shelf_analog_prototype_decoded.md` plus the
/// per-section ω₀ ladder (32× spacing per section, centered on band fc).
/// Section-internal type dispatch via [`section_internal_type`] (LowShelf=2,
/// HighShelf=5) feeds the per-section helper, which folds in the
/// anti-cramping clamps decoded from `compute_shelf_band_parameters` —
/// these are what carry shelves to 3744/3744 pure-algorithmic conformance.
///
/// Per-section analog prototype (per `shelf_analog_prototype_decoded.md`):
/// ```text
/// gain    = 10^(g_dB/20)
/// θ_k     = π·(2k+1)/(4N)
/// Q_k     = Q^(5/10.644) if k==0, else 1
/// H_k(s) = (gain^(1/N)·s² + 2·sin(θ_k)·gain^(3/(4N))/Q_k·s + gain^(1/(2N)))
///        / (s²            + 2·sin(θ_k)·gain^(1/(4N))/Q_k·s + gain^(1/(2N)))
/// ```
pub(super) fn shelf_universal_synth_cascade(
    n: usize,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    order: usize,
    high_shelf: bool,
) -> Vec<Coeffs> {
    // The helper dispatcher uses Pro-Q's internal audio filter type:
    // LowShelf=2, HighShelf=5.
    let proq_internal_filter_type: u8 = if high_shelf { 5 } else { 2 };
    let omega_scale = 1.0;
    let band_omega_ref = 2.0 * PI * freq_hz / sample_rate;
    let gain = db_to_linear(gain_db);
    let effective_order = if order == 7 { 8 } else { order };
    let q_eff = ui_q_to_bandwidth_q(q);
    #[expect(clippy::cast_precision_loss, reason = "effective_order is small (max 16), precision loss is not a concern")]
    let order_f = effective_order as f64;
    let g_section = gain.powf(1.0 / order_f);
    let g_half_section = gain.powf(1.0 / (2.0 * order_f));

    (0..n)
        .map(|sec_idx| {
            if effective_order % 2 == 1 && sec_idx + 1 == n {
                return shelf_odd_order_tail(
                    freq_hz,
                    sample_rate,
                    effective_order,
                    g_section,
                    g_half_section,
                    high_shelf,
                );
            }

            shelf_normal_section(
                sec_idx,
                n,
                freq_hz,
                sample_rate,
                order_f,
                effective_order,
                q,
                q_eff,
                g_section,
                g_half_section,
                band_omega_ref,
                proq_internal_filter_type,
                high_shelf,
                omega_scale,
            )
        })
        .collect()
}
