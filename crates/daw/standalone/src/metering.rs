//! Per-track peak metering store.
//!
//! A cheap, lock-free bridge between the realtime audio callback (writer) and
//! the [`Peaks`](daw_proto::Peaks) service (reader). One [`TrackMeter`] cell per
//! track index holds the latest block peak plus a decaying peak-hold, as linear
//! `0..1` magnitudes per channel. When no audio engine is running the cells stay
//! at silence, so `track_peak` degrades to `-inf dB` — exactly like the old
//! stub did.
//!
//! Always compiled (the atomics are free without the `audio` feature); only the
//! *writer* (the cpal mixer) is gated behind `audio`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Peak-hold decay applied per audio block (≈100 blocks/sec at 48k/512), so the
/// hold marker falls over roughly half a second after a transient.
pub const HOLD_DECAY: f32 = 0.96;

/// One track's meter cell: instantaneous block peak + decaying hold, per channel
/// (L / R), stored as `f32` bits in atomics so the audio callback writes and the
/// service reads without a lock.
#[derive(Debug, Default)]
pub struct TrackMeter {
    left: AtomicU32,
    right: AtomicU32,
    hold_left: AtomicU32,
    hold_right: AtomicU32,
}

impl TrackMeter {
    #[inline]
    fn load(a: &AtomicU32) -> f32 {
        f32::from_bits(a.load(Ordering::Relaxed))
    }
    #[inline]
    fn store(a: &AtomicU32, v: f32) {
        a.store(v.to_bits(), Ordering::Relaxed);
    }

    /// Write a new block peak for both channels, decaying the existing hold and
    /// latching it up to the new peak. Call once per audio block.
    pub fn write(&self, left: f32, right: f32, hold_decay: f32) {
        Self::store(&self.left, left);
        Self::store(&self.right, right);
        let hl = (Self::load(&self.hold_left) * hold_decay).max(left);
        let hr = (Self::load(&self.hold_right) * hold_decay).max(right);
        Self::store(&self.hold_left, hl);
        Self::store(&self.hold_right, hr);
    }

    /// Linear instantaneous peak for a channel (`0` = left/mono, `1` = right).
    pub fn peak(&self, channel: u32) -> f32 {
        if channel == 1 {
            Self::load(&self.right)
        } else {
            Self::load(&self.left)
        }
    }

    /// Linear peak-hold for a channel.
    pub fn hold(&self, channel: u32) -> f32 {
        if channel == 1 {
            Self::load(&self.hold_right)
        } else {
            Self::load(&self.hold_left)
        }
    }
}

/// A fixed bank of [`TrackMeter`] cells, one per track index, shared (`Arc`)
/// between the audio engine and the metering service. The cell count is fixed at
/// construction; rebuild the bank (and re-attach the engine) when the track
/// count changes.
#[derive(Debug)]
pub struct Meters {
    cells: Vec<TrackMeter>,
}

impl Meters {
    /// A bank with `track_count` silent cells.
    pub fn new(track_count: usize) -> Arc<Self> {
        let mut cells = Vec::with_capacity(track_count);
        cells.resize_with(track_count, TrackMeter::default);
        Arc::new(Self { cells })
    }

    /// An empty bank (no cells) — the default when no audio engine is attached.
    pub fn empty() -> Arc<Self> {
        Arc::new(Self { cells: Vec::new() })
    }

    /// The cell for `index`, if present.
    pub fn cell(&self, index: usize) -> Option<&TrackMeter> {
        self.cells.get(index)
    }

    /// Number of track cells in the bank.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}

/// Convert a linear magnitude (`0..1`) to dBFS, flooring silence at `-150 dB`.
pub fn linear_to_db(linear: f32) -> f64 {
    if linear <= 1e-6 {
        -150.0
    } else {
        20.0 * (linear as f64).log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_conversion_floors_silence_and_tops_at_unity() {
        assert_eq!(linear_to_db(0.0), -150.0);
        assert_eq!(linear_to_db(1e-9), -150.0);
        assert!((linear_to_db(1.0)).abs() < 1e-9); // 0 dBFS
        assert!((linear_to_db(0.5) - (-6.0206)).abs() < 1e-3); // ≈ -6 dB
    }

    #[test]
    fn peak_hold_latches_then_decays() {
        let m = TrackMeter::default();
        m.write(0.8, 0.4, 0.5);
        assert_eq!(m.peak(0), 0.8);
        assert_eq!(m.peak(1), 0.4);
        assert_eq!(m.hold(0), 0.8); // first write latches the hold up

        // A quieter block: instantaneous peak follows, hold decays (0.8*0.5=0.4)
        // but never below the new peak.
        m.write(0.1, 0.1, 0.5);
        assert_eq!(m.peak(0), 0.1);
        assert!((m.hold(0) - 0.4).abs() < 1e-6);

        // Louder transient re-latches the hold.
        m.write(0.9, 0.9, 0.5);
        assert_eq!(m.hold(0), 0.9);
    }

    #[test]
    fn bank_sizing_and_bounds() {
        let bank = Meters::new(3);
        assert_eq!(bank.len(), 3);
        assert!(bank.cell(0).is_some());
        assert!(bank.cell(2).is_some());
        assert!(bank.cell(3).is_none());

        let empty = Meters::empty();
        assert!(empty.is_empty());
        assert!(empty.cell(0).is_none());
    }
}
