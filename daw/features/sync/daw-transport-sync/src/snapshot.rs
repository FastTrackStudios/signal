//! What the audio thread knows at the start of each buffer, published
//! without locks.
//!
//! The audio thread writes one [`AudioSnapshot`] per buffer into a
//! [`SnapshotCell`]; anyone reads it. A seqlock: the writer makes the
//! sequence odd, stores the fields, makes it even; a reader that sees an
//! odd or changed sequence reads again. The writer never waits and never
//! allocates — safe on a real-time thread.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering, fence};

/// A project, as the backend identifies it for sync. `[0; 16]` is "the
/// current project" for a backend that only ever syncs one.
pub type ProjectId = [u8; 16];

/// One audio buffer's observation. All values are at the start of the
/// buffer — the moment its first sample leaves (or is handed to) the
/// device.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioSnapshot {
    /// Increments once per buffer: a reader can tell it missed some.
    pub sequence: u64,
    pub project_id: ProjectId,
    /// When, in the backend's sync clock (see [`crate::clock`]),
    /// microseconds.
    pub host_micros: f64,
    /// The playhead at that moment, seconds.
    pub playhead_seconds: f64,
    /// How fast the playhead moves against the clock (1.0 = nominal) — the
    /// transport's rate, drift correction included.
    pub playrate: f64,
    /// Device sample rate, Hz.
    pub sample_rate: f64,
    /// Frames in this buffer.
    pub buffer_len: u32,
    pub is_playing: bool,
}

impl Default for AudioSnapshot {
    fn default() -> Self {
        Self {
            sequence: 0,
            project_id: [0; 16],
            host_micros: 0.0,
            playhead_seconds: 0.0,
            playrate: 1.0,
            sample_rate: 48_000.0,
            buffer_len: 0,
            is_playing: false,
        }
    }
}

impl AudioSnapshot {
    /// This snapshot as a [`crate::Position`] in its own clock.
    #[must_use]
    pub const fn position(&self) -> crate::Position {
        crate::Position {
            host_micros: self.host_micros,
            playhead_seconds: self.playhead_seconds,
            playrate: self.playrate,
            is_playing: self.is_playing,
        }
    }
}

fn split(id: ProjectId) -> (u64, u64) {
    let mut hi = [0u8; 8];
    let mut lo = [0u8; 8];
    hi.copy_from_slice(&id[0..8]);
    lo.copy_from_slice(&id[8..16]);
    (u64::from_le_bytes(hi), u64::from_le_bytes(lo))
}

fn combine(hi: u64, lo: u64) -> ProjectId {
    let mut out = [0u8; 16];
    out[0..8].copy_from_slice(&hi.to_le_bytes());
    out[8..16].copy_from_slice(&lo.to_le_bytes());
    out
}

/// Single-writer (the audio thread), many-reader cell for the latest
/// [`AudioSnapshot`].
pub struct SnapshotCell {
    seq: AtomicU64,
    sequence: AtomicU64,
    project_hi: AtomicU64,
    project_lo: AtomicU64,
    host_micros: AtomicU64,
    playhead: AtomicU64,
    playrate: AtomicU64,
    sample_rate: AtomicU64,
    buffer_len: AtomicU32,
    is_playing: AtomicU32,
}

impl SnapshotCell {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            seq: AtomicU64::new(0),
            sequence: AtomicU64::new(0),
            project_hi: AtomicU64::new(0),
            project_lo: AtomicU64::new(0),
            host_micros: AtomicU64::new(0),
            playhead: AtomicU64::new(0),
            playrate: AtomicU64::new(0),
            sample_rate: AtomicU64::new(0),
            buffer_len: AtomicU32::new(0),
            is_playing: AtomicU32::new(0),
        }
    }

    /// Publish a snapshot. Audio thread only (one writer); wait-free.
    #[inline]
    pub fn store(&self, snap: &AudioSnapshot) {
        let start = self.seq.load(Ordering::Relaxed).wrapping_add(1);
        self.seq.store(start, Ordering::Relaxed);
        // The odd sequence must be visible before any field changes: a
        // release fence orders it ahead of the relaxed stores below (a
        // release STORE would only order what came before it).
        fence(Ordering::Release);
        self.sequence.store(snap.sequence, Ordering::Relaxed);
        let (hi, lo) = split(snap.project_id);
        self.project_hi.store(hi, Ordering::Relaxed);
        self.project_lo.store(lo, Ordering::Relaxed);
        self.host_micros
            .store(snap.host_micros.to_bits(), Ordering::Relaxed);
        self.playhead
            .store(snap.playhead_seconds.to_bits(), Ordering::Relaxed);
        self.playrate
            .store(snap.playrate.to_bits(), Ordering::Relaxed);
        self.sample_rate
            .store(snap.sample_rate.to_bits(), Ordering::Relaxed);
        self.buffer_len.store(snap.buffer_len, Ordering::Relaxed);
        self.is_playing
            .store(u32::from(snap.is_playing), Ordering::Relaxed);
        self.seq.store(start.wrapping_add(1), Ordering::Release);
    }

    /// The latest snapshot; `None` before the first, or if the writer kept
    /// it busy for every retry (sub-microsecond windows — rare).
    #[must_use]
    pub fn load(&self) -> Option<AudioSnapshot> {
        for _ in 0..8 {
            let s1 = self.seq.load(Ordering::Acquire);
            if s1 == 0 {
                return None;
            }
            if s1 & 1 != 0 {
                core::hint::spin_loop();
                continue;
            }
            let snap = AudioSnapshot {
                sequence: self.sequence.load(Ordering::Relaxed),
                project_id: combine(
                    self.project_hi.load(Ordering::Relaxed),
                    self.project_lo.load(Ordering::Relaxed),
                ),
                host_micros: f64::from_bits(self.host_micros.load(Ordering::Relaxed)),
                playhead_seconds: f64::from_bits(self.playhead.load(Ordering::Relaxed)),
                playrate: f64::from_bits(self.playrate.load(Ordering::Relaxed)),
                sample_rate: f64::from_bits(self.sample_rate.load(Ordering::Relaxed)),
                buffer_len: self.buffer_len.load(Ordering::Relaxed),
                is_playing: self.is_playing.load(Ordering::Relaxed) != 0,
            };
            // Every field read must happen before the sequence is read
            // again: an acquire fence orders the relaxed loads above ahead
            // of it (an acquire LOAD would only order what comes after).
            fence(Ordering::Acquire);
            if self.seq.load(Ordering::Relaxed) == s1 {
                return Some(snap);
            }
        }
        None
    }
}

impl Default for SnapshotCell {
    fn default() -> Self {
        Self::new()
    }
}
