//! High-pass slope 8 (Db48, N=12 poles, 6 sections).
//!
//! Per-section Qs decoded from probe sweep
//! (`docs/reports/proq4/re/hp_slope8_per_section.csv` +
//! `hp_slope8_recovered.csv`). Each section uses the Lagrange-MZT formula
//! with `alpha = √2/Q_section`, where the recovered Q_section values equal
//! the textbook Butterworth pole-Q values for N=12 (poles in conjugate
//! pairs) times √2:
//!
//! ```text
//!     Q_section_k = √2 / (2·cos(θ_k))    with  θ_k = (2k+1)·π/24
//! ```
//!
//! for k = 5, 4, 3, 2, 1, 0 (highest-Q section first). The highest-Q
//! section (sec0) is additionally scaled by Q_user, clamped to 40.

use crate::biquad::Coeffs;

pub(super) fn cascade(freq_hz: f64, q: f64, sample_rate: f64) -> Vec<Coeffs> {
    hp_slope8_cascade(freq_hz, q, sample_rate)
}

fn hp_slope8_cascade(freq_hz: f64, q_user: f64, sample_rate: f64) -> Vec<Coeffs> {
    // Closed-form per-section path (decoded 2026-05-01 from
    // hp_s8_all_sections_subfreq.csv at SR=48000 + RE of compute_peak_type3).
    // See proq4_mzt::hp_slope8_section_biquad for the formula.
    (0..6)
        .map(|sec| crate::proq4_mzt::hp_slope8_section_biquad(sec, freq_hz, q_user, sample_rate))
        .collect()
}
