//! The guitar's own input stage: its trim and its input EQ, before the
//! chain — set per guitar (and per rig) in Setup.
//!
//!   trim     dB, a plain gain: brings the guitar's peaks to where the
//!            presets expect them
//!   low cut  a 12 dB/oct high-pass (Hz)
//!   bass     a low shelf at 120 Hz (dB)
//!   mid      a peak at 800 Hz, Q 0.8 (dB)
//!   treble   a high shelf at 3.2 kHz (dB)
//!
//! [`InputTone`] is the control side (atomics, written from any thread);
//! [`InputStage`] runs on the audio thread: it re-reads the tone when its
//! generation moves and recomputes its coefficients then — never allocates,
//! never locks. RBJ cookbook biquads, transposed direct form II.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// The guitar's tone into the rig, as the audio thread reads it.
#[derive(Debug)]
pub struct InputTone {
    trim_db: AtomicU32,
    low_cut_hz: AtomicU32,
    bass_db: AtomicU32,
    mid_db: AtomicU32,
    treble_db: AtomicU32,
    generation: AtomicU64,
}

impl Default for InputTone {
    fn default() -> Self {
        let z = || AtomicU32::new(0.0f32.to_bits());
        Self {
            trim_db: z(),
            low_cut_hz: AtomicU32::new(20.0f32.to_bits()),
            bass_db: z(),
            mid_db: z(),
            treble_db: z(),
            generation: AtomicU64::new(1),
        }
    }
}

/// The values of an [`InputTone`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneValues {
    pub trim_db: f32,
    pub low_cut_hz: f32,
    pub bass_db: f32,
    pub mid_db: f32,
    pub treble_db: f32,
}

impl InputTone {
    /// Set the tone (any thread); the audio thread picks it up next block.
    pub fn set(&self, v: ToneValues) {
        self.trim_db
            .store(v.trim_db.clamp(-24.0, 24.0).to_bits(), Ordering::Relaxed);
        self.low_cut_hz
            .store(v.low_cut_hz.clamp(10.0, 400.0).to_bits(), Ordering::Relaxed);
        self.bass_db
            .store(v.bass_db.clamp(-15.0, 15.0).to_bits(), Ordering::Relaxed);
        self.mid_db
            .store(v.mid_db.clamp(-15.0, 15.0).to_bits(), Ordering::Relaxed);
        self.treble_db
            .store(v.treble_db.clamp(-15.0, 15.0).to_bits(), Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Release);
    }

    #[must_use]
    pub fn get(&self) -> ToneValues {
        let f = |a: &AtomicU32| f32::from_bits(a.load(Ordering::Relaxed));
        ToneValues {
            trim_db: f(&self.trim_db),
            low_cut_hz: f(&self.low_cut_hz),
            bass_db: f(&self.bass_db),
            mid_db: f(&self.mid_db),
            treble_db: f(&self.treble_db),
        }
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Coeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Coeffs {
    const UNITY: Self = Self {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    fn norm(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0: (b0 / a0) as f32,
            b1: (b1 / a0) as f32,
            b2: (b2 / a0) as f32,
            a1: (a1 / a0) as f32,
            a2: (a2 / a0) as f32,
        }
    }

    fn high_pass(sr: f64, f: f64, q: f64) -> Self {
        let w = std::f64::consts::TAU * (f / sr).min(0.49);
        let (c, a) = (w.cos(), w.sin() / (2.0 * q));
        Self::norm(
            (1.0 + c) / 2.0,
            -(1.0 + c),
            (1.0 + c) / 2.0,
            1.0 + a,
            -2.0 * c,
            1.0 - a,
        )
    }

    fn peak(sr: f64, f: f64, q: f64, db: f64) -> Self {
        if db.abs() < 0.01 {
            return Self::UNITY;
        }
        let a = 10f64.powf(db / 40.0);
        let w = std::f64::consts::TAU * (f / sr).min(0.49);
        let (c, al) = (w.cos(), w.sin() / (2.0 * q));
        Self::norm(
            1.0 + al * a,
            -2.0 * c,
            1.0 - al * a,
            1.0 + al / a,
            -2.0 * c,
            1.0 - al / a,
        )
    }

    fn shelf(sr: f64, f: f64, db: f64, high: bool) -> Self {
        if db.abs() < 0.01 {
            return Self::UNITY;
        }
        let a = 10f64.powf(db / 40.0);
        let w = std::f64::consts::TAU * (f / sr).min(0.49);
        let (c, s) = (w.cos(), w.sin());
        // Shelf slope 1.
        let al = s / 2.0 * std::f64::consts::SQRT_2;
        let k = 2.0 * a.sqrt() * al;
        if high {
            Self::norm(
                a * ((a + 1.0) + (a - 1.0) * c + k),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                a * ((a + 1.0) + (a - 1.0) * c - k),
                (a + 1.0) - (a - 1.0) * c + k,
                2.0 * ((a - 1.0) - (a + 1.0) * c),
                (a + 1.0) - (a - 1.0) * c - k,
            )
        } else {
            Self::norm(
                a * ((a + 1.0) - (a - 1.0) * c + k),
                2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                a * ((a + 1.0) - (a - 1.0) * c - k),
                (a + 1.0) + (a - 1.0) * c + k,
                -2.0 * ((a - 1.0) + (a + 1.0) * c),
                (a + 1.0) + (a - 1.0) * c - k,
            )
        }
    }
}

/// One biquad's state, per channel.
#[derive(Clone, Copy, Debug, Default)]
struct State {
    z1: f32,
    z2: f32,
}

impl State {
    #[inline]
    fn run(&mut self, c: &Coeffs, x: f32) -> f32 {
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        y
    }
}

const BANDS: usize = 4;

/// The stage itself, on the audio thread.
#[derive(Debug)]
pub struct InputStage {
    sample_rate: f64,
    seen: u64,
    gain: f32,
    coeffs: [Coeffs; BANDS],
    state: [[State; BANDS]; 2],
    /// Nothing to do: unity trim, the bands flat, the cut at the bottom.
    flat: bool,
}

impl Default for InputStage {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            seen: 0,
            gain: 1.0,
            coeffs: [Coeffs::UNITY; BANDS],
            state: [[State::default(); BANDS]; 2],
            flat: true,
        }
    }
}

impl InputStage {
    /// A new rate: the coefficients follow on the next block.
    pub fn prepare(&mut self, sample_rate: f64) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
        self.seen = 0;
        self.state = [[State::default(); BANDS]; 2];
    }

    fn update(&mut self, v: ToneValues) {
        let sr = self.sample_rate;
        self.gain = 10f32.powf(v.trim_db / 20.0);
        self.coeffs = [
            if v.low_cut_hz > 20.5 {
                Coeffs::high_pass(sr, f64::from(v.low_cut_hz), std::f64::consts::FRAC_1_SQRT_2)
            } else {
                Coeffs::UNITY
            },
            Coeffs::shelf(sr, 120.0, f64::from(v.bass_db), false),
            Coeffs::peak(sr, 800.0, 0.8, f64::from(v.mid_db)),
            Coeffs::shelf(sr, 3200.0, f64::from(v.treble_db), true),
        ];
        self.flat = v.trim_db.abs() < 0.01
            && v.low_cut_hz <= 20.5
            && v.bass_db.abs() < 0.01
            && v.mid_db.abs() < 0.01
            && v.treble_db.abs() < 0.01;
    }

    /// Run a stereo block in place.
    pub fn process(&mut self, tone: &InputTone, l: &mut [f32], r: &mut [f32]) {
        let generation = tone.generation();
        if generation != self.seen {
            self.seen = generation;
            self.update(tone.get());
        }
        if self.flat {
            return;
        }
        for (ch, buf) in [l, r].into_iter().enumerate() {
            let st = &mut self.state[ch];
            for x in buf.iter_mut() {
                let mut y = *x * self.gain;
                for (b, c) in self.coeffs.iter().enumerate() {
                    y = st[b].run(c, y);
                }
                *x = y;
            }
        }
    }
}
