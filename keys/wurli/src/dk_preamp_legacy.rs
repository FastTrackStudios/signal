//! DK (Discretization-Kernel) preamp — full coupled 2-stage BJT circuit solver.
//!
//! Solves the complete Wurlitzer 200A preamp as an 8-node MNA system with
//! trapezoidal discretization and Newton-Raphson on the 2×2 nonlinear kernel.
//!
//! Key advantage over EbersMollPreamp: C-3/C-4 Miller capacitors are modeled
//! as coupled state variables, giving correct bandwidth (~15.5 kHz independent
//! of R_ldr) and gain-bandwidth scaling (GBW ∝ gain, not constant).
//!
//! Input coupling: The series Cin (0.022µF) + R1 (22K) is modeled as a bilinear
//! companion element. This correctly blocks DC (preserving the R2/R3 bias point)
//! while loading the base at audio frequencies with the proper R1 impedance.
//!
//! See docs/research/dk-preamp-derivation.md for the full mathematical derivation.
#![allow(clippy::needless_range_loop)]

use crate::preamp::PreampModel;

// ── Circuit constants ───────────────────────────────────────────────────────

const VCC: f64 = 15.0;

// Resistors (ohms)
const R1: f64 = 22_000.0; // Input to base1 (in series with Cin)
const R2: f64 = 2_000_000.0; // base1 to Vcc (bias)
const R3: f64 = 470_000.0; // base1 to GND (bias)
const RE1: f64 = 33_000.0; // emit1 to GND
const RC1: f64 = 150_000.0; // coll1 to Vcc
const RE2A: f64 = 270.0; // emit2 to emit2b
const RE2B: f64 = 820.0; // emit2b to GND
const RC2: f64 = 1_800.0; // coll2 to Vcc
const R9: f64 = 6_800.0; // coll2 to out
const R10: f64 = 56_000.0; // out to fb

// Capacitors (farads)
const CIN: f64 = 0.022e-6; // Input coupling cap (in series with R1)
const C3: f64 = 100.0e-12; // Miller, Stage 1 (coll1 ↔ base1)
const C4: f64 = 100.0e-12; // Miller, Stage 2 (coll2 ↔ coll1)
const CE1: f64 = 4.7e-6; // Feedback coupling (emit1 ↔ fb)
const CE2: f64 = 22.0e-6; // Stage 2 emitter bypass (emit2 ↔ emit2b)

// BJT (2N5089, forward-active)
const IS: f64 = 3.03e-14; // Saturation current
const VT: f64 = 0.026; // Thermal voltage (25°C)
const IS_OVER_VT: f64 = IS / VT;

// Max Vbe clamp — prevents exp overflow while allowing full operating range.
// Real 2N5089 Vbe never exceeds ~0.8V; 0.85V gives ample margin.
const VBE_MAX: f64 = 0.85;

// Node indices
const BASE1: usize = 0;
const EMIT1: usize = 1;
const COLL1: usize = 2;
const EMIT2: usize = 3;
const EMIT2B: usize = 4;
const COLL2: usize = 5;
const OUT: usize = 6;
const FB: usize = 7;

const N: usize = 8; // number of nodes

// ── 8×8 matrix type aliases ─────────────────────────────────────────────────

type Mat8 = [[f64; N]; N];
type Vec8 = [f64; N];

fn mat8_zero() -> Mat8 {
    [[0.0; N]; N]
}
fn vec8_zero() -> Vec8 {
    [0.0; N]
}

/// Matrix-vector multiply: y = A * x
fn mat_vec_mul(a: &Mat8, x: &Vec8) -> Vec8 {
    let mut y = vec8_zero();
    for i in 0..N {
        let mut sum = 0.0;
        for j in 0..N {
            sum += a[i][j] * x[j];
        }
        y[i] = sum;
    }
    y
}

/// Matrix add: C = A + B
fn mat_add(a: &Mat8, b: &Mat8) -> Mat8 {
    let mut c = mat8_zero();
    for i in 0..N {
        for j in 0..N {
            c[i][j] = a[i][j] + b[i][j];
        }
    }
    c
}

/// Matrix subtract: C = A - B
fn mat_sub(a: &Mat8, b: &Mat8) -> Mat8 {
    let mut c = mat8_zero();
    for i in 0..N {
        for j in 0..N {
            c[i][j] = a[i][j] - b[i][j];
        }
    }
    c
}

/// Scale matrix: B = scalar * A
fn mat_scale(scalar: f64, a: &Mat8) -> Mat8 {
    let mut b = mat8_zero();
    for i in 0..N {
        for j in 0..N {
            b[i][j] = scalar * a[i][j];
        }
    }
    b
}

/// Gauss-Jordan inverse of an 8×8 matrix. Panics if singular.
fn mat_inverse(m: &Mat8) -> Mat8 {
    let mut aug = [[0.0f64; N * 2]; N];
    for i in 0..N {
        for j in 0..N {
            aug[i][j] = m[i][j];
            aug[i][N + j] = if i == j { 1.0 } else { 0.0 };
        }
    }

    for col in 0..N {
        let mut max_val = aug[col][col].abs();
        let mut max_row = col;
        for row in (col + 1)..N {
            if aug[row][col].abs() > max_val {
                max_val = aug[row][col].abs();
                max_row = row;
            }
        }
        assert!(max_val > 1e-30, "Singular matrix in Gauss-Jordan inverse");

        if max_row != col {
            aug.swap(col, max_row);
        }

        let pivot = aug[col][col];
        for j in 0..(N * 2) {
            aug[col][j] /= pivot;
        }

        for row in 0..N {
            if row != col {
                let factor = aug[row][col];
                for j in 0..(N * 2) {
                    aug[row][j] -= factor * aug[col][j];
                }
            }
        }
    }

    let mut inv = mat8_zero();
    for i in 0..N {
        for j in 0..N {
            inv[i][j] = aug[i][N + j];
        }
    }
    inv
}

// ── DK Preamp ───────────────────────────────────────────────────────────────

pub struct DkPreamp {
    // ── Explicit R_ldr approach ──
    //
    // R_ldr is NOT stamped into G. Instead, the R_ldr current (v_FB/R_ldr)
    // is handled as an explicit source term, corrected via Sherman-Morrison
    // on the FIXED S_base = inv(2C/T + G_base) matrix.
    //
    // Why: When R_ldr is in G, changing R_ldr changes A = 2C/T + G, which
    // creates a mismatch between the forward matrix (A with new R_ldr) and
    // the history stored in Ce1's companion model (computed with old R_ldr).
    // Ce1's companion conductance (g_c = 2*4.7µF/T = 829 S at 88.2 kHz)
    // dominates the MNA system, so even small matrix changes create massive
    // DC transients as Ce1 charge redistribution overwhelms the AC signal.
    //
    // With R_ldr explicit, S_base and A_neg_base are CONSTANT. The Ce1
    // companion is always self-consistent. R_ldr only affects the v_pred
    // via a scalar SM correction on v_FB, preserving the Ce1 charge state.

    // ── Fixed matrices (never change after construction) ──
    s_base: Mat8,     // inv(A_base) where A_base = 2C/T + G_base (no R_ldr)
    a_neg_base: Mat8, // 2C/T - G_base (no R_ldr)
    k: [[f64; 2]; 2], // DK kernel = N_v * S_base * N_i (R_ldr-independent)
    two_w: Vec8,      // 2 * w

    // ── SM projection vectors for R_ldr ──
    s_fb_col: Vec8, // S_base[:,FB] — column FB of S_base
    #[cfg_attr(not(test), allow(dead_code))]
    s_fb_row: Vec8, // S_base[FB,:] — row FB of S_base (used in tests)
    s_fb_fb: f64,   // S_base[FB][FB] — SM denominator scalar
    nv_sfb: [f64; 2], // N_v * s_fb_col: NL voltage extraction at FB col
    sfb_ni: [f64; 2], // s_fb_row * N_i: NL current injection at FB row

    // ── DC operating point ──
    v_dc: Vec8,      // DC node voltages at current R_ldr
    g_dc_base: Mat8, // G_dc without R_ldr or g_cin (for DC solve)

    // ── Cin-R1 companion (shared constants) ──
    g_cin: f64,
    c_cin: f64,
    gc_1pc: f64,

    // ── Per-instance mutable state ──
    //
    // The main state processes the audio signal. The shadow state runs in
    // parallel with zero input, producing only the tremolo pump. Subtracting
    // shadow output from main output cancels all pump harmonics without
    // any frequency-domain filtering — zero bass loss, zero phase distortion.
    //
    // Why: R_ldr modulation at 5.63 Hz creates a ~4.5V pp pump at the output
    // (confirmed by SPICE: Ce1 transient dynamics dominate, not DC shift).
    // The pump has harmonics spanning 28-200+ Hz that overlap bass fundamentals.
    // No HPF can separate them without cutting bass. Shadow subtraction is
    // frequency-independent and exact (for small audio signals ≪ operating point).
    main: DkState,
    shadow: DkState,

    // ── Shared R_ldr tracking ──
    r_ldr: f64,
    g_ldr: f64,      // 1/r_ldr (current conductance)
    g_ldr_prev: f64, // g_ldr from previous timestep
}

/// Per-instance mutable state for the DK solver.
/// Both main and shadow instances share the same fixed matrices and R_ldr.
#[derive(Clone)]
struct DkState {
    j_cin: f64,
    cin_rhs_prev: f64,
    v: Vec8,        // Absolute node voltages
    i_nl: [f64; 2], // Absolute NL currents
    v_nl: [f64; 2], // Full Vbe (for NR warm start)
}

impl DkState {
    /// Initialize state at a DC operating point.
    fn at_dc(g_cin: f64, v_nl_dc: [f64; 2], v_dc: Vec8) -> Self {
        Self {
            j_cin: g_cin * v_dc[BASE1],
            cin_rhs_prev: g_cin * v_dc[BASE1],
            v: v_dc,
            i_nl: [bjt_ic(v_nl_dc[0]), bjt_ic(v_nl_dc[1])],
            v_nl: v_nl_dc,
        }
    }
}

impl DkPreamp {
    /// No-op in the legacy preamp — authentic circuit noise is a
    /// melange-only feature. The hand-written MNA solver has no noise
    /// model. Method mirrored here so the plugin compiles regardless of
    /// which preamp is selected (default = this; `--features melange-preamp`
    /// = melange solver).
    pub fn set_noise_enabled(&mut self, _on: bool) {}

    /// No-op in the legacy preamp — see `set_noise_enabled`.
    pub fn set_thermal_gain(&mut self, _gain: f64) {}

    pub fn new(sample_rate: f64) -> Self {
        let t = 1.0 / sample_rate;
        let two_over_t = 2.0 / t;

        // ── Cin-R1 companion model parameters ──
        let alpha_cin = 2.0 * R1 * CIN * sample_rate;
        let g_cin = (2.0 * CIN * sample_rate) / (1.0 + alpha_cin);
        let c_cin = (1.0 - alpha_cin) / (1.0 + alpha_cin);
        let gc_1pc = g_cin * (1.0 + c_cin);

        // ── Stamp G_base matrix (without R_ldr, WITH g_cin) ──
        let mut g_base = mat8_zero();
        let mut w = vec8_zero();

        g_base[BASE1][BASE1] += 1.0 / R2;
        w[BASE1] += VCC / R2;
        g_base[BASE1][BASE1] += 1.0 / R3;
        g_base[EMIT1][EMIT1] += 1.0 / RE1;
        g_base[COLL1][COLL1] += 1.0 / RC1;
        w[COLL1] += VCC / RC1;
        stamp_resistor(&mut g_base, EMIT2, EMIT2B, RE2A);
        g_base[EMIT2B][EMIT2B] += 1.0 / RE2B;
        g_base[COLL2][COLL2] += 1.0 / RC2;
        w[COLL2] += VCC / RC2;
        stamp_resistor(&mut g_base, COLL2, OUT, R9);
        stamp_resistor(&mut g_base, OUT, FB, R10);

        // G_dc_base: no R_ldr, no g_cin (for DC solves)
        let g_dc_base = g_base;

        // Add g_cin to g_base (for transient matrices)
        g_base[BASE1][BASE1] += g_cin;

        // ── Stamp C matrix ──
        let mut c = mat8_zero();
        stamp_capacitor(&mut c, COLL1, BASE1, C3);
        stamp_capacitor(&mut c, COLL2, COLL1, C4);
        stamp_capacitor(&mut c, EMIT1, FB, CE1);
        stamp_capacitor(&mut c, EMIT2, EMIT2B, CE2);
        let two_c_over_t = mat_scale(two_over_t, &c);

        let two_w: Vec8 = core::array::from_fn(|i| 2.0 * w[i]);

        // ── Build FIXED transient matrices (no R_ldr) ──
        let a_base = mat_add(&two_c_over_t, &g_base);
        let a_neg_base = mat_sub(&two_c_over_t, &g_base);
        let s_base = mat_inverse(&a_base);
        let k = compute_k(&s_base);

        // Extract SM projection vectors
        let s_fb_col: Vec8 = core::array::from_fn(|i| s_base[i][FB]);
        let s_fb_row: Vec8 = core::array::from_fn(|i| s_base[FB][i]);
        let s_fb_fb = s_base[FB][FB];

        // Pre-compute NL extraction/injection vectors for K correction
        let nv_sfb = [
            s_fb_col[BASE1] - s_fb_col[EMIT1], // N_v[0,:] . s_fb_col
            s_fb_col[COLL1] - s_fb_col[EMIT2], // N_v[1,:] . s_fb_col
        ];
        let sfb_ni = [
            s_fb_row[EMIT1] - s_fb_row[COLL1], // s_fb_row . N_i[:,0]
            s_fb_row[EMIT2] - s_fb_row[COLL2], // s_fb_row . N_i[:,1]
        ];

        // ── DC solve at initial R_ldr ──
        let r_ldr_init = 1_000_000.0;
        let (v_nl_dc, v_dc) = Self::full_dc_solve(&g_dc_base, &w, r_ldr_init);

        // Both main and shadow start at identical DC operating point
        let init_state = DkState::at_dc(g_cin, v_nl_dc, v_dc);

        Self {
            s_base,
            a_neg_base,
            k,
            two_w,

            s_fb_col,
            s_fb_row,
            s_fb_fb,
            nv_sfb,
            sfb_ni,

            v_dc,
            g_dc_base,

            g_cin,
            c_cin,
            gc_1pc,

            shadow: init_state.clone(),
            main: init_state,

            r_ldr: r_ldr_init,
            g_ldr: 1.0 / r_ldr_init,
            g_ldr_prev: 1.0 / r_ldr_init,
        }
    }

    /// Full DC solve: find quiescent operating point at a given R_ldr.
    /// Returns (v_nl_dc, v_dc).
    fn full_dc_solve(g_dc_base: &Mat8, w: &Vec8, r_ldr: f64) -> ([f64; 2], Vec8) {
        let mut g_full = *g_dc_base;
        g_full[FB][FB] += 1.0 / r_ldr;
        let s_dc = mat_inverse(&g_full);
        let k_dc = compute_k(&s_dc);
        let sv = mat_vec_mul(&s_dc, w);
        let p_dc = [sv[BASE1] - sv[EMIT1], sv[COLL1] - sv[EMIT2]];

        let mut v_nl = [0.56, 0.66];
        for _iter in 0..100 {
            let (ic0, gm0) = bjt_ic_gm(v_nl[0]);
            let (ic1, gm1) = bjt_ic_gm(v_nl[1]);
            let f = [
                v_nl[0] - p_dc[0] - k_dc[0][0] * ic0 - k_dc[0][1] * ic1,
                v_nl[1] - p_dc[1] - k_dc[1][0] * ic0 - k_dc[1][1] * ic1,
            ];
            if f[0].abs() < 1e-12 && f[1].abs() < 1e-12 {
                break;
            }

            let j00 = 1.0 - k_dc[0][0] * gm0;
            let j01 = -k_dc[0][1] * gm1;
            let j10 = -k_dc[1][0] * gm0;
            let j11 = 1.0 - k_dc[1][1] * gm1;
            let det = j00 * j11 - j01 * j10;
            let inv_det = 1.0 / det;
            let dv0 = inv_det * (j11 * f[0] - j01 * f[1]);
            let dv1 = inv_det * (j00 * f[1] - j10 * f[0]);
            let max_step = 2.0 * VT;
            v_nl[0] -= dv0.clamp(-max_step, max_step);
            v_nl[1] -= dv1.clamp(-max_step, max_step);
        }

        let ic = [bjt_ic(v_nl[0]), bjt_ic(v_nl[1])];
        let mut dc_rhs = *w;
        dc_rhs[EMIT1] += ic[0];
        dc_rhs[COLL1] -= ic[0];
        dc_rhs[EMIT2] += ic[1];
        dc_rhs[COLL2] -= ic[1];
        let v_dc = mat_vec_mul(&s_dc, &dc_rhs);

        (v_nl, v_dc)
    }

    /// Enable/disable shadow preamp bypass.
    /// When tremolo depth ≈ 0, R_ldr is constant so shadow output is constant DC.
    /// Bypassing saves ~50% of DK solver cost.
    /// Get w = two_w / 2 (the original DC source vector).
    fn two_w_half(&self) -> Vec8 {
        core::array::from_fn(|i| self.two_w[i] * 0.5)
    }
}

/// Compute K = N_v * S * N_i from an 8x8 matrix S.
fn compute_k(s: &Mat8) -> [[f64; 2]; 2] {
    [
        [
            s[BASE1][EMIT1] - s[BASE1][COLL1] - s[EMIT1][EMIT1] + s[EMIT1][COLL1],
            s[BASE1][EMIT2] - s[BASE1][COLL2] - s[EMIT1][EMIT2] + s[EMIT1][COLL2],
        ],
        [
            s[COLL1][EMIT1] - s[COLL1][COLL1] - s[EMIT2][EMIT1] + s[EMIT2][COLL1],
            s[COLL1][EMIT2] - s[COLL1][COLL2] - s[EMIT2][EMIT2] + s[EMIT2][COLL2],
        ],
    ]
}

/// Core DK trapezoidal step — free function for borrow-checker compatibility.
///
/// Both main and shadow instances call this with the same immutable config but
/// different mutable state. Making this a free function (not a method) allows
/// Rust's borrow checker to split borrows at the field level: config fields
/// borrowed immutably, state field borrowed mutably, in the same call.
///
/// The shadow runs with `input=0.0` to produce the pure pump signal.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn dk_step(
    a_neg_base: &Mat8,
    two_w: &Vec8,
    s_base: &Mat8,
    s_fb_col: &Vec8,
    s_fb_fb: f64,
    g_ldr: f64,
    g_ldr_prev: f64,
    k: &[[f64; 2]; 2],
    nv_sfb: &[f64; 2],
    sfb_ni: &[f64; 2],
    g_cin: f64,
    gc_1pc: f64,
    c_cin: f64,
    state: &mut DkState,
    input: f64,
) -> f64 {
    // 1. History: rhs = A_neg_base * v[n] + sources
    let mut rhs = mat_vec_mul(a_neg_base, &state.v);

    // Subtract previous R_ldr current (explicit, trapezoidal backward term)
    rhs[FB] -= g_ldr_prev * state.v[FB];

    // Cin-R1 companion
    let cin_rhs_now = g_cin * input + state.j_cin;
    rhs[BASE1] += cin_rhs_now + state.cin_rhs_prev;

    // Previous NL currents
    rhs[EMIT1] += state.i_nl[0];
    rhs[COLL1] -= state.i_nl[0];
    rhs[EMIT2] += state.i_nl[1];
    rhs[COLL2] -= state.i_nl[1];

    // DC sources (2w)
    for i in 0..N {
        rhs[i] += two_w[i];
    }

    // 2. v_pred_base = S_base * rhs (without R_ldr on LHS)
    let v_pred_base = mat_vec_mul(s_base, &rhs);

    // 3. SM correction for current R_ldr
    let sm_k = g_ldr / (1.0 + s_fb_fb * g_ldr);
    let sm_vpred = sm_k * v_pred_base[FB];
    let mut v_pred = vec8_zero();
    for i in 0..N {
        v_pred[i] = v_pred_base[i] - sm_vpred * s_fb_col[i];
    }

    // 4. Predicted NL voltages
    let p = [v_pred[BASE1] - v_pred[EMIT1], v_pred[COLL1] - v_pred[EMIT2]];

    // 5. NR solve on 2x2 system with R_ldr-corrected K
    let k00 = k[0][0] - sm_k * nv_sfb[0] * sfb_ni[0];
    let k01 = k[0][1] - sm_k * nv_sfb[0] * sfb_ni[1];
    let k10 = k[1][0] - sm_k * nv_sfb[1] * sfb_ni[0];
    let k11 = k[1][1] - sm_k * nv_sfb[1] * sfb_ni[1];

    let mut v_nl = state.v_nl;

    for _iter in 0..6 {
        let (ic0, gm0) = bjt_ic_gm(v_nl[0]);
        let (ic1, gm1) = bjt_ic_gm(v_nl[1]);

        let f0 = v_nl[0] - p[0] - k00 * ic0 - k01 * ic1;
        let f1 = v_nl[1] - p[1] - k10 * ic0 - k11 * ic1;

        if f0.abs() < 1e-9 && f1.abs() < 1e-9 {
            break;
        }

        let j00 = 1.0 - k00 * gm0;
        let j01 = -k01 * gm1;
        let j10 = -k10 * gm0;
        let j11 = 1.0 - k11 * gm1;

        let det = j00 * j11 - j01 * j10;
        if det.abs() < 1e-30 {
            break;
        }
        let inv_det = 1.0 / det;

        v_nl[0] -= inv_det * (j11 * f0 - j01 * f1);
        v_nl[1] -= inv_det * (j00 * f1 - j10 * f0);
    }

    // 6. Final NL currents
    let ic_new = [bjt_ic(v_nl[0]), bjt_ic(v_nl[1])];

    // 7. Node voltage update
    let sfb_ni_dot_ic = sfb_ni[0] * ic_new[0] + sfb_ni[1] * ic_new[1];
    for i in 0..N {
        let s_ni_i = ic_new[0] * (s_base[i][EMIT1] - s_base[i][COLL1])
            + ic_new[1] * (s_base[i][EMIT2] - s_base[i][COLL2]);
        state.v[i] = v_pred[i] + s_ni_i - sm_k * sfb_ni_dot_ic * s_fb_col[i];
    }

    // 8. Cin-R1 companion update
    state.cin_rhs_prev = cin_rhs_now;
    let dv_cin = input - state.v[BASE1];
    state.j_cin = -gc_1pc * dv_cin - c_cin * state.j_cin;

    // 9. State update
    state.i_nl = ic_new;
    state.v_nl = v_nl;

    state.v[OUT]
}

impl PreampModel for DkPreamp {
    fn process_sample(&mut self, input: f64) -> f64 {
        // Run main solver with audio input.
        // Field-level borrow splitting: config fields (&self.xxx) are immutable,
        // state field (&mut self.main) is mutable — different fields, no conflict.
        let main_out = dk_step(
            &self.a_neg_base,
            &self.two_w,
            &self.s_base,
            &self.s_fb_col,
            self.s_fb_fb,
            self.g_ldr,
            self.g_ldr_prev,
            &self.k,
            &self.nv_sfb,
            &self.sfb_ni,
            self.g_cin,
            self.gc_1pc,
            self.c_cin,
            &mut self.main,
            input,
        );

        // Shadow solver: zero input → pure pump. Always runs — no bypass
        // optimization. Toggling the solver on/off caused clicks (state
        // discontinuities amplified by downstream gain), and the crossfade
        // workarounds added complexity without fully fixing all edge cases.
        // The cost of one extra DK step per sample is negligible vs 64
        // reed oscillators, and pump cancellation is always exact.
        let pump = dk_step(
            &self.a_neg_base,
            &self.two_w,
            &self.s_base,
            &self.s_fb_col,
            self.s_fb_fb,
            self.g_ldr,
            self.g_ldr_prev,
            &self.k,
            &self.nv_sfb,
            &self.sfb_ni,
            self.g_cin,
            self.gc_1pc,
            self.c_cin,
            &mut self.shadow,
            0.0,
        );

        // Update shared R_ldr tracking (after both steps used g_ldr_prev)
        self.g_ldr_prev = self.g_ldr;

        // Subtract pump: cancels all tremolo pump harmonics (28-200+ Hz)
        // without any frequency-domain filtering. Zero bass loss.
        let result = main_out - pump;

        // NaN guard: if NR diverged, reset state and return silence.
        // Branch never taken in normal operation.
        if !result.is_finite() {
            self.reset();
            return 0.0;
        }

        result
    }

    fn set_ldr_resistance(&mut self, r_ldr_path: f64) {
        let new_r = r_ldr_path.max(1000.0);
        if (new_r - self.r_ldr).abs() > 0.01 {
            self.r_ldr = new_r;
            self.g_ldr = 1.0 / new_r;
        }
    }

    fn reset(&mut self) {
        // Full DC solve at current R_ldr
        let w = self.two_w_half();
        let (v_nl_dc, v_dc) = Self::full_dc_solve(&self.g_dc_base, &w, self.r_ldr);

        self.v_dc = v_dc;
        self.g_ldr = 1.0 / self.r_ldr;
        self.g_ldr_prev = self.g_ldr;

        // Reset both main and shadow to identical DC operating point
        let state = DkState::at_dc(self.g_cin, v_nl_dc, v_dc);
        self.shadow = state.clone();
        self.main = state;
    }
}

// ── Resistor/capacitor stamp helpers ────────────────────────────────────────

fn stamp_resistor(g: &mut Mat8, i: usize, j: usize, r: f64) {
    let cond = 1.0 / r;
    g[i][i] += cond;
    g[j][j] += cond;
    g[i][j] -= cond;
    g[j][i] -= cond;
}

fn stamp_capacitor(c: &mut Mat8, i: usize, j: usize, cap: f64) {
    c[i][i] += cap;
    c[j][j] += cap;
    c[i][j] -= cap;
    c[j][i] -= cap;
}

// ── BJT model ───────────────────────────────────────────────────────────────

/// BJT collector current: Ic = Is * (exp(Vbe/Vt) - 1).
/// Vbe clamped to [-1.0, VBE_MAX] to prevent exp overflow.
/// No artificial saturation limiting — the circuit topology and NR solver
/// naturally constrain the operating point.
#[inline]
fn bjt_ic(vbe: f64) -> f64 {
    let vbe_clamped = vbe.clamp(-1.0, VBE_MAX);
    IS * ((vbe_clamped / VT).exp() - 1.0)
}

/// BJT transconductance: gm = (Is/Vt) * exp(Vbe/Vt).
/// Only used in small-signal transfer function tests.
#[cfg(test)]
#[inline]
fn bjt_gm(vbe: f64) -> f64 {
    let vbe_clamped = vbe.clamp(-1.0, VBE_MAX);
    IS_OVER_VT * (vbe_clamped / VT).exp()
}

/// BJT collector current AND transconductance from a single exp().
/// Returns (ic, gm) = (Is*(e-1), Is/Vt*e) where e = exp(Vbe/Vt).
/// Saves one exp() per BJT per NR iteration in the hot loop.
#[inline]
fn bjt_ic_gm(vbe: f64) -> (f64, f64) {
    let vbe_clamped = vbe.clamp(-1.0, VBE_MAX);
    let e = (vbe_clamped / VT).exp();
    (IS * (e - 1.0), IS_OVER_VT * e)
}

// ── Tests ───────────────────────────────────────────────────────────────────
