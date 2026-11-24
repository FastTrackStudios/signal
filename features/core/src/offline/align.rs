//! `align`: lining a mic up with another, as an offline block whose result
//! is static values.
//!
//! - `delay_samples`: a whole-sample shift, either way. Moving a track
//!   *earlier* needs the future, so the block reports the advance it was
//!   built with as latency and delays by that much more: the host's delay
//!   compensation takes the latency away and leaves the shift. The advance
//!   is fixed when the block is built (an analysis sets the value, then
//!   the chain is rebuilt), so the latency never moves while it plays; with
//!   no advance it is 0.
//! - `invert`: the polarity, exactly (a negation).
//! - the phase correction: up to two second-order all-pass sections
//!   (`ap1_hz`/`ap1_q`, `ap2_hz`/`ap2_q`: RBJ all-pass biquads) and one
//!   first-order section (`ap0_hz`), each off at 0 Hz. An all-pass moves
//!   phase and nothing else: the magnitude is flat to numerical precision,
//!   and it adds no latency, so it runs live. Studio's `LISTEN_ALIGN`
//!   models and searches exactly these sections, so its predictions hold.
//!
//! [`find`] (feature `offline`) is the plain delay-and-polarity search for
//! a host without `LISTEN_ALIGN`.

use crate::block::BlockProcessor;
use crate::param::{ParamSpec, Unit};
use alloc::vec::Vec;

/// The block's name.
pub const NAME: &str = "align";

pub const DELAY: usize = 0;
pub const INVERT: usize = 1;
pub const AP1_HZ: usize = 2;
pub const AP1_Q: usize = 3;
pub const AP2_HZ: usize = 4;
pub const AP2_Q: usize = 5;
pub const AP0_HZ: usize = 6;

/// The longest shift either way, frames: 100 ms at 48 kHz.
pub const MAX_SHIFT: usize = 4800;

pub static PARAMS: [ParamSpec; 7] = [
    ParamSpec::new(
        "delay_samples",
        "Delay",
        Unit::Plain(0),
        -4800.0,
        4800.0,
        0.0,
    ),
    ParamSpec::new("invert", "Invert", Unit::Toggle, 0.0, 1.0, 0.0),
    ParamSpec::new("ap1_hz", "All-Pass 1", Unit::Hz, 0.0, 20000.0, 0.0),
    ParamSpec::new("ap1_q", "All-Pass 1 Q", Unit::Plain(2), 0.1, 10.0, 0.707),
    ParamSpec::new("ap2_hz", "All-Pass 2", Unit::Hz, 0.0, 20000.0, 0.0),
    ParamSpec::new("ap2_q", "All-Pass 2 Q", Unit::Plain(2), 0.1, 10.0, 0.707),
    ParamSpec::new(
        "ap0_hz",
        "All-Pass (1st order)",
        Unit::Hz,
        0.0,
        20000.0,
        0.0,
    ),
];

/// One all-pass section: second order (an RBJ all-pass biquad) or first
/// order; transposed direct form II, f64.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AllPass {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
    s: [[f64; 2]; 2],
}

impl AllPass {
    /// The RBJ all-pass biquad at `hz` (−180° there), of width `q`.
    #[must_use]
    pub fn second(hz: f64, q: f64, rate: f64) -> Self {
        let w = 2.0 * core::f64::consts::PI * (hz / rate).clamp(1e-6, 0.4999);
        let alpha = libm::sin(w) / (2.0 * q.max(1e-3));
        let a0 = 1.0 + alpha;
        let c = libm::cos(w);
        Self {
            b0: (1.0 - alpha) / a0,
            b1: -2.0 * c / a0,
            b2: 1.0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha) / a0,
            s: [[0.0; 2]; 2],
        }
    }

    /// The first-order all-pass at `hz` (−90° there).
    #[must_use]
    pub fn first(hz: f64, rate: f64) -> Self {
        let t = libm::tan(core::f64::consts::PI * (hz / rate).clamp(1e-6, 0.4999));
        let a = (t - 1.0) / (t + 1.0);
        Self {
            b0: a,
            b1: 1.0,
            b2: 0.0,
            a1: a,
            a2: 0.0,
            s: [[0.0; 2]; 2],
        }
    }

    /// Its response at `hz`: (magnitude, phase in radians), from its
    /// coefficients.
    #[must_use]
    pub fn response(&self, hz: f64, rate: f64) -> (f64, f64) {
        let w = 2.0 * core::f64::consts::PI * hz / rate;
        let (c1, s1, c2, s2) = (
            libm::cos(w),
            libm::sin(w),
            libm::cos(2.0 * w),
            libm::sin(2.0 * w),
        );
        let (nr, ni) = (
            self.b0 + self.b1 * c1 + self.b2 * c2,
            -(self.b1 * s1) - self.b2 * s2,
        );
        let (dr, di) = (
            1.0 + self.a1 * c1 + self.a2 * c2,
            -(self.a1 * s1) - self.a2 * s2,
        );
        (
            libm::sqrt((nr * nr + ni * ni) / (dr * dr + di * di)),
            libm::atan2(ni, nr) - libm::atan2(di, dr),
        )
    }

    #[inline]
    fn tick(&mut self, ch: usize, x: f64) -> f64 {
        let Some(s) = self.s.get_mut(ch) else {
            return x;
        };
        let y = self.b0.mul_add(x, s[0]);
        s[0] = self.b1.mul_add(x, self.a1.mul_add(-y, s[1]));
        s[1] = self.b2.mul_add(x, -self.a2 * y);
        y
    }

    fn clear(&mut self) {
        self.s = [[0.0; 2]; 2];
    }
}

/// The apply stage.
pub struct AlignApply {
    sample_rate: f64,
    values: [f32; 7],
    /// The advance it was built with: its latency, frames.
    advance: usize,
    /// The delay it runs: `advance + delay_samples`, frames, >= 0.
    delay: usize,
    sign: f32,
    line: [Vec<f32>; 2],
    write: usize,
    /// The sections on (preallocated: at most three).
    sections: [Option<AllPass>; 3],
}

impl AlignApply {
    /// Built with `delay_samples` as its spec has it: an advance (a
    /// negative delay) becomes its latency.
    #[must_use]
    pub fn new(sample_rate: f32, delay_samples: f32) -> Self {
        let d = PARAMS[DELAY].clamp(delay_samples);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let advance = if d < 0.0 { (-d) as usize } else { 0 };
        let len = advance + MAX_SHIFT + 1;
        let mut s = Self {
            sample_rate: f64::from(sample_rate.max(1.0)),
            values: PARAMS.map(|p| p.default),
            advance,
            delay: advance,
            sign: 1.0,
            line: [alloc::vec![0.0; len], alloc::vec![0.0; len]],
            write: 0,
            sections: [None; 3],
        };
        s.set_param(DELAY, d);
        s
    }

    fn redesign(&mut self) {
        let v = |i: usize| f64::from(self.values[i]);
        let rate = self.sample_rate;
        let keep = |old: Option<AllPass>, new: Option<AllPass>| match (old, new) {
            // A section re-tuned keeps its state: no click on a change.
            (Some(o), Some(mut n)) => {
                n.s = o.s;
                Some(n)
            }
            (_, n) => n,
        };
        let on = |hz: f64| hz > 0.0;
        self.sections = [
            keep(
                self.sections[0],
                on(v(AP1_HZ)).then(|| AllPass::second(v(AP1_HZ), v(AP1_Q), rate)),
            ),
            keep(
                self.sections[1],
                on(v(AP2_HZ)).then(|| AllPass::second(v(AP2_HZ), v(AP2_Q), rate)),
            ),
            keep(
                self.sections[2],
                on(v(AP0_HZ)).then(|| AllPass::first(v(AP0_HZ), rate)),
            ),
        ];
    }

    /// The sections it runs.
    pub fn sections(&self) -> impl Iterator<Item = &AllPass> {
        self.sections.iter().flatten()
    }
}

impl BlockProcessor for AlignApply {
    fn params(&self) -> &'static [ParamSpec] {
        &PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let Some(slot) = self.values.get_mut(index) else {
            return;
        };
        *slot = value;
        match index {
            DELAY => {
                #[allow(clippy::cast_possible_truncation)]
                let d = libm::roundf(value) as i64;
                let cap = i64::try_from(MAX_SHIFT).unwrap_or(0);
                let adv = i64::try_from(self.advance).unwrap_or(0);
                self.delay = usize::try_from((adv + d.min(cap)).max(0)).unwrap_or(0);
            }
            INVERT => self.sign = if value >= 0.5 { -1.0 } else { 1.0 },
            _ => self.redesign(),
        }
    }

    fn reset(&mut self) {
        for l in &mut self.line {
            l.fill(0.0);
        }
        for s in self.sections.iter_mut().flatten() {
            s.clear();
        }
    }

    fn latency(&self) -> u32 {
        u32::try_from(self.advance).unwrap_or(u32::MAX)
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = self.line[0].len();
        let frames = left.len().min(right.len());
        if n == 0 || frames == 0 {
            return;
        }
        let back = n - self.delay.min(n - 1);
        for (ch, buf) in [left, right].into_iter().enumerate() {
            let Some(line) = self.line.get_mut(ch) else {
                continue;
            };
            let mut w = self.write;
            for v in buf.iter_mut().take(frames) {
                if let Some(slot) = line.get_mut(w) {
                    *slot = *v;
                }
                let mut y = f64::from(line.get((w + back) % n).copied().unwrap_or(0.0) * self.sign);
                for s in self.sections.iter_mut().flatten() {
                    y = s.tick(ch, y);
                }
                #[allow(clippy::cast_possible_truncation)]
                {
                    *v = y as f32;
                }
                w = (w + 1) % n;
            }
        }
        self.write = (self.write + frames) % n;
    }
}

#[cfg(feature = "offline")]
pub use search::*;

#[cfg(feature = "offline")]
mod search {
    /// What [`find`] found: the shift that best lines `target` up with
    /// `reference`, and whether it is upside down.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Found {
        /// Frames to delay `target` by (negative: earlier).
        pub delay_samples: i32,
        pub invert: bool,
        /// The normalized correlation at it, 0..1.
        pub correlation: f64,
    }

    /// The shift within `max_shift` frames either way, and the polarity,
    /// at which `target` best matches `reference`: the peak of their
    /// cross-correlation, by magnitude (a negative peak is an inverted
    /// polarity). Brute force: give it a few seconds that hold the hits.
    #[must_use]
    pub fn find(reference: &[f64], target: &[f64], max_shift: usize) -> Found {
        let n = reference.len().min(target.len());
        let energy = |x: &[f64]| x.iter().map(|v| v * v).sum::<f64>();
        let norm = libm::sqrt(energy(&reference[..n]) * energy(&target[..n])).max(1e-30);
        let max = i64::try_from(max_shift).unwrap_or(0);
        let mut best = (0_i64, 0.0_f64);
        for lag in -max..=max {
            // target delayed by `lag` against the reference: sum r[i] t[i - lag].
            let mut acc = 0.0;
            for (i, r) in reference.iter().enumerate().take(n) {
                let j = i64::try_from(i).unwrap_or(0) - lag;
                if let Some(t) = usize::try_from(j).ok().and_then(|j| target.get(j)) {
                    acc += r * t;
                }
            }
            if acc.abs() > best.1.abs() {
                best = (lag, acc);
            }
        }
        Found {
            delay_samples: i32::try_from(best.0).unwrap_or(0),
            invert: best.1 < 0.0,
            correlation: (best.1.abs() / norm).min(1.0),
        }
    }
}
