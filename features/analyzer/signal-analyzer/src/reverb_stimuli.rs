//! The stimulus set for reverb matching.
//!
//! An impulse response alone does not describe a modulated, non-linear,
//! time-varying reverb — which every interesting one is. Each stimulus
//! here isolates one thing the reverb does to real material:
//!
//! | stimulus  | what it exposes                                              |
//! |-----------|--------------------------------------------------------------|
//! | `impulse` | pre-delay, build-up, decay per band, echo density, width     |
//! | `burst`   | 50 ms of noise: a steadier excitation for decay and colour   |
//! | `snare`   | a transient with a body: attack smear, "grain" vs "fog"      |
//! | `sine220` | a held low tone: pitch modulation (sidebands), low decay     |
//! | `sine1k`  | a held mid tone: modulation where the ear is most sensitive  |
//! | `pluck`   | a strummed chord: musical listening material                 |
//! | `pad`     | a held saw chord: how the tail blooms under sustained input  |
//!
//! Every stimulus is mono, deterministic (the same file every run, so
//! renders are comparable across sessions), and peaks at or below 0.5 so a
//! reverb with gain in its tank keeps headroom.

use std::f64::consts::{PI, TAU};

/// One stimulus of the set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stimulus {
    Impulse,
    Burst,
    Snare,
    Sine220,
    Sine1k,
    Pluck,
    Pad,
}

/// Amplitude of the sine stimuli — [`crate::reverb_character`] reads wet
/// gain against it.
pub const SINE_AMPLITUDE: f64 = 0.3;

impl Stimulus {
    pub const ALL: [Self; 7] = [
        Self::Impulse,
        Self::Burst,
        Self::Snare,
        Self::Sine220,
        Self::Sine1k,
        Self::Pluck,
        Self::Pad,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Impulse => "impulse",
            Self::Burst => "burst",
            Self::Snare => "snare",
            Self::Sine220 => "sine220",
            Self::Sine1k => "sine1k",
            Self::Pluck => "pluck",
            Self::Pad => "pad",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name().eq_ignore_ascii_case(name))
    }

    /// Where the stimulus stops driving the reverb, in seconds — decay is
    /// measured from here.
    #[must_use]
    pub const fn excitation_end_s(self) -> f64 {
        match self {
            Self::Impulse => 0.0,
            Self::Burst => 0.05,
            Self::Snare => 0.25,
            Self::Sine220 | Self::Sine1k => 4.0,
            Self::Pluck => 2.5,
            Self::Pad => 2.0,
        }
    }

    /// The held tone of a sine stimulus, for the modulation measurement.
    #[must_use]
    pub const fn tone_hz(self) -> Option<f64> {
        match self {
            Self::Sine220 => Some(220.0),
            Self::Sine1k => Some(1000.0),
            _ => None,
        }
    }

    /// Render the stimulus at `sample_rate`.
    #[must_use]
    pub fn generate(self, sample_rate: f64) -> Vec<f32> {
        let sr = sample_rate;
        let frames = |s: f64| (s * sr) as usize;
        let out: Vec<f64> = match self {
            Self::Impulse => {
                let mut v = vec![0.0; frames(0.1)];
                if let Some(first) = v.first_mut() {
                    *first = 0.5;
                }
                v
            }
            Self::Burst => {
                let n = frames(0.05);
                let mut rng = Lcg(7);
                (0..n).map(|i| rng.white() * 0.35 * fade(i, n, 1.0, sr)).collect()
            }
            Self::Snare => {
                let n = frames(0.25);
                let mut rng = Lcg(11);
                (0..n)
                    .map(|i| {
                        let t = i as f64 / sr;
                        let body = (TAU * 185.0 * t).sin() * (-t / 0.04).exp() * 0.3;
                        let snap = rng.white() * (-t / 0.06).exp() * 0.25;
                        (body + snap) * 0.85 * fade(i, n, 0.3, sr)
                    })
                    .collect()
            }
            Self::Sine220 | Self::Sine1k => {
                let f = self.tone_hz().unwrap_or(1000.0);
                // Long enough to average the modulation over a dozen
                // windows: one window of a randomly modulated tail is a
                // single noisy draw.
                let n = frames(4.0);
                (0..n)
                    .map(|i| (TAU * f * i as f64 / sr).sin() * SINE_AMPLITUDE * fade(i, n, 5.0, sr))
                    .collect()
            }
            Self::Pluck => pluck(frames(2.5), sr),
            Self::Pad => {
                // A minor triad of band-limited saws (8 partials), slow attack.
                let n = frames(2.0);
                (0..n)
                    .map(|i| {
                        let t = i as f64 / sr;
                        let v: f64 = [220.0, 261.63, 329.63]
                            .iter()
                            .flat_map(|f| (1..=8).map(move |h| (TAU * f * f64::from(h) * t).sin() / f64::from(h)))
                            .sum();
                        v * 0.08 * (t / 0.15).min(1.0) * fade(i, n, 20.0, sr)
                    })
                    .collect()
            }
        };
        out.into_iter().map(|v| v as f32).collect()
    }
}

/// Karplus–Strong strum of an open E major chord, strings 25 ms apart.
fn pluck(n: usize, sr: f64) -> Vec<f64> {
    let mut out = vec![0.0f64; n];
    let mut rng = Lcg(3);
    for (k, f) in [82.41, 123.47, 164.81, 207.65, 246.94, 329.63].iter().enumerate() {
        let start = (0.025 * k as f64 * sr) as usize;
        let period = (sr / f).round().max(2.0) as usize;
        let mut buf: Vec<f64> = (0..period).map(|_| rng.white()).collect();
        let mut idx = 0;
        for o in out.iter_mut().skip(start) {
            let next = (idx + 1) % period;
            let (a, b) = (buf.get(idx).copied().unwrap_or(0.0), buf.get(next).copied().unwrap_or(0.0));
            *o += a * 0.12;
            if let Some(slot) = buf.get_mut(idx) {
                *slot = 0.996 * 0.5 * (a + b);
            }
            idx = next;
        }
    }
    let len = out.len();
    out.iter().enumerate().map(|(i, v)| v * fade(i, len, 2.0, sr)).collect()
}

/// Deterministic white noise in [-1, 1).
struct Lcg(u32);

impl Lcg {
    fn white(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(self.0 >> 8) / f64::from(1u32 << 24) * 2.0 - 1.0
    }
}

/// Raised-cosine fade of `ms` at both ends of `[0, len)`.
fn fade(i: usize, len: usize, ms: f64, sr: f64) -> f64 {
    let n = (ms * 1e-3 * sr).max(1.0);
    let a = (i as f64 / n).min(1.0);
    let b = (len.saturating_sub(i) as f64 / n).min(1.0);
    let c = |x: f64| 0.5 - 0.5 * (x * PI).cos();
    c(a) * c(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stimulus_is_deterministic_and_has_headroom() {
        for s in Stimulus::ALL {
            let a = s.generate(48_000.0);
            assert_eq!(a, s.generate(48_000.0), "{} is deterministic", s.name());
            let peak = a.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.05 && peak <= 0.5, "{} peak {peak}", s.name());
            assert_eq!(Stimulus::from_name(s.name()), Some(s));
        }
    }
}
