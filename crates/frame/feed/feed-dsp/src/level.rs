//! Meters: [`LevelTap`] (peak and RMS per channel) and [`ScalarTap`] (one
//! value an engine computes itself — gain reduction, an LFO, CPU).
//!
//! Both hand the control thread "the extreme since the last read", so a
//! short peak between two reads is never lost, and a read with nothing
//! new repeats the previous value rather than dropping to silence.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use feed_proto::{FeedInfo, FeedOptions};

use crate::history::Aggregate;
use crate::{FeedSource, amp_to_db};

/// "Nothing since the last read."
const EMPTY: u32 = u32::MAX;

struct LevelShared {
    /// |peak| since the last read, as f32 bits (`EMPTY` = none).
    peak: Box<[AtomicU32]>,
    /// The RMS follower's latest value, f32 bits.
    rms: Box<[AtomicU32]>,
}

/// The audio-thread end.
pub struct LevelTap {
    shared: Arc<LevelShared>,
    /// Mean-square follower state per channel.
    ms: Box<[f32]>,
    /// One-pole coefficient per sample (300 ms).
    coeff: f32,
}

/// The control-thread end.
pub struct LevelReader {
    shared: Arc<LevelShared>,
    last_peak: Box<[f64]>,
}

/// A level meter over `channels` channels at `sample_rate` (RMS window
/// 300 ms). Allocates here; the tap never does.
#[must_use]
pub fn level(channels: usize, sample_rate: f64) -> (LevelTap, LevelReader) {
    let channels = channels.max(1);
    let shared = Arc::new(LevelShared {
        peak: (0..channels).map(|_| AtomicU32::new(EMPTY)).collect(),
        rms: (0..channels).map(|_| AtomicU32::new(0)).collect(),
    });
    let coeff = (1.0 - (-1.0 / (0.3 * sample_rate)).exp()) as f32;
    (
        LevelTap {
            shared: shared.clone(),
            ms: vec![0.0; channels].into(),
            coeff,
        },
        LevelReader {
            shared,
            last_peak: vec![crate::FLOOR_DB; channels].into(),
        },
    )
}

/// Raise `slot` to `v` (a non-negative f32: its bits order like its value).
#[inline]
fn raise(slot: &AtomicU32, v: f32) {
    let bits = v.max(0.0).to_bits();
    let _ = slot.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |cur| {
        (cur == EMPTY || bits > cur).then_some(bits)
    });
}

impl LevelTap {
    #[must_use]
    pub fn channels(&self) -> usize {
        self.ms.len()
    }

    /// One channel's block. Wait-free, no allocation.
    #[inline]
    pub fn push(&mut self, channel: usize, samples: &[f32]) {
        let Some(ms) = self.ms.get_mut(channel) else {
            return;
        };
        let mut peak = 0.0f32;
        for &x in samples {
            peak = peak.max(x.abs());
            *ms += self.coeff * (x * x - *ms);
        }
        raise(&self.shared.peak[channel], peak);
        self.shared.rms[channel].store(ms.sqrt().to_bits(), Ordering::Relaxed);
    }

    /// An interleaved block (`L R L R …`). Wait-free, no allocation.
    #[inline]
    pub fn push_interleaved(&mut self, samples: &[f32]) {
        let ch = self.ms.len();
        for c in 0..ch {
            let mut peak = 0.0f32;
            let ms = &mut self.ms[c];
            for x in samples.iter().skip(c).step_by(ch) {
                peak = peak.max(x.abs());
                *ms += self.coeff * (x * x - *ms);
            }
            raise(&self.shared.peak[c], peak);
            self.shared.rms[c].store(ms.sqrt().to_bits(), Ordering::Relaxed);
        }
    }
}

impl LevelReader {
    #[must_use]
    pub fn channels(&self) -> usize {
        self.last_peak.len()
    }

    /// Peak dBFS per channel since the last read (the previous reading
    /// when nothing arrived).
    pub fn peak_db(&mut self, out: &mut [f64]) {
        for (c, o) in out.iter_mut().enumerate().take(self.last_peak.len()) {
            let bits = self.shared.peak[c].swap(EMPTY, Ordering::AcqRel);
            if bits != EMPTY {
                self.last_peak[c] = amp_to_db(f64::from(f32::from_bits(bits)));
            }
            *o = self.last_peak[c];
        }
    }

    /// RMS dBFS per channel (300 ms).
    pub fn rms_db(&self, out: &mut [f64]) {
        for (o, r) in out.iter_mut().zip(self.shared.rms.iter()) {
            *o = amp_to_db(f64::from(f32::from_bits(r.load(Ordering::Relaxed))));
        }
    }
}

impl FeedSource for LevelReader {
    fn read(&mut self, _dt: f64, out: &mut Vec<f64>) {
        out.resize(self.channels(), crate::FLOOR_DB);
        self.peak_db(out);
    }
}

/// The audio-thread end of a scalar.
pub struct ScalarTap {
    slot: Arc<AtomicU32>,
    agg: Aggregate,
}

/// The control-thread end of a scalar.
pub struct ScalarReader {
    slot: Arc<AtomicU32>,
    last: f64,
}

/// A scalar the engine computes (gain reduction with [`Aggregate::Min`],
/// a level with `Max`, anything else with `Last`), reading `rest` until
/// the first push.
#[must_use]
pub fn scalar(agg: Aggregate, rest: f64) -> (ScalarTap, ScalarReader) {
    let slot = Arc::new(AtomicU32::new(EMPTY));
    (
        ScalarTap {
            slot: slot.clone(),
            agg,
        },
        ScalarReader { slot, last: rest },
    )
}

impl ScalarTap {
    /// Fold `v` into the value since the last read. Lock-free, no
    /// allocation.
    #[inline]
    pub fn push(&self, v: f32) {
        let agg = self.agg;
        let _ = self
            .slot
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |cur| {
                if cur == EMPTY {
                    return Some(v.to_bits());
                }
                let c = f32::from_bits(cur);
                Some(
                    match agg {
                        Aggregate::Max => c.max(v),
                        Aggregate::Min => c.min(v),
                        Aggregate::Mean | Aggregate::Last => v,
                    }
                    .to_bits(),
                )
            });
    }
}

impl ScalarReader {
    /// The value since the last read (the previous one when none arrived).
    pub fn take(&mut self) -> f64 {
        let bits = self.slot.swap(EMPTY, Ordering::AcqRel);
        if bits != EMPTY {
            self.last = f64::from(f32::from_bits(bits));
        }
        self.last
    }
}

impl FeedSource for ScalarReader {
    fn configure(&mut self, _info: &FeedInfo, _options: &FeedOptions) {}

    fn read(&mut self, _dt: f64, out: &mut Vec<f64>) {
        out.clear();
        out.push(self.take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_since_last_read_per_channel() {
        let (mut tap, mut r) = level(2, 48_000.0);
        let mut out = [0.0; 2];
        r.peak_db(&mut out);
        assert_eq!(out, [crate::FLOOR_DB; 2], "silence before data");
        tap.push(0, &[0.1, -0.5, 0.2]);
        tap.push(0, &[0.25]);
        tap.push(1, &[0.05]);
        r.peak_db(&mut out);
        assert!(
            (out[0] - amp_to_db(0.5)).abs() < 1e-6,
            "the block peak survives a quieter block"
        );
        assert!((out[1] - amp_to_db(0.05)).abs() < 1e-5);
        r.peak_db(&mut out);
        assert!(
            (out[0] - amp_to_db(0.5)).abs() < 1e-6,
            "nothing new: hold the last reading"
        );
        tap.push_interleaved(&[0.0, 1.0, 0.0, -1.0]);
        r.peak_db(&mut out);
        assert!(out[0] <= crate::FLOOR_DB && out[1].abs() < 1e-9, "{out:?}");
    }

    #[test]
    fn rms_of_a_steady_sine_is_minus_3db() {
        let sr = 48_000.0;
        let (mut tap, r) = level(1, sr);
        let s: Vec<f32> = (0..48_000)
            .map(|i| (2.0 * std::f64::consts::PI * 1000.0 * f64::from(i) / sr).sin() as f32)
            .collect();
        tap.push(0, &s);
        let mut out = [0.0];
        r.rms_db(&mut out);
        assert!((out[0] + 3.01).abs() < 0.2, "{}", out[0]);
    }

    #[test]
    fn scalars_keep_the_extreme_between_reads() {
        let (tap, mut r) = scalar(Aggregate::Min, 0.0);
        assert_eq!(r.take(), 0.0);
        tap.push(-2.0);
        tap.push(-6.0);
        tap.push(-1.0);
        assert_eq!(r.take(), -6.0);
        assert_eq!(r.take(), -6.0, "held");
        tap.push(-0.5);
        assert_eq!(r.take(), -0.5);
    }
}
