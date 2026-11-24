//! `align`: lining a mic up with another, as an offline block whose result
//! is static values.
//!
//! - `delay_samples`: a whole-sample shift, either way. Moving a track
//!   *earlier* needs the future, so the block reports the advance it was
//!   built with as its latency and delays by that much more: the host's
//!   delay compensation takes the latency away and leaves the shift. The
//!   advance is fixed when the block is built (an analysis sets the
//!   value, then the chain is rebuilt), so the latency never moves while
//!   it plays; a later value earlier than that is held at it.
//! - `invert`: the polarity, exactly (a negation).
//! - `rotate_deg` at `rotate_hz`, shaped by `rotate_q`: a second-order
//!   all-pass whose phase at `rotate_hz` is `-rotate_deg` (a lag) —
//!   magnitude flat, the phase turned where the two mics disagree. 0°
//!   leaves the all-pass out.
//!
//! Studio's `LISTEN_ALIGN` searches for the alignment and sets these;
//! [`find`] (feature `offline`) is the plain search for a host without it.

use alloc::vec::Vec;

use crate::block::BlockProcessor;
use crate::param::{ParamSpec, Unit};

/// The block's name.
pub const NAME: &str = "align";

pub const DELAY: usize = 0;
pub const INVERT: usize = 1;
pub const ROTATE_HZ: usize = 2;
pub const ROTATE_DEG: usize = 3;
pub const ROTATE_Q: usize = 4;

/// The longest shift either way, frames: 100 ms at 48 kHz.
pub const MAX_SHIFT: usize = 4800;

pub static PARAMS: [ParamSpec; 5] = [
    ParamSpec::new(
        "delay_samples",
        "Delay",
        Unit::Plain(0),
        -4800.0,
        4800.0,
        0.0,
    ),
    ParamSpec::new("invert", "Invert", Unit::Toggle, 0.0, 1.0, 0.0),
    ParamSpec::new("rotate_hz", "Rotate At", Unit::Hz, 20.0, 2000.0, 100.0),
    ParamSpec::new("rotate_deg", "Rotate", Unit::Plain(1), 0.0, 359.0, 0.0),
    ParamSpec::new("rotate_q", "Rotate Q", Unit::Plain(2), 0.1, 4.0, 0.7),
];

/// A second-order all-pass, transposed direct form II.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AllPass {
    pub b0: f64,
    pub b1: f64,
    /// `b2` is 1 (an all-pass's numerator is its denominator reversed).
    pub a1: f64,
    pub a2: f64,
    s: [[f64; 2]; 2],
}

impl AllPass {
    /// The all-pass whose phase at `hz` is `-deg`, of steepness `q`.
    ///
    /// The analog prototype's phase is `-2 atan2(w wc / q, wc² - w²)`; the
    /// centre `wc` that gives the asked-for phase at `w` is a root of a
    /// quadratic. Designed at `w` prewarped, the bilinear transform puts
    /// exactly that phase at `hz`.
    #[must_use]
    pub fn design(sample_rate: f64, hz: f64, deg: f64, q: f64) -> Self {
        let k = 2.0 * sample_rate;
        let w = k * libm::tan(core::f64::consts::PI * (hz / sample_rate).clamp(1e-6, 0.49));
        let theta = (deg.rem_euclid(360.0) / 2.0).to_radians();
        let q = q.max(1e-3);
        let t = libm::tan(theta);
        let wc = if (theta - core::f64::consts::FRAC_PI_2).abs() < 1e-9 {
            w
        } else {
            let disc = libm::sqrt(1.0 / (q * q) + 4.0 * t * t);
            let root = if t > 0.0 {
                (1.0 / q + disc) / (2.0 * t)
            } else {
                (1.0 / q - disc) / (2.0 * t)
            };
            w * root
        };
        let (k2, kw, wc2) = (k * k, k * wc / q, wc * wc);
        let a0 = k2 + kw + wc2;
        Self {
            b0: (k2 - kw + wc2) / a0,
            b1: 2.0 * (wc2 - k2) / a0,
            a1: 2.0 * (wc2 - k2) / a0,
            a2: (k2 - kw + wc2) / a0,
            s: [[0.0; 2]; 2],
        }
    }

    /// Its response at `hz`: (magnitude, phase in degrees).
    #[must_use]
    pub fn response(&self, sample_rate: f64, hz: f64) -> (f64, f64) {
        let w = 2.0 * core::f64::consts::PI * hz / sample_rate;
        let (c1, s1, c2, s2) = (
            libm::cos(w),
            libm::sin(w),
            libm::cos(2.0 * w),
            libm::sin(2.0 * w),
        );
        // H = (b0 + b1 z^-1 + z^-2) / (1 + a1 z^-1 + a2 z^-2), z^-1 = e^-jw
        let (nr, ni) = (self.b0 + self.b1 * c1 + c2, -(self.b1 * s1) - s2);
        let (dr, di) = (
            1.0 + self.a1 * c1 + self.a2 * c2,
            -(self.a1 * s1) - self.a2 * s2,
        );
        let mag = libm::sqrt((nr * nr + ni * ni) / (dr * dr + di * di));
        let phase = libm::atan2(ni, nr) - libm::atan2(di, dr);
        (mag, phase.to_degrees())
    }

    #[inline]
    fn tick(&mut self, ch: usize, x: f64) -> f64 {
        let Some(s) = self.s.get_mut(ch) else {
            return x;
        };
        let y = self.b0.mul_add(x, s[0]);
        s[0] = self.b1.mul_add(x, self.a1.mul_add(-y, s[1]));
        s[1] = x - self.a2 * y;
        y
    }
}

/// The apply stage.
pub struct AlignApply {
    sample_rate: f64,
    values: [f32; 5],
    /// The advance it was built with: its latency, frames.
    advance: usize,
    /// The delay it runs: `advance + delay_samples`, frames, >= 0.
    delay: usize,
    sign: f32,
    rotate: Option<AllPass>,
    line: [Vec<f32>; 2],
    write: usize,
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
            rotate: None,
            line: [alloc::vec![0.0; len], alloc::vec![0.0; len]],
            write: 0,
        };
        s.set_param(DELAY, d);
        s
    }

    fn redesign(&mut self) {
        let deg = f64::from(self.values[ROTATE_DEG]);
        self.rotate = (deg.rem_euclid(360.0) > 1e-6).then(|| {
            AllPass::design(
                self.sample_rate,
                f64::from(self.values[ROTATE_HZ]),
                deg,
                f64::from(self.values[ROTATE_Q]),
            )
        });
    }

    /// The all-pass it runs, if any.
    #[must_use]
    pub const fn all_pass(&self) -> Option<&AllPass> {
        self.rotate.as_ref()
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
        if let Some(r) = &mut self.rotate {
            r.s = [[0.0; 2]; 2];
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
            let (Some(line), rotate) = (self.line.get_mut(ch), &mut self.rotate) else {
                continue;
            };
            let mut w = self.write;
            for s in buf.iter_mut().take(frames) {
                if let Some(slot) = line.get_mut(w) {
                    *slot = *s;
                }
                let mut y = line.get((w + back) % n).copied().unwrap_or(0.0) * self.sign;
                if let Some(ap) = rotate.as_mut() {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        y = ap.tick(ch, f64::from(y)) as f32;
                    }
                }
                *s = y;
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
