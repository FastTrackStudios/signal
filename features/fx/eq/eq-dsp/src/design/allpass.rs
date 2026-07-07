//! Allpass cascade — Pro-Q 4 algorithmic path.
//!
//! Canonical 2nd-order analog allpass sections with per-section α via
//! Butterworth pole spacing, first-section α Q-modulated.
//! See `docs/reports/proq4/re/allpass_formula.md` +
//! `allpass_q_dependence_decoded.md`.

use std::f64::consts::PI;

use crate::biquad::Coeffs;

pub(super) fn design_allpass_with_lookup(
    n: usize,
    freq_hz: f64,
    q: f64,
    sample_rate: f64,
    pole_count: usize,
) -> Vec<Coeffs> {
    let _ = pole_count;
    design_allpass(n, freq_hz, q, sample_rate)
}
/// + `allpass_q_dependence_decoded.md`).  Each cascade section is a
/// canonical analog allpass:
///
/// ```text
///   H_k(s) = (s² − α_k·s + 1) / (s² + α_k·s + 1)
/// ```
///
/// where α_k uses Butterworth pole spacing, with the **first section
/// alone** Q-modulated:
///
/// ```text
///   α_butter(k, N) = 2·sin( (2k+1)·π / (2·N) )
///   α_0       = α_butter(0, N) / Q_user
///   α_k (k≥1) = α_butter(k, N)
/// ```
///
/// Slope → N mapping (probe-decoded):
///   slope=2 → N=2 (1 section), slope=4 → N=4 (2 sections),
///   slope=6 → N=6 (3 sections), slope=8 → N=12 (6 sections).
///
/// Each section maps to digital via standard prewarped bilinear with
/// `t = tan(π·fc/sr)`.  Verified bit-exact (≤4 dec) against probe
/// captures across (fc, Q, slope) grid.
pub(super) fn design_allpass(n: usize, freq_hz: f64, q: f64, sample_rate: f64) -> Vec<Coeffs> {
    // The caller passes n = ceil(order/2) where order tracks user slope.
    // Map back to filter order N:
    //   slope=2 → n=1 → N=2
    //   slope=4 → n=2 → N=4
    //   slope=6 → n=3 → N=6
    //   slope=8 → n=4 → N=12 (Pro-Q 4 doubles the cascade order at slope=8)
    let n_filter = if n >= 4 { 12 } else { (2 * n).max(2) };
    let n_sec = n_filter.div_ceil(2);
    let q_user = q.max(1e-6);

    let t = (PI * freq_hz / sample_rate).tan();
    let t2 = t * t;

    let mut sections = Vec::with_capacity(n_sec);
    for k in 0..n_sec {
        let alpha_butter = 2.0 * ((2 * k + 1) as f64 * PI / (2.0 * n_filter as f64)).sin();
        // Section-0 Q modulation. At slope=8 (N=12) Pro-Q clamps Q_eff to
        // ~7.4 (verified bit-exact via PROBE_HOOK_AUDIO_BIQUAD across fc).
        // Slopes 2/4/6 (N=2/4/6) use plain butter/Q for all Q.
        let alpha = if k == 0 {
            let q_eff = if n_filter >= 12 {
                q_user.min(7.398)
            } else {
                q_user
            };
            alpha_butter / q_eff
        } else {
            alpha_butter
        };
        // Prewarped BLT of analog allpass H(s) = (s² − α·s + 1)/(s² + α·s + 1)
        // via s = (1/t)·(z-1)/(z+1):
        let nb0 = 1.0 - alpha * t + t2;
        let nb1 = -2.0 + 2.0 * t2;
        let nb2 = 1.0 + alpha * t + t2;
        let da0 = 1.0 + alpha * t + t2;
        let da1 = -2.0 + 2.0 * t2;
        let da2 = 1.0 - alpha * t + t2;
        let inv = 1.0 / da0;
        sections.push([1.0, da1 * inv, da2 * inv, nb0 * inv, nb1 * inv, nb2 * inv]);
    }
    sections
}

/// Bandpass Variant — UI Shape 10 in Pro-Q 4.
///
/// Empirical finding: `bandpass_variant_*.csv` reference captures are BYTE-IDENTICAL
/// to `allpass_*.csv` references. fts-analyzer's Shape 10 capture is the same
/// response as Shape 9 (Allpass) — likely Pro-Q 4 treats UI Shape 10 as an allpass
/// or fts-analyzer's script conflates them. Either way, implementation matches Allpass.
pub(super) fn design_bandpass_variant(
    n: usize,
    freq_hz: f64,
    q: f64,
    sample_rate: f64,
) -> Vec<Coeffs> {
    design_allpass(n, freq_hz, q, sample_rate)
}
