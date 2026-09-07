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

use dsp_core::num;

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
    #[must_use]
    pub const fn from_index(i: u32) -> Self {
        match i {
            1 => Self::LowShelf,
            2 => Self::HighShelf,
            _ => Self::Bell,
        }
    }
}

/// Below this the band is treated as off — the section is skipped entirely,
/// which is what makes a flat EQ bit-exact rather than merely close.
const INAUDIBLE_DB: f32 = 0.01;

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

/// DF1 state for one section.
#[derive(Debug, Clone, Copy, Default)]
struct State {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl State {
    /// One sample through `H(z)`.
    #[inline]
    fn step(&mut self, c: &Coeffs, x: f32) -> f32 {
        let y = c.b0 * x + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;
        self.push(x, y);
        y
    }

    /// One sample through `1/H(z)`: numerator and denominator swapped, which
    /// is why `b0` divides rather than multiplies.
    #[inline]
    fn step_inverse(&mut self, c: &Coeffs, x: f32) -> f32 {
        let y = (x + c.a1 * self.x1 + c.a2 * self.x2 - c.b1 * self.y1 - c.b2 * self.y2) / c.b0;
        self.push(x, y);
        y
    }

    #[inline]
    const fn push(&mut self, x: f32, y: f32) {
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
    }
}

/// One band, designed: its coefficients and both filter histories.
///
/// These were four parallel `[_; BANDS]` arrays walked by index. Keeping them
/// in one struct means the coefficients and the history they belong to cannot
/// drift out of step, and the passes below become a fold over sections rather
/// than an indexed loop the compiler has to be trusted about.
#[derive(Debug, Clone, Copy, Default)]
struct Section {
    coeffs: Coeffs,
    /// False when the band's gain is inaudible — the section is skipped
    /// entirely, which is also why a flat EQ is bit-exact rather than merely
    /// close.
    active: bool,
    pre: State,
    post: State,
}

/// The pair, per audio channel: instantiate one per channel like the
/// preamps ([`crate::preamp::ClassAPreamp`] is per-channel in the plugin).
#[derive(Default)]
pub struct EmphasisEq {
    bands: [EmphBand; BANDS],
    sections: [Section; BANDS],
    sample_rate: f32,
    /// Pink-weighted RMS gain of the emphasis curve — what it does to the
    /// level reaching the shaper. Feed into the preamp's makeup calibration
    /// (`fx.sat.emphasis.makeup`).
    sigma_gain: f32,
}

impl EmphasisEq {
    #[must_use]
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
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.sections.iter().any(|section| section.active)
    }

    /// Design all sections from the band table. Setter-path arithmetic —
    /// never call from `process`-per-sample, but per block is fine (every
    /// section is a handful of ops and there is no allocation).
    pub fn set_bands(&mut self, bands: &[EmphBand; BANDS]) {
        self.bands = *bands;
        for (section, band) in self.sections.iter_mut().zip(bands) {
            section.active = band.gain_db.abs() >= INAUDIBLE_DB;
            section.coeffs = if section.active {
                design(band, self.sample_rate)
            } else {
                Coeffs::default()
            };
        }
        self.sigma_gain = self.compute_sigma_gain();
    }

    /// The bands as set.
    #[must_use]
    pub const fn bands(&self) -> &[EmphBand; BANDS] {
        &self.bands
    }

    #[must_use]
    pub const fn sigma_gain(&self) -> f32 {
        self.sigma_gain
    }

    /// Emphasis (pre-stage) pass for channel `ch`'s sample.
    #[inline]
    pub fn pre(&mut self, x: f32) -> f32 {
        self.sections
            .iter_mut()
            .filter(|section| section.active)
            .fold(x, |v, section| {
                let coeffs = section.coeffs;
                section.pre.step(&coeffs, v)
            })
    }

    /// De-emphasis (post-stage) pass: the exact inverse sections, in reverse
    /// order. `H⁻¹ = (1 + a1 z⁻¹ + a2 z⁻²) / (b0 + b1 z⁻¹ + b2 z⁻²)`.
    #[inline]
    pub fn post(&mut self, x: f32) -> f32 {
        self.sections
            .iter_mut()
            .rev()
            .filter(|section| section.active)
            .fold(x, |v, section| {
                let coeffs = section.coeffs;
                section.post.step_inverse(&coeffs, v)
            })
    }

    pub fn reset(&mut self) {
        for section in &mut self.sections {
            section.pre = State::default();
            section.post = State::default();
        }
    }

    /// The emphasis curve's magnitude in dB at `freq` — what the editor
    /// draws (`fx.sat.emphasis.display`).
    #[must_use]
    pub fn magnitude_db(&self, freq: f32) -> f32 {
        self.sections
            .iter()
            .filter(|section| section.active)
            .map(|section| section_mag_db(&section.coeffs, freq, self.sample_rate))
            .sum()
    }

    /// Pink-weighted RMS gain over 20 Hz–20 kHz: equal power per octave, so
    /// log-spaced sample points weight equally.
    fn compute_sigma_gain(&self) -> f32 {
        /// Log-spaced probe points across the audible band.
        const POINTS: usize = 24;

        if !self.is_active() {
            return 1.0;
        }
        let last = num::count_to_f32(POINTS.saturating_sub(1));
        let sum: f32 = (0..POINTS)
            .map(|k| {
                // 20 Hz … 20 kHz, log spaced: 20 * 10^(3k/(N-1)).
                let f = 20.0 * pow10(3.0 * num::count_to_f32(k) / last);
                let g = crate::db_to_gain(self.magnitude_db(f));
                g * g
            })
            .sum();
        crate::sqrt_approx(sum / num::count_to_f32(POINTS)).clamp(0.05, 20.0)
    }
}

/// |H| of one section at `freq`, in dB.
///
/// Evaluated in f64: at the band's own centre the quadratic form cancels
/// down four orders of magnitude, so f32 trig reads the peak wrong.
fn section_mag_db(c: &Coeffs, freq: f32, sample_rate: f32) -> f32 {
    let w = core::f64::consts::TAU * (f64::from(freq) / f64::from(sample_rate)).clamp(0.0, 0.5);
    let (cw, c2w) = (cos64(w), cos64(2.0 * w));
    let (b0, b1, b2) = (f64::from(c.b0), f64::from(c.b1), f64::from(c.b2));
    let (a1, a2) = (f64::from(c.a1), f64::from(c.a2));
    // |B(e^jw)|² for b0 + b1 z⁻¹ + b2 z⁻²:
    let num = b0 * b0 + b1 * b1 + b2 * b2 + 2.0 * (b0 * b1 + b1 * b2) * cw + 2.0 * b0 * b2 * c2w;
    let den = 1.0 + a1 * a1 + a2 * a2 + 2.0 * (a1 + a1 * a2) * cw + 2.0 * a2 * c2w;
    crate::num::narrow(10.0 * log10_64((num / den.max(1e-30)).max(1e-30)))
}

/// RBJ peak / shelf design, α = sin(w0)/(2Q).
fn design(band: &EmphBand, sample_rate: f32) -> Coeffs {
    let f = band.freq_hz.clamp(10.0, sample_rate * 0.45);
    let q = band.q.clamp(0.05, 18.0);
    // A = 10^(gain/40).
    let a_lin = pow10(band.gain_db.clamp(-24.0, 24.0) / 40.0);
    let w0 = core::f64::consts::TAU * f64::from(f) / f64::from(sample_rate);
    let cw = num::narrow(cos64(w0));
    let sw = num::narrow(sin64(w0));
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
                + r2 * (-1.0 / 5040.0 + r2 * (1.0 / 362_880.0 + r2 * (-1.0 / 39_916_800.0))))))
}

fn floor64(x: f64) -> f64 {
    num::floor_f64(x)
}

/// 10^x via the crate's exp2: 10^x = 2^(x·log2 10).
fn pow10(x: f32) -> f32 {
    crate::exp2_approx(x * core::f32::consts::LOG2_10)
}

/// log10 in f64 via bit tricks + atanh-form series on the mantissa
/// (~1e-9 relative).
fn log10_64(x: f64) -> f64 {
    let bits = x.max(1e-300).to_bits();
    let exp = i32::try_from((bits >> 52) & 0x7FF)
        .unwrap_or(0)
        .saturating_sub(1023);
    let mant = f64::from_bits((bits & 0x000F_FFFF_FFFF_FFFF) | 0x3FF0_0000_0000_0000);
    // ln(m) = 2 atanh((m−1)/(m+1)), m ∈ [1,2): 5 series terms suffice.
    let t = (mant - 1.0) / (mant + 1.0);
    let t2 = t * t;
    let ln_m = 2.0 * t * (1.0 + t2 * (1.0 / 3.0 + t2 * (1.0 / 5.0 + t2 * (1.0 / 7.0 + t2 / 9.0))));
    (f64::from(exp) + ln_m / core::f64::consts::LN_2) * core::f64::consts::LOG10_2
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec::Vec;

    fn bands(list: &[(EmphShape, f32, f32, f32)]) -> [EmphBand; BANDS] {
        let mut out = [EmphBand::default(); BANDS];
        for (i, &(shape, f, g, q)) in list.iter().enumerate() {
            out[i] = EmphBand {
                shape,
                freq_hz: f,
                gain_db: g,
                q,
            };
        }
        out
    }

    fn noise(len: usize) -> Vec<f32> {
        // Deterministic white-ish test signal.
        let mut x = 0x1234_5678_u32;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                num::narrow(f64::from(x) / f64::from(u32::MAX)) - 0.5
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
        for &x in &noise(64) {
            // Bit patterns: an inactive section is skipped entirely, so this
            // is exactness, not tolerance.
            assert_eq!(eq.pre(x).to_bits(), x.to_bits());
            assert_eq!(eq.post(x).to_bits(), x.to_bits());
        }
        assert_eq!(eq.sigma_gain().to_bits(), 1.0_f32.to_bits());
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
        eq.set_bands(&bands(&[(EmphShape::LowShelf, 20_000.0, 6.0, 0.5)]));
        let g = eq.sigma_gain();
        assert!(g > 1.5, "broad +6 dB shelf reads sigma gain {g}");
    }
}
