//! Notch cascade — cascaded RBJ biquad notches.
//!
//! Standard RBJ "notch" formula bilinear-prewarped at fc, then bilinear
//! transformed to z-domain. For higher Pro-Q slopes (4..9), we stack
//! multiple RBJ sections at the same fc with effective per-section Q
//! distributed Butterworth-style so the cumulative −3 dB bandwidth still
//! tracks the user Q parameter.
//!
//! Digital biquad via bilinear (RBJ cookbook):
//! ```text
//!   ω = 2π·fc/fs
//!   α = sin(ω) / (2·Q)
//!   b0 = 1,        b1 = -2·cos(ω),  b2 = 1
//!   a0 = 1 + α,    a1 = -2·cos(ω),  a2 = 1 - α
//! ```
//! (normalized by a0).

use std::f64::consts::PI;

use crate::biquad::Coeffs;

/// RBJ notch biquad at digital corner `ω = 2π·fc/fs` with given section Q.
fn rbj_notch_section(freq_hz: f64, q_section: f64, sample_rate: f64) -> Coeffs {
    let q = q_section.max(1e-6);
    let w = 2.0 * PI * freq_hz / sample_rate;
    let cos_w = w.cos();
    let alpha = w.sin() / (2.0 * q);
    let a0 = 1.0 + alpha;
    let inv_a0 = 1.0 / a0;
    [
        1.0,
        -2.0 * cos_w * inv_a0,
        (1.0 - alpha) * inv_a0,
        inv_a0,
        -2.0 * cos_w * inv_a0,
        inv_a0,
    ]
}

pub(super) fn mzt_notch_simple_cascade(
    n: usize,
    freq_hz: f64,
    q: f64,
    sample_rate: f64,
) -> Vec<Coeffs> {
    // `n = ceil(order/2)` from the dispatcher — use directly as the section
    // count so Pro-Q's slope ladder still scales the notch's rejection
    // steepness.
    let n_sections = n.max(1);
    let q_user = q.max(1e-6);
    // Butterworth Q distribution: each section gets Q = Q_user · sin(θ_k) / √2
    // with θ_k = π·(2k+1)/(2·N), the standard pole-angle ladder.
    //
    // The √2 is the same convention the bell reads Q on — a displayed 1.0 is
    // Butterworth, so the filter Q is 1/√2 — and the ladder used to carry a
    // factor of 2 instead, which built every notch 2√2 times too narrow. On a
    // Q 4 notch at 1 kHz the plugin is 5.23 dB down at 891 Hz and this was
    // 1.11, an effective Q of 2.86 against 8.1.
    let n_sections_u32 = u32::try_from(n_sections).unwrap_or(u32::MAX);
    (0..n_sections_u32)
        .map(|k| {
            let k_f = f64::from(k);
            let n_sections_f = f64::from(n_sections_u32);
            let theta = PI * (2.0f64.mul_add(k_f, 1.0)) / (2.0 * n_sections_f);
            let q_section =
                (q_user * theta.sin() * std::f64::consts::FRAC_1_SQRT_2).max(1e-6);
            rbj_notch_section(freq_hz, q_section, sample_rate)
        })
        .collect()
}
