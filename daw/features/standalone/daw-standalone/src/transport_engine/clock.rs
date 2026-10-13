//! Sample-accurate time newtypes. WASM- and `no_std`-friendly (only
//! depends on `core`). Modeled after Firewheel's `clock.rs`, trimmed
//! to what `daw-standalone` actually needs.
//!
//! Three units:
//!
//! - [`InstantSamples`] — absolute sample frame at the output sample
//!   rate. The audio-thread source of truth.
//! - [`InstantSeconds`] — wall-clock-style seconds, derived from
//!   samples + sample rate.
//! - [`InstantMusical`] — beats (quarter notes) since musical zero,
//!   derived via the tempo map.
//!
//! Conversions take an explicit sample rate (and tempo for musical)
//! so the types stay POD — no hidden state.
//!
//! `i64` for samples gives ~6.6 million years at 44.1k; we won't
//! overflow.

use core::ops::{Add, AddAssign, Sub};

/// Absolute sample-frame position at the output sample rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct InstantSamples(pub i64);

/// Absolute position in seconds.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
#[repr(transparent)]
pub struct InstantSeconds(pub f64);

/// Absolute musical position in beats (quarter notes).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
#[repr(transparent)]
pub struct InstantMusical(pub f64);

/// Sample-rate-aware conversions. Carries the rate so individual
/// `InstantX` values stay unit-only.
#[derive(Debug, Clone, Copy)]
pub struct SampleClock {
    pub sample_rate: u32,
    /// `1.0 / sample_rate as f64` — cached to skip the divide in the
    /// hot path.
    pub sample_rate_recip: f64,
}

impl SampleClock {
    pub fn new(sample_rate: u32) -> Self {
        debug_assert!(sample_rate > 0);
        Self {
            sample_rate,
            sample_rate_recip: 1.0 / sample_rate as f64,
        }
    }

    /// Samples → seconds. Branches on 44.1k / 48k for an integer-divide
    /// fast path; falls back to the generic path otherwise.
    #[inline]
    pub fn samples_to_seconds(&self, s: InstantSamples) -> InstantSeconds {
        let (whole, fract) = whole_seconds_and_fract(s.0, self.sample_rate);
        InstantSeconds(whole as f64 + fract as f64 * self.sample_rate_recip)
    }

    /// Seconds → samples. Rounds fractional component to nearest.
    #[inline]
    pub fn seconds_to_samples(&self, s: InstantSeconds) -> InstantSamples {
        let whole = s.0.floor() as i64;
        let fract = (s.0.fract() * self.sample_rate as f64).round() as i64;
        InstantSamples(whole * self.sample_rate as i64 + fract)
    }
}

#[inline]
fn whole_seconds_and_fract(samples: i64, sample_rate: u32) -> (i64, u32) {
    let (whole, fract) = match sample_rate {
        44100 => (samples / 44100, samples % 44100),
        48000 => (samples / 48000, samples % 48000),
        sr => {
            let sr = sr as i64;
            (samples / sr, samples % sr)
        }
    };
    if fract < 0 {
        (whole - 1, sample_rate - fract.unsigned_abs() as u32)
    } else {
        (whole, fract as u32)
    }
}

// ── Arithmetic ────────────────────────────────────────────────────────

impl Add<i64> for InstantSamples {
    type Output = InstantSamples;
    #[inline]
    fn add(self, rhs: i64) -> InstantSamples {
        InstantSamples(self.0 + rhs)
    }
}
impl AddAssign<i64> for InstantSamples {
    #[inline]
    fn add_assign(&mut self, rhs: i64) {
        self.0 += rhs;
    }
}
impl Sub for InstantSamples {
    type Output = i64;
    #[inline]
    fn sub(self, rhs: InstantSamples) -> i64 {
        self.0 - rhs.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_seconds_roundtrip_48k() {
        let c = SampleClock::new(48_000);
        for s in [0i64, 1, 48_000, 96_000, 1_234_567] {
            let sec = c.samples_to_seconds(InstantSamples(s));
            let back = c.seconds_to_samples(sec);
            assert_eq!(back.0, s, "roundtrip failed at {s}");
        }
    }

    #[test]
    fn samples_seconds_roundtrip_44_1k() {
        let c = SampleClock::new(44_100);
        for s in [0i64, 1, 44_100, 88_200, 1_234_567] {
            let sec = c.samples_to_seconds(InstantSamples(s));
            let back = c.seconds_to_samples(sec);
            assert_eq!(back.0, s);
        }
    }

    #[test]
    fn negative_samples() {
        let c = SampleClock::new(48_000);
        let s = c.samples_to_seconds(InstantSamples(-48_000));
        assert!((s.0 - -1.0).abs() < 1e-9);
    }
}
