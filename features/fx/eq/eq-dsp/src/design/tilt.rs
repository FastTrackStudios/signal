//! Tilt-shelf cascade — low-shelf cut + high-shelf boost with opposite gains.
//!
//! A tilt-shelf reshapes the spectrum around `freq_hz` so positive gain
//! tilts the response upward toward high frequencies (HS boost / LS cut)
//! and negative gain tilts it downward. Built as two Pro-Q-conformant
//! shelf cascades at the same fc with `±gain_db/2`, sharing the user Q
//! and Pro-Q's slope ladder (UI slope N → N-pole shelves).

use crate::biquad::Coeffs;

use super::shelf::shelf_universal_synth_cascade;

pub(super) fn mzt_tilt_shelf_cascade(
    n: usize,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    pole_count: usize,
) -> Vec<Coeffs> {
    if gain_db.abs() < 1e-9 {
        return Vec::new();
    }
    let half_gain = gain_db * 0.5;
    // Low-shelf takes the OPPOSITE sign — positive tilt = boost highs / cut lows.
    let mut sections =
        shelf_universal_synth_cascade(n, freq_hz, q, -half_gain, sample_rate, pole_count, false);
    sections.extend(shelf_universal_synth_cascade(
        n,
        freq_hz,
        q,
        half_gain,
        sample_rate,
        pole_count,
        true,
    ));
    sections
}
