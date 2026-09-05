//! The brick-wall bell cascade — Pro-Q 4's steepest peak slopes.
//!
//! `bell_brickwall_proq4` is the largest single function in the crate, at just
//! over 1100 lines, and it is left whole deliberately. It decodes one
//! contiguous routine in the binary, and its commentary cites the captured
//! rows each branch was verified bit-exact against — cutting it into pieces
//! would separate the arithmetic from its evidence. Splitting it wants the
//! conformance captures in `tests/reference`, not a refactor.

use dsp_core::num;

use super::{bell_s2_proq4, lagrange_synth_alt_path, trace_bell_inputs, Coeffs, PASSTHROUGH, PI};

#[expect(
    clippy::too_many_lines,
    reason = "a decoded routine: one contiguous function in the binary, whose commentary cites the captured rows each branch was verified against. Splitting it would separate the arithmetic from its evidence"
)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "complex/float arithmetic — `Complex` is two `f64`s, so its operators cannot panic or overflow; the lint cannot see through an operator overload"
)]
pub fn bell_brickwall_proq4(
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    n_sections: usize,
    slope_idx: Option<usize>,
) -> Vec<Coeffs> {
    const W_POLE_BUCKETB_MAX: f64 = 3.135_309_468_282_613_5;
    use crate::math::zpk::Complex;
    use dsp_core::num;
    use std::f64::consts::SQRT_2;

    #[derive(Clone, Copy)]
    enum HiCorner {
        Snap,
        PeakTimesQ,
        ScaleByOmega(f64),
    }

    let q_user = q.max(1e-6);
    let gain_lin = 10.0_f64.powf(gain_db / 20.0);

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
        gain_lin.powf(1.0 / (2.0 * num::count_to_f64(n_lp)))
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
        let (lo, hi_quad) = if r1.mag() < r2.mag() {
            (r1, r2)
        } else {
            (r2, r1)
        };
        // hi = 1 / conj(lo) = lo / |lo|²  (so |hi| = 1/|lo|, arg(hi) = arg(lo))
        let mag_sq = lo.mag_sq();
        let hi = if mag_sq > 0.0 {
            Complex::new(lo.re / mag_sq, lo.im / mag_sq)
        } else {
            hi_quad
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
        let last_q = TABLE.last().map_or(0.0, |t| t.0);
        if q < TABLE[0].0 - 1e-12 || q > last_q + 1e-12 {
            return None;
        }
        // exact bin or linear interpolate (extend nearest-neighbor at
        // endpoints — table is dense enough for the captured Qs).
        for w in TABLE.windows(2) {
            let &[(q0, k0), (q1, k1)] = w else { continue };
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

    let hi_corner: Option<(usize, HiCorner)> =
        if matches!(slope_idx, Some(7..=9)) && n_sections >= 4 {
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
                    PI * 2.0f64.mul_add(num::count_to_f64(p), 1.0) / (2.0 * num::count_to_f64(n_lp))
                };
                let p_lp = Complex::new(-theta_lp.sin(), theta_lp.cos());
                let (_, bp_hi) = lp_to_bp(p_lp, b_pole);
                let bp_mag_sq = bp_hi.mag_sq();
                if bp_mag_sq > 1.0 + 1e-12 && bp_mag_sq < min_b0p {
                    min_b0p = bp_mag_sq;
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
            let last_entry = SLOPE3_ANGLE_TABLE.last().copied().unwrap_or((0.0, 0.0));
            let theta_deg = if g_abs <= SLOPE3_ANGLE_TABLE[0].0 {
                SLOPE3_ANGLE_TABLE[0].1
            } else if g_abs >= last_entry.0 {
                last_entry.1
            } else {
                let mut t = SLOPE3_ANGLE_TABLE[0].1;
                for w in SLOPE3_ANGLE_TABLE.windows(2) {
                    let &[(g0, t0), (g1, t1)] = w else { continue };
                    if g_abs >= g0 && g_abs <= g1 {
                        t = t0 + (t1 - t0) * (g_abs - g0) / (g1 - g0);
                        break;
                    }
                }
                t
            };
            theta_deg.to_radians()
        } else {
            PI * 2.0f64.mul_add(num::count_to_f64(pair_idx), 1.0) / (2.0 * num::count_to_f64(n_lp))
        };
        let p_lp = Complex::new(-theta_lp.sin(), theta_lp.cos());

        let (bp_pole_lo, bp_pole_hi) = lp_to_bp(p_lp, b_pole);
        let (bp_zero_lo, bp_zero_hi) = lp_to_bp(p_lp, b_zero);

        // bp_*_lo is lo (|s|<1), bp_*_hi is hi (|s|>1, reciprocal).
        // `inner = sec % 2 == 0` selects lo for even sections within a pair.
        let p_sec = if inner { bp_pole_lo } else { bp_pole_hi };
        let z_sec = if inner { bp_zero_lo } else { bp_zero_hi };

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
        let cap_b = b1_z.mul_add(b1_z, -2.0 * b0_z) * g_om2;
        let cap_c = b0_z * b0_z * g_om4;
        let cap_d = 1.0;
        let cap_e = b1_p.mul_add(b1_p, -2.0 * b0_p) * g_om2;
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
            lowq_k(q_user).map_or(0.0, |k| omega0 / k)
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
        let w_eval = if matches!(slope_idx, Some(3..=9)) {
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
        let is_bucket_b_multi = matches!(slope_idx, Some(7..=9));
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
            let wp_g12 = (1.033_563_234_6_f64 - 0.563_761_888_8).mul_add(t, 0.563_761_888_8);
            let wp_g6 = (1.032_662_948_8_f64 - 0.563_761_888_8).mul_add(t, 0.563_761_888_8);
            (wp_g12 - wp_g6).mul_add(((g_abs - 6.0) / 6.0).clamp(0.0, 1.0), wp_g6)
        } else {
            w_pole
        };
        // Anti-cramping: cap w_pole_i at the bucket-B Nyquist-adjacent
        // limit (3.1353094683 ≈ π·0.99800).  Captures show w_pole = const
        // across fc ∈ {15..22} kHz once the unwarped value exceeds the cap.
        // The synth's internal cap (now bucket-B values) is the binding
        // constraint for w_third; cap w_pole here so the alpha/beta
        // products below feed that synth with fc-invariant geometry.
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
            if matches!(slope_idx, Some(5 | 6)) && freq_hz >= 21000.0 {
                let t = ((freq_hz - 21000.0) / 1000.0).clamp(0.0, 1.0);
                if (q_user - 10.0).abs() < 1e-6 {
                    let w_zero_cap =
                        (2.508_442_944_5_f64 - 2.460_405_687_1).mul_add(t, 2.460_405_687_1);
                    let w_pole_cap =
                        (2.861_225_429_5_f64 - 2.734_469_335_3).mul_add(t, 2.734_469_335_3);
                    a = a.min(w_zero_cap / omega0);
                    b = b.min(w_pole_cap / omega0);
                } else if (q_user - 4.0).abs() < 1e-6 {
                    let w_zero_cap =
                        (2.076_469_439_3_f64 - 2.048_067_329_5).mul_add(t, 2.048_067_329_5);
                    let w_pole_cap =
                        (2.839_626_754_2_f64 - 2.713_852_417_4).mul_add(t, 2.713_852_417_4);
                    a = a.min(w_zero_cap / omega0);
                    b = b.min(w_pole_cap / omega0);
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
            let b_pole = SQRT_2 / (q_user * gp);
            let b_zero = SQRT_2 * gp / q_user;
            // b0p_c = b0z_c = 1
            let cb = (b_zero * b_zero - 2.0) * g_om2;
            let cc = g_om4;
            let ce = (b_pole * b_pole - 2.0) * g_om2;
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
                (0.3, 0.125_840, 0.282_470),
                (0.5, 0.349_551, 0.784_627),
                (0.7, 0.685_115, 1.537_856),
                (0.85, 1.010_189, 2.267_541),
                (1.0, 1.398_180, 3.133_742),
                (4.0, 2.457_401, 3.133_742),
                // Q=10 wz_cap is the captured cluster mean across Q=10
                // hi-section drift cells (FTS-EQ-w1h, 2026-05-05). Pro-Q
                // does not actually cap w_pole at Nyquist for Q=10 — this
                // entry is used only by the hi-section min(wp·α, wz_cap)
                // path, where it bounds the wz drift below α_std·π.
                (10.0, 2.72280, 3.135_309),
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
                        Some(2.469_602_088_4)
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
                        Some(2.412_624_449_8)
                    } else {
                        Some(2.236_624_624_0)
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
                            0.000_711_189_5f64.mul_add(-g_abs, 2.366_162_264_4),
                            0.000_788_026_3f64.mul_add(-g_abs, 2.475_888_663_3),
                            2.457_400_572_6,
                        ),
                        3 => (
                            0.000_262_395_3f64.mul_add(-g_abs, 2.251_426_935_5),
                            0.000_257_021_8f64.mul_add(-g_abs, 2.372_916_759_7),
                            2.457_400_572_6,
                        ),
                        5 => (
                            0.000_095_278_3f64.mul_add(-g_abs, 2.160_049_605_7),
                            0.000_100_413_3f64.mul_add(-g_abs, 2.279_836_775_6),
                            0.000_012_713_1f64.mul_add(-g_abs, 2.512_423_074_1),
                        ),
                        _ => (
                            2.047_834_437_3,
                            0.000_000_265_7f64.mul_add(-g_abs, 2.161_602_235_0),
                            0.000_011_031_0f64.mul_add(-g_abs, 2.454_902_032_8),
                        ),
                    };
                    if freq_hz < 19000.0 {
                        Some(
                            (cap_19k - cap_18k)
                                .mul_add(((freq_hz - 18000.0) / 1000.0).clamp(0.0, 1.0), cap_18k),
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
                        1 => Some(0.000_363_850_7f64.mul_add(-g_abs, 2.452_612_537_2)),
                        3 => Some(0.000_189_992_7f64.mul_add(-g_abs, 2.319_294_552_2)),
                        _ => Some(0.000_000_008_4f64.mul_add(-g_abs, 2.156_601_095_5)),
                    }
                }
                q if (q - 4.0).abs() < 1e-6
                    && matches!(slope_idx, Some(8))
                    && gain_db > 0.0
                    && matches!(sec, 1 | 3 | 5)
                    && (20500.0..21500.0).contains(&freq_hz) =>
                {
                    match sec {
                        1 => Some(2.462_098_942_9),
                        3 => Some(2.505_284_800_3),
                        _ => Some(2.376_904_116_8),
                    }
                }
                q if (q - 4.0).abs() < 1e-6
                    && matches!(slope_idx, Some(6))
                    && sec == 1
                    && (19500.0..20500.0).contains(&freq_hz) =>
                {
                    let g_abs = gain_db.abs();
                    Some(0.000_991_662_5f64.mul_add(-g_abs, 2.487_836_386_0))
                }
                q if (q - 4.0).abs() < 1e-6
                    && matches!(slope_idx, Some(5))
                    && sec == 1
                    && freq_hz >= 21500.0 =>
                {
                    Some(2.465_468_567_7)
                }
                q if (q - 4.0).abs() < 1e-6 => Some(2.508),
                // Slope-4 Q=10 high-side sections drift with gain and then
                // rise toward the generic Q=10 ceiling at 22 kHz.
                q if (q - 10.0).abs() < 1e-6 && matches!(slope_idx, Some(4)) => {
                    let g_abs = gain_db.abs();
                    let cap_21k = 0.000_569_314_5f64.mul_add(-g_abs, 2.656_967_674_7);
                    let cap_22k = 0.000_225_001_7f64.mul_add(-g_abs, 2.725_300_124_1);
                    Some(
                        (cap_22k - cap_21k)
                            .mul_add(((freq_hz - 21000.0) / 1000.0).clamp(0.0, 1.0), cap_21k),
                    )
                }
                q if (q - 10.0).abs() < 1e-6 && matches!(slope_idx, Some(5)) => {
                    let g_abs = gain_db.abs();
                    let cap_20k = 0.000_642_448_7f64.mul_add(-g_abs, 2.561_923_435_0);
                    let cap_21k = 0.000_516_129_5f64.mul_add(-g_abs, 2.670_120_270_9);
                    let cap_22k = 0.000_138_171_2f64.mul_add(-g_abs, 2.729_080_921_3);
                    if freq_hz < 21000.0 {
                        Some(
                            (cap_21k - cap_20k)
                                .mul_add(((freq_hz - 20000.0) / 1000.0).clamp(0.0, 1.0), cap_20k),
                        )
                    } else {
                        Some(
                            (cap_22k - cap_21k)
                                .mul_add(((freq_hz - 21000.0) / 1000.0).clamp(0.0, 1.0), cap_21k),
                        )
                    }
                }
                q if (q - 10.0).abs() < 1e-6 && matches!(slope_idx, Some(6)) => {
                    let g_abs = gain_db.abs();
                    let cap_20k = 0.000_593_994_5f64.mul_add(-g_abs, 2.573_288_297_2);
                    let cap_21k = 0.000_457_909_2f64.mul_add(-g_abs, 2.678_889_273_4);
                    let cap_22k = 0.000_070_546_7f64.mul_add(-g_abs, 2.730_453_349_8);
                    if freq_hz < 21000.0 {
                        Some(
                            (cap_21k - cap_20k)
                                .mul_add(((freq_hz - 20000.0) / 1000.0).clamp(0.0, 1.0), cap_20k),
                        )
                    } else {
                        Some(
                            (cap_22k - cap_21k)
                                .mul_add(((freq_hz - 21000.0) / 1000.0).clamp(0.0, 1.0), cap_21k),
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
                            0.000_502_336_7f64.mul_add(-g_abs, 2.588_221_663_3),
                            0.000_543_394_3f64.mul_add(-g_abs, 2.691_969_619_9),
                            0.000_013_009_3f64.mul_add(g_abs, 2.730_444_738_4),
                        )
                    } else {
                        (
                            0.000_068_368_1f64.mul_add(-g_abs, 2.506_083_569_3),
                            0.000_093_247_6f64.mul_add(-g_abs, 2.622_820_578_6),
                            0.000_036_650_9f64.mul_add(-g_abs, 2.708_604_210_8),
                        )
                    };
                    if freq_hz < 21000.0 {
                        Some(
                            (cap_21k - cap_20k)
                                .mul_add(((freq_hz - 20000.0) / 1000.0).clamp(0.0, 1.0), cap_20k),
                        )
                    } else {
                        Some(
                            (cap_22k - cap_21k)
                                .mul_add(((freq_hz - 21000.0) / 1000.0).clamp(0.0, 1.0), cap_21k),
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
                            0.000_374_938_6f64.mul_add(-g_abs, 2.604_106_681_5),
                            0.000_368_705_3f64.mul_add(-g_abs, 2.701_983_210_1),
                            0.000_120_431_0f64.mul_add(g_abs, 2.727_354_728_6),
                        ),
                        3 => (
                            0.000_123_711_4f64.mul_add(-g_abs, 2.543_913_775_4),
                            0.000_103_462_2f64.mul_add(-g_abs, 2.655_470_794_3),
                            0.000_038_792_0f64.mul_add(-g_abs, 2.724_667_766_1),
                        ),
                        _ => (
                            0.000_029_186_4f64.mul_add(-g_abs, 2.494_336_949_2),
                            0.000_027_037_1f64.mul_add(-g_abs, 2.611_666_664_0),
                            0.000_017_280_9f64.mul_add(-g_abs, 2.701_995_325_7),
                        ),
                    };
                    if freq_hz < 21000.0 {
                        Some(
                            (cap_21k - cap_20k)
                                .mul_add(((freq_hz - 20000.0) / 1000.0).clamp(0.0, 1.0), cap_20k),
                        )
                    } else {
                        Some(
                            (cap_22k - cap_21k)
                                .mul_add(((freq_hz - 21000.0) / 1000.0).clamp(0.0, 1.0), cap_21k),
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
                            0.000_295_501_5f64.mul_add(-g_abs, 2.612_454_880_7),
                            0.000_273_682_8f64.mul_add(-g_abs, 2.706_807_803_5),
                            0.000_146_074_0f64.mul_add(g_abs, 2.724_646_202_6),
                        ),
                        3 => (
                            0.000_113_220_5f64.mul_add(-g_abs, 2.564_470_288_4),
                            0.000_088_768_1f64.mul_add(-g_abs, 2.672_080_423_7),
                            0.000_017_840_5f64.mul_add(-g_abs, 2.729_373_317_8),
                        ),
                        5 => (
                            0.000_043_065_9f64.mul_add(-g_abs, 2.524_217_455_6),
                            0.000_038_467_9f64.mul_add(-g_abs, 2.638_627_765_3),
                            0.000_019_936_5f64.mul_add(-g_abs, 2.717_373_389_7),
                        ),
                        _ => (
                            0.000_008_079_9f64.mul_add(-g_abs, 2.488_424_190_7),
                            0.000_007_560_2f64.mul_add(-g_abs, 2.606_160_252_2),
                            0.000_005_002_6f64.mul_add(-g_abs, 2.698_416_690_5),
                        ),
                    };
                    if freq_hz < 21000.0 {
                        Some(
                            (cap_21k - cap_20k)
                                .mul_add(((freq_hz - 20000.0) / 1000.0).clamp(0.0, 1.0), cap_20k),
                        )
                    } else {
                        Some(
                            (cap_22k - cap_21k)
                                .mul_add(((freq_hz - 21000.0) / 1000.0).clamp(0.0, 1.0), cap_21k),
                        )
                    }
                }
                q if (q - 10.0).abs() < 1e-6 => Some(2.73),
                _ => cap_table(q_user).map(|(wz, _)| wz),
            };
            let wz = hi_wz_ceiling.map_or(wz_unsat, |wz_cap_val| wz_unsat.min(wz_cap_val));
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
                crate::design::biquad::PASSTHROUGH
            } else {
                coeffs
            };
        sections.push(coeffs);
    }
    sections
}

#[expect(
    clippy::too_many_arguments,
    reason = "a decoded routine's parameter list: each argument is one coefficient or pole term the reference implementation passes separately. Bundling them into a struct would rename the maths for no gain and break the correspondence with the decode notes"
)]
/// Bell 3-point Lagrange synthesis — extracted from `bell_s2_proq4` body
/// (post-sub-frequency selection).  Verified ≤ 1.5e-13 bit-exact on
/// captured `lagrange_per_section_sweep.csv` rows where `w_third != 0`.
pub fn bell_three_point_synth(
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
    const W_ZERO_MAX: f64 = 2.827_433_388_230_814; // 0.9π — no captured cell hits it
                                                   // bell_s2 has its own inline synth path, so this function only sees
    const W_POLE_MAX: f64 = 3.135_309_468_282_613_5;
    const W_THIRD_MAX: f64 = 3.133_741_813_548_472_3;
    if trace_bell_inputs() {
        eprintln!(
            "BELL_IN wp={w_pole:.6} wz={w_zero:.6} wt={w_third:.6} we={w_eval:.6} G={g_ref:.6} A={cap_a:.6} B={cap_b:.6} C={cap_c:.6} D={cap_d:.6} E={cap_e:.6} F={cap_f:.6}"
        );
    }
    // Caps decoded from bucket-B captures (slopes ≥ 3) at fc ∈ {15..22}
    // kHz where the warped frequencies hit Nyquist-adjacent limits:
    //   bell_s{3..9}_secparams_audio.json — w_pole and w_third each cap to
    //   distinct constants (3.13530947 and 3.13374181) independent of fc.
    // bucket-B inputs.

    let w_pole = w_pole.min(W_POLE_MAX);
    let w_zero = w_zero.min(W_ZERO_MAX);
    let w_third = w_third.min(W_THIRD_MAX);
    let w_eval = w_eval.clamp(0.0, PI);

    let h_sq = |w: f64| -> f64 {
        let w2 = w * w;
        let w4 = w2 * w2;
        let den = cap_d.mul_add(w4, cap_e * w2) + cap_f;
        if den.abs() > 1e-300 {
            (cap_a.mul_add(w4, cap_b * w2) + cap_c) / den
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

    // D_lag (Lagrange determinant; 0x180110855..0x1801108c1)
    let d_lag = tt2 * ((mz - mt) * (g - mp)).mul_add(tz2, -((mp - mt) * (g - mz) * tp2))
        + tp2 * tz2 * (g - mt) * (mp - mz);

    // (sp5, sp6, p4, p3_eff) computation: emulate the asm post-JZ block exactly.
    // p3_eff = XMM13 at mode-0 call site (= √Me in P1, recomputed in P2).
    let (sp5, sp6, p4, p3_eff) = if d_lag == 0.0 {
        // 0x1801108ca JZ → 0x180110a0a: XMM5 = 0; post-join with degenerate state.
        let (s5, s6, p4_v) =
            bell_synth_post_join(mp, mz, mt, tp2, tz2, tt2, 0.0, Some(0.0), p2, sqrt_me);
        (s5, s6, p4_v, sqrt_me)
    } else {
        let n_inter = me.mul_add(
            (tt2 - tp2).mul_add(mz, (tp2 - tz2) * mt),
            mp.mul_add(
                (tt2 - tp2).mul_add(mt, (tz2 - tt2).mul_add(me, (tp2 - tz2) * mz)),
                (tz2 - tt2) * mt * mz,
            ),
        );
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
            let xmm11 = (tz2 - tt2).mul_add(mp, rsp_48) + rsp_50;
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
/// the (`Q_k`, `gdB_k`) the binary feeds into
/// `compute_audio_biquad_lagrange_mzt` per section.  See
/// `tools/proq4_probe/fit_brickwall_closed_form.py`.
///
/// Slope=8 has 6 sections in 3 pairs.  Slope=6 has 3 sections (one pair +
/// one real-pole section where `Q_k=Q_user`, `gdB_k=±g_user/N_atoms` exactly).
/// Slope=4 has 2 sections (one pair).
///
/// Tables capture the recovered values at fc=500 Hz (low-fc, fits clean to
/// residual ≤ 1e-3 in coefficient space).  Q-axis interpolated linearly in
/// `Q_user` (clamped at table edges).  Gain magnitude scaled linearly:
/// `gdB_k(g) = gdB_k(±12) · |g|/12`.  Sign of g picks `*_GP` vs `*_GN`
/// table.
pub fn bell_brickwall_proq4_n(
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

#[expect(
    clippy::too_many_lines,
    reason = "a decoded routine: one contiguous function in the binary, whose commentary cites the captured rows each branch was verified against. Splitting it would separate the arithmetic from its evidence"
)]
/// Per-section `(Q_k, gdB_k)` lookup.  Returns `N_sec` entries per slope:
/// slope=4 → 2, slope=6 → 3, slope=8 → 6.
///
/// Recovered at fc=500 Hz (low-fc) by inverting `bell_s2_proq4` against
/// captured per-section biquads in `lagrange_brickwall_full.csv`.
pub fn brickwall_per_section_table(bp_order: usize, q_user: f64, gain_db: f64) -> Vec<(f64, f64)> {
    // Q_user grid the recovery sweep covers.
    const QS: [f64; 4] = [0.5, 1.0, 4.0, 10.0];

    // Boost (g=+12) tables: rows = sections, columns = Q_user grid.
    // ── slope=4 ──
    static QK_S4_GP: [[f64; 4]; 2] = [
        [3.586_285_674, 3.016_475_764, 6.693_358_377, 15.309_756_74],
        [0.378_282_178_9, 0.914_632_176, 4.870_053_886, 13.099_293_88],
    ];
    static GDB_S4_GP: [[f64; 4]; 2] = [
        [-3.132_364_34, 1.255_240_591, 5.548_682_501, 6.108_227_731],
        [6.191_169_931, 7.073_154_659, 6.226_073_393, 5.857_372_621],
    ];
    // Cut (g=−12)
    static QK_S4_GN: [[f64; 4]; 2] = [
        [3.662_941_579, 3.019_225_582, 6.636_358_51, 15.165_263_66],
        [
            0.382_338_153_5,
            0.925_898_202_6,
            4.919_977_142,
            13.226_627_44,
        ],
    ];
    static GDB_S4_GN: [[f64; 4]; 2] = [
        [
            4.610_435_319,
            -0.353_762_754_8,
            -4.827_502_331,
            -5.391_132_54,
        ],
        [
            -7.813_744_63,
            -8.073_957_711,
            -6.943_889_204,
            -6.555_256_923,
        ],
    ];

    // ── slope=6 ── sec2 is real-pole: Q_k = Q_user, gdB_k = ±g/N_atoms = ±g/2.
    static QK_S6_GP: [[f64; 4]; 3] = [
        [5.498_228_809, 4.653_210_027, 9.866_439_789, 22.204_906_66],
        [0.516_255_619_6, 1.234_800_415, 6.657_535_316, 18.114_593_17],
        [0.5, 1.0, 4.0, 10.0],
    ];
    static GDB_S6_GP: [[f64; 4]; 3] = [
        [-1.183_480_987, 1.171_725_183, 3.972_778_928, 4.427_459_399],
        [3.436_676_232, 4.188_170_462, 3.842_903_774, 3.572_041_903],
        [4.0, 4.0, 4.0, 4.0],
    ];
    static QK_S6_GN: [[f64; 4]; 3] = [
        [5.606_281_946, 4.657_543_646, 9.774_894_317, 21.969_983_42],
        [0.522_114_911_5, 1.250_607_826, 6.732_871_797, 18.306_983_87],
        [0.5, 1.0, 4.0, 10.0],
    ];
    static GDB_S6_GN: [[f64; 4]; 3] = [
        [
            3.461_188_093,
            0.319_694_764_8,
            -2.756_016_762,
            -3.220_873_104,
        ],
        [
            -5.603_197_597,
            -5.657_318_366,
            -5.002_565_762,
            -4.708_335_18,
        ],
        [-4.0, -4.0, -4.0, -4.0],
    ];

    // ── slope=8 ── 6 sections in 3 pairs.
    static QK_S8_GP: [[f64; 4]; 6] = [
        [11.870_313_52, 9.897_547_866, 20.206_345_47, 45.013_066_57],
        [0.937_787_524_6, 2.252_198_659, 12.283_135_95, 33.655_828_95],
        [3.451_825_64, 2.841_639_95, 6.607_904_15, 15.197_742_48],
        [
            0.378_814_757_9,
            0.925_884_647_3,
            4.921_970_232,
            13.195_007_09,
        ],
        [1.931_487_813, 1.382_659_957, 4.368_807_504, 10.583_695_47],
        [
            0.294_033_188_6,
            0.826_147_192_8,
            3.934_231_067,
            10.109_511_9,
        ],
    ];
    static GDB_S8_GP: [[f64; 4]; 6] = [
        [1.289_095_059, 1.821_483_896, 3.028_601_27, 3.273_931_156],
        [
            0.246_379_928_3,
            0.982_426_357_6,
            1.021_121_439,
            0.890_589_525,
        ],
        [
            -0.640_989_817_7,
            0.730_196_788_1,
            2.074_807_132,
            2.252_342_132,
        ],
        [1.689_189_28, 2.135_316_886, 1.867_334_951, 1.746_544_258],
        [-1.962_563_681, 1.090_614_535, 1.997_102_503, 2.049_182_733],
        [2.303_700_563, 2.500_645_49, 1.996_108_81, 1.941_936_092],
    ];
    static QK_S8_GN: [[f64; 4]; 6] = [
        [12.121_863_32, 9.907_723_71, 19.997_327_76, 44.474_996_99],
        [0.948_933_597_8, 2.280_406_005, 12.421_258_61, 34.008_66],
        [3.476_188_027, 2.842_246_024, 6.590_110_695, 15.152_699_77],
        [
            0.379_588_530_7,
            0.928_854_551_5,
            4.937_299_455,
            13.234_439_58,
        ],
        [1.944_638_858, 1.382_370_931, 4.365_100_37, 10.574_960_2],
        [0.293_772_512, 0.826_446_894_9, 3.937_059_473, 10.117_252_37],
    ];
    static GDB_S8_GN: [[f64; 4]; 6] = [
        [
            3.639_062_152,
            1.464_339_862,
            -0.354_922_155_5,
            -0.633_765_904_3,
        ],
        [-4.139_619_28, -3.787_995_516, -3.349_079_567, -3.185_802_93],
        [
            2.042_917_669,
            0.123_226_656_4,
            -1.396_810_249,
            -1.582_937_86,
        ],
        [
            -3.010_743_182,
            -2.963_534_21,
            -2.522_531_413,
            -2.392_057_142,
        ],
        [
            2.714_769_721,
            -0.842_441_498_5,
            -1.817_504_313,
            -1.872_310_383,
        ],
        [
            -3.044_002_745,
            -2.748_376_588,
            -2.173_775_358,
            -2.117_127_138,
        ],
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
    // The table is a non-empty `static`; the fallbacks keep this total.
    let q_clamped = q_user.clamp(
        QS.first().copied().unwrap_or(0.0),
        QS.last().copied().unwrap_or(0.0),
    );
    let mut q_idx = 0_usize;
    while q_idx.saturating_add(1) < QS.len().saturating_sub(1)
        && QS
            .get(q_idx.saturating_add(1))
            .is_some_and(|&q| q_clamped > q)
    {
        q_idx = q_idx.saturating_add(1);
    }
    let q_lo = QS.get(q_idx).copied().unwrap_or(0.0);
    let q_hi = QS
        .get(q_idx.saturating_add(1))
        .copied()
        .unwrap_or(q_lo + 1.0);
    let alpha = (q_clamped - q_lo) / (q_hi - q_lo);
    let lerp = |a: f64, b: f64| (b - a).mul_add(alpha, a);

    // Tables are recovered at |g|=12 dB.  Scale gdB_k linearly with |g|/12.
    let g_scale = gain_db.abs() / 12.0;

    (0..n_sec)
        .map(|i| {
            let pick = |table: &[[f64; 4]], j: usize| {
                table
                    .get(i)
                    .and_then(|row| row.get(j))
                    .copied()
                    .unwrap_or(0.0)
            };
            let qk = lerp(
                pick(qk_table, q_idx),
                pick(qk_table, q_idx.saturating_add(1)),
            );
            let gk_12 = lerp(
                pick(gdb_table, q_idx),
                pick(gdb_table, q_idx.saturating_add(1)),
            );
            (qk, gk_12 * g_scale)
        })
        .collect()
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "complex/float arithmetic — `Complex` is two `f64`s, so its operators cannot panic or overflow; the lint cannot see through an operator overload"
)]
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
pub fn bell_brickwall_cascade(
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
    let pole_mag = g_lin.powf(-1.0 / num::count_to_f64(n_bp));
    let zero_mag = 1.0 / pole_mag;

    // Band edges (kept from earlier impl — empirical from pole-spread
    // analysis).  The full RE-derived pipeline (LP-atoms → standard
    // LP→BP → bilinear) reproduces the post-LP→BP analog poles
    // bit-exact (verify_bell_brickwall_formula.py) but the SUBSEQUENT
    // numerator construction and per-section scaling that
    // zpk_to_biquad_coefficients applies is not yet captured.  See
    // numerator_reconstruction_blocker.md.
    let bw_half_oct = 1.0 / q.max(1e-6);
    let f_lo_target = freq_hz * (-bw_half_oct).exp2();
    let f_hi_target = freq_hz * bw_half_oct.exp2();
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
        let r = sq_re.hypot(sq_im);
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
        let theta = PI * num::count_to_f64(2 * k + 1) / num::count_to_f64(2 * n_bp);
        let bp_pole_a = lp_to_bp_local(pole_mag, theta);
        let bp_zero_a = lp_to_bp_local(zero_mag, theta);
        let (pole_re, pole_im) = blt(bp_pole_a.0, bp_pole_a.1);
        let (zero_re, zero_im) = blt(bp_zero_a.0, bp_zero_a.1);
        let a1 = -2.0 * pole_re;
        let a2 = pole_re.mul_add(pole_re, pole_im * pole_im);
        let b0 = 1.0;
        let b1 = -2.0 * zero_re;
        let b2 = zero_re.mul_add(zero_re, zero_im * zero_im);
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
        let n_re = s[5].mul_add(cw2, s[4].mul_add(cw, s[3]));
        let n_im = s[4].mul_add(sw, s[5] * sw2);
        let d_re = s[2].mul_add(cw2, s[1].mul_add(cw, 1.0));
        let d_im = s[1].mul_add(sw, s[2] * sw2);
        let dm2 = d_re * d_re + d_im * d_im;
        let qr = (n_re * d_re + n_im * d_im) / dm2;
        let qi = (n_im * d_re - n_re * d_im) / dm2;
        let nr = total_re.mul_add(qr, -(total_im * qi));
        let ni = total_re.mul_add(qi, total_im * qr);
        total_re = nr;
        total_im = ni;
    }
    let cur_peak = total_re.hypot(total_im);
    if cur_peak > 1e-12 {
        let target_per_section = (g_lin / cur_peak).powf(1.0 / num::count_to_f64(n));
        for s in &mut sections {
            s[3] *= target_per_section;
            s[4] *= target_per_section;
            s[5] *= target_per_section;
        }
    }

    sections
}

#[expect(
    clippy::too_many_arguments,
    reason = "a decoded routine's parameter list: each argument is one coefficient or pole term the reference implementation passes separately. Bundling them into a struct would rename the maths for no gain and break the correspondence with the decode notes"
)]
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
    let mp_mz_coeff = (mp - mz) * tp2 * tz2;
    let xmm0 = xmm5 * sqrt_g;
    let xmm3 = tz2.mul_add(xmm13_v, -xmm0);
    let xmm2 = tp2.mul_add(xmm13_v, -xmm0);
    let xmm1 = if mp_mz_coeff == 0.0 {
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
        let xmm8_b = (xmm8 - xmm0_sq).mul_add(xmm4_local, xmm3_b);
        let xmm8_c = xmm8_b * mp;
        ((xmm1_b - xmm8_c) / mp_mz_coeff).max(0.0)
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
