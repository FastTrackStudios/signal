//! `align`: lining a mic up with another, as an offline block whose result
//! is static values.
//!
//! - `delay_samples`: a whole-sample shift, either way. Moving a track
//!   *earlier* needs the future, so the block reports the advance it was
//!   built with as latency and delays by that much more: the host's delay
//!   compensation takes the latency away and leaves the shift. The advance
//!   is fixed when the block is built (an analysis sets the value, then
//!   the chain is rebuilt), so the latency never moves while it plays.
//! - `invert`: the polarity, exactly (a negation).
//! - `rotate_deg` of the band around `rotate_hz` (width `rotate_q`): the
//!   band `b` (an RBJ band-pass, 0 dB at its centre) replaced by
//!   `cos θ·b + sin θ·H(b)`, `H` its Hilbert transform — exactly how
//!   Studio's `LISTEN_ALIGN` models a rotation (`listen/align.rs`), so its
//!   predictions hold. A Hilbert transform needs the future: it is an FIR
//!   ([`HILBERT_HALF`] each side, Blackman), run by FFT, and the block
//!   reports that much more latency, the dry path delayed to line up. The
//!   rotation is there when the block is built with an angle (feature
//!   `std`); 0° leaves it out, and its latency.
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

/// The Hilbert FIR's taps each side of its centre: its group delay.
pub const HILBERT_HALF: usize = 8191;
/// Frames per FFT hop of the Hilbert's convolution: latency it adds.
pub const HILBERT_HOP: usize = 8192;
/// The FFT size (>= taps + hop - 1).
pub const HILBERT_FFT: usize = 32_768;

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
    ParamSpec::new("rotate_deg", "Rotate", Unit::Plain(1), -180.0, 180.0, 0.0),
    ParamSpec::new("rotate_q", "Rotate Q", Unit::Plain(2), 0.1, 4.0, 1.0),
];

/// An RBJ band-pass (0 dB at its centre), as `LISTEN_ALIGN`'s.
#[derive(Clone, Copy, Debug, Default)]
pub struct Band {
    b0: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x: [f64; 2],
    y: [f64; 2],
}

impl Band {
    #[must_use]
    pub fn new(hz: f64, q: f64, rate: f64) -> Self {
        let w = 2.0 * core::f64::consts::PI * hz / rate;
        let alpha = libm::sin(w) / (2.0 * q.max(1e-3));
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            b2: -alpha / a0,
            a1: -2.0 * libm::cos(w) / a0,
            a2: (1.0 - alpha) / a0,
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    /// Direct form I, as `LISTEN_ALIGN` runs it.
    #[inline]
    pub fn tick(&mut self, s: f64) -> f64 {
        let y = self.b0 * s + self.b2 * self.x[1] - self.a1 * self.y[0] - self.a2 * self.y[1];
        self.x = [s, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

#[cfg(feature = "std")]
mod hilbert {
    //! The Hilbert FIR, by overlap-save FFT: preallocated, no allocation
    //! in `tick`.

    use alloc::sync::Arc;
    use alloc::vec::Vec;

    use realfft::num_complex::Complex;
    use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

    use super::{HILBERT_FFT, HILBERT_HALF, HILBERT_HOP};

    /// One channel's convolution with the Hilbert FIR: an output `latency`
    /// frames after its input (the FIR's half plus a hop).
    pub struct Hilbert {
        fwd: Arc<dyn RealToComplex<f64>>,
        inv: Arc<dyn ComplexToReal<f64>>,
        taps: Arc<[Complex<f64>]>,
        history: Vec<f64>,
        frame: Vec<f64>,
        spec: Vec<Complex<f64>>,
        scratch_f: Vec<Complex<f64>>,
        scratch_i: Vec<Complex<f64>>,
        out: Vec<f64>,
        at: usize,
    }

    /// The FIR's spectrum: `2/(πm)` at odd `m`, Blackman-windowed.
    pub fn taps(fwd: &Arc<dyn RealToComplex<f64>>) -> Arc<[Complex<f64>]> {
        let n = 2 * HILBERT_HALF + 1;
        let mut h = fwd.make_input_vec();
        for (k, v) in h.iter_mut().enumerate().take(n) {
            let m = k as i64 - HILBERT_HALF as i64;
            if m % 2 != 0 {
                let x = k as f64 / (n - 1) as f64;
                let w = 0.42 - 0.5 * libm::cos(2.0 * core::f64::consts::PI * x)
                    + 0.08 * libm::cos(4.0 * core::f64::consts::PI * x);
                *v = 2.0 / (core::f64::consts::PI * m as f64) * w;
            }
        }
        let mut spec = fwd.make_output_vec();
        let _ = fwd.process(&mut h, &mut spec);
        let scale = 1.0 / HILBERT_FFT as f64;
        spec.iter().map(|c| c * scale).collect()
    }

    impl Hilbert {
        pub fn new(planner: &mut RealFftPlanner<f64>, taps: Arc<[Complex<f64>]>) -> Self {
            let fwd = planner.plan_fft_forward(HILBERT_FFT);
            let inv = planner.plan_fft_inverse(HILBERT_FFT);
            Self {
                scratch_f: fwd.make_scratch_vec(),
                scratch_i: inv.make_scratch_vec(),
                spec: fwd.make_output_vec(),
                frame: fwd.make_input_vec(),
                history: alloc::vec![0.0; HILBERT_FFT],
                out: alloc::vec![0.0; HILBERT_HOP],
                at: 0,
                fwd,
                inv,
                taps,
            }
        }

        /// Frames from an input to its output.
        pub const fn latency() -> usize {
            HILBERT_HALF + HILBERT_HOP
        }

        /// One frame in, the Hilbert transform of the frame `latency()`
        /// before out.
        #[inline]
        pub fn tick(&mut self, x: f64) -> f64 {
            let y = self.out.get(self.at).copied().unwrap_or(0.0);
            if let Some(slot) = self.history.get_mut(HILBERT_FFT - HILBERT_HOP + self.at) {
                *slot = x;
            }
            self.at += 1;
            if self.at == HILBERT_HOP {
                self.at = 0;
                self.hop();
            }
            y
        }

        fn hop(&mut self) {
            self.frame.copy_from_slice(&self.history);
            if self
                .fwd
                .process_with_scratch(&mut self.frame, &mut self.spec, &mut self.scratch_f)
                .is_err()
            {
                return;
            }
            for (c, t) in self.spec.iter_mut().zip(self.taps.iter()) {
                *c *= t;
            }
            if let Some(dc) = self.spec.first_mut() {
                dc.im = 0.0;
            }
            if let Some(ny) = self.spec.last_mut() {
                ny.im = 0.0;
            }
            if self
                .inv
                .process_with_scratch(&mut self.spec, &mut self.frame, &mut self.scratch_i)
                .is_err()
            {
                return;
            }
            if let Some(valid) = self.frame.get(HILBERT_FFT - HILBERT_HOP..) {
                self.out.copy_from_slice(valid);
            }
            self.history.copy_within(HILBERT_HOP.., 0);
        }

        pub fn reset(&mut self) {
            self.history.fill(0.0);
            self.out.fill(0.0);
            self.at = 0;
        }
    }
}

/// The band rotation of one channel.
#[cfg(feature = "std")]
struct Rotation {
    band: Band,
    hilbert: hilbert::Hilbert,
    /// The input and its band, delayed to meet the Hilbert's output.
    dry: Vec<f64>,
    band_dry: Vec<f64>,
    at: usize,
}

/// The apply stage.
pub struct AlignApply {
    sample_rate: f64,
    values: [f32; 5],
    /// The advance it was built with, frames.
    advance: usize,
    /// The delay it runs: `advance + delay_samples`, frames, >= 0.
    delay: usize,
    sign: f32,
    line: [Vec<f32>; 2],
    write: usize,
    #[cfg(feature = "std")]
    rotation: Option<[Rotation; 2]>,
    cos: f64,
    sin: f64,
}

impl AlignApply {
    /// Built with `delay_samples` and `rotate_deg` as its spec has them: an
    /// advance (a negative delay) and a rotation are its latency.
    #[must_use]
    pub fn new(sample_rate: f32, delay_samples: f32, rotate_deg: f32) -> Self {
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
            #[cfg(feature = "std")]
            rotation: None,
            cos: 1.0,
            sin: 0.0,
        };
        #[cfg(feature = "std")]
        if libm::fabsf(rotate_deg) > 1e-6 {
            let mut planner = realfft::RealFftPlanner::<f64>::new();
            let fwd = planner.plan_fft_forward(HILBERT_FFT);
            let taps = hilbert::taps(&fwd);
            let lag = hilbert::Hilbert::latency();
            let one = |planner: &mut realfft::RealFftPlanner<f64>| Rotation {
                band: Band::default(),
                hilbert: hilbert::Hilbert::new(planner, taps.clone()),
                dry: alloc::vec![0.0; lag + 1],
                band_dry: alloc::vec![0.0; lag + 1],
                at: 0,
            };
            s.rotation = Some([one(&mut planner), one(&mut planner)]);
        }
        #[cfg(not(feature = "std"))]
        let _ = rotate_deg;
        s.set_param(DELAY, d);
        s.set_param(ROTATE_DEG, rotate_deg);
        s
    }

    fn redesign(&mut self) {
        let deg = f64::from(self.values[ROTATE_DEG]).to_radians();
        (self.cos, self.sin) = (libm::cos(deg), libm::sin(deg));
        #[cfg(feature = "std")]
        if let Some(rot) = &mut self.rotation {
            for r in rot.iter_mut() {
                let (x, y) = (r.band.x, r.band.y);
                r.band = Band::new(
                    f64::from(self.values[ROTATE_HZ]),
                    f64::from(self.values[ROTATE_Q]),
                    self.sample_rate,
                );
                (r.band.x, r.band.y) = (x, y);
            }
        }
    }

    /// Whether it rotates (built with an angle).
    #[must_use]
    pub const fn rotates(&self) -> bool {
        #[cfg(feature = "std")]
        {
            self.rotation.is_some()
        }
        #[cfg(not(feature = "std"))]
        {
            false
        }
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
        #[cfg(feature = "std")]
        if let Some(rot) = &mut self.rotation {
            for r in rot.iter_mut() {
                r.band.x = [0.0; 2];
                r.band.y = [0.0; 2];
                r.hilbert.reset();
                r.dry.fill(0.0);
                r.band_dry.fill(0.0);
                r.at = 0;
            }
        }
    }

    fn latency(&self) -> u32 {
        #[cfg(feature = "std")]
        let rot = if self.rotation.is_some() {
            hilbert::Hilbert::latency()
        } else {
            0
        };
        #[cfg(not(feature = "std"))]
        let rot = 0;
        u32::try_from(self.advance + rot).unwrap_or(u32::MAX)
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = self.line[0].len();
        let frames = left.len().min(right.len());
        if n == 0 || frames == 0 {
            return;
        }
        let back = n - self.delay.min(n - 1);
        let (c, s) = (self.cos, self.sin);
        for (ch, buf) in [left, right].into_iter().enumerate() {
            let Some(line) = self.line.get_mut(ch) else {
                continue;
            };
            #[cfg(feature = "std")]
            let mut rot = self.rotation.as_mut().and_then(|r| r.get_mut(ch));
            let mut w = self.write;
            for v in buf.iter_mut().take(frames) {
                if let Some(slot) = line.get_mut(w) {
                    *slot = *v;
                }
                let y = line.get((w + back) % n).copied().unwrap_or(0.0) * self.sign;
                #[cfg(feature = "std")]
                let y = match rot.as_mut() {
                    None => y,
                    Some(r) => {
                        let x = f64::from(y);
                        let b = r.band.tick(x);
                        let h = r.hilbert.tick(b);
                        let len = r.dry.len();
                        let (dx, db) = (
                            r.dry.get(r.at).copied().unwrap_or(0.0),
                            r.band_dry.get(r.at).copied().unwrap_or(0.0),
                        );
                        if let (Some(a), Some(bb)) = (r.dry.get_mut(r.at), r.band_dry.get_mut(r.at))
                        {
                            *a = x;
                            *bb = b;
                        }
                        r.at = (r.at + 1) % len;
                        #[allow(clippy::cast_possible_truncation)]
                        let out = (dx - db + c * db + s * h) as f32;
                        out
                    }
                };
                *v = y;
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
