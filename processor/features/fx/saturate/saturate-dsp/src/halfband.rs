//! Linear-phase oversampling: cascaded 2× half-band FIR stages.
//!
//! Each stage is a Kaiser-windowed half-band (`4K − 1` taps, every other one
//! zero, centre ½), designed at construction — no tables. A 2× round trip
//! (interpolate, run at the high rate, decimate) delays by a whole number
//! of low-rate samples (`2K − 1` or `2K`), so the dry path can be aligned to the sample and a mix
//! of the two stays coherent.
//!
//! The two stages used for 4×:
//! - outer (base rate ↔ 2×): `K = 16`, β = 8 — flat (±0.001 dB) to 0.208·fs,
//!   ≥ 74 dB down from 0.292·fs (20 kHz / 28 kHz at 48 kHz);
//! - inner (2× ↔ 4×): `K = 6`, β = 8 — ≥ 75 dB down past 0.375 of its rate.
//!
//! Latency: 2× is 31 base-rate samples, 4× is 37.

use core::f64::consts::PI;

/// Largest supported `K` (history per stage is `2K`).
const MAX_K: usize = 16;

/// The outer stage's half length.
pub const OUTER_K: usize = 16;
/// The inner stage's half length.
pub const INNER_K: usize = 6;

/// Zeroth-order modified Bessel function, by its power series.
fn bessel_i0(x: f64) -> f64 {
    let q = 0.25 * x * x;
    let (mut term, mut sum) = (1.0, 1.0);
    for k in 1..60_u32 {
        let kf = f64::from(k);
        term *= q / (kf * kf);
        sum += term;
        if term < 1e-17 * sum {
            break;
        }
    }
    sum
}

/// The non-zero off-centre taps `h[c ± (2j+1)]`, `j = 0..K`, of a
/// Kaiser-windowed half-band, normalised so the filter's DC gain is exactly 1.
fn design(k: usize, beta: f64) -> [f64; MAX_K] {
    let mut taps = [0.0; MAX_K];
    let half = dsp_core::num::count_to_f64(k.saturating_mul(2).saturating_sub(1));
    let i0b = bessel_i0(beta);
    let mut sum = 0.0;
    let mut sign = 1.0;
    for (j, tap) in taps.iter_mut().enumerate().take(k) {
        // offset 2j+1 from the centre: sinc(d/2)/2 = ±1/(πd), Kaiser-windowed
        let offset = dsp_core::num::count_to_f64(j.saturating_mul(2).saturating_add(1));
        let r = offset / half;
        let window = bessel_i0(beta * libm::sqrt((1.0 - r * r).max(0.0))) / i0b;
        *tap = sign / (PI * offset) * window;
        sum += *tap;
        sign = -sign;
    }
    // centre ½ plus both sides must make 1
    let scale = 0.25 / sum;
    for t in &mut taps {
        *t *= scale;
    }
    taps
}

/// Low-rate history with a contiguous oldest→newest window of `2K` samples.
#[derive(Clone, Copy, Debug)]
struct History {
    buf: [f64; 4 * MAX_K],
    pos: usize,
    len: usize,
}

impl History {
    const fn new(len: usize) -> Self {
        Self { buf: [0.0; 4 * MAX_K], pos: 0, len }
    }

    #[inline]
    fn push(&mut self, sample: f64) {
        let next = self.pos.saturating_add(1);
        self.pos = if next >= self.len { 0 } else { next };
        if let Some(s) = self.buf.get_mut(self.pos) {
            *s = sample;
        }
        if let Some(s) = self.buf.get_mut(self.pos.saturating_add(self.len)) {
            *s = sample;
        }
    }

    /// Oldest first, newest last.
    #[inline]
    fn window(&self) -> &[f64] {
        let from = self.pos.saturating_add(1);
        self.buf.get(from..from.saturating_add(self.len)).unwrap_or(&[])
    }

    const fn clear(&mut self) {
        self.buf = [0.0; 4 * MAX_K];
    }
}

/// `Σ_j h_j·(w[K−1−j] + w[K+j])` — the symmetric half-band sum over a
/// `2K` window.
#[inline]
fn fold(taps: &[f64], w: &[f64]) -> f64 {
    let k = taps.len();
    let (lo, hi) = w.split_at(k.min(w.len()));
    taps.iter().zip(lo.iter().rev()).zip(hi.iter()).map(|((h, a), b)| h * (a + b)).sum()
}

/// One 2× stage: interpolator and decimator sharing a design.
#[derive(Clone, Copy, Debug)]
struct Stage {
    taps: [f64; MAX_K],
    k: usize,
    /// Decimate one low-rate sample earlier (`K − 1` instead of `K`).
    lean: bool,
    up: History,
    /// The decimator's odd-phase samples (the taps).
    down_odd: History,
    /// Its even-phase samples (the centre), `K` (or `K − 1`) back.
    down_even: [f64; MAX_K + 1],
    even_pos: usize,
}

impl Stage {
    fn new(k: usize, beta: f64, lean: bool) -> Self {
        let k = k.clamp(1, MAX_K);
        let span = k.saturating_mul(2);
        Self { taps: design(k, beta), k, lean, up: History::new(span), down_odd: History::new(span), down_even: [0.0; MAX_K + 1], even_pos: 0 }
    }

    fn taps(&self) -> &[f64] {
        self.taps.get(..self.k).unwrap_or(&[])
    }

    /// One low-rate sample in, two high-rate samples out (earlier first),
    /// delayed by `K` low-rate samples.
    #[inline]
    fn up(&mut self, sample: f64) -> [f64; 2] {
        self.up.push(sample);
        let window = self.up.window();
        let centre = window.get(self.k.saturating_sub(1)).copied().unwrap_or(0.0);
        [centre, 2.0 * fold(self.taps(), window)]
    }

    /// Two high-rate samples in (earlier first), one low-rate out, delayed by
    /// `K` (lean: `K − 1`) low-rate samples.
    #[inline]
    fn down(&mut self, [even, odd]: [f64; 2]) -> f64 {
        let len = if self.lean { self.k } else { self.k.saturating_add(1) };
        let next = self.even_pos.saturating_add(1);
        self.even_pos = if next >= len { 0 } else { next };
        if let Some(s) = self.down_even.get_mut(self.even_pos) {
            *s = even;
        }
        // the slot after the newest is the oldest
        let after = self.even_pos.saturating_add(1);
        let oldest = if after >= len { 0 } else { after };
        let centre = self.down_even.get(oldest).copied().unwrap_or(0.0);
        if self.lean {
            self.down_odd.push(odd);
            0.5 * centre + fold(self.taps(), self.down_odd.window())
        } else {
            let y = 0.5 * centre + fold(self.taps(), self.down_odd.window());
            self.down_odd.push(odd);
            y
        }
    }

    const fn reset(&mut self) {
        self.up.clear();
        self.down_odd.clear();
        self.down_even = [0.0; MAX_K + 1];
    }
}

/// 1×, 2× or 4× oversampling around a per-sample function.
#[derive(Clone, Copy, Debug)]
pub struct Oversampler {
    factor: usize,
    outer: Stage,
    inner: Stage,
}

impl Oversampler {
    /// `factor` is 1, 2 or 4 (anything else is taken as the nearest).
    #[must_use]
    pub fn new(factor: usize) -> Self {
        let factor = match factor {
            0 | 1 => 1,
            2 | 3 => 2,
            _ => 4,
        };
        // the inner round trip must be a whole number of base-rate samples
        let inner = Stage::new(INNER_K, 8.0, false);
        Self { factor, outer: Stage::new(OUTER_K, 8.0, true), inner }
    }

    #[must_use]
    pub const fn factor(&self) -> usize {
        self.factor
    }

    /// Round-trip delay in base-rate samples.
    #[must_use]
    pub const fn latency(&self) -> usize {
        match self.factor {
            1 => 0,
            2 => 2 * OUTER_K - 1,
            _ => 2 * OUTER_K - 1 + INNER_K,
        }
    }

    pub const fn reset(&mut self) {
        self.outer.reset();
        self.inner.reset();
    }

    /// Run `f` on `x` at `factor` times the rate.
    #[inline]
    pub fn process(&mut self, sample: f64, mut run: impl FnMut(f64) -> f64) -> f64 {
        match self.factor {
            1 => run(sample),
            2 => {
                let [first, second] = self.outer.up(sample);
                let out = [run(first), run(second)];
                self.outer.down(out)
            }
            _ => {
                let [first, second] = self.outer.up(sample);
                let [a0, a1] = self.inner.up(first);
                let [b0, b1] = self.inner.up(second);
                let (y0, y1, y2, y3) = (run(a0), run(a1), run(b0), run(b1));
                let lo = self.inner.down([y0, y1]);
                let hi = self.inner.down([y2, y3]);
                self.outer.down([lo, hi])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse(factor: usize) -> [f64; 128] {
        let mut os = Oversampler::new(factor);
        let mut out = [0.0; 128];
        for (i, o) in out.iter_mut().enumerate() {
            *o = os.process(if i == 0 { 1.0 } else { 0.0 }, |v| v);
        }
        out
    }

    #[test]
    fn round_trip_is_a_symmetric_pulse_at_the_reported_latency() {
        for factor in [1, 2, 4] {
            let h = impulse(factor);
            let lat = Oversampler::new(factor).latency();
            let peak = h.iter().enumerate().fold((0, 0.0_f64), |(bi, bv), (i, v)| if v.abs() > bv { (i, v.abs()) } else { (bi, bv) });
            assert_eq!(peak.0, lat, "factor {factor}");
            for d in 1..lat {
                let (a, b) = (h[lat - d], h[lat + d]);
                assert!((a - b).abs() < 1e-12, "factor {factor} asymmetric at ±{d}: {a} {b}");
            }
            let dc: f64 = h.iter().sum();
            assert!((dc - 1.0).abs() < 1e-9, "factor {factor} dc {dc}");
        }
    }
}
