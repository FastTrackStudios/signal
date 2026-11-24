//! The audio arriving at an offline block, captured while the song plays:
//! how a block analyses in a host that cannot render it offline (any DAW
//! but Studio). Play the song through once; the analysis reads what was
//! heard.
//!
//! The audio thread copies each piece into a [`CaptureRing`] (fixed,
//! lock-free, stamped with its song frame); the control side drains it
//! into a [`Recording`] by song position, so a stop, a loop or a seek
//! lands each piece where it was in the song.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// Words a piece takes in the ring beyond its samples: its frame (two
/// words) and its length.
const HEADER: usize = 3;

/// A single-producer, single-consumer ring of words: the audio thread
/// writes pieces, the control thread reads them. Sized when made; a piece
/// that does not fit is dropped (counted), never waited for.
pub struct CaptureRing {
    words: alloc::boxed::Box<[AtomicU32]>,
    head: AtomicUsize,
    tail: AtomicUsize,
    dropped: AtomicUsize,
}

impl CaptureRing {
    /// Room for `seconds` of stereo at `sample_rate` between drains.
    #[must_use]
    pub fn new(sample_rate: f32, seconds: f32) -> Self {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = (sample_rate.max(1.0) * seconds.max(0.1)) as usize;
        Self {
            words: (0..frames * 2 + HEADER * 64)
                .map(|_| AtomicU32::new(0))
                .collect(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
        }
    }

    fn cap(&self) -> usize {
        self.words.len()
    }

    /// Audio thread: one piece at song frame `frame`. Allocation-free.
    pub fn push(&self, frame: u64, left: &[f32], right: &[f32]) {
        let len = left.len().min(right.len());
        let need = HEADER + 2 * len;
        let tail = self.tail.load(Ordering::Relaxed);
        let used = tail.wrapping_sub(self.head.load(Ordering::Acquire));
        if self.cap().saturating_sub(used) < need {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let cap = self.cap();
        let put = |i: usize, w: u32| {
            if let Some(slot) = self.words.get((tail + i) % cap) {
                slot.store(w, Ordering::Relaxed);
            }
        };
        #[allow(clippy::cast_possible_truncation)]
        {
            put(0, frame as u32);
            put(1, (frame >> 32) as u32);
            put(2, len as u32);
        }
        for (k, (l, r)) in left.iter().zip(right).take(len).enumerate() {
            put(HEADER + 2 * k, l.to_bits());
            put(HEADER + 2 * k + 1, r.to_bits());
        }
        self.tail.store(tail.wrapping_add(need), Ordering::Release);
    }

    /// Pieces dropped for want of room since it was made.
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Control thread: every piece waiting, into `into`.
    pub fn drain(&self, into: &mut Recording) {
        let cap = self.cap();
        let get = |i: usize| {
            self.words
                .get(i % cap)
                .map_or(0, |w| w.load(Ordering::Relaxed))
        };
        let mut head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        while head != tail {
            let frame = u64::from(get(head)) | (u64::from(get(head + 1)) << 32);
            let len = usize::try_from(get(head + 2)).unwrap_or(0);
            into.put(
                frame,
                (0..len).map(|k| {
                    (
                        f32::from_bits(get(head + HEADER + 2 * k)),
                        f32::from_bits(get(head + HEADER + 2 * k + 1)),
                    )
                }),
            );
            head = head.wrapping_add(HEADER + 2 * len);
        }
        self.head.store(head, Ordering::Release);
    }
}

/// What was heard, by song frame: gaps (never played) are silence.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recording {
    /// The first frame held.
    pub start: u64,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl Recording {
    fn put(&mut self, frame: u64, samples: impl Iterator<Item = (f32, f32)>) {
        if self.left.is_empty() {
            self.start = frame;
        }
        if frame < self.start {
            let pad = usize::try_from(self.start - frame).unwrap_or(0);
            self.left.splice(0..0, core::iter::repeat_n(0.0, pad));
            self.right.splice(0..0, core::iter::repeat_n(0.0, pad));
            self.start = frame;
        }
        let first = usize::try_from(frame - self.start).unwrap_or(0);
        for (k, (l, r)) in samples.enumerate() {
            let at = first + k;
            if at >= self.left.len() {
                self.left.resize(at + 1, 0.0);
                self.right.resize(at + 1, 0.0);
            }
            if let (Some(dl), Some(dr)) = (self.left.get_mut(at), self.right.get_mut(at)) {
                *dl = l;
                *dr = r;
            }
        }
    }

    /// The front channel (the left: a dual-capsule vocal's front capsule),
    /// as the analysis reads it.
    #[must_use]
    pub fn front(&self) -> Vec<f64> {
        self.left.iter().map(|s| f64::from(*s)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pieces_land_where_they_played_through_a_seek() {
        let ring = CaptureRing::new(100.0, 1.0);
        let mut rec = Recording::default();
        ring.push(10, &[1.0, 2.0], &[-1.0, -2.0]);
        ring.push(12, &[3.0], &[-3.0]);
        ring.drain(&mut rec);
        ring.push(5, &[9.0], &[-9.0]); // a seek back
        ring.push(11, &[7.0], &[-7.0]); // played again: replaces
        ring.drain(&mut rec);
        assert_eq!(rec.start, 5);
        assert_eq!(rec.left, [9.0, 0.0, 0.0, 0.0, 0.0, 1.0, 7.0, 3.0]);
        assert_eq!(rec.right[5..], [-1.0, -7.0, -3.0]);
        assert_eq!(ring.dropped(), 0);
    }

    #[test]
    fn a_full_ring_drops_rather_than_waits() {
        let ring = CaptureRing::new(10.0, 0.1);
        let big = alloc::vec![0.5_f32; 500];
        ring.push(0, &big, &big);
        assert_eq!(ring.dropped(), 1);
    }
}
