//! Pro-Q 4's cascade coefficient computation for peak/bell and shelf (type 12) filters.
//!
//! `compute_cascade_coefficients` (0x1800fec20) computes ZPK directly for peak/bell
//! filters without going through Butterworth prototypes. It uses a specialized approach:
//!
//! - For type 0 (peak/bell): RBJ cookbook with per-section gain distribution.
//!   Higher orders distribute gain across sections with exponential spacing.
//!
//! - For type 0xc (shelf alt / type 12): gain = sqrt(gain), with geometric gain
//!   spacing across sections for smooth shelf transitions.
//!
//! Key insight: Pro-Q 4 does NOT simply stack identical biquads. Each section gets
//! a different `gain_db/section` to create the proper cascade response.

use std::f64::consts::PI;

use crate::design::biquad::{Coeffs, PASSTHROUGH};

mod bandpass;
mod brickwall;
mod lagrange;
mod s2;
mod tables;
mod notch;
mod shelf_alt;

pub use bandpass::*;
pub use notch::*;
pub use shelf_alt::*;

// Implementation split across the files above. The module's surface is
// deliberately unchanged, so every existing `cascade::foo` call site still
// resolves and this split stays a pure move.
pub(crate) use brickwall::{
    bell_brickwall_cascade, bell_brickwall_proq4, bell_brickwall_proq4_n, bell_three_point_synth,
    brickwall_per_section_table,
};
pub(crate) use lagrange::{bell_bucket_b_section_from_analog, lagrange3pt_synth_kernel, lagrange_synth_alt_path};
pub(crate) use s2::{bell_s2_proq4, highpass_s2_proq4, highpass_section_proq4, lowpass_s2_proq4, mode0_forward, proq4_s2_from_prototype_with_subfreq, proq4_s2_from_prototype_with_subfreq_pub};
pub(crate) use tables::{apply_proq4_prewarp, bp_cascade_for_q, lp_atoms_for_slope};

/// Compute cascade biquads for a peak/bell filter.
///
/// Uses Vicanek matched peak EQ with per-section gain distribution.
/// Each section gets `gain_db/N` dB with the same user Q.
///
/// Pro-Q 4 binary (`compute_cascade_coefficients` @ 0x1800fec20) uses a
/// Butterworth zero cascade at angles `θ_k` = π(2k+1)/(2·order) with gain
/// accumulation ∏ `0.25/cos²(θ_k)`. The exact multi-section Q mapping is
/// complex and not yet fully extracted. The Vicanek approach gives 99.3%
/// parity for single/dual sections and ~65% for higher orders.
#[must_use]
pub fn compute_cascade_peak(
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    order: usize,
) -> Vec<Coeffs> {
    compute_cascade_peak_with_slope(freq_hz, q, gain_db, sample_rate, order, None)
}

#[must_use]
pub fn compute_cascade_peak_with_slope(
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    order: usize,
    slope_idx: Option<usize>,
) -> Vec<Coeffs> {
    let n = (order / 2).max(1);

    if gain_db.abs() < 0.001 {
        return vec![PASSTHROUGH; n];
    }

    let _ = slope_idx;
    if n == 1 {
        return vec![bell_s2_proq4(freq_hz, q, gain_db, sample_rate)];
    }

    // n ≥ 2: legacy LP→BP→BLT cascade — 68/416 baseline on s=5 / s=8.
    //
    // Closed-form `bell_brickwall_proq4` (below) is 36/56 sections bit-exact
    // (≤ 1e-12) at the per-section synthesis stage against
    // `docs/reports/proq4/re/lagrange_per_section_sweep.csv`, but routing it
    // here regressed cascade-product conformance to 0/416 on s=5 / s=8.
    // Pro-Q 4 dispatches each section into either the Bell 3-point Lagrange
    // branch (`compute_audio_biquad_lagrange_mzt @ 0x180110855`) or the
    // brickwall 2-point alt-path (`@ 0x180110728`) based on
    // `byte[proto+0x48]`, which empirically tracks `w_third == 0` (alt-path)
    // vs `w_third != 0` (Bell).  The current closed form forces alt-path for
    // every section, producing wrong peak placement at user fc — see
    // `tools/proq4_probe/verify_brickwall_per_section.py`.  Hold on legacy
    // until the upstream selector + per-section `w_third` is decoded.
    let _ = bell_brickwall_proq4_n;
    let _ = bell_brickwall_cascade;
    // Try the closed-form bucket-B path. n_sections matches the section count
    // we have captured per slope (s=3/4 → 2, s=5/6 → 3, s=7 → 4, s=8 → 6,
    // s=9 → 8). When pole_count comes from slope_from_order this is correct.
    bell_brickwall_proq4(freq_hz, q, gain_db, sample_rate, n, slope_idx)
}

/// Bell brickwall closed form for slope ≥ 4.
///
/// Pipeline (per Ghidra RE + capture analysis 2026-05-01):
///
/// **Analog ZPK structure (verified slope=4, all captured (fc, Q, g)):**
/// - The analog prototype is a Butterworth bandpass obtained via the
///   classical LP→BP transform `s → Q·(s/ω₀ + ω₀/s)` applied to a
///   Butterworth LP of order `N_LP = slope/2`.
/// - For each LP-pole `p_LP_k = e^(j(π − θ_k))` (`θ_k` = `π(2k+1)/(2·N_LP)`),
///   the LP→BP transform yields *two* BP poles solving
///   `s² − (p_LP/Q')·s + 1 = 0`, with reciprocal magnitudes (one inside
///   the unit circle, one outside).  The two sections per LP-pole
///   correspond to the two reciprocal roots.
/// - **Pole/zero gain split** (decoded from C/F = `g_ref` symmetry):
///     - boost (`g_dB` > 0): `Q'_pole = Q`,  `Q'_zero = Q / √g_lin`
///     - cut   (`g_dB` < 0): `Q'_pole = Q · √g_lin`, `Q'_zero = Q`
/// - The `(A, B, C, D, E, F)` polynomial in `ω` (digital rad/sample)
///   is built from these analog s-plane (b2=1) quadratics with
///   `(A,D)=1`, `(B,E)=(b1²−2b0)·ω₀²`, `(C,F)=b0²·ω₀⁴`.
/// - Match against `solve_bq_sweep.csv` (slope=4, 256 rows): structurally
///   exact, but residual ≤ 0.5% per coefficient — Pro-Q applies an
///   additional Q-correction on the LP→BP `Q'` that has not yet been
///   bit-exactly decoded (likely involves the elliptic LP→BP variant
///   used elsewhere in the binary, see `prototype.rs::butterworth_bp_elliptic`).
///
/// **Sub-frequency derivation:**
/// - `w_pole` and `w_third` are the two positive roots of the |H(jω)|²
///   peak-finder quadratic in `u = ω²`:
///       `(A·E − B·D)·u² + 2(A·F − C·D)·u + (B·F − C·E) = 0`.
/// - Verified bit-exact (≤ 1e-15) against `solve_bq_sweep.csv` for all
///   captured slope=4 rows (`root_count=2` column).  The smaller root is
///   `w_pole_solve`, the larger is `w_third_solve`.
/// - **Caveat**: for some sections (e.g. sec=0 in fc=500/Q=1) the
///   `persec.w_pole` used by the audio synthesis is *not* the smaller
///   solve root.  An additional per-section selection layer maps the
///   peak-finder roots into the (`w_pole`, `w_zero`, `w_third`) triple fed to
///   the Lagrange synth.  This selection is undecoded; the captures
///   suggest a recipe involving `(w_pole, w_zero, w_third) ≈ ω₀·(α, α/100, α/10)`
///   for one branch and `(big_root, mid, big_root/10)` for the other.
///
/// **Per-section dispatch (verified):**
/// - `w_third == 0` → call `lagrange_synth_alt_path` (2-point alt-path).
/// - `w_third != 0` → call the Bell 3-point Lagrange (same body as
///   `bell_s2_proq4` post sub-frequency selection).
/// - `tools/proq4_probe/verify_brickwall_per_section.py` confirms 60/64
///   (≤ 1e-15) and 4/64 (≤ 1.9e-13) when *correct* sub-frequencies are
///   supplied; the failure mode of routing this to `compute_cascade_peak`
///   is upstream sub-frequency derivation, not the synth itself.
///
/// **Status:** held off the audio path — `compute_cascade_peak` still
/// dispatches into the legacy `bell_brickwall_cascade` table-based
/// path (68/416 baseline on s=5/s=8) until the (a) elliptic-corrected
/// `Q'`, and (b) per-section sub-frequency selection are decoded.
/// Whether the `FTSEQ_TRACE_BELL_INPUTS` debug trace is on.
///
/// Read once. The raw `env::var` was on the per-call bell path, where it takes
/// a process-wide lock and allocates every time a coefficient is designed.
fn trace_bell_inputs() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    #[expect(
        clippy::disallowed_methods,
        reason = "read exactly once through OnceLock, not per call"
    )]
    *ON.get_or_init(|| std::env::var("FTSEQ_TRACE_BELL_INPUTS").is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Evaluate magnitude in dB of a cascade of biquad sections at digital frequency w.
    fn mag_db_sos(sections: &[Coeffs], w: f64) -> f64 {
        use crate::math::zpk::Complex;
        let ejw = Complex::from_polar(1.0, w);
        let ejw2 = ejw * ejw;
        let mut h = Complex::new(1.0, 0.0);
        for c in sections {
            let den = Complex::new(c[0], 0.0)
                + ejw * Complex::new(c[1], 0.0)
                + ejw2 * Complex::new(c[2], 0.0);
            let num = Complex::new(c[3], 0.0)
                + ejw * Complex::new(c[4], 0.0)
                + ejw2 * Complex::new(c[5], 0.0);
            h = h * num / den;
        }
        20.0 * h.mag().log10()
    }

    /// Lock-in test: the decoded `byte[0x48]=1` 2-point alt-path closed form
    /// reproduces the captured (Pro-Q 4) per-section biquad bit-exactly for
    /// a representative high-Q brickwall row.
    ///
    /// Captured row from `lagrange_per_section_sweep.csv` joined with
    /// `solve_bq_sweep.csv`: `(slope=4, fc=10000, Q=4, g=+12, sec=1)`.
    /// The previous 3-point synthesis produced a degenerate biquad on this
    /// row (residual 1.85 in coefficient space); the alt-path matches to
    /// double-precision epsilon.  See
    /// `docs/reports/proq4/re/high_q_correction_decoded.md`.
    #[test]
    fn alt_path_high_q_brickwall_bit_exact() {
        let cap_a = 1.0;
        let cap_b = -4.600_014_557_548_51;
        let cap_c = 5.992_163_077_648_592;
        let cap_d = 1.0;
        let cap_e = -4.028_747_455_414_541;
        let cap_f = 4.186_816_988_762_886;
        let w_pole = 1.399_769_735_042_840_4;
        let w_zero = 2.088_334_741_098_665;
        let w_eval = 2.721_736_862_492_950_3;
        let g_ref = 1.431_197_755_653_31;

        let sos = lagrange_synth_alt_path(
            cap_a, cap_b, cap_c, cap_d, cap_e, cap_f, w_pole, w_zero, w_eval, g_ref,
        );

        let b0_cap = 1.185_467_242_021_393_6;
        let b1_cap = -0.066_053_337_170_110_29;
        let b2_cap = 0.706_915_821_490_845_2;
        let a1_cap = -0.259_292_924_885_168_2;
        let a2_cap = 0.785_907_360_462_303_3;

        // sos layout: [a0=1, a1, a2, b0, b1, b2]
        let max_err = [
            (sos[3] - b0_cap).abs(),
            (sos[4] - b1_cap).abs(),
            (sos[5] - b2_cap).abs(),
            (sos[1] - a1_cap).abs(),
            (sos[2] - a2_cap).abs(),
        ]
        .into_iter()
        .fold(0.0_f64, f64::max);

        assert!(
            max_err <= 1e-12,
            "alt-path coefficient mismatch: max_err = {max_err:.3e}"
        );
    }

    #[test]
    fn peak_zero_gain_is_passthrough() {
        let sos = compute_cascade_peak(1000.0, 2.0, 0.0, 48000.0, 2);
        assert_eq!(sos.len(), 1);
        assert_eq!(sos[0], PASSTHROUGH);
    }

    #[test]
    fn peak_single_section_gain() {
        let sos = compute_cascade_peak(1000.0, 2.0, 6.0, 48000.0, 2);
        assert_eq!(sos.len(), 1);
        let w0 = 2.0 * PI * 1000.0 / 48000.0;
        let mag = mag_db_sos(&sos, w0);
        assert!(
            (mag - 6.0).abs() < 0.5,
            "peak should be ~6 dB at center, got {mag}"
        );
    }

    #[test]
    fn peak_multi_section_gain() {
        let sos = compute_cascade_peak(1000.0, 2.0, 12.0, 48000.0, 4);
        assert_eq!(sos.len(), 2);
        let w0 = 2.0 * PI * 1000.0 / 48000.0;
        let mag = mag_db_sos(&sos, w0);
        assert!(
            (mag - 12.0).abs() < 1.0,
            "cascade peak should be ~12 dB at center, got {mag}"
        );
    }

    #[test]
    fn peak_dc_is_unity() {
        let sos = compute_cascade_peak(1000.0, 2.0, 6.0, 48000.0, 2);
        let dc = mag_db_sos(&sos, 0.001);
        assert!(dc.abs() < 0.5, "DC should be ~0 dB, got {dc}");
    }

    #[test]
    fn shelf_alt_zero_gain_is_passthrough() {
        let sos = compute_cascade_shelf_alt(1000.0, 1.0, 0.0, 48000.0, 2);
        assert_eq!(sos.len(), 3);
        for (i, s) in sos.iter().enumerate() {
            assert_eq!(*s, PASSTHROUGH, "Section {i} should be passthrough");
        }
    }

    #[test]
    fn shelf_alt_has_gain_at_center() {
        let sos = compute_cascade_shelf_alt(1000.0, 1.0, 12.0, 48000.0, 2);
        // Always 3 sections from hardcoded ZPK path
        assert_eq!(sos.len(), 3);
        // All sections should be valid (non-NaN) and not passthrough
        for (i, section) in sos.iter().enumerate() {
            for (j, &coeff) in section.iter().enumerate() {
                assert!(
                    coeff.is_finite(),
                    "section[{i}][{j}] is not finite: {coeff}"
                );
            }
            assert_ne!(
                *section, PASSTHROUGH,
                "Section {i} should not be passthrough for non-zero gain"
            );
        }
    }

    #[test]
    fn shelf_alt_multi_section() {
        // Always 3 sections regardless of order
        let sos = compute_cascade_shelf_alt(1000.0, 1.0, 12.0, 48000.0, 6);
        assert_eq!(sos.len(), 3);
        // All sections should be valid (non-NaN)
        for (i, section) in sos.iter().enumerate() {
            for (j, &coeff) in section.iter().enumerate() {
                assert!(
                    coeff.is_finite(),
                    "section[{i}][{j}] is not finite: {coeff}"
                );
            }
        }
    }
}
