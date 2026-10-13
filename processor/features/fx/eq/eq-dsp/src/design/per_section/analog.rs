//! The analog-domain building blocks the per-section helpers share.
//!
//! A biquad in `s`, its magnitude-squared coefficients, the quadratic solver
//! that finds its pole pair, and the prototype the shape helpers fill in.
//! Nothing here is shape-specific.

/// Generic analog biquad prototype `(b2z·s² + b1z·s + b0z) / (b2p·s² + b1p·s + b0p)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnalogBiquad {
    pub b2z: f64,
    pub b1z: f64,
    pub b0z: f64,
    pub b2p: f64,
    pub b1p: f64,
    pub b0p: f64,
}

/// Squared-magnitude polynomial coefficients (A..F per
/// `lagrange_mzt_universal_decode.md`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MagSqCoeffs {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl AnalogBiquad {
    /// Mirrors `compute_zpk_transfer_function_coefficients @ 0x1800fd420`
    /// for the generic case (writes proto[+0x20..+0x48]).
    ///
    /// |H(jw)|² = (A·w⁴ + B·w² + C) / (D·w⁴ + E·w² + F) with
    /// `ω₀` scaling baked into B/C/E/F.
    #[must_use]
    pub fn squared_mag_coeffs(&self, omega0: f64) -> MagSqCoeffs {
        let g_om2 = omega0 * omega0;
        let g_om4 = g_om2 * g_om2;
        MagSqCoeffs {
            a: self.b2z * self.b2z,
            b: self.b1z.mul_add(self.b1z, -(2.0 * self.b2z * self.b0z)) * g_om2,
            c: self.b0z * self.b0z * g_om4,
            d: self.b2p * self.b2p,
            e: self.b1p.mul_add(self.b1p, -(2.0 * self.b2p * self.b0p)) * g_om2,
            f: self.b0p * self.b0p * g_om4,
        }
    }
}

/// Pro-Q's `compute_zpk_transfer_function_coefficients @ 0x1800fd420`,
/// generic case.
///
/// Faithful port that handles both the **linear branch** (when both `b2z`
/// and `b2p` are sub-epsilon, treating the prototype as a 1st-order
/// quadratic) and the **quadratic branch**. The binary uses the float-cast
/// magnitude check `|b2z|` or `|b2p| > 1.192e-7` to pick.
///
/// Returns `(MagSqCoeffs, is_quadratic)`. `is_quadratic` is the binary's
/// `iVar4 == 2` flag, needed by `solve_biquad_denominator_quadratic_generic`.
#[must_use]
pub fn compute_zpk_transfer_coeffs_generic(
    analog: &AnalogBiquad,
    omega: f64,
) -> (MagSqCoeffs, bool) {
    const EPS_F32: f32 = 1.192_092_9e-7;
    let omega_sq = omega * omega;
    let omega_qd = omega_sq * omega_sq;

    let is_quadratic =
        analog.b2z.abs() > f64::from(EPS_F32) || analog.b2p.abs() > f64::from(EPS_F32);

    let coeffs = if is_quadratic {
        MagSqCoeffs {
            a: analog.b2z * analog.b2z,
            b: analog
                .b1z
                .mul_add(analog.b1z, -(2.0 * analog.b2z * analog.b0z))
                * omega_sq,
            c: analog.b0z * analog.b0z * omega_qd,
            d: analog.b2p * analog.b2p,
            e: analog
                .b1p
                .mul_add(analog.b1p, -(2.0 * analog.b2p * analog.b0p))
                * omega_sq,
            f: analog.b0p * analog.b0p * omega_qd,
        }
    } else {
        // Linear branch (b2 ≈ 0): A=0, B=b1², C=b0²·ω².
        MagSqCoeffs {
            a: 0.0,
            b: analog.b1z * analog.b1z,
            c: analog.b0z * analog.b0z * omega_sq,
            d: 0.0,
            e: analog.b1p * analog.b1p,
            f: analog.b0p * analog.b0p * omega_sq,
        }
    };

    (coeffs, is_quadratic)
}

/// Roots returned by [`solve_biquad_denominator_quadratic_generic`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoleRoots {
    pub w1: f64,
    pub w2: f64,
    pub count: u8,
}

/// Pro-Q's `solve_biquad_denominator_quadratic @ 0x1800fd240`, generic case.
///
/// Solves the cross-determinant quadratic in ω² for the squared-magnitude
/// crossing frequencies of `|H(jω)|² = (A·ω⁴+B·ω²+C)/(D·ω⁴+E·ω²+F)`:
/// `det_a · (ω²)² + det_b · (ω²) + det_c = 0` with
/// `det_a = A·E − B·D`, `det_b = 2·(F·A − C·D)`, `det_c = F·B − C·E`.
///
/// When `det_a == 0` the quadratic degenerates to linear; the binary swaps
/// `det_a/det_b` and skips the final sqrt of the root (`take_sqrt = false`).
/// `count` reports how many roots were strictly positive.
#[must_use]
pub fn solve_biquad_denominator_quadratic_generic(
    coeffs: &MagSqCoeffs,
    is_quadratic: bool,
) -> PoleRoots {
    let MagSqCoeffs { a, b, c, d, e, f } = *coeffs;

    if !is_quadratic {
        return PoleRoots {
            w1: 0.0,
            w2: 0.0,
            count: 0,
        };
    }

    let mut det_a = a.mul_add(e, -(b * d));
    let mut det_b = 2.0 * (f.mul_add(a, -(c * d)));
    let take_sqrt = if det_a == 0.0 {
        det_a = det_b;
        det_b = 0.0;
        false
    } else {
        true
    };
    if det_a == 0.0 {
        return PoleRoots {
            w1: 0.0,
            w2: 0.0,
            count: 0,
        };
    }

    let det_c = f.mul_add(b, -(c * e));
    let disc = det_b * det_b - 4.0 * det_a * det_c;
    if disc < 0.0 {
        return PoleRoots {
            w1: 0.0,
            w2: 0.0,
            count: 0,
        };
    }

    let sqrt_disc = disc.sqrt();
    let inv2a = 0.5 / det_a;
    let neg_b = -det_b * inv2a;
    let radical = sqrt_disc * inv2a;
    let mut w_sq_lo = neg_b + radical;
    let mut w_sq_hi = neg_b - radical;

    if take_sqrt {
        if w_sq_lo > 0.0 {
            w_sq_lo = w_sq_lo.sqrt();
        }
        if w_sq_hi > 0.0 {
            w_sq_hi = w_sq_hi.sqrt();
        }
    }

    let pos_lo = w_sq_lo > 0.0;
    let pos_hi = w_sq_hi > 0.0;
    if !pos_lo && !pos_hi {
        return PoleRoots {
            w1: 0.0,
            w2: 0.0,
            count: 0,
        };
    }
    if pos_lo && !pos_hi {
        return PoleRoots {
            w1: w_sq_lo,
            w2: 0.0,
            count: 1,
        };
    }
    if !pos_lo && pos_hi {
        return PoleRoots {
            w1: w_sq_hi,
            w2: 0.0,
            count: 1,
        };
    }
    if w_sq_lo <= w_sq_hi {
        PoleRoots {
            w1: w_sq_lo,
            w2: w_sq_hi,
            count: 2,
        }
    } else {
        PoleRoots {
            w1: w_sq_hi,
            w2: w_sq_lo,
            count: 2,
        }
    }
}

/// Pro-Q's `vtable[0x10]` scalar magnitude evaluator
/// (`evaluate_biquad_squared_magnitude_scalar @ 0x1800fd0b0`).
///
/// Returns `max(num/den, 0)` with the den-zero guard the binary uses (the
/// binary returns 0 when den is sub-normal; we mirror via `> 1e-300`).
#[must_use]
pub fn eval_squared_mag_scalar(coeffs: &MagSqCoeffs, w: f64) -> f64 {
    let w2 = w * w;
    let w4 = w2 * w2;
    let num = w4.mul_add(coeffs.a, w2 * coeffs.b) + coeffs.c;
    let den = w4.mul_add(coeffs.d, w2 * coeffs.e) + coeffs.f;
    if den.abs() > 1e-300 {
        (num / den).max(0.0)
    } else {
        0.0
    }
}

/// ω₀ scaling factor applied per band-level filter type before feeding the
/// Lagrange-MZT synth.
///
/// Verified for shelves: `ω₀ = 0.64 · (2π·fc/sr)` (= 16/25, bit-exact across
/// SR ∈ {44100, 48000, 88200, 96000}). Per
/// `docs/reports/proq4/re/lagrange_mzt_universal_decode.md`.
///
/// Tilt/bandpass/notch use `1.0` until probe sweeps confirm otherwise.
#[must_use]
pub fn omega_scale_for_band_type(band_filter_type: u8) -> f64 {
    match band_filter_type {
        // 7 = LowShelf, 8 = HighShelf, 9 = TiltShelf — TiltShelf's 0.64
        // applicability is unverified but we mirror shelves until probed.
        7..=9 => 16.0 / 25.0,
        // All other band types (peak, bandpass, notch, allpass, …) use
        // ω_naive until probe confirms a per-type override.
        _ => 1.0,
    }
}

/// Subset of Pro-Q's 0xC0-byte prototype struct that the per-section helpers
/// read and write. Field names mirror the offsets used in
/// `per_section_helpers_decompiled.md`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Prototype {
    /// proto[1] @ +0x08 — wp (pole sub-frequency, radians).
    pub wp: f64,
    /// proto[2] @ +0x10 — wz (zero sub-frequency, radians).
    pub wz: f64,
    /// proto[3] @ +0x18 — wt (third sub-frequency, radians).
    pub wt: f64,
    /// proto[4] @ +0x20 — `w_eval` (magnitude evaluation point, radians).
    pub w_eval: f64,

    /// proto[7] @ +0x38 — `iVar5`/dispatch sub-mode (1 = real-root path,
    /// 2 = complex-root path, 0 = default).
    pub mode: i32,

    /// proto[0x11] @ +0x88 — duplicate of the solved root count used by
    /// shelf-style helpers to choose their Q source. This is distinct from
    /// proto[0x13], the section helper dispatch type.
    pub root_count_dup: i32,

    /// proto[0xb] @ +0x58 — magnitude cache for wp, written by
    /// `update_tracked_band_frequencies` as `vt10(wp)`. (Earlier doc
    /// labeled these as "previous wp/wz" — that was wrong; they hold
    /// `|H(jwp)|²` and `|H(jwz)|²` respectively.)
    pub prev_wp: f64,
    /// proto[0xc] @ +0x60 — magnitude cache for wz (= `vt10(wz)`).
    pub prev_wz: f64,

    /// proto[5] @ +0x28 — band-edge frequency reference, read by
    /// `check_frequency_within_band_limits` as `|freq - proto[5]|`.
    pub band_edge_low: f64,

    /// proto[6] @ +0x30 — alternate band-edge reference used by
    /// `check_frequency_within_band_limits` when the upper-bound parameter
    /// is non-positive.
    pub band_edge_high: f64,

    /// proto[0xe] @ +0x70, proto[0xf] @ +0x78, proto[0x10] @ +0x80 —
    /// stored-constant slots written by certain branches (notch46 type=2,
    /// `band_shelf` v2 swap-branch).
    pub stored_e: f64,
    pub stored_f: f64,
    pub stored_g: f64,

    /// proto+0x8c (f32 in binary) — secondary alpha-scratch slot, paired
    /// with `alpha_scratch_94` to form the notch46 composite scratch
    /// `fVar7 = (alpha_scratch_8c)²·0.25 + alpha_scratch_94`.
    pub alpha_scratch_8c: f32,

    /// proto[0x94] @ +0x94 (f32 in binary, modeled as f64) — alpha-scratch
    /// input (= sec[0x5c]/Q from upstream).
    pub alpha_scratch_94: f32,

    /// proto[0x13] @ +0x98 — per-section internal filter type, mirrored
    /// from `sec[+0x58]`. Drives helper dispatch.
    pub section_type: i32,

    /// proto[0x14] @ +0xa0 — `sec[+0x50] * sec[+0x10]` = band-level ω
    /// reference (radians), set upstream by the cascade builder.
    pub band_omega_ref: f64,

    /// proto[10] @ +0x50 — Q-related scratch double (used as denominator in
    /// `√2 / proto[10]` for bandshelf v2/10).
    pub q_scratch_50: f64,

    /// proto[0x12] @ +0x90 area (int) — section variant sign indicator
    /// (`-1`, `0`, `+1`); steers the `w_eval` branch in bandshelf v2.
    pub proto_0x12_sign: i32,

    /// Byte flag at +0x68 (read as `*(char *)(proto + 0xd)` in Pro-Q) —
    /// participates in the bandshelf v2 → shelf7 delegation predicate.
    pub flag_byte_68: u8,

    /// Byte flag at +0x69 — gates the bandshelf v2 → shelf7 delegation.
    pub flag_byte_69: u8,

    /// Analog biquad coefficients used by helpers that re-evaluate the
    /// squared-mag polynomial via `vtable[0x10]` (helpers for sections 8
    /// and 10). `None` for helpers that don't need it.
    pub analog: Option<AnalogBiquad>,

    /// ω₀ used to refresh A..F in `vtable[0x10]` calls; must match the
    /// per-band-type scaling that the synth ultimately uses.
    pub omega_band: f64,
}
