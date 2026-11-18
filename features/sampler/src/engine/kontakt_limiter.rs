//! Kontakt's classic Limiter (group insert kind 3 / effect 0x1c,
//! `BEffectLimiter`, Kontakt 8.7.1 arm64): a soft gain curve from silence
//! up, not a threshold, over a 32-frame line.
//!
//! Ported from KONTRA (`crates/dsp/dynamics/src/kontakt_limiter.rs`,
//! local/inspect-vars e747c01; Apache-2.0), which decoded it from the
//! binary. Per frame, in f32 and in the binary's order (`Process` 0x4f642c):
//!
//! 1. input gain `2^(dB/6)`; the audio goes into a 32-frame line;
//! 2. the louder channel's magnitude `d`; the envelope rises to it at once
//!    and falls linearly toward it by `ln2 / N` per frame, `N` the release
//!    in frames (at least 1);
//! 3. the largest envelope over the last 33 frames (the 32 stored and this
//!    one);
//! 4. an RBJ low-pass biquad on that maximum, `fc = rate/32`, `α = 0.91·sin ω`;
//! 5. gain `1 / (1 + y²/4)` up to `y = 2`, `1 / y` above, on the delayed
//!    audio, clipped at ±1.
//!
//! The envelope and the biquad states carry a tiny constant (`TINY`, the
//! float 0x219392ef) the binary adds on every store; `sin` and `cos` are the
//! binary's polynomials (`Initialize` 0x4f5df4). The slot's own output gain
//! is applied after it, by the insert chain.

const LINE: usize = 32;
/// The float the binary adds to its states on every store.
const TINY: f32 = f32::from_bits(0x2193_92ef);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct KontaktSettings {
    /// Input gain, dB (−24..=24 on the knob).
    pub input_db: f32,
    /// Release, ms (10..=1000 on the knob).
    pub release_ms: f32,
}

/// The detector's low-pass: feedback `c1`, `c2`; feed-forward `b0`, `b1`,
/// `b2`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Biquad {
    pub c1: f32,
    pub c2: f32,
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
}

impl Biquad {
    /// `Initialize` 0x4f5df4: `fc = rate/32` clamped to
    /// `rate/24576..=rate/2.125`, `sin`/`cos` by the binary's polynomials.
    #[must_use]
    pub fn at(rate: u32) -> Self {
        let sr = rate as f32;
        let fc = sr * 0.031_25;
        let (lo, hi) = (sr / 24576.0, sr / 2.125);
        let c = if fc > hi {
            hi
        } else if lo > fc {
            lo
        } else {
            fc
        };
        let w = (f32::from_bits(0x40c9_0fe4) / sr) * c;
        let w2 = w * w;
        let mut p = w2 * f32::from_bits(0xb2cd_4ca8);
        for k in [0x3638_b94f, 0xb950_0c03, 0x3c08_8885, 0xbe2a_aac1] {
            p += f32::from_bits(k);
            p *= w2;
        }
        let sin = w * (p + 1.0);
        let alpha = sin * f32::from_bits(0x3f68_f5c3);
        let a = 1.0 / (alpha + 1.0);
        let mut q = w2 * f32::from_bits(0xb48b_dad9);
        for k in [0x37cf_b5a1, 0xbab6_09bd, 0x3d2a_aa99, 0xbeff_f5e8] {
            q += f32::from_bits(k);
            q *= w2;
        }
        let cos = q + 1.0;
        let om = 1.0 - cos;
        let b02 = (om * 0.5) * a;
        Self {
            c1: (cos + cos) * a,
            c2: a * -(1.0 - alpha),
            b0: b02,
            b1: om * a,
            b2: b02,
        }
    }
}

/// The gain at a smoothed envelope `y`: `1/(1 + y²/4)` up to 2, `1/y` above.
#[must_use]
pub(crate) fn kontakt_gain(y: f32) -> f32 {
    let k = y.min(2.0);
    1.0 / ((y - k * ((k * -0.25) + 1.0)) + 1.0)
}

#[derive(Clone, Debug)]
pub(crate) struct KontaktLimiter {
    input: f32,
    release: f32,
    filter: Biquad,
    line: [[f32; 2]; LINE],
    line_at: usize,
    ring: [f32; LINE],
    ring_at: usize,
    env: f32,
    w1: f32,
    w2: f32,
}

impl KontaktLimiter {
    #[must_use]
    pub fn new(s: KontaktSettings, rate: u32) -> Self {
        let mut l = Self {
            input: 1.0,
            release: 1.0,
            filter: Biquad::at(rate),
            line: [[0.0; 2]; LINE],
            line_at: 0,
            ring: [0.0; LINE],
            ring_at: 0,
            env: TINY,
            w1: 0.0,
            w2: TINY,
        };
        l.set(s, rate);
        l
    }

    /// New knobs while it plays (`SetObj` 0x40372c); the state carries on.
    pub fn set(&mut self, s: KontaktSettings, rate: u32) {
        self.input = (s.input_db * f32::from_bits(0x3e2a_aaab)).exp2();
        let frames = (s.release_ms * f32::from_bits(0x3a83_126f)) * rate as f32;
        self.release = f32::from_bits(0x3f31_7215) / frames.max(1.0);
    }

    pub fn process(&mut self, io: &mut [[f32; 2]]) {
        let f = self.filter;
        for frame in io {
            let s = [frame[0] * self.input, frame[1] * self.input];
            let d = s[0].abs().max(s[1].abs());
            self.line_at = next(self.line_at);
            let delayed = self.line[self.line_at];
            self.line[self.line_at] = s;
            self.ring_at = next(self.ring_at);
            let diff = d - self.env;
            let e = if diff > 0.0 {
                d
            } else {
                self.env + diff * self.release
            };
            self.env = e + TINY;
            let m = self.ring.iter().fold(e, |m, &r| m.max(r));
            self.ring[self.ring_at] = e;
            let w = ((self.w1 * f.c1) + m) + (self.w2 * f.c2);
            let y = ((self.w1 * f.b1) + (f.b0 * w)) + (self.w2 * f.b2);
            self.w2 = self.w1 + TINY;
            self.w1 = w + TINY;
            let g = kontakt_gain(y);
            for (o, x) in frame.iter_mut().zip(delayed) {
                *o = (x * g).max(-1.0).min(1.0);
            }
        }
    }
}

/// The binary's ring step: `i > 30 ? i − 31 : i + 1`.
fn next(i: usize) -> usize {
    if i > LINE - 2 { i + 1 - LINE } else { i + 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coefficients_at_48k() {
        let b = Biquad::at(48_000);
        for (got, want) in [
            (b.c1, 1.665_836_6),
            (b.c2, -0.698_467_2),
            (b.b0, 0.008_157_64),
            (b.b1, 0.016_315_28),
        ] {
            assert!((got - want).abs() < 1e-6, "{got} vs {want}");
        }
    }

    #[test]
    fn static_curve() {
        // An envelope of 1.0 is held 1.938 dB down; 2 and above hit 1.0.
        assert!((20.0 * kontakt_gain(1.0).log10() + 1.938).abs() < 1e-3);
        assert!((kontakt_gain(4.0) * 4.0 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn delay_is_32_frames() {
        let mut l = KontaktLimiter::new(
            KontaktSettings {
                input_db: 0.0,
                release_ms: 20.0,
            },
            48_000,
        );
        let mut io = vec![[0.0f32; 2]; 64];
        io[0] = [1e-3, 1e-3];
        l.process(&mut io);
        let at = io.iter().position(|f| f[0] != 0.0);
        assert_eq!(at, Some(32));
    }

    /// The decode's burst check (simulated from the binary): a +3 dBFS
    /// 1 kHz sine for 20 ms, release 20 ms, peaks at 0.942 out, so the clip
    /// never touches it.
    #[test]
    fn burst_peak() {
        let rate = 48_000;
        let a = 10f32.powf(3.0 / 20.0);
        let mut io = vec![[0.0f32; 2]; 9600];
        for (n, frame) in io.iter_mut().enumerate().skip(480).take(960) {
            let t = (n - 480) as f32 / rate as f32;
            let v = a * (std::f32::consts::TAU * 1000.0 * t).sin();
            *frame = [v, v];
        }
        let mut l = KontaktLimiter::new(
            KontaktSettings {
                input_db: 0.0,
                release_ms: 20.0,
            },
            rate,
        );
        l.process(&mut io);
        let peak = io.iter().map(|f| f[0].abs()).fold(0.0, f32::max);
        assert!((peak - 0.942).abs() < 0.005, "peak {peak}");
    }
}
