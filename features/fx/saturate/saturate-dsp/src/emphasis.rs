//! The emphasis / de-emphasis EQ pair — spec `fx.sat.emphasis`
//! (docs/spec/fx/embedded-eq.md).
//!
//! A 6-band parametric EQ applied *before* the saturation stage, and its
//! **exact inverse** after it — the generalization of the ±tilt shelf pair
//! in [`crate::preamp`]. Net-flat for a linear signal, so the curve chooses
//! *what distorts*, never what the output sounds like: boost 3 kHz +6 dB and
//! 3 kHz drives the stage 6 dB harder, then comes back down 6 dB on the way
//! out.
//!
//! The de-emphasis is not a mirrored-gain second EQ (two mirrored peaking
//! biquads only cancel approximately at high gains/Qs): each post section is
//! the pre section's **algebraic inverse** — numerator and denominator
//! swapped — run in reverse order. RBJ peak/shelf sections are minimum
//! phase, so the inverse is stable by construction
//! (`fx.sat.emphasis.mirror`).
//!
//! Shapes are Bell and Low/High Shelf only: a cut/notch/pass has no inverse,
//! so the spec excludes them from the emphasis EQ.
//!
//! `no_std` like the rest of the crate: the one transcendental (cos for the
//! coefficient math) is a range-reduced polynomial below.

/// Number of emphasis bands.
pub const BANDS: usize = 6;

/// The shapes an emphasis band can take (`fx.sat.emphasis.mirror`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmphShape {
    #[default]
    Bell = 0,
    LowShelf = 1,
    HighShelf = 2,
}

impl EmphShape {
    pub fn from_index(i: u32) -> Self {
        match i {
            1 => Self::LowShelf,
            2 => Self::HighShelf,
            _ => Self::Bell,
        }
    }
}

/// One band's settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmphBand {
    pub shape: EmphShape,
    pub freq_hz: f32,
    pub gain_db: f32,
    pub q: f32,
}

impl Default for EmphBand {
    fn default() -> Self {
        Self {
            shape: EmphShape::Bell,
            freq_hz: 1000.0,
            gain_db: 0.0,
            q: 0.707,
        }
    }
}

/// Normalized biquad coefficients (a0 = 1).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Coeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

/// Per-channel DF1 state for one section.
#[derive(Debug, Clone, Copy, Default)]
struct State {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

/// The pair, per audio channel: instantiate one per channel like the
/// preamps ([`crate::preamp::ClassAPreamp`] is per-channel in the plugin).
#[derive(Default)]
pub struct EmphasisEq {
    bands: [EmphBand; BANDS],
    coeffs: [Coeffs; BANDS],
    active: [bool; BANDS],
    pre_state: [State; BANDS],
    post_state: [State; BANDS],
    sample_rate: f32,
    /// Pink-weighted RMS gain of the emphasis curve — what it does to the
    /// level reaching the shaper. Feed into the preamp's makeup calibration
    /// (`fx.sat.emphasis.makeup`).
    sigma_gain: f32,
}

impl EmphasisEq {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            sigma_gain: 1.0,
            ..Default::default()
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        let bands = self.bands;
        self.set_bands(&bands);
        self.reset();
    }

    /// Whether any band does anything — a flat EQ is skipped entirely so the
    /// default plugin stays bit-identical.
    pub fn is_active(&self) -> bool {
        self.active.iter().any(|&a| a)
    }

    /// Design all sections from the band table. Setter-path arithmetic —
    /// never call from `process`-per-sample, but per block is fine (every
    /// section is a handful of ops and there is no allocation).
    pub fn set_bands(&mut self, bands: &[EmphBand; BANDS]) {
        self.bands = *bands;
        for (i, band) in bands.iter().enumerate() {
            let inaudible = band.gain_db.abs() < 0.01;
            self.active[i] = !inaudible;
            if inaudible {
                self.coeffs[i] = Coeffs::default();
                continue;
            }
            self.coeffs[i] = design(band, self.sample_rate);
        }
        self.sigma_gain = self.compute_sigma_gain();
    }

    /// The bands as set.
    pub fn bands(&self) -> &[EmphBand; BANDS] {
        &self.bands
    }

    pub fn sigma_gain(&self) -> f32 {
        self.sigma_gain
    }

    /// Emphasis (pre-stage) pass for channel `ch`'s sample.
    #[inline]
    pub fn pre(&mut self, x: f32) -> f32 {
        let mut v = x;
        for i in 0..BANDS {
            if !self.active[i] {
                continue;
            }
            let c = &self.coeffs[i];
            let s = &mut self.pre_state[i];
            let y = c.b0 * v + c.b1 * s.x1 + c.b2 * s.x2 - c.a1 * s.y1 - c.a2 * s.y2;
            s.x2 = s.x1;
            s.x1 = v;
            s.y2 = s.y1;
            s.y1 = y;
            v = y;
        }
        v
    }

    /// De-emphasis (post-stage) pass: the exact inverse sections, in reverse
    /// order. `H⁻¹ = (1 + a1 z⁻¹ + a2 z⁻²) / (b0 + b1 z⁻¹ + b2 z⁻²)`.
    #[inline]
    pub fn post(&mut self, x: f32) -> f32 {
        let mut v = x;
        for i in (0..BANDS).rev() {
            if !self.active[i] {
                continue;
            }
            let c = &self.coeffs[i];
            let s = &mut self.post_state[i];
            let y = (v + c.a1 * s.x1 + c.a2 * s.x2 - c.b1 * s.y1 - c.b2 * s.y2) / c.b0;
            s.x2 = s.x1;
            s.x1 = v;
            s.y2 = s.y1;
            s.y1 = y;
            v = y;
        }
        v
    }

    pub fn reset(&mut self) {
        self.pre_state = [State::default(); BANDS];
        self.post_state = [State::default(); BANDS];
    }

    /// The emphasis curve's magnitude in dB at `freq` — what the editor
    /// draws (`fx.sat.emphasis.display`).
    pub fn magnitude_db(&self, freq: f32) -> f32 {
        let mut db = 0.0;
        for i in 0..BANDS {
            if self.active[i] {
                db += section_mag_db(&self.coeffs[i], freq, self.sample_rate);
            }
        }
        db
    }

    /// Pink-weighted RMS gain over 20 Hz–20 kHz: equal power per octave, so
    /// log-spaced sample points weight equally.
    fn compute_sigma_gain(&self) -> f32 {
        if !self.is_active() {
            return 1.0;
        }
        const POINTS: usize = 24;
        let mut sum = 0.0f32;
        for k in 0..POINTS {
            // 20 Hz … 20 kHz, log spaced: 20 * 10^(3k/(N-1)).
            let exp = 3.0 * k as f32 / (POINTS - 1) as f32;
            let f = 20.0 * pow10(exp);
            let g = crate::db_to_gain(self.magnitude_db(f));
            sum += g * g;
        }
        crate::sqrt_approx(sum / POINTS as f32).clamp(0.05, 20.0)
    }
}

/// |H| of one section at `freq`, in dB.
///
/// Evaluated in f64: at the band's own centre the quadratic form cancels
/// down four orders of magnitude, so f32 trig reads the peak wrong.
fn section_mag_db(c: &Coeffs, freq: f32, sample_rate: f32) -> f32 {
    let w = core::f64::consts::TAU * (freq as f64 / sample_rate as f64).clamp(0.0, 0.5);
    let (cw, c2w) = (cos64(w), cos64(2.0 * w));
    let (b0, b1, b2) = (c.b0 as f64, c.b1 as f64, c.b2 as f64);
    let (a1, a2) = (c.a1 as f64, c.a2 as f64);
    // |B(e^jw)|² for b0 + b1 z⁻¹ + b2 z⁻²:
    let num = b0 * b0 + b1 * b1 + b2 * b2 + 2.0 * (b0 * b1 + b1 * b2) * cw + 2.0 * b0 * b2 * c2w;
    let den = 1.0 + a1 * a1 + a2 * a2 + 2.0 * (a1 + a1 * a2) * cw + 2.0 * a2 * c2w;
    (10.0 * log10_64((num / den.max(1e-30)).max(1e-30))) as f32
}

/// RBJ peak / shelf design, α = sin(w0)/(2Q).
fn design(band: &EmphBand, sample_rate: f32) -> Coeffs {
    let f = band.freq_hz.clamp(10.0, sample_rate * 0.45);
    let q = band.q.clamp(0.05, 18.0);
    // A = 10^(gain/40).
    let a_lin = pow10(band.gain_db.clamp(-24.0, 24.0) / 40.0);
    let w0 = core::f64::consts::TAU * f as f64 / sample_rate as f64;
    let cw = cos64(w0) as f32;
    let sw = sin64(w0) as f32;
    let alpha = sw / (2.0 * q);
    let sqrt_a = crate::sqrt_approx(a_lin);

    let (b0, b1, b2, a0, a1, a2) = match band.shape {
        EmphShape::Bell => (
            1.0 + alpha * a_lin,
            -2.0 * cw,
            1.0 - alpha * a_lin,
            1.0 + alpha / a_lin,
            -2.0 * cw,
            1.0 - alpha / a_lin,
        ),
        EmphShape::LowShelf => (
            a_lin * ((a_lin + 1.0) - (a_lin - 1.0) * cw + 2.0 * sqrt_a * alpha),
            2.0 * a_lin * ((a_lin - 1.0) - (a_lin + 1.0) * cw),
            a_lin * ((a_lin + 1.0) - (a_lin - 1.0) * cw - 2.0 * sqrt_a * alpha),
            (a_lin + 1.0) + (a_lin - 1.0) * cw + 2.0 * sqrt_a * alpha,
            -2.0 * ((a_lin - 1.0) + (a_lin + 1.0) * cw),
            (a_lin + 1.0) + (a_lin - 1.0) * cw - 2.0 * sqrt_a * alpha,
        ),
        EmphShape::HighShelf => (
            a_lin * ((a_lin + 1.0) + (a_lin - 1.0) * cw + 2.0 * sqrt_a * alpha),
            -2.0 * a_lin * ((a_lin - 1.0) + (a_lin + 1.0) * cw),
            a_lin * ((a_lin + 1.0) + (a_lin - 1.0) * cw - 2.0 * sqrt_a * alpha),
            (a_lin + 1.0) - (a_lin - 1.0) * cw + 2.0 * sqrt_a * alpha,
            2.0 * ((a_lin - 1.0) - (a_lin + 1.0) * cw),
            (a_lin + 1.0) - (a_lin - 1.0) * cw - 2.0 * sqrt_a * alpha,
        ),
    };
    Coeffs {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}

// ── no_std math ───────────────────────────────────────────────────────────

/// cos in f64 via the sin polynomial below.
fn cos64(x: f64) -> f64 {
    sin64(x + core::f64::consts::FRAC_PI_2)
}

/// sin in f64: range-reduced to [−π/2, π/2], 13th-order odd Taylor
/// (~1e-11 abs on the reduced range) — filter-coefficient grade without
/// std/libm.
fn sin64(x: f64) -> f64 {
    let tau = core::f64::consts::TAU;
    let mut r = x - floor64(x / tau + 0.5) * tau;
    if r > core::f64::consts::FRAC_PI_2 {
        r = core::f64::consts::PI - r;
    } else if r < -core::f64::consts::FRAC_PI_2 {
        r = -core::f64::consts::PI - r;
    }
    let r2 = r * r;
    r * (1.0
        + r2 * (-1.0 / 6.0
            + r2 * (1.0 / 120.0
                + r2 * (-1.0 / 5040.0
                    + r2 * (1.0 / 362_880.0 + r2 * (-1.0 / 39_916_800.0))))))
}

fn floor64(x: f64) -> f64 {
    let t = x as i64 as f64;
    if x < t { t - 1.0 } else { t }
}

/// 10^x via the crate's exp2: 10^x = 2^(x·log2 10).
fn pow10(x: f32) -> f32 {
    crate::exp2_approx(x * core::f32::consts::LOG2_10)
}

/// log10 in f64 via bit tricks + atanh-form series on the mantissa
/// (~1e-9 relative).
fn log10_64(x: f64) -> f64 {
    let bits = x.max(1e-300).to_bits();
    let exp = ((bits >> 52) & 0x7FF) as i64 - 1023;
    let mant = f64::from_bits((bits & 0x000F_FFFF_FFFF_FFFF) | 0x3FF0_0000_0000_0000);
    // ln(m) = 2 atanh((m−1)/(m+1)), m ∈ [1,2): 5 series terms suffice.
    let t = (mant - 1.0) / (mant + 1.0);
    let t2 = t * t;
    let ln_m =
        2.0 * t * (1.0 + t2 * (1.0 / 3.0 + t2 * (1.0 / 5.0 + t2 * (1.0 / 7.0 + t2 / 9.0))));
    (exp as f64 + ln_m / core::f64::consts::LN_2) * core::f64::consts::LOG10_2
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec::Vec;

    fn bands(list: &[(EmphShape, f32, f32, f32)]) -> [EmphBand; BANDS] {
        let mut out = [EmphBand::default(); BANDS];
        for (i, &(shape, f, g, q)) in list.iter().enumerate() {
            out[i] = EmphBand { shape, freq_hz: f, gain_db: g, q };
        }
        out
    }

    fn noise(len: usize) -> Vec<f32> {
        // Deterministic white-ish test signal.
        let mut x = 0x12345678u32;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x as f32 / u32::MAX as f32) - 0.5
            })
            .collect()
    }

    // r[verify fx.sat.emphasis.mirror]
    #[test]
    fn the_pair_is_net_flat_for_a_linear_signal() {
        let mut eq = EmphasisEq::new(48_000.0);
        eq.set_bands(&bands(&[
            (EmphShape::Bell, 3000.0, 9.0, 2.0),
            (EmphShape::LowShelf, 120.0, -6.0, 0.9),
            (EmphShape::HighShelf, 8000.0, 12.0, 0.7),
            (EmphShape::Bell, 700.0, -12.0, 4.0),
        ]));
        let input = noise(48_000);
        let mut max_err = 0.0f32;
        for (n, &x) in input.iter().enumerate() {
            let emphasized = eq.pre(x);
            let y = eq.post(emphasized);
            // Skip the filters' settle-in.
            if n > 2000 {
                max_err = max_err.max((y - x).abs());
            }
        }
        assert!(max_err < 1e-3, "pre→post did not cancel: err {max_err}");
    }

    // r[verify fx.sat.emphasis]
    #[test]
    fn a_flat_eq_is_skipped_and_bit_exact() {
        let mut eq = EmphasisEq::new(48_000.0);
        assert!(!eq.is_active());
        for &x in noise(64).iter() {
            assert_eq!(eq.pre(x), x);
            assert_eq!(eq.post(x), x);
        }
        assert_eq!(eq.sigma_gain(), 1.0);
    }

    // r[verify fx.sat.emphasis]
    #[test]
    fn the_curve_reads_back_what_was_asked_for() {
        let mut eq = EmphasisEq::new(48_000.0);
        eq.set_bands(&bands(&[(EmphShape::Bell, 1000.0, 6.0, 1.0)]));
        let peak = eq.magnitude_db(1000.0);
        assert!((peak - 6.0).abs() < 0.2, "bell peak reads {peak} dB");
        let skirt = eq.magnitude_db(100.0);
        assert!(skirt.abs() < 0.5, "far skirt reads {skirt} dB");
    }

    // r[verify fx.sat.emphasis.makeup]
    #[test]
    fn sigma_gain_tracks_broad_boosts() {
        let mut eq = EmphasisEq::new(48_000.0);
        // +6 dB everywhere (a wide tilt-ish boost) must push sigma toward 2×.
        eq.set_bands(&bands(&[
            (EmphShape::LowShelf, 20_000.0, 6.0, 0.5),
        ]));
        let g = eq.sigma_gain();
        assert!(g > 1.5, "broad +6 dB shelf reads sigma gain {g}");
    }
}
