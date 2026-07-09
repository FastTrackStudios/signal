//! Bandpass cascade — Pro-Q 4 algorithmic path.
//!
//! See `docs/reports/proq4/re/bandpass_formula.md`.

use crate::biquad::Coeffs;
use crate::cascade;

use super::common::cascade_qs;

/// Bandpass cascade.
///
/// - `n == 1` (slope=2): bit-exact Lagrange-MZT via `bandpass_s2_proq4`.
/// - `n ∈ {2, 3, 6}` (slope=4/6/8): analog-form path-A pipeline from
///   `docs/reports/proq4/re/bandpass_formula.md`, BLT'd via the Pro-Q 4
///   `Q_pre` prewarp.
/// - Other `n`: legacy MZT cascade fallback.
pub(super) fn mzt_bandpass_simple_cascade(
    n: usize,
    freq_hz: f64,
    q: f64,
    sample_rate: f64,
    order: usize,
) -> Vec<Coeffs> {
    let n = n.max(1);
    if n == 1 {
        return vec![cascade::bandpass_s2_proq4(freq_hz, q, sample_rate)];
    }
    // Slope ≥ 4: analog-form path-A pipeline from
    // `docs/reports/proq4/re/bandpass_formula.md`.  BP shares the Notch
    // denominator pipeline (`notch_analog_sections`); numerator is
    // α·√a2·s per section with α = √2/Q.
    let slope = match order {
        3 => 3,
        4 => 4,
        5 => 5,
        6 => 6,
        7 => 7,
        12 => 8,
        16 => 9,
        _ => {
            return cascade_qs(n, q)
                .into_iter()
                .map(|sq| crate::proq4_mzt::design_bandpass_mzt(freq_hz, sq, sample_rate))
                .collect();
        }
    };
    cascade::bandpass_cascade_proq4(freq_hz, q, sample_rate, slope)
}
