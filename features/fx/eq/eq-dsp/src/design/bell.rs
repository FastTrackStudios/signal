//! Bell / Peak cascade dispatcher — routes to `cascade::compute_cascade_peak_with_slope`.

use crate::biquad::Coeffs;
use crate::cascade;

/// Peak/Bell cascade. For N=1 (slope-2): 7-step proq4_peak pipeline (Vicanek
/// matched-magnitude on numerator + impulse-invariance denominator). For N>1:
/// per-section gain/N dB + Butterworth-distributed Q via proq4_peak.
///
/// Verified via instrumented Pro-Q 4 (commit 2b500c27 — proq4_probe hooks):
///   compute_cascade_coefficients(order=2, type=0, gain) emits ONE section with
///   pole pair at (-sin(π/4), ±cos(π/4)) = (-0.707, ±0.707) on the unit circle
///   (analog) and zeros at +∞.  Section_gain = 1.0.  fc-dependence enters
///   through a downstream step (likely the gain-accumulator path) we have not
///   yet hooked, so our final-biquad math may differ from Pro-Q 4's by the
///   Pro-Q-4-specific γ/ψ correction terms (see SESSION_PROGRESS.md).
///
/// The audio-path γ = 1+(2/π)·t² correction in proq4_mzt::design_peak is
/// available but currently scores lower in conformance — Vicanek handles
/// low-Q gain-dependent p3 better than the high-Q-only γ approximation.
pub(super) fn mzt_peak_cascade(
    n: usize,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    pole_count: usize,
) -> Vec<Coeffs> {
    let n = n.max(1);
    // Pro-Q slope index. Pole_count alone is ambiguous for slope=5 (3 sec) vs
    // slope=6 (3 sec) since both yield order=6. The optional env var
    // FTSEQ_BELL_SLOPE disambiguates for tests/regen.
    let env_slope = std::env::var("FTSEQ_BELL_SLOPE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok());
    let slope_idx = env_slope.or_else(|| super::common::slope_from_pole_count(pole_count));
    cascade::compute_cascade_peak_with_slope(freq_hz, q, gain_db, sample_rate, 2 * n, slope_idx)
}
