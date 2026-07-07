//! Shared helpers for Pro-Q 4 algorithmic filter cascades.
//!
//! Centralizes the recurring DSP idioms with explicit RE provenance so each
//! call site doesn't repeat the magic constants.

use std::f64::consts::PI;

use crate::constants::LN10_OVER_20;

/// Map filter `pole_count` (= order arg from `design_filter`) to the Pro-Q 4
/// slope index used by lookup tables and slope-dispatch matches.
///
/// Decoded from probe captures across `(order, slope)` grid — see commit
/// `2b500c27` (probe hook) and per-filter slope mapping in
/// `docs/reports/proq4/algorithm_status.md`.
///
/// Returns `None` for pole counts outside the Pro-Q 4 grid (slopes 0..9
/// correspond to pole counts {1, 2, 3, 4, 5, 6, 7, 12, 16}).
#[inline]
pub(crate) fn slope_from_pole_count(pole_count: usize) -> Option<usize> {
    match pole_count {
        1 => Some(0),
        2 => Some(2),
        3 => Some(3),
        4 => Some(4),
        5 => Some(5),
        6 => Some(6),
        7 => Some(7),
        12 => Some(8),
        16 => Some(9),
        _ => None,
    }
}

/// Linear voltage gain `10^(dB/20)` via `exp(dB · ln(10)/20)`.
///
/// All Pro-Q 4 algorithmic paths use this exact form (verified bit-exact
/// across `proq4_mzt::design_*`).  Kept inline so the compiler can fold it
/// into a single `exp` call at the call site.
#[inline]
pub(crate) fn db_to_linear(gain_db: f64) -> f64 {
    (gain_db * LN10_OVER_20).exp()
}

/// Pro-Q 4 UI Q → bandwidth Q mapping: `Q_bw = Q^(5/10.644)`.
///
/// The exponent `5/10.644 ≈ 0.4697` is the empirical fit from
/// `docs/reports/proq4/re/shelf_q_scaling.md`.  Floor of `1e-6` matches the
/// Pro-Q binary's protection against `pow(0, _)`.
#[inline]
pub(crate) fn ui_q_to_bandwidth_q(q: f64) -> f64 {
    q.max(1e-6).powf(5.0 / 10.644)
}

/// Butterworth Q values per section for a 2N-pole filter.
/// At user_q=1, each section's MZT-form Q equals its natural Butterworth value,
/// giving a proper Butterworth cascade. For user_q != 1, the highest-Q section
/// is scaled by user_q^(1/N) so that the cumulative effect of N sections matches
/// Pro-Q 4's resonance amount (matches LP/HP cascade at fc=1k Q=10 to ~0.5 dB).
pub(crate) fn cascade_qs(n: usize, user_q: f64) -> Vec<f64> {
    let order = 2 * n;
    let sqrt2 = std::f64::consts::SQRT_2;
    let natural_qs: Vec<f64> = (0..n)
        .map(|k| {
            let theta = PI * (2 * k + 1) as f64 / (2 * order) as f64;
            sqrt2 / (2.0 * theta.cos())
        })
        .collect();
    let (idx_max, _) =
        natural_qs.iter().enumerate().fold(
            (0, 0.0_f64),
            |(i, m), (j, &v)| {
                if v > m { (j, v) } else { (i, m) }
            },
        );
    // Q scaling on the highest-Q (idx_max) section.  Verified linear
    // (`q_factor = user_q`, NOT `user_q.powf(0.85)`) by probe captures of
    // HP slope=4 (`PROBE_HOOK_LAGRANGE` + `PROBE_DESIGN_POST`) at fc=1000
    // across Q∈{0.5,1,4,10}: both sections share `b1p_proto = √2/Q_user`
    // (i.e. the Q-loaded section uses analog α = √2/(Q_natural · Q_user)).
    // Switching from `pow(0.85)` to linear closes:
    //   high_cut s=4 16 → 94 / 104 (alg. only, FTSEQ_BYPASS_LOOKUP=all)
    //   high_cut s=6 17 → 73 / 104
    //   low_cut  s=4 26 → 104 / 104 (bit-exact)
    //   low_cut  s=6 26 → 104 / 104 (bit-exact)
    let q_factor = user_q;
    natural_qs
        .iter()
        .enumerate()
        .map(|(k, &nq)| if k == idx_max { nq * q_factor } else { nq })
        .collect()
}
