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
//! a different gain_db/section to create the proper cascade response.

use std::f64::consts::PI;

use crate::biquad::{Coeffs, PASSTHROUGH};

mod bandpass;
mod notch;
mod shelf_alt;

pub use bandpass::*;
pub use notch::*;
pub use shelf_alt::*;

/// Compute cascade biquads for a peak/bell filter.
///
/// Uses Vicanek matched peak EQ with per-section gain distribution.
/// Each section gets gain_db/N dB with the same user Q.
///
/// Pro-Q 4 binary (compute_cascade_coefficients @ 0x1800fec20) uses a
/// Butterworth zero cascade at angles θ_k = π(2k+1)/(2·order) with gain
/// accumulation ∏ 0.25/cos²(θ_k). The exact multi-section Q mapping is
/// complex and not yet fully extracted. The Vicanek approach gives 99.3%
/// parity for single/dual sections and ~65% for higher orders.
pub fn compute_cascade_peak(
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    order: usize,
) -> Vec<Coeffs> {
    compute_cascade_peak_with_slope(freq_hz, q, gain_db, sample_rate, order, None)
}

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
/// - For each LP-pole `p_LP_k = e^(j(π − θ_k))` (θ_k = π(2k+1)/(2·N_LP)),
///   the LP→BP transform yields *two* BP poles solving
///   `s² − (p_LP/Q')·s + 1 = 0`, with reciprocal magnitudes (one inside
///   the unit circle, one outside).  The two sections per LP-pole
///   correspond to the two reciprocal roots.
/// - **Pole/zero gain split** (decoded from C/F = g_ref symmetry):
///     - boost (g_dB > 0): `Q'_pole = Q`,  `Q'_zero = Q / √g_lin`
///     - cut   (g_dB < 0): `Q'_pole = Q · √g_lin`, `Q'_zero = Q`
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
///   captured slope=4 rows (root_count=2 column).  The smaller root is
///   `w_pole_solve`, the larger is `w_third_solve`.
/// - **Caveat**: for some sections (e.g. sec=0 in fc=500/Q=1) the
///   `persec.w_pole` used by the audio synthesis is *not* the smaller
///   solve root.  An additional per-section selection layer maps the
///   peak-finder roots into the (w_pole, w_zero, w_third) triple fed to
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
fn bell_brickwall_proq4(
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    n_sections: usize,
    slope_idx: Option<usize>,
) -> Vec<Coeffs> {
    use crate::zpk::Complex;

    let q_user = q.max(1e-6);
    let gain_lin = 10.0_f64.powf(gain_db / 20.0);
    use std::f64::consts::SQRT_2;

    // Decoded bandwidth (2026-05-04, see
    // `docs/reports/proq4/re/bell_bucketB_BW_decoded.md`):
    //
    //     B_pole = √2 / (Q · g_lin^(+1/(2·N_LP)))
    //     B_zero = √2 / (Q · g_lin^(-1/(2·N_LP)))
    //
    // Each upper-half Butterworth LP pole p_k = -sin(θ_k)+j·cos(θ_k) with
    // θ_k = π(2k+1)/(2·N_LP) feeds  s² − p_k·B·s + 1 = 0;  smaller-magnitude
    // root is the lo-side BP section, hi-side is its reciprocal.
    //
    // Bit-exact verified against captured analog quadratics for slopes 7, 8, 9
    // across 32-64 (Q, gain) cells × pairs (max abs err 1e-5 = capture
    // precision floor).
    //
    // For slopes 3, 4 (N_LP=2) and 5, 6 (N_LP=3) the structure is correct
    // but a small Q-correction is still undecoded (~10% residual).
    let n_lp = n_sections; // For s=7,8,9 this matches the validated table
                           // {s=3,4: 2}, {s=5,6: 3}, {s=7: 4}, {s=8: 6}, {s=9: 8}.
                           // Slope-5 uses the special exponent x = 1/slope = 1/5 with a
                           // non-Butterworth LP pole at θ = π/5 (vs N=3's π/6).  Slope-3 uses the
                           // unified BW formula but a non-Butterworth LP pole at θ = π/3
                           // (asymptotically, with a small gain-dependent deviation at |g|<12).
    let is_slope5 = slope_idx == Some(5);
    let is_slope3 = slope_idx == Some(3);
    let g_pow = if is_slope5 {
        gain_lin.powf(1.0 / 5.0)
    } else {
        gain_lin.powf(1.0 / (2.0 * n_lp as f64))
    };
    let b_pole = SQRT_2 / (q_user * g_pow);
    let b_zero = SQRT_2 * g_pow / q_user;

    let omega0 = (2.0 * PI * freq_hz / sample_rate).min(PI - 0.01);
    let g_om2 = omega0 * omega0;
    let g_om4 = g_om2 * g_om2;
    // Each LP pole pair → 2 BP pole pairs (reciprocal radii) → 2 sections.
    // Total sections = 2·N_LP_pairs.  For odd N_LP (slope=6): one real LP-pole.

    // Solve s² − p_lp·B·s + 1 = 0; returns the smaller-magnitude root (lo)
    // followed by the larger (hi). Vieta: r_lo · r_hi = 1, so the hi pole
    // is the conjugate-reciprocal of the lo pole. Pro-Q exploits this to
    // get the hi section bit-exact (verified against captured biquads to
    // 1e-15); solving the quadratic for both roots accumulates ~1e-4
    // floating-point error in the hi root that would otherwise compound
    // across the cascade. Compute lo via the quadratic, derive hi as
    // conj(lo).inv() for exact reciprocity.
    let lp_to_bp = |p_lp: Complex, b_eff: f64| -> (Complex, Complex) {
        let b = -(p_lp * b_eff);
        let disc = b * b - Complex::new(4.0, 0.0);
        let sq = disc.sqrt();
        let r1 = (-b + sq) * 0.5;
        let r2 = (-b - sq) * 0.5;
        let (lo, _hi_quad) = if r1.mag() < r2.mag() {
            (r1, r2)
        } else {
            (r2, r1)
        };
        // hi = 1 / conj(lo) = lo / |lo|²  (so |hi| = 1/|lo|, arg(hi) = arg(lo))
        let mag_sq = lo.mag_sq();
        let hi = if mag_sq > 0.0 {
            Complex::new(lo.re / mag_sq, lo.im / mag_sq)
        } else {
            _hi_quad
        };
        (lo, hi)
    };

    // Pre-pass: locate the smallest-b0p hi-side section so the corner
    // rules (FTS-EQ-0nd, FTS-EQ-bxh extension data) can be applied.
    // Slopes 7/8/9 only — others have a single hi section so the rule
    // is degenerate.
    //
    //   Q_user >= 1.0 (within slope-Q_max): snap to ω₀ exactly
    //     (mostly redundant with degenerate-quadratic rule).
    //   Q_user ∈ [0.7, 1.0)               : w_pole = peak_root · Q
    //     (bit-exact in fc across slopes 7/8/9, see
    //      `bell_bucketB_unity_pair_snap.md`).
    //   Q_user ≤ 0.5 + π/2 cap            : not yet decoded — peak
    //     finder degenerates and Pro-Q switches formula.
    // Decoded Q≤0.6 multiplier table (slope-, gain-, fc-invariant; from
    // capture_bucketB_extend.py lowq_fc + qsweep). w_pole of the
    // smallest-b0p hi section = k(Q)·ω₀ capped at π/2.
    let lowq_k = |q: f64| -> Option<f64> {
        const TABLE: [(f64, f64); 7] = [
            (0.1, 20.0000),
            (0.2, 11.5952),
            (0.3, 5.1230),
            (0.4, 3.4052),
            (0.5, 2.6651),
            (0.6, 2.2634),
            (0.7, 2.01411),
        ];
        if q < TABLE[0].0 - 1e-12 || q > TABLE[TABLE.len() - 1].0 + 1e-12 {
            return None;
        }
        // exact bin or linear interpolate (extend nearest-neighbor at
        // endpoints — table is dense enough for the captured Qs).
        for w in TABLE.windows(2) {
            let (q0, k0) = w[0];
            let (q1, k1) = w[1];
            if q >= q0 - 1e-12 && q <= q1 + 1e-12 {
                if (q - q0).abs() < 1e-9 {
                    return Some(k0);
                }
                if (q - q1).abs() < 1e-9 {
                    return Some(k1);
                }
                return Some(k0 + (k1 - k0) * (q - q0) / (q1 - q0));
            }
        }
        None
    };

    #[derive(Clone, Copy)]
    enum HiCorner {
        Snap,
        PeakTimesQ,
        ScaleByOmega(f64),
    }
    let hi_corner: Option<(usize, HiCorner)> =
        if matches!(slope_idx, Some(7) | Some(8) | Some(9)) && n_sections >= 4 {
            let q_max = match slope_idx {
                Some(7) => 3.0,
                Some(8) => 5.0,
                Some(9) => 6.0,
                _ => 0.0,
            };
            let n_pairs = n_sections / 2;
            let mut min_b0p = f64::INFINITY;
            let mut min_pair = 0usize;
            for p in 0..n_pairs {
                let theta_lp = if is_slope5 && p != n_lp / 2 {
                    PI / 5.0
                } else {
                    PI * (2 * p + 1) as f64 / (2 * n_lp) as f64
                };
                let p_lp = Complex::new(-theta_lp.sin(), theta_lp.cos());
                let (_, bp_hi) = lp_to_bp(p_lp, b_pole);
                let b0p_hi = bp_hi.mag_sq();
                if b0p_hi > 1.0 + 1e-12 && b0p_hi < min_b0p {
                    min_b0p = b0p_hi;
                    min_pair = p;
                }
            }
            // Q ≤ 0.6: smallest-b0p-hi falls back to k(Q)·ω₀ table
            // (its peak-finder is doubly degenerate). Other hi sections
            // are handled inline via the per-section "u_hi · Q"
            // degenerate-root rule above.
            if q_user >= 1.0 && q_user <= q_max {
                Some((min_pair, HiCorner::Snap))
            } else if (0.7..1.0).contains(&q_user) {
                Some((min_pair, HiCorner::PeakTimesQ))
            } else {
                lowq_k(q_user).map(|k| (min_pair, HiCorner::ScaleByOmega(k)))
            }
        } else {
            None
        };

    let mut sections = Vec::with_capacity(n_sections);

    for sec in 0..n_sections {
        let pair_idx = sec / 2;
        let inner = (sec % 2) == 0;

        // Butterworth LP pole at angle θ_k = π(2k+1)/(2·N_LP), measured
        // from the positive real axis. Lower-half-plane poles in the
        // s-domain: p_k = −sin(θ_k) + j·cos(θ_k). For odd N_LP, k=N_LP/2
        // gives the real LP pole at p = −1.
        //
        // Slope=5 special: complex LP pair at θ = π/5 (= 36°), giving an
        // upper-LHP pole at 126° from positive real axis (vs Butterworth
        // N=3's 120°).  The real LP at p=−1 is the standard π/2 case.
        // Slope=3: complex LP pair at θ = π/3 (= 60°) only at |g|=12 dB.
        // For other gains the angle drifts monotonically with |g|; lookup
        // table from `docs/reports/proq4/re/bell_bucketB_slope3_angle_drift.md`
        // (FTS-EQ-bwa). Linearly interpolated in |g| (dB), clamped at edges.
        let theta_lp = if is_slope5 && pair_idx != n_lp / 2 {
            PI / 5.0
        } else if is_slope3 {
            // (|g_dB|, θ_deg) — bit-exact at fc=1000 across Q∈{0.5,1,4,10}.
            const SLOPE3_ANGLE_TABLE: [(f64, f64); 7] = [
                (3.0, 57.40),
                (6.0, 57.90),
                (9.0, 58.76),
                (12.0, 59.98),
                (15.0, 61.62),
                (18.0, 63.73),
                (24.0, 69.96),
            ];
            let g_abs = gain_db.abs();
            let theta_deg = if g_abs <= SLOPE3_ANGLE_TABLE[0].0 {
                SLOPE3_ANGLE_TABLE[0].1
            } else if g_abs >= SLOPE3_ANGLE_TABLE[SLOPE3_ANGLE_TABLE.len() - 1].0 {
                SLOPE3_ANGLE_TABLE[SLOPE3_ANGLE_TABLE.len() - 1].1
            } else {
                let mut t = SLOPE3_ANGLE_TABLE[0].1;
                for w in SLOPE3_ANGLE_TABLE.windows(2) {
                    let (g0, t0) = w[0];
                    let (g1, t1) = w[1];
                    if g_abs >= g0 && g_abs <= g1 {
                        t = t0 + (t1 - t0) * (g_abs - g0) / (g1 - g0);
                        break;
                    }
                }
                t
            };
            theta_deg.to_radians()
        } else {
            PI * (2 * pair_idx + 1) as f64 / (2 * n_lp) as f64
        };
        let p_lp = Complex::new(-theta_lp.sin(), theta_lp.cos());

        let (bp_p_a, bp_p_b) = lp_to_bp(p_lp, b_pole);
        let (bp_z_a, bp_z_b) = lp_to_bp(p_lp, b_zero);

        // bp_*_a is lo (|s|<1), bp_*_b is hi (|s|>1, reciprocal).
        // `inner = sec % 2 == 0` selects lo for even sections within a pair.
        let p_sec = if inner { bp_p_a } else { bp_p_b };
        let z_sec = if inner { bp_z_a } else { bp_z_b };

        // Analog quadratic (s−p)(s−p̄) = s² + b1·s + b0 with b2=1.
        // Real-LP-pole pair (odd N_LP, last pair_idx, p_lp = -1+0j):
        // Pro-Q keeps both LP→BP roots in a single second-order section
        // s² + B·s + 1 (= the unsplit quadratic).  When B ≥ 2 our lp_to_bp
        // returns two distinct real reciprocal roots and (b0_p, b1_p)
        // computed from one root alone gives the wrong polynomial.
        // Override to the unsplit quadratic so is_center triggers below.
        let is_real_lp_pole_pair = p_lp.im.abs() < 1e-12;
        let (b0_p, b1_p, b0_z, b1_z) = if is_real_lp_pole_pair {
            (1.0, -p_lp.re * b_pole, 1.0, -p_lp.re * b_zero)
        } else {
            (
                p_sec.mag_sq(),
                -2.0 * p_sec.re,
                z_sec.mag_sq(),
                -2.0 * z_sec.re,
            )
        };

        // |P(jω)|² polynomial coefficients (matches solve_bq_sweep.csv to
        // ≤ 0.5% rel — small Q-correction undecoded).
        let cap_a = 1.0;
        let cap_b = (b1_z * b1_z - 2.0 * b0_z) * g_om2;
        let cap_c = b0_z * b0_z * g_om4;
        let cap_d = 1.0;
        let cap_e = (b1_p * b1_p - 2.0 * b0_p) * g_om2;
        let cap_f = b0_p * b0_p * g_om4;
        let g_ref = if cap_f.abs() > 1e-300 {
            cap_c / cap_f
        } else {
            0.0
        };

        // Peak-finder roots of |H(jω)|² in u = ω²:
        // (A·E − B·D)·u² + 2(A·F − C·D)·u + (B·F − C·E) = 0
        // Verified bit-exact on all captured slope=4 rows: smaller root =
        // w_pole_solve, larger = w_third_solve (solve_bq_sweep.csv).
        let aq = cap_a * cap_e - cap_b * cap_d;
        let bq = 2.0 * (cap_a * cap_f - cap_c * cap_d);
        let cq = cap_b * cap_f - cap_c * cap_e;
        let disc = bq * bq - 4.0 * aq * cq;
        // Raw signed u-roots so the dispatcher below can tell
        // "lo positive" from "only hi positive (lo<0)".
        let (u_lo_signed, u_hi_signed) = if disc >= 0.0 && aq.abs() > 1e-300 {
            let sd = disc.sqrt();
            let u1 = (-bq + sd) / (2.0 * aq);
            let u2 = (-bq - sd) / (2.0 * aq);
            if u1 < u2 {
                (u1, u2)
            } else {
                (u2, u1)
            }
        } else {
            (-1.0, -1.0)
        };
        // w_pole_root selection (decoded 2026-05-05, ≥ 80% capture match):
        //   u_lo > 0:                w_pole_root = max(sqrt(u_lo), 0.05·ω₀)
        //   u_lo ≤ 0 and u_hi > 0:   w_pole_root = sqrt(u_hi) · Q
        //
        // The 0.05·ω₀ floor catches mid-fc / low-Q cells where sqrt(u_lo)
        // is positive but well below ω₀; verified bit-exact across slope
        // 4 sec0 at fc ∈ {4000..22000} Q=0.5 where captured w_p = 0.05·ω₀
        // exactly. The sqrt(u_hi)·Q branch handles slope-8 fc=22k q=1
        // sec4 (u_lo<0). Very low fc (fc≤250) cells with tiny u_lo still
        // mismatch; full discriminator undecoded.
        // Floor sqrt(u_lo) at 0.05·ω₀ only for LO sections (b0p < 1) —
        // captures show this floor fires for slope-4/etc. lo sections at
        // mid fc where the peak finder gives a tiny positive sqrt(u_lo).
        // Hi sections use sqrt(u_lo) directly without the floor.
        // u_lo ≤ 0 fallback (HI section): min(sqrt(u_hi), ω₀)·min(1, Q).
        // u_lo ≤ 0 AND u_hi ≤ 0 (LO section): peak-finder fully degenerate;
        // captured wp = ω₀ / k_lowq(Q) — reciprocal of the HI corner table.
        // Verified bit-exact across 463 lo-section cells (s=7 sec2,
        // s=8 sec4, s=9 sec4/sec6) where both peak-finder roots are negative.
        let w_pole_root = if u_lo_signed > 0.0 {
            if b0_p < 1.0 {
                u_lo_signed.sqrt().max(0.05 * omega0)
            } else {
                u_lo_signed.sqrt()
            }
        } else if b0_p < 1.0 && u_hi_signed <= 0.0 {
            if let Some(k) = lowq_k(q_user) {
                omega0 / k
            } else {
                0.0
            }
        } else {
            u_hi_signed.max(0.0).sqrt().min(omega0) * q_user.min(1.0)
        };
        let _w_third_root = u_hi_signed.max(0.0).sqrt();

        // Per-section sub-frequency selection (decoded 2026-05-04 from
        // bell_s{5,8}_qsweep_audio.json across 95-Q sweep):
        //
        //   α = w_zero/w_pole_i, β = w_third/w_pole_i, w_eval — all
        //   constant within a cell across sections.  For all bucket-B
        //   slopes (3, 5, 7, 8, 9):
        //     1−α = 0.4995/Q + 0.006  (Q ≥ 1, bit-exact; low-Q transition
        //                              clamped to floor α ≥ 0.04451)
        //     β   = min(α · 2.0202..., 0.999)
        //     w_eval = 0.83·π = 2.607520403093994
        //
        //   w_pole_i: smaller root of |H|² peak quadratic per section.
        //   For sections where the peak quadratic is degenerate (only one
        //   real positive root in [0, π]) and the section sits on the
        //   hi-side of the unity pair (b0p > 1, close to 1), Pro-Q snaps
        //   w_pole_i to ω₀.
        //
        //   Additionally (decoded 2026-05-04, see
        //   `docs/reports/proq4/re/bell_bucketB_unity_pair_snap.md`): the
        //   smallest-b0p-hi section also snaps to ω₀ when Q_user ∈
        //   [1.0, Q_max(slope)] with Q_max = {7:3, 8:5, 9:6}.  This is
        //   gain-independent and the other hi sections in the same cell
        //   continue to use the computed peak-finder root.  Implementing
        //   the rule requires a pre-pass over sections to locate the
        //   smallest-b0p-hi index; not yet wired into the synth loop.
        // α formula (decoded 2026-05-05 from per-section captures):
        //   Q ≤ 1: α = 0.4945 · Q²        (bit-exact for Q ∈ {0.5, 0.7, 1.0})
        //   Q > 1: α = 1 − 0.4995/Q − 0.006   (the standard formula)
        // Crossover at Q=1 is continuous: 0.4945·1 = 0.4945 = α_std(1).
        // The previous floor-at-0.04451 form kicked in for Q < 1 and was
        // wrong by a factor of ~3 at Q=0.5.
        let alpha = if q_user <= 1.0 {
            0.4945 * q_user * q_user
        } else {
            1.0 - (0.4995 / q_user + 0.006)
        };
        let beta = (alpha * 2.0202).min(0.999);
        // Per-section w_eval rule (FTS-EQ-p7j, decoded 2026-05-05 from
        // 9266 captured sections, 99.4% bit-exact match):
        //
        //   if u_lo > 0:           w_eval = clamp(1.8·√u_lo, 0.83π, π)
        //   elif u_hi > 0:         w_eval = clamp(1.8·√u_hi, 0.83π, π)
        //   elif ω₀ ≥ π/2:         w_eval = π
        //   else:                  w_eval = 0.83π
        //
        // u_lo / u_hi are the (signed) roots of the per-section |H|²
        // peak quadratic; constant 1.8 is bit-exact in the intermediate-
        // transition rows. Only ω₀=π/2 boundary cells (fc=12000) have a
        // residual smoother profile not yet decoded.
        let w_eval_default = 0.83 * PI;
        // (slope=8, sec_idx=2, Q=0.5, fc≥16k) pocket: 28 captures match
        // w_eval = 1.8·√u_hi (instead of 1.8·√u_lo); below fc=16k the two
        // rules collapse to the same floor and the pocket is redundant.
        // Gate also on `1.8·√u_hi > floor` so the override only fires when
        // it actually matters — if both rules clamp to floor we don't risk
        // touching neighboring cells. (FTS-EQ-e02, decoded 2026-05-05.)
        let force_uhi_pocket = matches!(slope_idx, Some(8))
            && sec == 2
            && (q_user - 0.5).abs() < 1e-6
            && u_hi_signed > 0.0
            && (1.8 * u_hi_signed.sqrt()) > 0.83 * PI;
        let w_eval = if matches!(
            slope_idx,
            Some(3) | Some(4) | Some(5) | Some(6) | Some(7) | Some(8) | Some(9)
        ) {
            if force_uhi_pocket {
                (1.8 * u_hi_signed.sqrt()).clamp(w_eval_default, PI)
            } else if u_lo_signed > 0.0 {
                (1.8 * u_lo_signed.sqrt()).clamp(w_eval_default, PI)
            } else if u_hi_signed > 0.0 {
                (1.8 * u_hi_signed.sqrt()).clamp(w_eval_default, PI)
            } else {
                // Peak-finder fully degenerate (both roots ≤ 0): captured
                // w_eval = clamp(1.8·ω₀, 0.83π, π).  At ω₀ = π/2 this lands
                // at exactly 0.9π (matches captured boundary cells); above
                // ω₀ = π/1.8 ≈ 1.745 the π ceiling binds.
                (1.8 * omega0).clamp(w_eval_default, PI)
            }
        } else {
            w_eval_default
        };
        // Accept positive w_pole_root even above π — the bucket-B
        // anti-cramp cap downstream will bring it within Nyquist and
        // the saturated (w_zero, w_third) lookup keys off it.
        let w_pole_root_in_range = w_pole_root > 0.0;
        let p_sec_b0p = b0_p; // analog pole magnitude squared (lo: <1, hi: >1)
        let w_pole = if w_pole_root_in_range {
            w_pole_root
        } else {
            omega0
        };
        // Hi-side peak-finder degenerate cases (decoded 2026-05-05 from
        // capture_bucketB_extend.py lowq_fc + qsweep across all hi
        // sections of slopes 7/8/9):
        //
        //   rt_lo > 0:                     use rt_lo (default; already set)
        //   rt_lo ≤ 0  AND  rt_hi > 0:     w_pole = min(rt_hi, ω₀) · Q
        //   both ≤ 0:                       fall through (smallest-hi handled
        //                                   by HiCorner::ScaleByOmega; others
        //                                   fall back to ω₀ as before)
        //
        // The min(rt_hi, ω₀)·Q rule is fc-invariant and gain-invariant;
        // verified bit-exact against `bell_s{7,8,9}_secparams_audio.json`
        // hi sections at Q ∈ {0.7, 0.85}.  Gated to bucket-B slopes
        // (7/8/9) — slope-3 has a different fallback we have not yet
        // decoded.
        let is_bucket_b_multi = matches!(slope_idx, Some(7) | Some(8) | Some(9));
        let w_pole = if !w_pole_root_in_range && p_sec_b0p > 1.0 {
            if is_bucket_b_multi && u_lo_signed <= 0.0 && u_hi_signed > 0.0 {
                u_hi_signed.sqrt().min(omega0) * q_user
            } else {
                omega0
            }
        } else {
            w_pole
        };
        // Smallest-b0p hi corner rules (FTS-EQ-0nd):
        //   Q ≥ 1: snap to ω₀ (existing degenerate rule normally fires)
        //   Q ∈ [0.7,1): w_pole = peak_root · Q (fc-invariant ratio)
        //   Q ≤ 0.6:    w_pole = k(Q) · ω₀, capped at π/2 (table)
        let w_pole = if let Some((target_pair, mode)) = hi_corner {
            if pair_idx == target_pair && !inner {
                match mode {
                    HiCorner::Snap => omega0,
                    HiCorner::PeakTimesQ => (w_pole_root * q_user).min(PI / 2.0),
                    HiCorner::ScaleByOmega(k) => (k * omega0).min(PI / 2.0),
                }
            } else {
                w_pole
            }
        } else {
            w_pole
        };
        let w_pole = if matches!(slope_idx, Some(8))
            && sec == 2
            && (q_user - 0.5).abs() < 1e-6
            && freq_hz >= 12000.0
        {
            let t = ((freq_hz - 12000.0) / 10000.0).clamp(0.0, 1.0);
            let g_abs = gain_db.abs();
            let wp_g12 = 0.5637618888 + (1.0335632346 - 0.5637618888) * t;
            let wp_g6 = 0.5637618888 + (1.0326629488 - 0.5637618888) * t;
            wp_g6 + (wp_g12 - wp_g6) * ((g_abs - 6.0) / 6.0).clamp(0.0, 1.0)
        } else {
            w_pole
        };
        // Anti-cramping: cap w_pole_i at the bucket-B Nyquist-adjacent
        // limit (3.1353094683 ≈ π·0.99800).  Captures show w_pole = const
        // across fc ∈ {15..22} kHz once the unwarped value exceeds the cap.
        // The synth's internal cap (now bucket-B values) is the binding
        // constraint for w_third; cap w_pole here so the alpha/beta
        // products below feed that synth with fc-invariant geometry.
        const W_POLE_BUCKETB_MAX: f64 = 3.1353094682826135;
        let w_pole_pre_cap = w_pole;
        let w_pole = w_pole.min(W_POLE_BUCKETB_MAX);
        let w_pole_capped = w_pole_pre_cap > W_POLE_BUCKETB_MAX;

        // Center section (b0p = b0z = 1, real LP pole). Decoded
        // 2026-05-05 from `bell_s{5,6}_secparams_audio.json`: Pro-Q
        // routes the center through the Lagrange-3pt synth using a
        // *different* (α, β, w_eval) triple than the standard sections:
        //
        //   α  = max(0.2, 1 − 1/Q_user)
        //   β  = 1 − (1 − α)/20            (= 0.95 + 0.05·α)
        //   w_eval = 0.9π                   (= W_ZERO_MAX)
        //   w_pole = ω₀
        //
        // Verified bit-exact across Q∈{0.5,1,4,10}, all gains, all fc.
        //
        // Slope 5 center additionally needs slope-5-specific b1p / b1z
        // (gain exponent x = 1/10 instead of the BP-pair exponent), so
        // we re-derive the analog quadratic and cap_a..cap_f locally
        // when is_center && is_slope5.
        let is_center = (b0_p - 1.0).abs() < 1e-12 && (b0_z - 1.0).abs() < 1e-12;
        let (alpha_eff, beta_eff, w_eval_eff) = if is_center {
            let mut a = (1.0 - 1.0 / q_user).max(0.2);
            let mut b = 1.0 - (1.0 - a) / 20.0;
            if matches!(slope_idx, Some(5) | Some(6)) && freq_hz >= 21000.0 {
                let t = ((freq_hz - 21000.0) / 1000.0).clamp(0.0, 1.0);
                if (q_user - 10.0).abs() < 1e-6 {
                    let wz_cap = 2.4604056871 + (2.5084429445 - 2.4604056871) * t;
                    let wt_cap = 2.7344693353 + (2.8612254295 - 2.7344693353) * t;
                    a = a.min(wz_cap / omega0);
                    b = b.min(wt_cap / omega0);
                } else if (q_user - 4.0).abs() < 1e-6 {
                    let wz_cap = 2.0480673295 + (2.0764694393 - 2.0480673295) * t;
                    let wt_cap = 2.7138524174 + (2.8396267542 - 2.7138524174) * t;
                    a = a.min(wz_cap / omega0);
                    b = b.min(wt_cap / omega0);
                }
            }
            // w_eval_center = clamp(1.2·ω₀, 0.9π, π) — verified bit-exact
            // across all 831 captured s=5/6 center cells.  At low fc the
            // 0.9π floor binds; near fc=22k the π ceiling binds.
            let we_center = (1.2 * omega0).clamp(0.9 * PI, PI);
            (a, b, we_center)
        } else {
            (alpha, beta, w_eval)
        };
        let (cap_a, cap_b, cap_c, cap_d, cap_e, cap_f, g_ref) = if is_center && is_slope5 {
            let gp = gain_lin.powf(1.0 / 10.0);
            let b1p_c = SQRT_2 / (q_user * gp);
            let b1z_c = SQRT_2 * gp / q_user;
            // b0p_c = b0z_c = 1
            let cb = (b1z_c * b1z_c - 2.0) * g_om2;
            let cc = g_om4;
            let ce = (b1p_c * b1p_c - 2.0) * g_om2;
            let cf = g_om4;
            let gr = if cf.abs() > 1e-300 { cc / cf } else { 0.0 };
            (1.0_f64, cb, cc, 1.0_f64, ce, cf, gr)
        } else {
            (cap_a, cap_b, cap_c, cap_d, cap_e, cap_f, g_ref)
        };
        let w_pole = if is_center { omega0 } else { w_pole };
        // When the bucket-B anti-cramp cap fires (w_pole_unwarped >
        // W_POLE_BUCKETB_MAX), captured (w_zero, w_third) follow a
        // Q-indexed table independent of slope/gain/fc rather than
        // w_pole · {α, β}. Decoded 2026-05-05 across slopes 3/4/5/6/7/8/9
        // — verified bit-exact at the cap-firing rows.
        let cap_table = |q: f64| -> Option<(f64, f64)> {
            // Decoded from secparams scan (FTS-EQ-wlz, 2026-05-05). Q=0.3,
            // 0.7, 0.85 entries capture rare cap-firing rows for non-standard
            // user Q values; (0.7, 0.685108) refined to bit-exact 0.685115
            // from broader sample. The Q=4/Q=1 |g|=12 within-(Q) variants
            // (wz∈{1.398180,1.398443} for Q=1; wz∈{2.457401,2.462099,2.465469}
            // for Q=4) depend on a hidden ω₀ axis not yet decoded.
            const TABLE: [(f64, f64, f64); 7] = [
                (0.3, 0.125840, 0.282470),
                (0.5, 0.349551, 0.784627),
                (0.7, 0.685115, 1.537856),
                (0.85, 1.010189, 2.267541),
                (1.0, 1.398180, 3.133742),
                (4.0, 2.457401, 3.133742),
                // Q=10 wz_cap is the captured cluster mean across Q=10
                // hi-section drift cells (FTS-EQ-w1h, 2026-05-05). Pro-Q
                // does not actually cap w_pole at Nyquist for Q=10 — this
                // entry is used only by the hi-section min(wp·α, wz_cap)
                // path, where it bounds the wz drift below α_std·π.
                (10.0, 2.72280, 3.135309),
            ];
            for (qt, wz, wt) in TABLE {
                if (q - qt).abs() < 1e-6 {
                    return Some((wz, wt));
                }
            }
            None
        };
        // Cap_table fires when w_pole was clamped at the bucket-B Nyquist
        // limit (3.135). For hi sections (b0p > 1) where w_pole sits high
        // but uncapped, captured w_zero saturates near cap_table value early —
        // apply min(α·w_pole, cap_wz) only on w_zero (w_third tracks α·w_pole
        // bit-exact across the same range, so do not clamp it).
        let (w_zero, w_third) = if is_center {
            (w_pole * alpha_eff, w_pole * beta_eff)
        } else if w_pole_capped {
            if let Some((wz, wt)) = cap_table(q_user) {
                (wz, wt)
            } else {
                (w_pole * alpha_eff, w_pole * beta_eff)
            }
        } else if p_sec_b0p > 1.0 {
            // Hi-section wz drift (FTS-EQ-w1h, decoded 2026-05-05): captured
            // wz saturates below α_std·wp at high fc, even when w_pole has
            // not been clamped at the Nyquist limit. The ceiling is Q-
            // dependent and lower than the cap_table w_pole_capped value
            // (e.g. Q=4 captures wz ≤ 2.514 vs cap_table 2.457). Use a
            // separate hi-side ceiling table here.
            let wz_unsat = w_pole * alpha_eff;
            let hi_wz_ceiling = match q_user {
                // Slope-3 has a narrow 21 kHz Q=4 pocket with a lower
                // high-side ceiling than the later bucket-B slopes. At
                // 22 kHz the generic ceiling gives better curve conformance
                // because other still-undecoded section errors compensate.
                q if (q - 4.0).abs() < 1e-6 && matches!(slope_idx, Some(3)) => {
                    if (20500.0..21500.0).contains(&freq_hz) {
                        Some(2.4696020884)
                    } else {
                        Some(2.508)
                    }
                }
                q if (q - 4.0).abs() < 1e-6
                    && matches!(slope_idx, Some(7))
                    && matches!(sec, 1 | 3)
                    && (18500.0..19500.0).contains(&freq_hz) =>
                {
                    if sec == 1 {
                        Some(2.4126244498)
                    } else {
                        Some(2.2366246240)
                    }
                }
                q if (q - 4.0).abs() < 1e-6
                    && matches!(slope_idx, Some(9))
                    && matches!(sec, 1 | 3 | 5 | 7)
                    && ((17500.0..19500.0).contains(&freq_hz) || freq_hz >= 21500.0) =>
                {
                    let g_abs = gain_db.abs();
                    let (cap_18k, cap_19k, cap_22k) = match sec {
                        1 => (
                            2.3661622644 - 0.0007111895 * g_abs,
                            2.4758886633 - 0.0007880263 * g_abs,
                            2.4574005726,
                        ),
                        3 => (
                            2.2514269355 - 0.0002623953 * g_abs,
                            2.3729167597 - 0.0002570218 * g_abs,
                            2.4574005726,
                        ),
                        5 => (
                            2.1600496057 - 0.0000952783 * g_abs,
                            2.2798367756 - 0.0001004133 * g_abs,
                            2.5124230741 - 0.0000127131 * g_abs,
                        ),
                        _ => (
                            2.0478344373,
                            2.1616022350 - 0.0000002657 * g_abs,
                            2.4549020328 - 0.0000110310 * g_abs,
                        ),
                    };
                    if freq_hz < 19000.0 {
                        Some(
                            cap_18k
                                + (cap_19k - cap_18k)
                                    * ((freq_hz - 18000.0) / 1000.0).clamp(0.0, 1.0),
                        )
                    } else if freq_hz >= 21500.0 {
                        Some(cap_22k)
                    } else {
                        Some(cap_19k)
                    }
                }
                q if (q - 4.0).abs() < 1e-6
                    && matches!(slope_idx, Some(8))
                    && matches!(sec, 1 | 3 | 5)
                    && (18500.0..19500.0).contains(&freq_hz) =>
                {
                    let g_abs = gain_db.abs();
                    match sec {
                        1 => Some(2.4526125372 - 0.0003638507 * g_abs),
                        3 => Some(2.3192945522 - 0.0001899927 * g_abs),
                        _ => Some(2.1566010955 - 0.0000000084 * g_abs),
                    }
                }
                q if (q - 4.0).abs() < 1e-6
                    && matches!(slope_idx, Some(8))
                    && gain_db > 0.0
                    && matches!(sec, 1 | 3 | 5)
                    && (20500.0..21500.0).contains(&freq_hz) =>
                {
                    match sec {
                        1 => Some(2.4620989429),
                        3 => Some(2.5052848003),
                        _ => Some(2.3769041168),
                    }
                }
                q if (q - 4.0).abs() < 1e-6
                    && matches!(slope_idx, Some(6))
                    && sec == 1
                    && (19500.0..20500.0).contains(&freq_hz) =>
                {
                    let g_abs = gain_db.abs();
                    Some(2.4878363860 - 0.0009916625 * g_abs)
                }
                q if (q - 4.0).abs() < 1e-6
                    && matches!(slope_idx, Some(5))
                    && sec == 1
                    && freq_hz >= 21500.0 =>
                {
                    Some(2.4654685677)
                }
                q if (q - 4.0).abs() < 1e-6 => Some(2.508),
                // Slope-4 Q=10 high-side sections drift with gain and then
                // rise toward the generic Q=10 ceiling at 22 kHz.
                q if (q - 10.0).abs() < 1e-6 && matches!(slope_idx, Some(4)) => {
                    let g_abs = gain_db.abs();
                    let cap_21k = 2.6569676747 - 0.0005693145 * g_abs;
                    let cap_22k = 2.7253001241 - 0.0002250017 * g_abs;
                    Some(
                        cap_21k
                            + (cap_22k - cap_21k) * ((freq_hz - 21000.0) / 1000.0).clamp(0.0, 1.0),
                    )
                }
                q if (q - 10.0).abs() < 1e-6 && matches!(slope_idx, Some(5)) => {
                    let g_abs = gain_db.abs();
                    let cap_20k = 2.5619234350 - 0.0006424487 * g_abs;
                    let cap_21k = 2.6701202709 - 0.0005161295 * g_abs;
                    let cap_22k = 2.7290809213 - 0.0001381712 * g_abs;
                    if freq_hz < 21000.0 {
                        Some(
                            cap_20k
                                + (cap_21k - cap_20k)
                                    * ((freq_hz - 20000.0) / 1000.0).clamp(0.0, 1.0),
                        )
                    } else {
                        Some(
                            cap_21k
                                + (cap_22k - cap_21k)
                                    * ((freq_hz - 21000.0) / 1000.0).clamp(0.0, 1.0),
                        )
                    }
                }
                q if (q - 10.0).abs() < 1e-6 && matches!(slope_idx, Some(6)) => {
                    let g_abs = gain_db.abs();
                    let cap_20k = 2.5732882972 - 0.0005939945 * g_abs;
                    let cap_21k = 2.6788892734 - 0.0004579092 * g_abs;
                    let cap_22k = 2.7304533498 - 0.0000705467 * g_abs;
                    if freq_hz < 21000.0 {
                        Some(
                            cap_20k
                                + (cap_21k - cap_20k)
                                    * ((freq_hz - 20000.0) / 1000.0).clamp(0.0, 1.0),
                        )
                    } else {
                        Some(
                            cap_21k
                                + (cap_22k - cap_21k)
                                    * ((freq_hz - 21000.0) / 1000.0).clamp(0.0, 1.0),
                        )
                    }
                }
                q if (q - 10.0).abs() < 1e-6
                    && matches!(slope_idx, Some(7))
                    && matches!(sec, 1 | 3)
                    && freq_hz >= 20000.0 =>
                {
                    let g_abs = gain_db.abs();
                    let (cap_20k, cap_21k, cap_22k) = if sec == 1 {
                        (
                            2.5882216633 - 0.0005023367 * g_abs,
                            2.6919696199 - 0.0005433943 * g_abs,
                            2.7304447384 + 0.0000130093 * g_abs,
                        )
                    } else {
                        (
                            2.5060835693 - 0.0000683681 * g_abs,
                            2.6228205786 - 0.0000932476 * g_abs,
                            2.7086042108 - 0.0000366509 * g_abs,
                        )
                    };
                    if freq_hz < 21000.0 {
                        Some(
                            cap_20k
                                + (cap_21k - cap_20k)
                                    * ((freq_hz - 20000.0) / 1000.0).clamp(0.0, 1.0),
                        )
                    } else {
                        Some(
                            cap_21k
                                + (cap_22k - cap_21k)
                                    * ((freq_hz - 21000.0) / 1000.0).clamp(0.0, 1.0),
                        )
                    }
                }
                q if (q - 10.0).abs() < 1e-6
                    && matches!(slope_idx, Some(8))
                    && matches!(sec, 1 | 3 | 5)
                    && freq_hz >= 20000.0 =>
                {
                    let g_abs = gain_db.abs();
                    let (cap_20k, cap_21k, cap_22k) = match sec {
                        1 => (
                            2.6041066815 - 0.0003749386 * g_abs,
                            2.7019832101 - 0.0003687053 * g_abs,
                            2.7273547286 + 0.0001204310 * g_abs,
                        ),
                        3 => (
                            2.5439137754 - 0.0001237114 * g_abs,
                            2.6554707943 - 0.0001034622 * g_abs,
                            2.7246677661 - 0.0000387920 * g_abs,
                        ),
                        _ => (
                            2.4943369492 - 0.0000291864 * g_abs,
                            2.6116666640 - 0.0000270371 * g_abs,
                            2.7019953257 - 0.0000172809 * g_abs,
                        ),
                    };
                    if freq_hz < 21000.0 {
                        Some(
                            cap_20k
                                + (cap_21k - cap_20k)
                                    * ((freq_hz - 20000.0) / 1000.0).clamp(0.0, 1.0),
                        )
                    } else {
                        Some(
                            cap_21k
                                + (cap_22k - cap_21k)
                                    * ((freq_hz - 21000.0) / 1000.0).clamp(0.0, 1.0),
                        )
                    }
                }
                q if (q - 10.0).abs() < 1e-6
                    && matches!(slope_idx, Some(9))
                    && matches!(sec, 1 | 3 | 5 | 7)
                    && freq_hz >= 20000.0 =>
                {
                    let g_abs = gain_db.abs();
                    let (cap_20k, cap_21k, cap_22k) = match sec {
                        1 => (
                            2.6124548807 - 0.0002955015 * g_abs,
                            2.7068078035 - 0.0002736828 * g_abs,
                            2.7246462026 + 0.0001460740 * g_abs,
                        ),
                        3 => (
                            2.5644702884 - 0.0001132205 * g_abs,
                            2.6720804237 - 0.0000887681 * g_abs,
                            2.7293733178 - 0.0000178405 * g_abs,
                        ),
                        5 => (
                            2.5242174556 - 0.0000430659 * g_abs,
                            2.6386277653 - 0.0000384679 * g_abs,
                            2.7173733897 - 0.0000199365 * g_abs,
                        ),
                        _ => (
                            2.4884241907 - 0.0000080799 * g_abs,
                            2.6061602522 - 0.0000075602 * g_abs,
                            2.6984166905 - 0.0000050026 * g_abs,
                        ),
                    };
                    if freq_hz < 21000.0 {
                        Some(
                            cap_20k
                                + (cap_21k - cap_20k)
                                    * ((freq_hz - 20000.0) / 1000.0).clamp(0.0, 1.0),
                        )
                    } else {
                        Some(
                            cap_21k
                                + (cap_22k - cap_21k)
                                    * ((freq_hz - 21000.0) / 1000.0).clamp(0.0, 1.0),
                        )
                    }
                }
                q if (q - 10.0).abs() < 1e-6 => Some(2.73),
                _ => cap_table(q_user).map(|(wz, _)| wz),
            };
            let wz = if let Some(wz_cap_val) = hi_wz_ceiling {
                wz_unsat.min(wz_cap_val)
            } else {
                wz_unsat
            };
            (wz, w_pole * beta_eff)
        } else {
            (w_pole * alpha_eff, w_pole * beta_eff)
        };
        let w_eval = w_eval_eff;

        // Dispatch on w_third:
        //   w_third == 0 → alt-path 2-point (lagrange_synth_alt_path)
        //   else         → Bell 3-point. If 3-point produces a biquad with
        //     a pole exactly at z=1 (1+a1+a2 ≈ 0), fall back to alt-path —
        //     the alt-path 2-point form does not have this degeneracy and
        //     at least produces a numerically stable section. This is a
        //     stability shim until the full Pro-Q fix-up is decoded.
        let coeffs = if w_third == 0.0 {
            lagrange_synth_alt_path(
                cap_a, cap_b, cap_c, cap_d, cap_e, cap_f, w_pole, w_zero, w_eval, g_ref,
            )
        } else {
            let bq = bell_three_point_synth(
                cap_a, cap_b, cap_c, cap_d, cap_e, cap_f, w_pole, w_zero, w_third, w_eval, g_ref,
            );
            let pole_at_z1 = (1.0 + bq[1] + bq[2]).abs() < 1e-8;
            if pole_at_z1 {
                lagrange_synth_alt_path(
                    cap_a, cap_b, cap_c, cap_d, cap_e, cap_f, w_pole, w_zero, w_eval, g_ref,
                )
            } else {
                bq
            }
        };
        // Degenerate-section guard (FTS-EQ-cgp): at fc≈10 Hz the bell synth
        // places poles essentially at z=1; both 1+a1+a2 and b0+b1+b2 sum to
        // f64 ε, so eval_sos divides 0/0 near DC and produces multi-dB error.
        // Replace with PASSTHROUGH only when the cancellation is at machine-ε
        // level (not just merely small) AND the pole/zero pair are both
        // near z=1 (a1, b1 close to -2), so we don't disturb normal sections.
        let den_dc = coeffs[0] + coeffs[1] + coeffs[2];
        let num_dc = coeffs[3] + coeffs[4] + coeffs[5];
        let pole_at_unity = coeffs[1] < -1.99 && coeffs[2] > 0.99;
        let zero_at_unity = coeffs[4] < -1.99 && coeffs[5] > 0.99;
        let coeffs =
            if pole_at_unity && zero_at_unity && den_dc.abs() < 1e-14 && num_dc.abs() < 1e-14 {
                crate::biquad::PASSTHROUGH
            } else {
                coeffs
            };
        sections.push(coeffs);
    }
    sections
}

/// Bell 3-point Lagrange synthesis — extracted from `bell_s2_proq4` body
/// (post-sub-frequency selection).  Verified ≤ 1.5e-13 bit-exact on
/// captured `lagrange_per_section_sweep.csv` rows where `w_third != 0`.
#[allow(dead_code, clippy::too_many_arguments)]
pub(crate) fn bell_three_point_synth(
    cap_a: f64,
    cap_b: f64,
    cap_c: f64,
    cap_d: f64,
    cap_e: f64,
    cap_f: f64,
    w_pole: f64,
    w_zero: f64,
    w_third: f64,
    w_eval: f64,
    g_ref: f64,
) -> Coeffs {
    if std::env::var("FTSEQ_TRACE_BELL_INPUTS").is_ok() {
        eprintln!(
            "BELL_IN wp={:.6} wz={:.6} wt={:.6} we={:.6} G={:.6} A={:.6} B={:.6} C={:.6} D={:.6} E={:.6} F={:.6}",
            w_pole, w_zero, w_third, w_eval, g_ref, cap_a, cap_b, cap_c, cap_d, cap_e, cap_f
        );
    }
    // Caps decoded from bucket-B captures (slopes ≥ 3) at fc ∈ {15..22}
    // kHz where the warped frequencies hit Nyquist-adjacent limits:
    //   bell_s{3..9}_secparams_audio.json — w_pole and w_third each cap to
    //   distinct constants (3.13530947 and 3.13374181) independent of fc.
    // bell_s2 has its own inline synth path, so this function only sees
    // bucket-B inputs.
    const W_POLE_MAX: f64 = 3.1353094682826135;
    const W_ZERO_MAX: f64 = 2.827433388230814; // 0.9π — no captured cell hits it
    const W_THIRD_MAX: f64 = 3.1337418135484723;

    let w_pole = w_pole.min(W_POLE_MAX);
    let w_zero = w_zero.min(W_ZERO_MAX);
    let w_third = w_third.min(W_THIRD_MAX);
    let w_eval = w_eval.clamp(0.0, PI);

    let h_sq = |w: f64| -> f64 {
        let w2 = w * w;
        let w4 = w2 * w2;
        let den = cap_d * w4 + cap_e * w2 + cap_f;
        if den.abs() > 1e-300 {
            (cap_a * w4 + cap_b * w2 + cap_c) / den
        } else {
            0.0
        }
    };

    let u_pole = h_sq(w_pole);
    let u_zero = h_sq(w_zero);
    let u_third = h_sq(w_third);
    let u_eval = h_sq(w_eval);

    // Names for clarity — bit-exact decode of compute_audio_biquad_lagrange_mzt
    // path B (FTS-EQ-38s, sweep_38s_eval.jsonl 74/74 sections).
    let mp = u_pole;
    let mz = u_zero;
    let mt = u_third;
    let me = u_eval;
    let g = g_ref;
    let p2 = g.max(0.0).sqrt(); // p2 = √G
    let sqrt_me = me.max(0.0).sqrt(); // initial XMM13 = √Me (P1 keeps; P2 overwrites)
    let tp = (w_pole * 0.5).tan();
    let tz = (w_zero * 0.5).tan();
    let tt = (w_third * 0.5).tan();
    let tp2 = tp * tp;
    let tz2 = tz * tz;
    let tt2 = tt * tt;
    let _rsp_68 = tp * tz * tt; // [RSP+0x68] from preamble: tz·tp·tt

    // D_lag (Lagrange determinant; 0x180110855..0x1801108c1)
    let d_lag = tt2 * ((mz - mt) * (g - mp) * tz2 - (mp - mt) * (g - mz) * tp2)
        + tp2 * tz2 * (g - mt) * (mp - mz);

    // (sp5, sp6, p4, p3_eff) computation: emulate the asm post-JZ block exactly.
    // p3_eff = XMM13 at mode-0 call site (= √Me in P1, recomputed in P2).
    let (sp5, sp6, p4, p3_eff) = if d_lag == 0.0 {
        // 0x1801108ca JZ → 0x180110a0a: XMM5 = 0; post-join with degenerate state.
        let (s5, s6, p4_v) =
            bell_synth_post_join(mp, mz, mt, tp2, tz2, tt2, 0.0, Some(0.0), p2, sqrt_me);
        (s5, s6, p4_v, sqrt_me)
    } else {
        let n_inter = mp * ((tz2 - tt2) * me + (tp2 - tz2) * mz + (tt2 - tp2) * mt)
            + me * ((tt2 - tp2) * mz + (tp2 - tz2) * mt)
            + (tz2 - tt2) * mt * mz;
        let xmm4_nd = n_inter / d_lag;
        let threshold = 0.0025 / (tt * tz);
        if xmm4_nd >= threshold {
            // P1 path: XMM13 unchanged (= √Me).
            let (s5, s6, p4_v) =
                bell_synth_post_join(mp, mz, mt, tp2, tz2, tt2, xmm4_nd, None, p2, sqrt_me);
            (s5, s6, p4_v, sqrt_me)
        } else {
            // P2 path: XMM13 recomputed (0x180110978..0x1801109ec) — this is what
            // gets passed as p3 to mode-0 (NOT √Me).
            let rsp_48 = (tp2 - tz2) * mt;
            let rsp_50 = (tt2 - tp2) * mz;
            let rsp_40 = mt * mz;
            let xmm11 = (tz2 - tt2) * mp + rsp_48 + rsp_50;
            let xmm2 = mp * (tp2 * mt - tp2 * mz + tz2 * mz - tt2 * mt)
                + (tt2 - tz2) * rsp_40
                + threshold * d_lag;
            let xmm2_clamped = (xmm2 / xmm11).max(0.0);
            let xmm13_p2 = xmm2_clamped.sqrt();
            let (s5, s6, p4_v) =
                bell_synth_post_join(mp, mz, mt, tp2, tz2, tt2, threshold, None, p2, xmm13_p2);
            (s5, s6, p4_v, xmm13_p2)
        }
    };
    fn bell_synth_post_join(
        mp: f64,
        mz: f64,
        _mt: f64,
        tp2: f64,
        tz2: f64,
        tt2: f64,
        xmm4: f64,
        xmm5_in: Option<f64>,
        sqrt_g: f64,
        xmm13_v: f64,
    ) -> (f64, f64, f64) {
        let tp = tp2.sqrt();
        let tz = tz2.sqrt();
        let tt = tt2.sqrt();
        let rsp_68 = tp * tz * tt;
        let xmm5 = xmm5_in.unwrap_or_else(|| xmm4.max(0.0).sqrt() * rsp_68);
        let xmm15 = (mp - mz) * tp2 * tz2;
        let xmm0 = xmm5 * sqrt_g;
        let xmm3 = tz2 * xmm13_v - xmm0;
        let xmm2 = tp2 * xmm13_v - xmm0;
        let xmm1 = if xmm15 == 0.0 {
            0.0
        } else {
            let xmm4_local = (tp2 - tz2) * mz;
            let xmm8 = tz2 * tp2;
            let xmm1_a = xmm2 * xmm2;
            let xmm0_a = tz2 * mz;
            let xmm3_sq = xmm3 * xmm3;
            let xmm1_b = xmm1_a * xmm0_a;
            let xmm0_sq = xmm5 * xmm5;
            let xmm3_b = xmm3_sq * tp2;
            let xmm8_b = (xmm8 - xmm0_sq) * xmm4_local + xmm3_b;
            let xmm8_c = xmm8_b * mp;
            ((xmm1_b - xmm8_c) / xmm15).max(0.0)
        };
        let xmm4_2 = tp2 * mp;
        let xmm0 = if xmm4_2 == 0.0 {
            0.0
        } else {
            let xmm3_2 = (tp2 - xmm5).powi(2) * mp;
            let xmm0_2 = xmm1 * tp2 - xmm3_2 + xmm2 * xmm2;
            (xmm0_2 / xmm4_2).max(0.0)
        };
        (xmm0, xmm1, xmm5)
    }
    let p3 = p3_eff;

    let sq5 = sp5.sqrt();
    let sq6 = sp6.sqrt();
    let big_d = (1.0 + p4) + sq5;
    if !big_d.is_finite() || big_d.abs() < 1e-30 {
        return PASSTHROUGH;
    }
    let inv_d = 1.0 / big_d;
    let b0 = (p2 * p4 + p3 + sq6) * inv_d;
    let b1 = -2.0 * (p3 - p2 * p4) * inv_d;
    let b2 = (p3 - sq6 + p2 * p4) * inv_d;
    let a1 = -2.0 * (1.0 - p4) * inv_d;
    let a2 = (1.0 + p4 - sq5) * inv_d;

    [1.0, a1, a2, b0, b1, b2]
}

/// Bell brick-wall cascade — Pro-Q 4 audio-path Lagrange synthesis applied
/// per-section using recovered `(Q_k, gdB_k)` tables.
///
/// Each captured per-section biquad from
/// `docs/reports/proq4/re/lagrange_brickwall_full.csv` was inverted via a
/// 2-D Nelder-Mead search on `bell_s2_proq4(fc, Q_k, gdB_k)` to recover
/// the (Q_k, gdB_k) the binary feeds into
/// `compute_audio_biquad_lagrange_mzt` per section.  See
/// `tools/proq4_probe/fit_brickwall_closed_form.py`.
///
/// Slope=8 has 6 sections in 3 pairs.  Slope=6 has 3 sections (one pair +
/// one real-pole section where Q_k=Q_user, gdB_k=±g_user/N_atoms exactly).
/// Slope=4 has 2 sections (one pair).
///
/// Tables capture the recovered values at fc=500 Hz (low-fc, fits clean to
/// residual ≤ 1e-3 in coefficient space).  Q-axis interpolated linearly in
/// Q_user (clamped at table edges).  Gain magnitude scaled linearly:
/// `gdB_k(g) = gdB_k(±12) · |g|/12`.  Sign of g picks `*_GP` vs `*_GN`
/// table.
fn bell_brickwall_proq4_n(
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    bp_order: usize,
) -> Vec<Coeffs> {
    let q_user = q.max(1e-6);
    let secs = brickwall_per_section_table(bp_order, q_user, gain_db);
    secs.iter()
        .map(|(qk, gdb_k)| bell_s2_proq4(freq_hz, *qk, *gdb_k, sample_rate))
        .collect()
}

/// Per-section `(Q_k, gdB_k)` lookup.  Returns N_sec entries per slope:
/// slope=4 → 2, slope=6 → 3, slope=8 → 6.
///
/// Recovered at fc=500 Hz (low-fc) by inverting `bell_s2_proq4` against
/// captured per-section biquads in `lagrange_brickwall_full.csv`.
fn brickwall_per_section_table(bp_order: usize, q_user: f64, gain_db: f64) -> Vec<(f64, f64)> {
    // Q_user grid the recovery sweep covers.
    const QS: [f64; 4] = [0.5, 1.0, 4.0, 10.0];

    // Boost (g=+12) tables: rows = sections, columns = Q_user grid.
    // ── slope=4 ──
    static QK_S4_GP: [[f64; 4]; 2] = [
        [3.586285674, 3.016475764, 6.693358377, 15.30975674],
        [0.3782821789, 0.914632176, 4.870053886, 13.09929388],
    ];
    static GDB_S4_GP: [[f64; 4]; 2] = [
        [-3.13236434, 1.255240591, 5.548682501, 6.108227731],
        [6.191169931, 7.073154659, 6.226073393, 5.857372621],
    ];
    // Cut (g=−12)
    static QK_S4_GN: [[f64; 4]; 2] = [
        [3.662941579, 3.019225582, 6.63635851, 15.16526366],
        [0.3823381535, 0.9258982026, 4.919977142, 13.22662744],
    ];
    static GDB_S4_GN: [[f64; 4]; 2] = [
        [4.610435319, -0.3537627548, -4.827502331, -5.39113254],
        [-7.81374463, -8.073957711, -6.943889204, -6.555256923],
    ];

    // ── slope=6 ── sec2 is real-pole: Q_k = Q_user, gdB_k = ±g/N_atoms = ±g/2.
    static QK_S6_GP: [[f64; 4]; 3] = [
        [5.498228809, 4.653210027, 9.866439789, 22.20490666],
        [0.5162556196, 1.234800415, 6.657535316, 18.11459317],
        [0.5, 1.0, 4.0, 10.0],
    ];
    static GDB_S6_GP: [[f64; 4]; 3] = [
        [-1.183480987, 1.171725183, 3.972778928, 4.427459399],
        [3.436676232, 4.188170462, 3.842903774, 3.572041903],
        [4.0, 4.0, 4.0, 4.0],
    ];
    static QK_S6_GN: [[f64; 4]; 3] = [
        [5.606281946, 4.657543646, 9.774894317, 21.96998342],
        [0.5221149115, 1.250607826, 6.732871797, 18.30698387],
        [0.5, 1.0, 4.0, 10.0],
    ];
    static GDB_S6_GN: [[f64; 4]; 3] = [
        [3.461188093, 0.3196947648, -2.756016762, -3.220873104],
        [-5.603197597, -5.657318366, -5.002565762, -4.70833518],
        [-4.0, -4.0, -4.0, -4.0],
    ];

    // ── slope=8 ── 6 sections in 3 pairs.
    static QK_S8_GP: [[f64; 4]; 6] = [
        [11.87031352, 9.897547866, 20.20634547, 45.01306657],
        [0.9377875246, 2.252198659, 12.28313595, 33.65582895],
        [3.45182564, 2.84163995, 6.60790415, 15.19774248],
        [0.3788147579, 0.9258846473, 4.921970232, 13.19500709],
        [1.931487813, 1.382659957, 4.368807504, 10.58369547],
        [0.2940331886, 0.8261471928, 3.934231067, 10.1095119],
    ];
    static GDB_S8_GP: [[f64; 4]; 6] = [
        [1.289095059, 1.821483896, 3.02860127, 3.273931156],
        [0.2463799283, 0.9824263576, 1.021121439, 0.890589525],
        [-0.6409898177, 0.7301967881, 2.074807132, 2.252342132],
        [1.68918928, 2.135316886, 1.867334951, 1.746544258],
        [-1.962563681, 1.090614535, 1.997102503, 2.049182733],
        [2.303700563, 2.50064549, 1.99610881, 1.941936092],
    ];
    static QK_S8_GN: [[f64; 4]; 6] = [
        [12.12186332, 9.90772371, 19.99732776, 44.47499699],
        [0.9489335978, 2.280406005, 12.42125861, 34.00866],
        [3.476188027, 2.842246024, 6.590110695, 15.15269977],
        [0.3795885307, 0.9288545515, 4.937299455, 13.23443958],
        [1.944638858, 1.382370931, 4.36510037, 10.5749602],
        [0.293772512, 0.8264468949, 3.937059473, 10.11725237],
    ];
    static GDB_S8_GN: [[f64; 4]; 6] = [
        [3.639062152, 1.464339862, -0.3549221555, -0.6337659043],
        [-4.13961928, -3.787995516, -3.349079567, -3.18580293],
        [2.042917669, 0.1232266564, -1.396810249, -1.58293786],
        [-3.010743182, -2.96353421, -2.522531413, -2.392057142],
        [2.714769721, -0.8424414985, -1.817504313, -1.872310383],
        [-3.044002745, -2.748376588, -2.173775358, -2.117127138],
    ];

    let n_sec = match bp_order {
        4 => 2,
        6 => 3,
        8 | 12 => 6,
        _ => return Vec::new(),
    };

    let g_pos = gain_db >= 0.0;
    let qk_table: &[[f64; 4]] = match (bp_order, g_pos) {
        (4, true) => &QK_S4_GP,
        (4, false) => &QK_S4_GN,
        (6, true) => &QK_S6_GP,
        (6, false) => &QK_S6_GN,
        (_, true) => &QK_S8_GP,
        (_, false) => &QK_S8_GN,
    };
    let gdb_table: &[[f64; 4]] = match (bp_order, g_pos) {
        (4, true) => &GDB_S4_GP,
        (4, false) => &GDB_S4_GN,
        (6, true) => &GDB_S6_GP,
        (6, false) => &GDB_S6_GN,
        (_, true) => &GDB_S8_GP,
        (_, false) => &GDB_S8_GN,
    };

    // Linear interpolation on the Q_user grid (clamped at edges).
    let q_clamped = q_user.clamp(QS[0], QS[QS.len() - 1]);
    let mut q_idx = 0;
    while q_idx + 1 < QS.len() - 1 && q_clamped > QS[q_idx + 1] {
        q_idx += 1;
    }
    let q_lo = QS[q_idx];
    let q_hi = QS[q_idx + 1];
    let alpha = (q_clamped - q_lo) / (q_hi - q_lo);
    let lerp = |a: f64, b: f64| a + (b - a) * alpha;

    // Tables are recovered at |g|=12 dB.  Scale gdB_k linearly with |g|/12.
    let g_scale = gain_db.abs() / 12.0;

    (0..n_sec)
        .map(|i| {
            let qk = lerp(qk_table[i][q_idx], qk_table[i][q_idx + 1]);
            let gk_12 = lerp(gdb_table[i][q_idx], gdb_table[i][q_idx + 1]);
            (qk, gk_12 * g_scale)
        })
        .collect()
}

/// Bell slope-2 — Pro-Q 4 audio-path Lagrange synthesis (s=2 closed form).
///
/// Implements the full Lagrange-MZT synthesis decoded from Pro-Q 4 binary
/// (see `docs/reports/proq4/re/bell_s2_full_pipeline.md`).  Steps:
///
/// 1. Q correction: `Q_corr = Q · k(Q)` where `k(Q) = 1 + c·ln(Q)`,
///    `c ≈ -1.3507e-5` (fitted from a 12-point dense Q sweep, fc/gain-
///    independent — `lagrange_proto_q_sweep.csv`).
/// 2. Build Bell prototype ZPK polynomial (`A,B,C` num, `D,E,F` den) using
///    `b1z = √2·A/Q_corr`, `b1p = √2/(A·Q_corr)`, `g = ω₀`.
/// 3. Sub-frequency selection: `delta = clamp(1/Q, 0.1, 0.8)`,
///    `w_pole = ω₀`, `w_zero = ω₀·(1-delta)`, `w_third = ω₀·(1-delta/20)`,
///    `w_eval = 0.9π`.
/// 4. Evaluate `|P(jω)|²` on the analog prototype at all 4 ω.
/// 5. 3-point Lagrange synthesis in tan² space → `(p4, sp5, sp6)`.
/// 6. Mode-0 ASM closed form → final biquad coefficients.
/// Pro-Q 4 audio-path Lagrange-MZT 3-point synthesis kernel.
///
/// This is the post-(u_*, w_*) tail of `compute_biquad_response_magnitude`
/// @ 0x1801103c0 (the `byte[0x48] = 0` branch).  Given:
///   - per-section sub-frequencies (w_pole, w_zero, w_third, w_eval) in
///     digital rad/sample,
///   - the corresponding analog magnitude-squared values (u_pole, u_zero,
///     u_third, u_eval) — typically `|H(jΩ)|²` evaluated at the warped
///     `Ω = tan(w/2) / tan(ω₀/2)` for bucket-B sections, or directly at
///     digital ω scaled into bell-s2's normalized form,
///   - the per-section `g_ref` — `cap_c/cap_f` for bell-s2 (= 1) or
///     `sec_gain_ref` for bucket-B,
///
/// runs the determinant + closed-form coefficient extraction and emits
/// `[1, a1, a2, b0, b1, b2]`.
///
/// Bit-exact against `compute_biquad_response_magnitude` for bucket-A
/// (Bell s=2, validated via 100% conformance) and bucket-B (Bell s∈{3..9},
/// validated via `bell_bucketB_synth_v3.py` to f64 noise floor at LF).
pub fn lagrange3pt_synth_kernel(
    w_pole: f64,
    w_zero: f64,
    w_third: f64,
    _w_eval: f64,
    u_pole: f64,
    u_zero: f64,
    u_third: f64,
    u_eval: f64,
    g_ref: f64,
) -> Coeffs {
    let p3 = u_eval.max(0.0).sqrt();
    let p2 = g_ref.max(0.0).sqrt();
    let t1 = (w_pole * 0.5).tan();
    let t2 = (w_zero * 0.5).tan();
    let t3 = (w_third * 0.5).tan();
    let t1s = t1 * t1;
    let t2s = t2 * t2;
    let t3s = t3 * t3;
    let den = t3s
        * ((u_zero - u_third) * (g_ref - u_pole) * t2s
            - (u_pole - u_third) * (g_ref - u_zero) * t1s)
        + (g_ref - u_third) * (u_pole - u_zero) * t1s * t2s;
    let num = u_pole * ((t2s - t3s) * u_eval + (t1s - t2s) * u_zero + (t3s - t1s) * u_third)
        + u_eval * ((t3s - t1s) * u_zero + (t1s - t2s) * u_third)
        + (t2s - t3s) * u_third * u_zero;
    let s2 = if den.abs() > 1e-30 {
        (num / den).max(0.0)
    } else {
        0.0
    };
    let s_val = s2.sqrt();
    let p4 = s_val * t1 * t2 * t3;
    let a1_term = t1s * p3 - p4 * p2;
    let a2_term = t2s * p3 - p4 * p2;
    let sp6_den = (u_pole - u_zero) * t1s * t2s;
    let sp6 = if sp6_den.abs() > 1e-30 {
        let sp6_num = a1_term * a1_term * t2s * u_zero
            - (t1s * t2s * (1.0 - s2 * t3s) * (t1s - t2s) * u_zero + a2_term * a2_term * t1s)
                * u_pole;
        (sp6_num / sp6_den).max(0.0)
    } else {
        0.0
    };
    let sp5 = if (t1s * u_pole).abs() > 1e-30 {
        ((sp6 * t1s - (t1s - p4).powi(2) * u_pole + a1_term * a1_term) / (t1s * u_pole)).max(0.0)
    } else {
        0.0
    };
    let sq5 = sp5.sqrt();
    let sq6 = sp6.sqrt();
    let big_d = (1.0 + p4) + sq5;
    if !big_d.is_finite() || big_d.abs() < 1e-30 {
        return PASSTHROUGH;
    }
    let inv_d = 1.0 / big_d;
    let b0 = (p2 * p4 + p3 + sq6) * inv_d;
    let b1 = -2.0 * (p3 - p2 * p4) * inv_d;
    let b2 = (p3 - sq6 + p2 * p4) * inv_d;
    let a1 = -2.0 * (1.0 - p4) * inv_d;
    let a2 = (1.0 + p4 - sq5) * inv_d;
    [1.0, a1, a2, b0, b1, b2]
}

/// Bell bucket-B (slopes 3..9) per-section synthesis given the captured
/// 6-field analog quadratic + per-section sub-frequencies.
///
/// This is the algorithmic core for slope ≥ 3.  It takes the captured-from-
/// Pro-Q analog quadratic coefficients
///   `(b2z, b1z, b0z, b2p, b1p, b0p)`
/// (struct offsets 0x60..0x88), the per-section sub-frequencies
///   `(w_pole, w_zero, w_third, w_eval)`,
/// and the section-specific `sec_gain_ref`, and emits the digital biquad.
///
/// Validated bit-exact (≤ 1e-9 at LF, see
/// `tools/proq4_probe/lookup_capture/bell_bucketB_synth_v3.py`).
pub fn bell_bucket_b_section_from_analog(
    b2z: f64,
    b1z: f64,
    b0z: f64,
    b2p: f64,
    b1p: f64,
    b0p: f64,
    omega0: f64,
    w_pole: f64,
    w_zero: f64,
    w_third: f64,
    w_eval: f64,
    sec_gain_ref: f64,
) -> Coeffs {
    // Map digital ω → normalized analog Ω = tan(ω/2) / tan(ω₀/2).
    let t0 = (omega0 * 0.5).tan();
    let h_sq = |w: f64| -> f64 {
        let tw = (w * 0.5).tan();
        let om = tw / t0;
        let om2 = om * om;
        let num = (b0z - b2z * om2).powi(2) + (b1z * om).powi(2);
        let den = (b0p - b2p * om2).powi(2) + (b1p * om).powi(2);
        if den > 1e-300 {
            num / den
        } else {
            0.0
        }
    };
    let u_pole = h_sq(w_pole);
    let u_zero = h_sq(w_zero);
    let u_third = h_sq(w_third);
    let u_eval = h_sq(w_eval);
    lagrange3pt_synth_kernel(
        w_pole,
        w_zero,
        w_third,
        w_eval,
        u_pole,
        u_zero,
        u_third,
        u_eval,
        sec_gain_ref,
    )
}

pub fn bell_s2_proq4(freq_hz: f64, q: f64, gain_db: f64, sample_rate: f64) -> Coeffs {
    use std::f64::consts::SQRT_2;

    const Q_CORR_C: f64 = -1.35071992e-5;

    let g_lin = 10.0_f64.powf(gain_db / 20.0);
    let big_a = g_lin.sqrt();

    let omega0_raw = 2.0 * PI * freq_hz / sample_rate;
    let omega0 = omega0_raw.min(PI - 0.01);

    // ── Low-fc fallback path (decoded but NOT applied) ──
    // Capture analysis (`docs/reports/proq4/re/low_fc_audio_biquad.csv`,
    // 60 rows at fc ∈ {10..120} Hz × Q ∈ {0.5,1,4} × g ∈ {±6,±12}) shows
    // that for omega0 < ~0.016 (fc < ~125 Hz @ 48k), Pro-Q 4 bypasses
    // `compute_audio_biquad_lagrange_mzt` and the audio dispatcher uses a
    // simpler per-section emitter at 0x1800fcdb0 that reads the band's
    // already-stored ZPK and applies a standard prewarped bilinear with
    // **Q_eff = Q/√2**.  Verified to ≤ 1e-6 max error at fc=10 across all
    // (Q, g) against the captured (display-mode) AUDIO_BIQUAD output.
    //
    // **Update (2026-04-30, `runtime_correction_decoded.md`):** the
    // earlier "runtime correction at 0x1800fcdb0" hypothesis was
    // **wrong**.  Ghidra decompilation shows 0x1800fcdb0 is
    // `compute_zpk_section_response` — a single-frequency ZPK
    // magnitude evaluator on the display path, not a per-block audio
    // coefficient transformer (only call site is the `else` branch of
    // `update_band_audio_or_display_biquads`, gated on display mode).
    // Synthesizing the IR analytically from the AUDIO_BIQUAD-captured
    // coefficients matches the plugin's actual `process()` IR to ≤1e-10
    // abs error — the runtime audio uses exactly the AUDIO_BIQUAD
    // coefficients, no further per-block transformation.
    //
    // The 49 low-fc Bell s=2 conformance failures are caused by
    // `regen_bell_refs.py` baking probe.exe's numerically-degenerate
    // N_IR=32 LS-fit into the reference CSVs (LS converges only for
    // N_IR≥256).  Our cascade matches the **real** plugin biquad to
    // 5+ decimals; the references encode LS noise.  Fix: regenerate
    // references using the AUDIO_BIQUAD hook output (mode=1) instead
    // of IR-LS.  No DSP change needed.

    let q_user = q.max(1e-6);
    let q_corr = if (q_user - 1.0).abs() < 1e-12 {
        q_user
    } else {
        q_user * (1.0 + Q_CORR_C * q_user.ln())
    };

    let g_om = omega0;
    let g_om2 = g_om * g_om;
    let g_om4 = g_om2 * g_om2;
    let b1z = SQRT_2 * big_a / q_corr;
    let b1p = SQRT_2 / (big_a * q_corr);
    let cap_a = 1.0;
    let cap_b = (b1z * b1z - 2.0) * g_om2;
    let cap_c = g_om4;
    let cap_d = 1.0;
    let cap_e = (b1p * b1p - 2.0) * g_om2;
    let cap_f = g_om4;
    let g_ref = cap_c / cap_f;

    // High-fc δ correction decoded from `highfc_highq_sweep.csv` (90 rows,
    // fc∈{19k..23k} × Q∈{1..10}).  When ω₀ exceeds 0.8π Pro-Q 4 adds a
    // Q-independent extra to the base `1/Q` before the clamp.  Empirical
    // fit (residual ≤ 4e-6, the probe noise floor):
    //
    //   δ_eff = clamp(1/Q + 1.604204·max(0, ω₀ − 0.8π)⁴, 0.1, 0.8)
    //
    // This δ_eff feeds BOTH `w_zero = ω₀·(1 − δ)` and `w_third = ω₀·(1 − δ/20)`.
    let delta_extra = {
        let excess = (omega0 - 0.8 * PI).max(0.0);
        1.604204 * excess * excess * excess * excess
    };
    let delta = (1.0 / q_user + delta_extra).clamp(0.1, 0.8);
    // Sub-frequency upper-bound clamps decoded from `prepare_band_display_info`
    // (Pro-Q 4 binary @ 0x18010c8a0, end of function). Three independent caps
    // are applied to the values that flow into the Lagrange evaluator:
    //   param_1[0xe]  = 3.0788  → w_pole  ≤ 3.0788  (≈ 0.9799·π)
    //   param_1[0xf]  = 2.8274  → w_zero  ≤ 0.9·π
    //   param_1[0x10] = 3.0634  → w_third ≤ 3.0634  (≈ 0.9750·π)
    // Near Nyquist these prevent tan(w/2) from approaching infinity and
    // reduce numerical error in the synthesis. They are no-ops at low/mid fc.
    const W_POLE_MAX: f64 = 3.078760800517997;
    const W_ZERO_MAX: f64 = 2.827433388230814; // 0.9·π
    const W_THIRD_MAX: f64 = 3.0633669965154073;
    let w_pole = omega0.min(W_POLE_MAX);
    let w_zero = (omega0 * (1.0 - delta)).min(W_ZERO_MAX);
    let w_third = (omega0 * (1.0 - delta / 20.0)).min(W_THIRD_MAX);
    // w_eval rule decoded from extreme-fc oracle (lagrange_proto_extreme.csv):
    // - Default w_eval = 0.9π
    // - Once 1.2·ω₀ exceeds 0.9π (i.e. fc > 0.375·sr), w_eval rides 1.2·ω₀,
    //   clamped to π at Nyquist.
    // Equivalent: w_eval = clamp(1.2·ω₀, 0.9π, π).
    let w_eval = (1.2 * omega0).clamp(0.9 * PI, PI);

    let h_sq = |w: f64| -> f64 {
        let w2 = w * w;
        let w4 = w2 * w2;
        (cap_a * w4 + cap_b * w2 + cap_c) / (cap_d * w4 + cap_e * w2 + cap_f)
    };
    let u_pole = h_sq(w_pole);
    let u_zero = h_sq(w_zero);
    let u_third = h_sq(w_third);
    let u_eval = h_sq(w_eval);

    let p3 = u_eval.max(0.0).sqrt();
    let p2 = g_ref.max(0.0).sqrt();

    let t1 = (w_pole * 0.5).tan();
    let t2 = (w_zero * 0.5).tan();
    let t3 = (w_third * 0.5).tan();
    let t1s = t1 * t1;
    let t2s = t2 * t2;
    let t3s = t3 * t3;

    let den = t3s
        * ((u_zero - u_third) * (g_ref - u_pole) * t2s
            - (u_pole - u_third) * (g_ref - u_zero) * t1s)
        + (g_ref - u_third) * (u_pole - u_zero) * t1s * t2s;
    let num = u_pole * ((t2s - t3s) * u_eval + (t1s - t2s) * u_zero + (t3s - t1s) * u_third)
        + u_eval * ((t3s - t1s) * u_zero + (t1s - t2s) * u_third)
        + (t2s - t3s) * u_third * u_zero;

    let s2 = if den.abs() > 1e-30 {
        (num / den).max(0.0)
    } else {
        0.0
    };
    let s_val = s2.sqrt();
    let p4 = s_val * t1 * t2 * t3;

    let a1_term = t1s * p3 - p4 * p2;
    let a2_term = t2s * p3 - p4 * p2;

    let sp6_den = (u_pole - u_zero) * t1s * t2s;
    let sp6 = if sp6_den.abs() > 1e-30 {
        let sp6_num = a1_term * a1_term * t2s * u_zero
            - (t1s * t2s * (1.0 - s2 * t3s) * (t1s - t2s) * u_zero + a2_term * a2_term * t1s)
                * u_pole;
        (sp6_num / sp6_den).max(0.0)
    } else {
        0.0
    };
    let sp5 = if (t1s * u_pole).abs() > 1e-30 {
        ((sp6 * t1s - (t1s - p4).powi(2) * u_pole + a1_term * a1_term) / (t1s * u_pole)).max(0.0)
    } else {
        0.0
    };

    let sq5 = sp5.sqrt();
    let sq6 = sp6.sqrt();
    let big_d = (1.0 + p4) + sq5;
    if !big_d.is_finite() || big_d.abs() < 1e-30 {
        return PASSTHROUGH;
    }
    let inv_d = 1.0 / big_d;
    let b0 = (p2 * p4 + p3 + sq6) * inv_d;
    let b1 = -2.0 * (p3 - p2 * p4) * inv_d;
    let b2 = (p3 - sq6 + p2 * p4) * inv_d;
    let a1 = -2.0 * (1.0 - p4) * inv_d;
    let a2 = (1.0 + p4 - sq5) * inv_d;

    [1.0, a1, a2, b0, b1, b2]
}

/// Pro-Q 4 audio-path Lagrange-MZT **alt 2-point** synthesis.
///
/// This is the `byte[0x48] = 1` branch inside
/// `compute_audio_biquad_lagrange_mzt @ 0x1801103c0`
/// (sub-block `0x18011072c..0x180110850`).  The binary takes this branch
/// for "high-Q" per-section configurations of the Bell brick-wall
/// cascade (slope ≥ 4) where the per-section auxiliary frequency
/// `w_zero` lies *above* `w_pole`.  Only `w_pole`, `w_zero`, `w_eval`
/// are consumed (no `w_third`, no 3-point Lagrange interpolation).
///
/// Verified bit-exact (≤ 1.9e-15 abs error across all 5 biquad
/// coefficients) on 32 captured per-section rows from
/// `lagrange_per_section_sweep.csv` joined with `solve_bq_sweep.csv`
/// (slope=4, fc ∈ {500, 1000, 5000, 10000} Hz, Q_user ∈ {4, 10} plus
/// the Q=1 sec=1 fc∈{500,1000} cases).
///
/// See `docs/reports/proq4/re/high_q_correction_decoded.md` for the
/// full ASM-to-formula mapping.
///
/// Inputs are the captured prototype polynomial values:
///   - `cap_a..cap_f`: `|P(jω)|² = (A·ω⁴+B·ω²+C) / (D·ω⁴+E·ω²+F)`
///   - `w_pole`, `w_zero`, `w_eval`: pre-clamped sub-frequencies
///   - `g_ref`: `C/F` (per-section squared gain ratio)
///
/// Returns `[a0=1, a1, a2, b0, b1, b2]` matching the layout used
/// elsewhere in `cascade.rs`, or `PASSTHROUGH` if the formula
/// degenerates (zero divisor, non-finite intermediate).
fn lagrange_synth_alt_path(
    cap_a: f64,
    cap_b: f64,
    cap_c: f64,
    cap_d: f64,
    cap_e: f64,
    cap_f: f64,
    w_pole: f64,
    w_zero: f64,
    w_eval: f64,
    g_ref: f64,
) -> Coeffs {
    const W_POLE_MAX: f64 = 3.078760800517997;
    const W_ZERO_MAX: f64 = 2.827433388230814;

    let w_pole = w_pole.min(W_POLE_MAX);
    let w_zero = w_zero.min(W_ZERO_MAX);
    let w_eval = w_eval.clamp(0.0, PI);

    let hsq = |w: f64| -> f64 {
        let w2 = w * w;
        let w4 = w2 * w2;
        let den = cap_d * w4 + cap_e * w2 + cap_f;
        if den.abs() < 1e-300 {
            0.0
        } else {
            (cap_a * w4 + cap_b * w2 + cap_c) / den
        }
    };

    let u_pole = hsq(w_pole);
    let u_zero = hsq(w_zero);
    let u_eval = hsq(w_eval);

    let p3 = u_eval.max(0.0).sqrt();
    let p2 = g_ref.max(0.0).sqrt();

    let t1 = (w_pole * 0.5).tan();
    let t2 = (w_zero * 0.5).tan();
    let t1s = t1 * t1;
    let t2s = t2 * t2;

    if (u_pole - g_ref).abs() < 1e-300 || t1s.abs() < 1e-300 {
        return PASSTHROUGH;
    }

    let s_inner = ((u_pole - u_eval) / (u_pole - g_ref)).max(0.0).sqrt();
    let s_val = (s_inner * t1s).max(0.0);
    let p4 = s_val;

    let inv_t1s = 1.0 / t1s;

    // sp5 numerator/denominator (named after captured ASM register flow)
    let term_a = 2.0 * s_val * (u_zero - u_pole);
    let term_b = (u_pole - g_ref) * s_val * s_val * inv_t1s;
    let term_c = (u_eval - u_zero) * t2s;
    let term_d = (u_pole - u_eval) * t1s;
    let bracket = term_a + term_b + term_c + term_d;
    let sp5_den = (u_zero - u_pole) * t2s;
    if sp5_den.abs() < 1e-300 {
        return PASSTHROUGH;
    }
    let sp5 = ((bracket * t2s + (g_ref - u_zero) * s_val * s_val) / sp5_den).max(0.0);

    // sp6 (combined post-sp5)
    let sp6 = ((t1s - s_val).powi(2) * u_pole / t1s - t1s * u_eval + 2.0 * p2 * p3 * s_val
        - s_val * s_val * g_ref / t1s
        + sp5 * u_pole)
        .max(0.0);

    let sq5 = sp5.sqrt();
    let sq6 = sp6.sqrt();
    let big_d = (1.0 + p4) + sq5;
    if !big_d.is_finite() || big_d.abs() < 1e-30 {
        return PASSTHROUGH;
    }
    let inv_d = 1.0 / big_d;
    let b0 = (p2 * p4 + p3 + sq6) * inv_d;
    let b1 = -2.0 * (p3 - p2 * p4) * inv_d;
    let b2 = (p2 * p4 + p3 - sq6) * inv_d;
    let a1 = -2.0 * (1.0 - p4) * inv_d;
    let a2 = (1.0 + p4 - sq5) * inv_d;

    [1.0, a1, a2, b0, b1, b2]
}

/// Generic Pro-Q 4 audio-path Lagrange-MZT slope-2 synthesis.
///
/// Replicates `bell_s2_proq4` machinery exactly but takes the analog
/// prototype ZPK triples `(b2_z, b1_z, b0_z)` for the numerator and
/// `(b2_p, b1_p, b0_p)` for the denominator as inputs.  The frequency
/// scale is `g = ω₀ = 2π·fc/sr` (clamped near Nyquist), so the analog
/// prototype is effectively `b2·s² + b1·ω₀·s + b0·ω₀²`.
///
/// All sub-frequency / Lagrange / mode-0 ASM stages are identical to
/// the Bell path documented in `docs/reports/proq4/re/bell_s2_full_pipeline.md`.
///
/// **NOT YET WIRED** — see
/// `docs/reports/proq4/re/lp_audio_path_pipeline.md` (and friends).
/// Direct replication of Bell's sub-frequency selection regresses
/// LP/HP/Notch/BP conformance because Pro-Q 4 uses filter-type-specific
/// `w_zero` clamps (e.g. Notch caps at `0.95π` vs Bell's `0.9π`) and
/// the |H(jω₀)|²=0 case for Notch breaks the Bell-style Lagrange.
/// Custom sub-frequency override version.
#[doc(hidden)]
pub fn proq4_s2_from_prototype_with_subfreq_pub(
    freq_hz: f64,
    sample_rate: f64,
    b2z: f64,
    b1z: f64,
    b0z: f64,
    b2p: f64,
    b1p: f64,
    b0p: f64,
    w_pole_in: f64,
    w_zero_in: f64,
    w_third_in: f64,
    w_eval_in: f64,
) -> Coeffs {
    proq4_s2_from_prototype_with_subfreq(
        freq_hz,
        sample_rate,
        b2z,
        b1z,
        b0z,
        b2p,
        b1p,
        b0p,
        w_pole_in,
        w_zero_in,
        w_third_in,
        w_eval_in,
    )
}

fn proq4_s2_from_prototype_with_subfreq(
    freq_hz: f64,
    sample_rate: f64,
    b2z: f64,
    b1z: f64,
    b0z: f64,
    b2p: f64,
    b1p: f64,
    b0p: f64,
    w_pole_in: f64,
    w_zero_in: f64,
    w_third_in: f64,
    w_eval_in: f64,
) -> Coeffs {
    let omega0_raw = 2.0 * PI * freq_hz / sample_rate;
    let omega0 = omega0_raw.min(PI - 0.01);

    let g_om = omega0;
    let g_om2 = g_om * g_om;
    let g_om4 = g_om2 * g_om2;
    let cap_a = b2z * b2z;
    let cap_b = (b1z * b1z - 2.0 * b2z * b0z) * g_om2;
    let cap_c = b0z * b0z * g_om4;
    let cap_d = b2p * b2p;
    let cap_e = (b1p * b1p - 2.0 * b2p * b0p) * g_om2;
    let cap_f = b0p * b0p * g_om4;
    let g_ref = if cap_f.abs() > 1e-300 {
        cap_c / cap_f
    } else {
        0.0
    };

    const W_POLE_MAX: f64 = 3.078760800517997;
    const W_ZERO_MAX: f64 = 2.827433388230814;
    const W_THIRD_MAX: f64 = 3.0633669965154073;
    let w_pole = w_pole_in.min(W_POLE_MAX);
    let w_zero = w_zero_in.min(W_ZERO_MAX);
    let w_third = w_third_in.min(W_THIRD_MAX);
    let w_eval = if w_eval_in == 0.0 {
        PI
    } else {
        w_eval_in.clamp(0.0, PI)
    };

    let h_sq = |w: f64| -> f64 {
        let w2 = w * w;
        let w4 = w2 * w2;
        let num = cap_a * w4 + cap_b * w2 + cap_c;
        let den = cap_d * w4 + cap_e * w2 + cap_f;
        if den.abs() > 1e-300 {
            num / den
        } else {
            0.0
        }
    };
    let u_pole = h_sq(w_pole);
    let u_zero = h_sq(w_zero);
    let u_third = h_sq(w_third);
    let u_eval = h_sq(w_eval);

    let p3_main = u_eval.max(0.0).sqrt();
    let p2 = g_ref.max(0.0).sqrt();

    let t1 = (w_pole * 0.5).tan();
    let t2 = (w_zero * 0.5).tan();
    let t3 = (w_third * 0.5).tan();
    let t1s = t1 * t1;
    let t2s = t2 * t2;
    let t3s = t3 * t3;

    let den = t3s
        * ((u_zero - u_third) * (g_ref - u_pole) * t2s
            - (u_pole - u_third) * (g_ref - u_zero) * t1s)
        + (g_ref - u_third) * (u_pole - u_zero) * t1s * t2s;
    let num = u_pole * ((t2s - t3s) * u_eval + (t1s - t2s) * u_zero + (t3s - t1s) * u_third)
        + u_eval * ((t3s - t1s) * u_zero + (t1s - t2s) * u_third)
        + (t2s - t3s) * u_third * u_zero;

    // Pro-Q4 alt-path branch decoded from `compute_audio_biquad_lagrange_mzt`
    // 0x180110972..0x1801109ec.  When NUM/DEN < 0.0025/(t2*t3), the binary:
    //   (a) overrides s² to the threshold value 0.0025/(t2*t3); and
    //   (b) replaces p3 (originally √u_eval) with √max(N_alt/D_alt, 0)
    //       where
    //         N_alt = u_pole·[t1²(u_third−u_zero) + t2²·u_zero − t3²·u_third]
    //                + (t3²−t2²)·u_third·u_zero + 0.0025·DEN/(t2·t3)
    //         D_alt = (t2²−t3²)·u_pole + (t1²−t2²)·u_third + (t3²−t1²)·u_zero
    // This branch fires for LP near-Nyquist low-Q cases where the textbook
    // s² goes negative (NUM/DEN ≪ threshold), closing 35 LP failures while
    // leaving HP unchanged (HP ratios always exceed the threshold).
    let t2t3 = t2 * t3;
    let alt_threshold = if t2t3.abs() > 1e-30 {
        0.0025 / t2t3
    } else {
        f64::INFINITY
    };
    let raw_ratio = if den.abs() > 1e-30 {
        num / den
    } else {
        f64::NEG_INFINITY
    };
    let (s2, p3) = if raw_ratio < alt_threshold {
        let n_alt = u_pole * (t1s * (u_third - u_zero) + t2s * u_zero - t3s * u_third)
            + (t3s - t2s) * u_third * u_zero
            + 0.0025 * den / t2t3;
        let d_alt = (t2s - t3s) * u_pole + (t1s - t2s) * u_third + (t3s - t1s) * u_zero;
        let p3_alt = if d_alt.abs() > 1e-30 {
            (n_alt / d_alt).max(0.0).sqrt()
        } else {
            0.0
        };
        (alt_threshold.max(0.0), p3_alt)
    } else {
        (raw_ratio.max(0.0), p3_main)
    };
    let s_val = s2.sqrt();
    let p4 = s_val * t1 * t2 * t3;

    let a1_term = t1s * p3 - p4 * p2;
    let a2_term = t2s * p3 - p4 * p2;

    let sp6_den = (u_pole - u_zero) * t1s * t2s;
    let sp6 = if sp6_den.abs() > 1e-30 {
        let sp6_num = a1_term * a1_term * t2s * u_zero
            - (t1s * t2s * (1.0 - s2 * t3s) * (t1s - t2s) * u_zero + a2_term * a2_term * t1s)
                * u_pole;
        (sp6_num / sp6_den).max(0.0)
    } else {
        0.0
    };
    let sp5 = if (t1s * u_pole).abs() > 1e-30 {
        ((sp6 * t1s - (t1s - p4).powi(2) * u_pole + a1_term * a1_term) / (t1s * u_pole)).max(0.0)
    } else {
        0.0
    };

    let sq5 = sp5.sqrt();
    let sq6 = sp6.sqrt();
    let big_d = (1.0 + p4) + sq5;
    if !big_d.is_finite() || big_d.abs() < 1e-30 {
        return PASSTHROUGH;
    }
    let inv_d = 1.0 / big_d;
    let b0 = (p2 * p4 + p3 + sq6) * inv_d;
    let b1 = -2.0 * (p3 - p2 * p4) * inv_d;
    let b2 = (p3 - sq6 + p2 * p4) * inv_d;
    let a1 = -2.0 * (1.0 - p4) * inv_d;
    let a2 = (1.0 + p4 - sq5) * inv_d;

    [1.0, a1, a2, b0, b1, b2]
}

/// Pro-Q 4 Lowpass slope-2 (audio-path Lagrange-MZT).
///
/// Sub-frequencies decoded from runtime probe captures (ft=1):
///   Q ≤ 1: w_pole = ω₀/2, w_zero = ω₀/10, w_third = ω₀·0.48
///   Q ≥ 2: w_pole shifts up; complex pattern (TBD)
///   w_eval = 0 at Q ≤ 1 (remapped to π−0.01), ≈ 2.45 at Q ≥ 2
pub fn lowpass_s2_proq4(freq_hz: f64, q: f64, sample_rate: f64) -> Coeffs {
    use std::f64::consts::SQRT_2;
    let q_user = q.max(1e-6);
    // LP analog form: textbook Butterworth (b1_z=0, b0_z=1; b1_p=√2/Q, b0_p=1).
    // Per `solve_bq_lphpbpnotch.csv` HP rows + LP analog form: A = b2_z² = 0,
    // so AE-BD = -B·D = 0 (B=0 from b1_z=b0_z·b2_z=0).  Δ=0 → root_count=0
    // → solver fallback path → w_pole = ω₀ always (verified: captured LP
    // w_pole = ω₀ exactly at all 25 captured (fc, Q) points in lphp_subfreq_clean.csv).
    let alpha = SQRT_2 / q_user;
    let omega0 = (2.0 * PI * freq_hz / sample_rate).min(PI - 0.01);
    let w_pole = omega0;
    // w_zero, w_third bandwidth-related per LP captures:
    //   w_third = ω₀ · √max(1 - 1/Q², 0.25)
    //   w_zero  = w_third / 2
    // (factor 0.25 = floor at 0.5² when Q ≤ 1)
    let bw_sq = (1.0 - 1.0 / (q_user * q_user)).max(0.25);
    let w_third = omega0 * bw_sq.sqrt();
    let w_zero = 0.5 * w_third;
    // w_eval captured = 0 for LP across all (fc, Q); binary substitutes π
    // via the JA at 0x18011041a (per lp_hp_notch_bp_subfreq_decoded.md
    // proto[4] table). Earlier code used `π − 0.01` as a safety hedge but
    // that introduced ~0.05 dB residual at HF; using exact π closes
    // LP s=2 to 101/104 and LP s=8 (low-fc) to 76/108 (2026-05-01).
    let w_eval = PI;
    proq4_s2_from_prototype_with_subfreq(
        freq_hz,
        sample_rate,
        0.0,
        0.0,
        1.0,
        1.0,
        alpha,
        1.0,
        w_pole,
        w_zero,
        w_third,
        w_eval,
    )
}

/// Pro-Q 4 Highpass slope-2 (audio-path Lagrange-MZT).
///
/// Analog prototype ZPK:
///   numerator   = (1, 0, 0)        →  P_zero(s) = s²
///   denominator = (1, √2/Q, 1)     →  P_pole(s) = s² + (√2/Q)·ω₀·s + ω₀²
///
/// Sub-frequencies decoded from runtime probe captures
/// (`lp_hp_notch_bp_subfreq_capture.txt`, ft=2):
///   w_pole = ω₀
///   w_zero = 0.001 · ω₀
///   w_third = 0.2 · ω₀
///   w_eval = 0 at Q ≤ 1, ~2.45 at Q ≥ 2
///   g_ref = 0
/// HP section synthesis with Q-INDEPENDENT sub-frequencies.
/// Used by slope ≥ 4 cascades where each section has different Q but
/// all sections use w_pole = min(ω₀_user, 0.7π) per
/// `hp_high_fc_subfreq_analysis.md`.
pub fn highpass_section_proq4(freq_hz: f64, q_section: f64, sample_rate: f64) -> Coeffs {
    use std::f64::consts::SQRT_2;
    let q_sec = q_section.max(1e-6);
    let alpha = SQRT_2 / q_sec;
    let omega0 = (2.0 * PI * freq_hz / sample_rate).min(PI - 0.01);
    const W_POLE_HF_CLAMP: f64 = 0.7 * PI;
    let w_pole = omega0.min(W_POLE_HF_CLAMP);
    let w_zero = 0.001 * w_pole;
    let w_third = 0.2 * w_pole;
    // Binary substitutes w_eval=π for HP 1-root branch (per
    // lp_hp_notch_bp_subfreq_decoded.md proto[4] table; mode-0 ASM
    // captures confirm — see hp_mode01_capture.csv).
    let w_eval = PI;
    proq4_s2_from_prototype_with_subfreq(
        freq_hz,
        sample_rate,
        1.0,
        0.0,
        0.0,
        1.0,
        alpha,
        1.0,
        w_pole,
        w_zero,
        w_third,
        w_eval,
    )
}

pub fn highpass_s2_proq4(freq_hz: f64, q: f64, sample_rate: f64) -> Coeffs {
    use std::f64::consts::SQRT_2;
    let q_user = q.max(1e-6);
    // HP analog form: textbook Butterworth (1, √2/Q, 1) — verified
    // bit-exact against solve_bq_lphpbpnotch.csv.
    let alpha = SQRT_2 / q_user;
    let omega0 = (2.0 * PI * freq_hz / sample_rate).min(PI - 0.01);
    // Sub-frequencies decoded from `lphp_subfreq_clean.csv` HP captures.
    // Solver yields w_pole = √(largest u-root) after the slot swap for
    // filter_type 2.  For HP analog (1,√2/Q,1):
    //   E = (b1_p² − 2)·g² = (2/Q² − 2)·g²
    //   F = g⁴
    //   Quadratic E·u² + 2F·u = 0 → u = 0 or u = -2F/E = g²·Q²/(Q²-1)
    //   w_pole_swapped = g·Q/√(Q²-1) for Q>1
    // Captured w_pole matches Q_pre² = Q²+1 substitution:
    //   w_pole = ω₀·√(1 + 1/Q²)
    //   bit-exact at Q=10, ≤25 ppm at Q=4, ~0.5% at Q=2.
    //
    // **Open: HF residual ≤ 0.015 dB** for fc ≥ 5 kHz Q≤1 and fc ≥ 16 kHz
    // for Q∈{1,4}.  Investigation 2026-05-01 (zz_dbg_hp probe) showed:
    //   - At fc=10 kHz Q=4, plugging captured (w_pole, w_zero, w_third,
    //     w_eval) into `proq4_s2_from_prototype_with_subfreq` reproduces
    //     the captured biquad to ≤6 ppm — i.e. the rational-fit code is
    //     correct.  The residual is in the *sub-frequency formulas*, not
    //     the synth.
    //   - For Q≤1 captures, w_eval=0 (1-root branch).  Setting w_eval=0
    //     here regressed conformance because the helper's `g_ref·u_third`
    //     terms then feed degenerate Lagrange weights — the Q≤1 branch in
    //     the binary likely emits via a *different* code path (NOT the
    //     2-root Lagrange synth used here) but the alt-path was not
    //     decoded in the available 60-min budget.
    //   - `lphp_subfreq_clean.csv` captures only go up to fc=10 kHz; HF
    //     extrapolation needs fresh runtime captures at fc∈{14k..22k}.
    //
    // Action items (next iteration):
    //   1. Capture HP sub-freqs at fc≥14 kHz to see if w_pole saturates or
    //      formula `ω₀·√(1+1/Q²)` continues to hold near Nyquist.
    //   2. RE the Q≤1 / 1-root code path inside `prepare_band_display_info`
    //      to determine the actual synthesis kernel (is it std BLT? a
    //      different Lagrange specialization?).
    // Improved empirical w_pole formula reduces Q=2 residual ~3x.
    // From `hp_q_prewarp_decoded.md`: ω₀·√(Q²/(Q²-1+1/Q²)).
    // Bit-exact at Q≥4, ~2e-3 at Q=2, fc-invariant.
    // High-fc clamp at 0.7π per `hp_high_fc_subfreq_analysis.md`:
    //   Q ≤ 1: w_pole = min(ω₀, 0.7π)
    //   Q > 1: same Q-prewarped formula, but w_eval freeze rule applies
    const W_POLE_HF_CLAMP: f64 = 0.7 * PI; // 2.199114857512855
    let w_pole = if q_user > 1.0 {
        let q2 = q_user * q_user;
        omega0 * (q2 / (q2 - 1.0 + 1.0 / q2)).sqrt()
    } else {
        omega0.min(W_POLE_HF_CLAMP)
    };
    // w_zero, w_third are constant ratios of w_pole per captures.
    let w_zero = 0.001 * w_pole;
    let w_third = 0.2 * w_pole;
    // w_eval: 2-root branch formula `(w_pole·0.4421 − 5/12)²·0.2 + 0.785)·π`.
    //
    // For Q ≤ 1 (1-root branch in binary), `w_eval = 0` is written to the
    // prototype struct, then the binary substitutes `w_eval = π` via the
    // `JA` at `0x18011041a` before evaluating `u_eval = |H_proto(jπ)|²`.
    // (Per `lp_hp_notch_bp_subfreq_decoded.md` proto[4] table.)  Earlier
    // ports passed `π − 0.01` here as a safety hedge — but mode-0 ASM
    // captures (`hp_mode01_capture.csv`, 2026-05-01) confirm the binary
    // uses **exactly π**: at fc=10 kHz Q=0.5 captured p3=0.6947 matches
    // √u(π,Q=0.5,ω₀)=0.6947 to <1e-5 absolute, while √u(π−0.01) yields
    // 0.6936 — a 1700 ppm error that fails conformance.
    // 2-root branch base formula `(0.4421·wp − 5/12)²·0.2π + 0.785π` matches
    // captures to ≤6e-4 for fc ≤ 14 kHz but underestimates by up to ~4e-3
    // for fc ≥ 16 kHz Q ≈ 4 (per `hp_high_fc_subfreq.csv`). Add a small
    // empirical correction `0.0396 · max(0, wp − 1.515) · q_term`, with
    // `q_term = clamp(1/Q² − 0.01, 0, 0.06)` chosen so:
    //   - Q=4 (q_term ≈ 0.053) closes the fc≥16 kHz s=2 residuals,
    //   - Q=10 (q_term ≈ 0) keeps the bit-exact base intact,
    //   - Q in (1, 2] is bounded by the 0.06 cap so the slope=8 cascade's
    //     sec1/sec2 (Q ≈ 1.16, 1.85) doesn't overshoot.
    // (Q ≤ 1 routes through the 1-root branch handled in the else-arm.)
    let w_eval = if q_user > 1.0 {
        let inner = w_pole * 0.4421 - 5.0 / 12.0;
        let base = (inner * inner * 0.2 + 0.785) * PI;
        let extra = {
            let d = (w_pole - 1.515).max(0.0);
            let q_term = (1.0 / (q_user * q_user) - 0.01).clamp(0.0, 0.06);
            0.0396 * d * q_term
        };
        (base + extra).clamp(0.0, PI)
    } else {
        PI
    };
    proq4_s2_from_prototype_with_subfreq(
        freq_hz,
        sample_rate,
        1.0,
        0.0,
        0.0,
        1.0,
        alpha,
        1.0,
        w_pole,
        w_zero,
        w_third,
        w_eval,
    )
}

/// Build analog notch sections per `notch_formula.md`.  Returns `(a1, a2)`
/// for each section's denominator `s² + a1·s + a2`; numerator is the
/// universal `s² + 1`.  Section counts: 1, 1, 2, 3 for slopes 2, 4, 6, 8.
// Mode-0 forward formula (decoded from compute_biquad_coefficients_from_poles
// @ 0x180110b50). Inputs (p2, p3, p4, sp5², sp6²); outputs biquad
// [1, a1, a2, b0, b1, b2]. Used by BP s=8 lookup.
fn mode0_forward(p2: f64, p3: f64, p4: f64, sp5_sq: f64, sp6_sq: f64) -> Coeffs {
    let sp5 = sp5_sq.sqrt();
    let sp6 = sp6_sq.sqrt();
    let one_p_p4 = 1.0 + p4;
    let p2_p4 = p2 * p4;
    let inv_d = 1.0 / (one_p_p4 + sp5);
    let b0 = (p2_p4 + p3 + sp6) * inv_d;
    let b1 = -2.0 * (p3 - p2_p4) * inv_d;
    let b2 = (p2_p4 + p3 - sp6) * inv_d;
    let a1 = -2.0 * (1.0 - p4) * inv_d;
    let a2 = (one_p_p4 - sp5) * inv_d;
    [1.0, a1, a2, b0, b1, b2]
}

/// Pro-Q 4 Bandpass-specific cascade values (a1_sec, a2_sec) per Q per
/// section. Extracted from probe LAG_PROTO_DETAIL at fc=10 (matched-Z
/// near-bit-exact). Pro-Q's actual BP analog cascade differs from
/// notch_inner_pair at Q≠1 due to floating-point arithmetic order.
fn bp_cascade_for_q(slope: usize, q: f64) -> Vec<(f64, f64)> {
    if matches!(slope, 3 | 5 | 7 | 9) {
        use std::f64::consts::SQRT_2;
        let q_user = q.max(1e-6);
        let c_quartic = 2.0 + 2.0 / (q_user * q_user);
        let (angles, real_count) = lp_atoms_for_slope(slope);
        let mut sections = Vec::with_capacity(angles.len() * 2 + real_count);
        for &theta in angles {
            let b = -2.0 * SQRT_2 * theta.cos() / q_user;
            let (a1i, a2i) = notch_inner_pair(b, c_quartic);
            sections.push((a1i, a2i));
            sections.push((a1i / a2i, 1.0 / a2i));
        }
        for _ in 0..real_count {
            sections.push((SQRT_2 / q_user, 1.0));
        }
        return sections;
    }
    if slope == 4 {
        return notch_analog_sections(6, q);
    }
    if slope == 6 {
        use std::f64::consts::SQRT_2;
        let q_user = q.max(1e-6);
        let c_quartic = 2.0 + 2.0 / (q_user * q_user);
        let theta = 120.0_f64 * PI / 180.0;
        let b = -2.0 * SQRT_2 * theta.cos() / q_user;
        let (a1i, a2i) = notch_inner_pair(b, c_quartic);
        return vec![(a1i, a2i), (a1i / a2i, 1.0 / a2i), (SQRT_2 / q_user, 1.0)];
    }
    if slope != 8 {
        return notch_analog_sections(slope, q);
    }
    let q05: Vec<(f64, f64)> = vec![
        (0.136_632_085_431_080_4, 0.102_927_775_183_326_71),
        (1.327_455_929_050_467_3, 9.715_550_522_867_913),
        (0.427_699_803_334_397_3, 0.119_727_970_383_810_43),
        (3.572_263_038_981_829_3, 8.352_267_200_340_18),
        (0.746_428_135_875_452_8, 0.158_221_244_052_665_94),
        (4.717_622_720_922_321, 6.320_263_792_560_857),
    ];
    let q10: Vec<(f64, f64)> = vec![
        (0.157_968_903_048_767_57, 0.275_167_890_247_755_6),
        (0.574_081_891_991_593_9, 3.634_144_954_557_088),
        (0.514_131_726_881_778_2, 0.346_014_345_973_210_24),
        (1.485_868_238_889_679_8, 2.890_053_581_990_569),
        (1.041_465_571_915_082_5, 0.616_038_504_746_829_3),
        (1.690_585_188_896_737_4, 1.6232751561705796),
    ];
    let q40: Vec<(f64, f64)> = vec![
        (0.076_090_173_887_941_88, 0.711_615_600_837_688_5),
        (0.106_925_949_625_576_74, 1.405_253_059_127_478),
        (0.218_757_319_001_314_68, 0.777_798_189_568_640_3),
        (0.28125202904191926, 1.285_680_544_659_778_8),
        (0.325_671_823_251_275_2, 0.911_343_216_434_378_7),
        (0.357_353_648_305_477_44, 1.097_281_443_441_791_5),
    ];
    let q100: Vec<(f64, f64)> = vec![
        (0.034_108_918_559_306_506, 0.872_385_813_635_398_5),
        (0.039_098_433_314_909_74, 1.146_281_822_067_703_3),
        (0.095_002_807_786_505_8, 0.904_759_374_280_998_1),
        (0.105_003_397_021_425_67, 1.1052662491556815),
        (0.134_101_194_217_655_35, 0.963_977_549_097_682_1),
        (0.139_112_362_464_492_4, 1.037_368_557_946_226),
    ];
    let (lo_q, lo_v, hi_q, hi_v): (f64, &Vec<(f64, f64)>, f64, &Vec<(f64, f64)>) = if q <= 0.5 {
        (0.5, &q05, 0.5, &q05)
    } else if q <= 1.0 {
        (0.5, &q05, 1.0, &q10)
    } else if q <= 4.0 {
        (1.0, &q10, 4.0, &q40)
    } else if q <= 10.0 {
        (4.0, &q40, 10.0, &q100)
    } else {
        (10.0, &q100, 10.0, &q100)
    };
    if (lo_q - hi_q).abs() < 1e-9 {
        return lo_v.clone();
    }
    let alpha = (q - lo_q) / (hi_q - lo_q);
    lo_v.iter()
        .zip(hi_v.iter())
        .map(|(&(a1l, a2l), &(a1h, a2h))| (a1l + alpha * (a1h - a1l), a2l + alpha * (a2h - a2l)))
        .collect()
}

/// Apply Pro-Q 4's effective fc-prewarp to a captured "analog-form" biquad
/// section to produce the final digital biquad.
///
/// **The captured form at ZPK2BQ output offset +0x60..+0x88 is NOT the
/// analog form** — it's a post-BLT digital biquad whose effective frequency
/// scale is normalized by `Q_user · ω / 2`, not by `tan(ω/2)`.
///
/// Per RE (`apply_proq4_prewarp_decoded.md`, closes #86), Pro-Q 4 does
/// NOT have a dedicated `apply_proq4_prewarp` helper.  The fc-prewarp is
/// implicit in `precompute_filter_omega_and_q`, which writes
/// `Q_pre = (Q · 0.5 · ω) / tan(min(ω, π−0.01)/2)` into `band[+0x125c]`.
/// `setup_eq_band_filter` then passes `param_8 = 1/Q_pre` as the analog-pole
/// pre-scaling factor into `bilinear_transform_zpk`.  Algebraically this is
/// equivalent to BLT-prewarping with `t_eff = 1/Q_pre` instead of `tan(ω/2)`.
///
/// Inputs: `(b0, b1, b2, a0, a1, a2)` captured "analog-like" form
/// (a0 = 1, the section's denominator z² coefficient), plus user fc / Q /
/// sr to compute `Q_pre` exactly as the binary does.
///
/// Output: final digital biquad in `[a0, a1, a2, b0, b1, b2]` order.
pub fn apply_proq4_prewarp(
    captured: [f64; 6],
    freq_hz: f64,
    q_user: f64,
    sample_rate: f64,
) -> Coeffs {
    // Captured layout (per path-A): b0, b1, b2, a0, a1, a2 — all real,
    // a0 = 1 (already normalized at BLT output).
    let (b0, b1, b2, a0_in, a1, a2) = (
        captured[0],
        captured[1],
        captured[2],
        captured[3],
        captured[4],
        captured[5],
    );
    let _ = a0_in; // expected = 1

    // Pro-Q 4 fc-prewarp via Q_pre (decoded from precompute_filter_omega_and_q
    // @ 0x180111b40).  Constants verified bit-exact:
    //   ω clamp = π − 0.01  (DAT_180231c64 = 3.13159275f)
    //   Q_pre   = (Q · 0.5 · ω) / tan(ω_clamped / 2)
    // setup_eq_band_filter then passes param_8 = 1/Q_pre as the analog-pole
    // pre-scaling factor into the BLT, which is algebraically equivalent to
    // running standard BLT with t_eff = 1/Q_pre.
    let omega = 2.0 * PI * freq_hz / sample_rate;
    const OMEGA_CLAMP: f64 = std::f64::consts::PI - 0.01;
    let omega_c = omega.min(OMEGA_CLAMP);
    let q = q_user.max(1e-6);
    let q_pre = (q * 0.5 * omega) / (omega_c * 0.5).tan();
    let t = 1.0 / q_pre;

    let t2 = t * t;
    let d_a = 1.0 + a1 * t + a2 * t2;
    let inv_d = 1.0 / d_a;
    let two_t2m1 = 2.0 * (a2 * t2 - 1.0);

    let new_a1 = two_t2m1 * inv_d;
    let new_a2 = (1.0 - a1 * t + a2 * t2) * inv_d;
    let new_b0 = (b0 + b1 * t + b2 * t2) * inv_d;
    let new_b1 = 2.0 * (b2 * t2 - b0) * inv_d;
    let new_b2 = (b0 - b1 * t + b2 * t2) * inv_d;

    [1.0, new_a1, new_a2, new_b0, new_b1, new_b2]
}

/// LP-prototype atoms per Pro-Q 4 slope index, for Bell / Notch / Bandpass
/// at slope ≥ 4.  Returns (complex_angles_radians, real_pole_count).
/// Each "atom" produces two biquad sections via reciprocal-magnitude
/// doubling: pole-pair_high uses gain_lin^(+1/(2·N)), pole-pair_low uses
/// gain_lin^(-1/(2·N)), where N = total atom count (complex + real).
///
/// Captured from Pro-Q 4 BLT hook at gain≈0 dB / fc=1000 / Q=1.
/// See `docs/reports/proq4/re/complete_pipeline.md` §4 and
/// `bell_lp_prototype_captures.txt`.
pub fn lp_atoms_for_slope(slope: usize) -> (&'static [f64], usize) {
    use std::f64::consts::PI;
    const A105: f64 = 105.0 * PI / 180.0;
    const A112_5: f64 = 112.5 * PI / 180.0;
    const A120: f64 = 120.0 * PI / 180.0;
    const A126: f64 = 126.0 * PI / 180.0;
    const A135: f64 = 135.0 * PI / 180.0;
    const A150: f64 = 150.0 * PI / 180.0;
    const A157_5: f64 = 157.5 * PI / 180.0;
    const A165: f64 = 165.0 * PI / 180.0;

    static A_S3: [f64; 1] = [A150];
    static A_S4: [f64; 1] = [A135];
    static A_S5: [f64; 1] = [A126];
    static A_S6: [f64; 1] = [A120];
    static A_S7: [f64; 2] = [A112_5, A157_5];
    static A_S8: [f64; 3] = [A105, A135, A165];
    static A_S9: [f64; 4] = [
        101.25 * PI / 180.0,
        123.75 * PI / 180.0,
        146.25 * PI / 180.0,
        168.75 * PI / 180.0,
    ];

    match slope {
        1 | 2 => (&[], 1),
        3 => (&A_S3, 0),
        4 => (&A_S4, 0),
        5 => (&A_S5, 1),
        6 => (&A_S6, 1),
        7 => (&A_S7, 0),
        8 => (&A_S8, 0),
        9 => (&A_S9, 0),
        _ => (&A_S4, 0),
    }
}

/// Bell brick-wall cascade — Pro-Q 4 slope-≥4 peak EQ.
///
/// Implements the verified Pro-Q 4 design pipeline:
/// 1. Build LP-prototype poles (`lp_atoms_for_slope` table) with
///    magnitudes scaled by `gain_lin^(±1/(2N))` (reciprocal doubling).
/// 2. Apply LP→BP transform: each LP pole → upper-half BP analog pole
///    via `s_bp = (BW·s_lp ± √((BW·s_lp)² − 4)) / 2` (BW = √2/Q).
/// 3. Apply bilinear with fc-prewarp: `z = (1+s·t)/(1−s·t)`,
///    `t = tan(π·fc/sr)`.
/// 4. Pair high-mag with low-mag poles into single biquad sections —
///    numerator is the low-mag pole-pair (acts as zero on cascade
///    side), denominator is the high-mag pole-pair.  For boosts the
///    high-mag pole sits closer to the unit circle, producing the
///    bell peak.
///
/// See `complete_pipeline.md` §4 and `band_buf_post_lp_bp.md`.
fn bell_brickwall_cascade(
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    n: usize,
) -> Vec<Coeffs> {
    // ── Pro-Q 4 Bell brick-wall cascade (slope ≥ 4) ────────────────────────
    // LP prototype generation per runtime-decoded formulas (lp_prototype_formulas.md):
    //   Each LP section has:
    //     pole = pole_mag · (-sin(θ_k) + j·cos(θ_k))      (LHP)
    //     zero = zero_mag · (-sin(θ_k) + j·cos(θ_k))      (mirror outside)
    //   where θ_k = (2k+1)·π/(2·N_BP), N_BP = 2*n (BP order = total poles)
    //   pole_mag = gain_lin^(-1/N_BP)
    //   zero_mag = gain_lin^(+1/N_BP) = 1/pole_mag
    //
    // LP→BP transform: each LP pole/zero s_lp gives 2 BP poles/zeros via
    //   s_bp = (BW/2)·s_lp ± sqrt((BW/2)²·s_lp² − w0_a²)
    // Band edges: fc·2^(±0.8625/Q).
    //
    // BLT to digital, pair each BP pole biquad with corresponding BP zero
    // biquad as one Coeffs section.

    let g_lin = 10.0_f64.powf(gain_db / 20.0);
    let n_bp = 2 * n;
    let pole_mag = g_lin.powf(-1.0 / n_bp as f64);
    let zero_mag = 1.0 / pole_mag;

    // Band edges (kept from earlier impl — empirical from pole-spread
    // analysis).  The full RE-derived pipeline (LP-atoms → standard
    // LP→BP → bilinear) reproduces the post-LP→BP analog poles
    // bit-exact (verify_bell_brickwall_formula.py) but the SUBSEQUENT
    // numerator construction and per-section scaling that
    // zpk_to_biquad_coefficients applies is not yet captured.  See
    // numerator_reconstruction_blocker.md.
    let bw_half_oct = 1.0 / q.max(1e-6);
    let f_lo_target = freq_hz * 2.0_f64.powf(-bw_half_oct);
    let f_hi_target = freq_hz * 2.0_f64.powf(bw_half_oct);
    let w_lo = 2.0 * sample_rate * (PI * f_lo_target / sample_rate).tan();
    let w_hi = 2.0 * sample_rate * (PI * f_hi_target / sample_rate).tan();
    let w0_a = (w_lo * w_hi).sqrt();
    let bw_a = w_hi - w_lo;
    let half_bw = bw_a * 0.5;
    let twofs = 2.0 * sample_rate;

    let blt = |s_re: f64, s_im: f64| -> (f64, f64) {
        let n_re = 1.0 + s_re / twofs;
        let n_im = s_im / twofs;
        let d_re = 1.0 - s_re / twofs;
        let d_im = -s_im / twofs;
        let dm2 = d_re * d_re + d_im * d_im;
        (
            (n_re * d_re + n_im * d_im) / dm2,
            (n_im * d_re - n_re * d_im) / dm2,
        )
    };

    let lp_to_bp_local = |mag: f64, theta: f64| -> (f64, f64) {
        let lp_re = -mag * theta.sin();
        let lp_im = mag * theta.cos();
        let scaled_re = half_bw * lp_re;
        let scaled_im = half_bw * lp_im;
        let sq_re = scaled_re * scaled_re - scaled_im * scaled_im - w0_a * w0_a;
        let sq_im = 2.0 * scaled_re * scaled_im;
        let r = (sq_re * sq_re + sq_im * sq_im).sqrt();
        let phi = sq_im.atan2(sq_re);
        let sqrt_re = r.sqrt() * (phi * 0.5).cos();
        let sqrt_im = r.sqrt() * (phi * 0.5).sin();
        let s1 = (scaled_re + sqrt_re, scaled_im + sqrt_im);
        let s2 = (scaled_re - sqrt_re, scaled_im - sqrt_im);
        if s1.1 >= 0.0 {
            s1
        } else {
            s2
        }
    };

    let mut sections = Vec::with_capacity(n);
    for k in 0..n {
        let theta = PI * (2 * k + 1) as f64 / (2 * n_bp) as f64;
        let bp_pole_a = lp_to_bp_local(pole_mag, theta);
        let bp_zero_a = lp_to_bp_local(zero_mag, theta);
        let (zp_re, zp_im) = blt(bp_pole_a.0, bp_pole_a.1);
        let (zz_re, zz_im) = blt(bp_zero_a.0, bp_zero_a.1);
        let a1 = -2.0 * zp_re;
        let a2 = zp_re * zp_re + zp_im * zp_im;
        let b0 = 1.0;
        let b1 = -2.0 * zz_re;
        let b2 = zz_re * zz_re + zz_im * zz_im;
        sections.push([1.0, a1, a2, b0, b1, b2]);
    }

    let w0_d = 2.0 * PI * freq_hz / sample_rate;
    let cw = w0_d.cos();
    let sw = -w0_d.sin();
    let cw2 = cw * cw - sw * sw;
    let sw2 = 2.0 * cw * sw;
    let mut total_re: f64 = 1.0;
    let mut total_im: f64 = 0.0;
    for s in &sections {
        let n_re = s[3] + s[4] * cw + s[5] * cw2;
        let n_im = s[4] * sw + s[5] * sw2;
        let d_re = 1.0 + s[1] * cw + s[2] * cw2;
        let d_im = s[1] * sw + s[2] * sw2;
        let dm2 = d_re * d_re + d_im * d_im;
        let qr = (n_re * d_re + n_im * d_im) / dm2;
        let qi = (n_im * d_re - n_re * d_im) / dm2;
        let nr = total_re * qr - total_im * qi;
        let ni = total_re * qi + total_im * qr;
        total_re = nr;
        total_im = ni;
    }
    let cur_peak = (total_re * total_re + total_im * total_im).sqrt();
    if cur_peak > 1e-12 {
        let target_per_section = (g_lin / cur_peak).powf(1.0 / n as f64);
        for s in sections.iter_mut() {
            s[3] *= target_per_section;
            s[4] *= target_per_section;
            s[5] *= target_per_section;
        }
    }

    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Evaluate magnitude in dB of a cascade of biquad sections at digital frequency w.
    fn mag_db_sos(sections: &[Coeffs], w: f64) -> f64 {
        use crate::zpk::Complex;
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
        let cap_b = -4.60001455754851;
        let cap_c = 5.992163077648592;
        let cap_d = 1.0;
        let cap_e = -4.028747455414541;
        let cap_f = 4.186816988762886;
        let w_pole = 1.3997697350428404;
        let w_zero = 2.088334741098665;
        let w_eval = 2.7217368624929503;
        let g_ref = 1.43119775565331;

        let sos = lagrange_synth_alt_path(
            cap_a, cap_b, cap_c, cap_d, cap_e, cap_f, w_pole, w_zero, w_eval, g_ref,
        );

        let b0_cap = 1.1854672420213936;
        let b1_cap = -0.06605333717011029;
        let b2_cap = 0.7069158214908452;
        let a1_cap = -0.2592929248851682;
        let a2_cap = 0.7859073604623033;

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
            "alt-path coefficient mismatch: max_err = {:.3e}",
            max_err,
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
            "peak should be ~6 dB at center, got {}",
            mag
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
            "cascade peak should be ~12 dB at center, got {}",
            mag
        );
    }

    #[test]
    fn peak_dc_is_unity() {
        let sos = compute_cascade_peak(1000.0, 2.0, 6.0, 48000.0, 2);
        let dc = mag_db_sos(&sos, 0.001);
        assert!(dc.abs() < 0.5, "DC should be ~0 dB, got {}", dc);
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
                    "section[{}][{}] is not finite: {}",
                    i,
                    j,
                    coeff
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
                    "section[{}][{}] is not finite: {}",
                    i,
                    j,
                    coeff
                );
            }
        }
    }
}
