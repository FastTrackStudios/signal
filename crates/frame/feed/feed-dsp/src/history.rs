//! [`HistoryTap`] → [`HistoryReader`]: a scrolling trace of a scalar (gain
//! reduction, input level) at a requested point rate. The audio thread
//! folds each block's value into the current point and, every
//! `sample_rate / rate_hz` frames, pushes the point; the control thread
//! snapshots the last `len` points, oldest first.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use feed_proto::{FeedInfo, FeedKind, FeedOptions};

use crate::FeedSource;
use crate::ring::{RingReader, RingWriter, ring};

/// How the values inside one point (or between two reads) combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregate {
    /// The loudest (levels).
    Max,
    /// The deepest (gain reduction, ≤ 0).
    Min,
    /// The frame-weighted mean. ([`crate::ScalarTap`] treats it as `Last`.)
    Mean,
    /// The latest.
    Last,
}

impl Aggregate {
    #[inline]
    pub(crate) fn fold(self, acc: f64, weight: f64, v: f64, w: f64) -> f64 {
        if weight <= 0.0 {
            return v;
        }
        match self {
            Self::Max => acc.max(v),
            Self::Min => acc.min(v),
            Self::Mean => (acc * weight + v * w) / (weight + w),
            Self::Last => v,
        }
    }
}

/// The audio-thread end.
pub struct HistoryTap {
    ring: RingWriter,
    frames_per_point: Arc<AtomicU32>,
    agg: Aggregate,
    acc: f64,
    /// Frames folded into `acc`.
    frames: u32,
}

/// The control-thread end.
pub struct HistoryReader {
    ring: RingReader,
    frames_per_point: Arc<AtomicU32>,
    sample_rate: f64,
    /// What a point reads before any data (0 dB of reduction, a floor).
    rest: f64,
    len: usize,
    scratch: Vec<f32>,
}

/// A history of up to `capacity` points at `rate_hz`, for a signal at
/// `sample_rate`, folding with `agg`; points before any data read `rest`.
/// Allocates here; the tap never does.
#[must_use]
pub fn history(
    capacity: usize,
    sample_rate: f64,
    rate_hz: f64,
    agg: Aggregate,
    rest: f64,
) -> (HistoryTap, HistoryReader) {
    let (w, r) = ring(capacity);
    let fpp = Arc::new(AtomicU32::new(frames_per_point(sample_rate, rate_hz)));
    let len = capacity.min(r.capacity());
    (
        HistoryTap {
            ring: w,
            frames_per_point: fpp.clone(),
            agg,
            acc: 0.0,
            frames: 0,
        },
        HistoryReader {
            ring: r,
            frames_per_point: fpp,
            sample_rate,
            rest,
            len,
            scratch: vec![0.0; len],
        },
    )
}

fn frames_per_point(sample_rate: f64, rate_hz: f64) -> u32 {
    let r = if rate_hz.is_finite() && rate_hz > 0.0 {
        rate_hz
    } else {
        30.0
    };
    (sample_rate / r).round().clamp(1.0, f64::from(u32::MAX)) as u32
}

impl HistoryTap {
    /// Fold `value`, which held for `frames` samples (usually one block).
    /// Pushes a point each time a point's worth of frames is in. Wait-free,
    /// no allocation.
    #[inline]
    pub fn push(&mut self, value: f32, frames: u32) {
        let fpp = self.frames_per_point.load(Ordering::Relaxed).max(1);
        let v = f64::from(value);
        let mut left = frames;
        while left > 0 {
            let take = left.min(fpp.saturating_sub(self.frames).max(1));
            self.acc = self
                .agg
                .fold(self.acc, f64::from(self.frames), v, f64::from(take));
            self.frames += take;
            left -= take;
            if self.frames >= fpp {
                self.ring.push(self.acc as f32);
                self.frames = 0;
            }
        }
    }

    /// Push one finished point directly. Wait-free, no allocation.
    #[inline]
    pub fn push_point(&mut self, value: f32) {
        self.ring.push(value);
    }
}

impl HistoryReader {
    /// Points the ring can hold.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.ring.capacity()
    }

    /// Change the point rate (takes effect from the audio thread's next
    /// point).
    pub fn set_rate(&self, rate_hz: f64) {
        self.frames_per_point.store(
            frames_per_point(self.sample_rate, rate_hz),
            Ordering::Relaxed,
        );
    }

    #[must_use]
    pub fn rate_hz(&self) -> f64 {
        self.sample_rate / f64::from(self.frames_per_point.load(Ordering::Relaxed).max(1))
    }

    /// The points [`FeedSource::read`] returns.
    pub fn set_len(&mut self, len: usize) {
        self.len = len.max(1);
    }

    /// The last `out.len()` points, oldest first; the front reads `rest`
    /// until that many exist (and beyond the ring's capacity).
    pub fn snapshot(&mut self, out: &mut [f64]) {
        let n = out.len().min(self.ring.capacity());
        self.scratch.resize(n, 0.0);
        let got = self.ring.latest(&mut self.scratch);
        let pad = out.len() - got;
        out[..pad].fill(self.rest);
        for (o, s) in out[pad..].iter_mut().zip(&self.scratch[n - got..]) {
            *o = f64::from(*s);
        }
    }
}

impl FeedSource for HistoryReader {
    fn configure(&mut self, info: &FeedInfo, _options: &FeedOptions) {
        if let FeedKind::History { len, .. } = info.kind {
            self.set_len(len as usize);
        }
        self.set_rate(info.rate_hz);
    }

    fn read(&mut self, _dt: f64, out: &mut Vec<f64>) {
        out.resize(self.len, self.rest);
        self.snapshot(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_are_oldest_first_and_padded() {
        let (mut tap, mut r) = history(16, 1000.0, 10.0, Aggregate::Last, 0.0);
        for i in 1..=10 {
            tap.push_point(i as f32);
        }
        let mut out = [0.0; 4];
        r.snapshot(&mut out);
        assert_eq!(out, [7.0, 8.0, 9.0, 10.0]);
        let mut long = [9.9; 12];
        r.snapshot(&mut long);
        assert_eq!(
            long,
            [0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]
        );
        // Wrapping keeps the order.
        for i in 11..=40 {
            tap.push_point(i as f32);
        }
        r.snapshot(&mut out);
        assert_eq!(out, [37.0, 38.0, 39.0, 40.0]);
    }

    #[test]
    fn blocks_fold_into_points_at_the_rate() {
        // 1 kHz signal, 10 points/s: a point every 100 frames.
        let (mut tap, mut r) = history(8, 1000.0, 10.0, Aggregate::Min, 0.0);
        for gr in [-1.0, -5.0, -2.0, -3.0] {
            tap.push(gr, 50);
        }
        let mut out = [0.0; 2];
        r.snapshot(&mut out);
        assert_eq!(out, [-5.0, -3.0], "deepest reduction per point");
        // One long block spans several points.
        tap.push(-4.0, 250);
        tap.push(-1.0, 50);
        let mut out = [0.0; 3];
        r.snapshot(&mut out);
        assert_eq!(out, [-4.0, -4.0, -4.0]);
        // Doubling the rate halves the frames per point.
        r.set_rate(20.0);
        assert_eq!(r.rate_hz(), 20.0);
        tap.push(-6.0, 50);
        tap.push(-7.0, 50);
        let mut out = [0.0; 2];
        r.snapshot(&mut out);
        assert_eq!(out, [-6.0, -7.0]);
    }

    #[test]
    fn mean_is_frame_weighted() {
        let (mut tap, mut r) = history(4, 100.0, 1.0, Aggregate::Mean, 0.0);
        tap.push(1.0, 25);
        tap.push(3.0, 75);
        let mut out = [0.0; 1];
        r.snapshot(&mut out);
        assert!((out[0] - 2.5).abs() < 1e-6);
    }
}
