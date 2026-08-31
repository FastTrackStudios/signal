//! Tilt-shelf cascade — low-shelf cut + high-shelf boost with opposite gains.
//!
//! A tilt-shelf reshapes the spectrum around `freq_hz` so positive gain
//! tilts the response upward toward high frequencies (HS boost / LS cut)
//! and negative gain tilts it downward. Built as two Pro-Q-conformant
//! shelf cascades at the same fc with `±gain_db`, sharing the user Q and
//! Pro-Q's slope ladder (UI slope N → N-pole shelves).
//!
//! **The gain is per side, not the span across the tilt.** A `+6 dB` tilt in
//! the plugin lands on -6 dB at the bottom of the spectrum and +6 at the top,
//! a twelve-decibel span; this used to halve it and produce -3/+3. Measured at
//! 1 kHz, Q 1, 12 dB/oct:
//!
//! ```text
//!      Hz     Pro-Q     halved       now
//!      125     -6.00      -3.00     -6.00
//!      707     -3.26      -1.79     -3.51
//!     1000      0.00       0.00      0.00
//!     8000      6.00       6.00      6.00
//! ```
//!
//! Note the transition is not a scaled copy — 707 Hz was 1.82x, not 2x — so
//! this is the gain each shelf is *designed* at, not a decibel scaling applied
//! afterwards. A quarter of a decibel of transition width is left at +6 and
//! 0.7 at -9; scaling the shelves' Q to chase it overshoots the other way
//! (0.7x moves -9 to -0.72 at 500 Hz) without a reason to prefer either, so it
//! is left alone until there is one.

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
    // Low-shelf takes the OPPOSITE sign — positive tilt = boost highs / cut lows.
    let mut sections =
        shelf_universal_synth_cascade(n, freq_hz, q, -gain_db, sample_rate, pole_count, false);
    sections.extend(shelf_universal_synth_cascade(
        n,
        freq_hz,
        q,
        gain_db,
        sample_rate,
        pole_count,
        true,
    ));
    sections
}
