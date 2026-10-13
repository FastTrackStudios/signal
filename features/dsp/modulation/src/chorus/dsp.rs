//! Small realtime building blocks shared by the chorus engines.
//!
//! Everything here allocates only in `new`/`resize` and is branch-light in
//! `tick`: a modulated delay line with 4-point Hermite reads, a TPT
//! state-variable lowpass that stays stable while its cutoff moves, the
//! LFO shapes, a one-pole smoother, a soft saturator, and the pitch "tamer"
//! that keeps a delay swing from turning into a seasick warble at fast rates.

use std::f64::consts::PI;

/// A power-of-two ring buffer read at a fractional delay.
///
/// `write` then `read(d)`: delay 0 is the sample just written. Reads are
/// 4-point, 3rd-order Hermite (Catmull-Rom) — Dattorro's recommendation
/// against linear interpolation for chorus is about the modulated HF loss
/// and the "zipper" linear interpolation adds as the fraction sweeps; an
/// allpass interpolator is cheaper but rings when the integer part changes.
pub struct ModLine {
    buf: Vec<f64>,
    mask: usize,
    w: usize,
}

impl ModLine {
    #[must_use]
    pub fn new(min_len: usize) -> Self {
        let len = (min_len + 4).next_power_of_two();
        Self {
            buf: vec![0.0; len],
            mask: len - 1,
            w: 0,
        }
    }

    /// Grow (never shrink) to hold `min_len` samples. Allocates — call from
    /// `update`, not `tick`.
    pub fn ensure(&mut self, min_len: usize) {
        if self.buf.len() < min_len + 4 {
            *self = Self::new(min_len);
        }
    }

    /// Longest delay a read can ask for, in samples.
    #[must_use]
    pub const fn max_delay(&self) -> f64 {
        (self.buf.len() - 4) as f64
    }

    #[inline]
    pub fn write(&mut self, x: f64) {
        self.w = (self.w + 1) & self.mask;
        self.buf[self.w] = x;
    }

    /// Read `d` samples back (clamped to 1 ..= `max_delay`).
    #[inline]
    #[must_use]
    pub fn read(&self, d: f64) -> f64 {
        let d = d.clamp(1.0, self.max_delay());
        let i = d as usize;
        let f = d - i as f64;
        let m = self.mask;
        let base = self.w.wrapping_sub(i);
        let xm1 = self.buf[base.wrapping_add(1) & m];
        let x0 = self.buf[base & m];
        let x1 = self.buf[base.wrapping_sub(1) & m];
        let x2 = self.buf[base.wrapping_sub(2) & m];
        let c1 = 0.5 * (x1 - xm1);
        let c2 = 0.5f64.mul_add(-x2, 2.0f64.mul_add(x1, 2.5f64.mul_add(-x0, xm1)));
        let c3 = 1.5f64.mul_add(x0 - x1, 0.5 * (x2 - xm1));
        c3.mul_add(f, c2).mul_add(f, c1).mul_add(f, x0)
    }

    pub fn clear(&mut self) {
        self.buf.fill(0.0);
    }
}

/// Topology-preserving-transform state-variable lowpass (Simper/Zavalishin).
///
/// Chosen over a direct-form biquad because its cutoff can move every
/// sample without the state blowing up — the BBD's clock-tracking filters
/// do exactly that.
#[derive(Clone, Copy, Default)]
pub struct SvfLp {
    a1: f64,
    a2: f64,
    a3: f64,
    ic1: f64,
    ic2: f64,
}

impl SvfLp {
    #[must_use]
    pub fn new(cutoff: f64, q: f64, sr: f64) -> Self {
        let mut s = Self::default();
        s.set(cutoff, q, sr);
        s
    }

    /// Cutoff is clamped under Nyquist, so a clock-tracked cutoff that runs
    /// past it simply opens the filter.
    #[inline]
    pub fn set(&mut self, cutoff: f64, q: f64, sr: f64) {
        let fc = cutoff.clamp(10.0, sr * 0.45);
        let g = (PI * fc / sr).tan();
        let k = 1.0 / q.max(0.1);
        self.a1 = 1.0 / g.mul_add(g + k, 1.0);
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    #[inline]
    pub fn tick(&mut self, x: f64) -> f64 {
        let v3 = x - self.ic2;
        let v1 = self.a1.mul_add(self.ic1, self.a2 * v3);
        let v2 = self.a2.mul_add(self.ic1, self.a3.mul_add(v3, self.ic2));
        self.ic1 = flush(2.0f64.mul_add(v1, -self.ic1));
        self.ic2 = flush(2.0f64.mul_add(v2, -self.ic2));
        v2
    }

    pub const fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }
}

#[inline]
fn flush(x: f64) -> f64 {
    if x.abs() < 1.0e-20 { 0.0 } else { x }
}

/// One-pole parameter smoother.
#[derive(Clone, Copy)]
pub struct Smooth {
    pub value: f64,
    k: f64,
}

impl Smooth {
    #[must_use]
    pub const fn new(value: f64) -> Self {
        Self { value, k: 0.001 }
    }

    /// Time constant in ms.
    pub fn set_time(&mut self, ms: f64, sr: f64) {
        self.k = 1.0 - (-1000.0 / (ms.max(0.01) * sr)).exp();
    }

    #[inline]
    pub fn tick(&mut self, target: f64) -> f64 {
        self.value = (target - self.value).mul_add(self.k, self.value);
        self.value
    }
}

/// A rounded triangle: `asin(r·sin θ)/asin(r)`.
///
/// `r → 0` is a sine, `r → 1` a triangle. An analogue triangle LFO is never
/// perfectly sharp — the op-amp integrator's corners round off — and a
/// perfectly sharp one makes the pitch (the delay's slope) flip from sharp to
/// flat in one sample, which is audible as a tick on sustained notes. `r` of
/// 0.97–0.99 keeps the triangle's even "two detuned voices" pitch with soft
/// corners.
#[inline]
#[must_use]
pub fn rounded_tri(phase: f64, r: f64) -> f64 {
    let s = (phase * 2.0 * PI).sin();
    if r < 0.02 {
        s
    } else {
        (r * s).asin() / r.asin()
    }
}

/// Peak slope of [`rounded_tri`] per unit amplitude per Hz — `2π` for a
/// sine, `4` for a triangle.
#[must_use]
pub fn rounded_tri_slope(r: f64) -> f64 {
    if r < 0.02 {
        2.0 * PI
    } else {
        2.0 * PI * r / r.asin()
    }
}

/// Cap the pitch a delay swing produces, softly.
///
/// A swing of `swing_s` seconds on an LFO at `rate_hz` bends the pitch by
/// `1200·log2(1 + slope·rate·swing)` cents. Analogue pedals let that grow
/// with the rate until the chorus is an out-of-tune vibrato; here the swing
/// is soft-limited so the peak deviation approaches `ceiling_cents` and never
/// passes it. At the rates a chorus is played at it changes nothing.
#[inline]
#[must_use]
pub fn tame_swing(swing_s: f64, rate_hz: f64, slope: f64, ceiling_cents: f64) -> f64 {
    let dev = (ceiling_cents / 1200.0).exp2() - 1.0;
    let a_max = dev / (slope * rate_hz.max(1.0e-3));
    // A 4th-order knee: under half the ceiling it changes nothing that
    // matters (< 0.4%), and it still never passes it.
    swing_s / (1.0 + (swing_s / a_max).powi(4)).sqrt().sqrt()
}

/// Peak pitch deviation, in cents, of a swing (the inverse of [`tame_swing`]).
#[must_use]
pub fn swing_cents(swing_s: f64, rate_hz: f64, slope: f64) -> f64 {
    1200.0 * (slope * rate_hz).mul_add(swing_s, 1.0).log2()
}

/// Gentle odd-order soft saturation, unity gain at small signals: the BBD's
/// headroom (and the feedback loop's safety net).
#[inline]
#[must_use]
pub fn soft_sat(x: f64, k: f64) -> f64 {
    x / x.mul_add(x * k, 1.0).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modline_integer_reads_are_exact() {
        let mut l = ModLine::new(64);
        for i in 0..40 {
            l.write(f64::from(i));
        }
        // Just wrote 39: delay 5 is 34.
        assert!((l.read(5.0) - 34.0).abs() < 1e-12);
        assert!((l.read(5.5) - 33.5).abs() < 1e-12);
    }

    #[test]
    fn rounded_tri_spans_unity_and_slopes_match() {
        for r in [0.0, 0.5, 0.97, 0.999] {
            let (mut lo, mut hi, mut slope) = (1.0f64, -1.0f64, 0.0f64);
            let n = 100_000i32;
            let mut prev = rounded_tri(0.0, r);
            for i in 1..n {
                let v = rounded_tri(f64::from(i) / f64::from(n), r);
                lo = lo.min(v);
                hi = hi.max(v);
                slope = slope.max((v - prev).abs() * f64::from(n));
                prev = v;
            }
            assert!((hi - 1.0).abs() < 1e-3 && (lo + 1.0).abs() < 1e-3, "r {r}");
            assert!(
                (slope / rounded_tri_slope(r) - 1.0).abs() < 0.02,
                "r {r}: {slope}"
            );
        }
    }

    #[test]
    fn tamer_never_passes_its_ceiling() {
        for rate in [0.1, 1.0, 5.0, 20.0] {
            for swing_ms in [0.1, 1.0, 5.0, 20.0] {
                let s = tame_swing(swing_ms * 1e-3, rate, 2.0 * PI, 60.0);
                assert!(swing_cents(s, rate, 2.0 * PI) <= 60.0 + 1e-9);
            }
        }
        // Slow and shallow: untouched (within 1%).
        let s = tame_swing(2e-3, 0.5, 2.0 * PI, 60.0);
        assert!((s / 2e-3 - 1.0).abs() < 0.01);
    }
}
