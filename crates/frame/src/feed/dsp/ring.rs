//! The one audio → control handoff every tap uses: a lock-free,
//! single-producer, overwriting ring of `f32`.
//!
//! The audio thread [`RingWriter::push`]es and never waits: when the ring
//! is full the oldest samples go. The control thread copies the latest `n`
//! out with [`RingReader::latest`]. Samples are stored as `AtomicU32` bits,
//! so a reader racing the writer sees old-or-new values, never torn ones.
//! Like a seqlock, the writer announces how far it is about to write
//! (`claimed`) before it writes and publishes (`written`) after, so a
//! reader knows which part of its copy the writer may have lapped: it
//! retries, and at worst returns only the part that is certainly intact.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering, fence};

struct Shared {
    data: Box<[AtomicU32]>,
    mask: usize,
    /// Positions the writer may be writing up to (announced before).
    claimed: AtomicU64,
    /// Samples written since creation (the next write's position).
    written: AtomicU64,
}

/// The audio-thread end. Not `Clone`: one producer.
pub struct RingWriter {
    shared: Arc<Shared>,
    /// Local copy of `written` (only this end writes it).
    pos: u64,
}

/// The control-thread end. Cloneable: any number of readers.
#[derive(Clone)]
pub struct RingReader {
    shared: Arc<Shared>,
}

/// A ring holding at least `capacity` samples (rounded up to a power of
/// two). Allocates here, never afterwards.
#[must_use]
pub fn ring(capacity: usize) -> (RingWriter, RingReader) {
    let cap = capacity.max(2).next_power_of_two();
    let shared = Arc::new(Shared {
        data: (0..cap).map(|_| AtomicU32::new(0)).collect(),
        mask: cap - 1,
        claimed: AtomicU64::new(0),
        written: AtomicU64::new(0),
    });
    (
        RingWriter {
            shared: shared.clone(),
            pos: 0,
        },
        RingReader { shared },
    )
}

impl RingWriter {
    /// Append one sample. Wait-free, no allocation.
    #[inline]
    pub fn push(&mut self, x: f32) {
        let s = &*self.shared;
        s.claimed.store(self.pos + 1, Ordering::Relaxed);
        fence(Ordering::Release);
        s.data[(self.pos as usize) & s.mask].store(x.to_bits(), Ordering::Relaxed);
        self.pos += 1;
        s.written.store(self.pos, Ordering::Release);
    }

    /// Append a block. Wait-free, no allocation.
    #[inline]
    pub fn push_slice(&mut self, xs: &[f32]) {
        let s = &*self.shared;
        s.claimed
            .store(self.pos + xs.len() as u64, Ordering::Relaxed);
        fence(Ordering::Release);
        for &x in xs {
            s.data[(self.pos as usize) & s.mask].store(x.to_bits(), Ordering::Relaxed);
            self.pos += 1;
        }
        s.written.store(self.pos, Ordering::Release);
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.shared.data.len()
    }
}

impl RingReader {
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.shared.data.len()
    }

    /// Samples written since creation.
    #[must_use]
    pub fn written(&self) -> u64 {
        self.shared.written.load(Ordering::Acquire)
    }

    /// Copy the latest `out.len()` samples into `out`, oldest first.
    /// Returns how many were available (≤ `out.len()` and ≤ capacity);
    /// when fewer, they fill the *end* of `out` and the front is left as is.
    pub fn latest(&self, out: &mut [f32]) -> usize {
        let s = &*self.shared;
        let cap = s.data.len() as u64;
        let mut intact = 0;
        for _ in 0..4 {
            let end = s.written.load(Ordering::Acquire);
            let n = (out.len() as u64).min(end).min(cap);
            let start = end - n;
            let dst_at = out.len() - n as usize;
            for (i, d) in out[dst_at..].iter_mut().enumerate() {
                *d = f32::from_bits(
                    s.data[((start + i as u64) as usize) & s.mask].load(Ordering::Relaxed),
                );
            }
            // Which positions might the writer have overwritten meanwhile?
            fence(Ordering::Acquire);
            let lapped_below = s.claimed.load(Ordering::Relaxed).saturating_sub(cap);
            if start >= lapped_below {
                return n as usize;
            }
            intact = end.saturating_sub(lapped_below).min(n) as usize;
        }
        // Under a writer this fast, return only the certainly-intact tail.
        intact
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_is_oldest_first_and_overwrites() {
        let (mut w, r) = ring(4);
        let mut out = [0.0; 3];
        assert_eq!(r.latest(&mut out), 0);
        w.push_slice(&[1.0, 2.0]);
        assert_eq!(r.latest(&mut out), 2);
        assert_eq!(out, [0.0, 1.0, 2.0]);
        w.push_slice(&[3.0, 4.0, 5.0, 6.0]);
        assert_eq!(r.latest(&mut out), 3);
        assert_eq!(out, [4.0, 5.0, 6.0]);
        let mut all = [0.0; 8];
        assert_eq!(r.latest(&mut all), 4, "never more than capacity");
        assert_eq!(&all[4..], &[3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn a_reader_racing_a_writer_sees_a_contiguous_run() {
        let (mut w, r) = ring(1024);
        let writer = std::thread::spawn(move || {
            for i in 0..200_000 {
                w.push(i as f32);
            }
        });
        let mut out = [0.0f32; 256];
        for _ in 0..2000 {
            let n = r.latest(&mut out);
            let run = &out[out.len() - n..];
            for pair in run.windows(2) {
                assert_eq!(pair[1] - pair[0], 1.0, "{pair:?}");
            }
        }
        writer.join().unwrap();
    }
}
