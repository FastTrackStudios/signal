//! What one audio buffer plays, decided at its start — and the
//! scheduled locate that can change it part-way through.
//!
//! Every driver of the transport (the cpal output callback, the duplex
//! callback, the soft clock) opens each buffer with
//! [`TransportShared::begin_block`](super::TransportShared::begin_block).
//! That one call, on the audio thread:
//!
//! 1. lands a [`ScheduledLocate`] whose moment falls in this buffer (or
//!    has already passed) on its exact frame,
//! 2. splits the buffer at that frame into at most two
//!    [`BlockSegment`]s — each "render the timeline from here at this
//!    rate", or silence,
//! 3. commits where the playhead ends up (fractional — varispeed
//!    never rounds a block),
//! 4. publishes the buffer's [`daw_transport_sync::AudioSnapshot`]: the
//!    playhead at the buffer's start *and when that was*.
//!
//! All of it is atomics: no locks, no allocation.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering, fence};

/// A run of frames inside one buffer, all rendered the same way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockSegment {
    /// First frame of the run within the buffer.
    pub offset: usize,
    /// Frames in the run.
    pub frames: usize,
    /// The timeline at the run's first frame, output-rate samples
    /// (fractional under varispeed).
    pub start_samples: f64,
    /// Timeline samples per output frame (the transport's varispeed rate).
    pub rate: f64,
    /// Whether the transport rolls through this run (silence when not).
    pub playing: bool,
}

impl BlockSegment {
    /// The timeline at buffer frame 0 as this run sees it — its start
    /// moved back by its offset at its rate. Where a clock-driven hook
    /// (the guide) should think the block begins.
    #[must_use]
    pub fn start_at_frame_zero(&self) -> f64 {
        if self.playing {
            (self.offset as f64).mul_add(-self.rate, self.start_samples)
        } else {
            self.start_samples
        }
    }
}

/// One buffer's plan: one segment, or two when a scheduled locate lands
/// inside it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockPlan {
    segments: [BlockSegment; 2],
    len: usize,
}

impl BlockPlan {
    pub(crate) const fn single(seg: BlockSegment) -> Self {
        Self {
            segments: [seg, seg],
            len: 1,
        }
    }

    pub(crate) const fn split(a: BlockSegment, b: BlockSegment) -> Self {
        Self {
            segments: [a, b],
            len: 2,
        }
    }

    /// The runs, in order, covering the whole buffer.
    #[must_use]
    pub fn segments(&self) -> &[BlockSegment] {
        &self.segments[..self.len]
    }

    /// The run at the buffer's start.
    #[must_use]
    pub const fn first(&self) -> BlockSegment {
        self.segments[0]
    }

    /// Whether any run plays (a buffer that does not can be silence).
    #[must_use]
    pub fn any_playing(&self) -> bool {
        self.segments().iter().any(|s| s.playing)
    }

    /// The last run that plays — the block's clock for a hook that takes
    /// one position per block.
    #[must_use]
    pub fn last_playing(&self) -> Option<BlockSegment> {
        self.segments().iter().rev().find(|s| s.playing).copied()
    }
}

/// "Be at `position_seconds` at `at_micros`, playing or not, at `rate`."
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScheduledLocate {
    /// The moment, in the sync clock
    /// ([`crate::transport_sync::now_micros`]), µs. `f64::NEG_INFINITY`
    /// = the next buffer's first frame, whenever that is (no lateness
    /// correction).
    pub at_micros: f64,
    /// The playhead at that moment, seconds.
    pub position_seconds: f64,
    pub playing: bool,
    /// The rate from that moment on.
    pub rate: f64,
}

/// A single scheduled locate, armed by control threads and landed by
/// the audio thread — a seqlock plus a "pending generation" word.
///
/// Writers (any control thread) serialise on the sequence with a CAS —
/// they may spin briefly against each other, never against the audio
/// thread. The audio thread never waits: a locate it catches mid-write
/// is simply taken one buffer later.
#[derive(Debug, Default)]
pub(crate) struct LocateSlot {
    seq: AtomicU64,
    /// The sequence value of the armed, not yet landed, locate; 0 = none.
    pending: AtomicU64,
    at_bits: AtomicU64,
    position_bits: AtomicU64,
    rate_bits: AtomicU64,
    playing: AtomicBool,
}

impl LocateSlot {
    /// Arm `locate`, replacing any armed one. Control threads only.
    pub(crate) fn arm(&self, locate: ScheduledLocate) {
        let odd = loop {
            let s = self.seq.load(Ordering::Relaxed);
            if s & 1 == 0
                && self
                    .seq
                    .compare_exchange_weak(
                        s,
                        s.wrapping_add(1),
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    )
                    .is_ok()
            {
                break s.wrapping_add(1);
            }
            core::hint::spin_loop();
        };
        fence(Ordering::Release);
        self.at_bits
            .store(locate.at_micros.to_bits(), Ordering::Relaxed);
        self.position_bits
            .store(locate.position_seconds.to_bits(), Ordering::Relaxed);
        self.rate_bits
            .store(locate.rate.to_bits(), Ordering::Relaxed);
        self.playing.store(locate.playing, Ordering::Relaxed);
        let even = odd.wrapping_add(1);
        self.seq.store(even, Ordering::Release);
        self.pending.store(even, Ordering::Release);
    }

    /// Drop an armed locate that has not landed.
    pub(crate) fn cancel(&self) {
        self.pending.store(0, Ordering::Release);
    }

    /// The armed locate and its generation, if one is armed and not being
    /// rewritten right now. Audio thread; wait-free.
    pub(crate) fn peek(&self) -> Option<(u64, ScheduledLocate)> {
        let generation = self.pending.load(Ordering::Acquire);
        if generation == 0 {
            return None;
        }
        let s1 = self.seq.load(Ordering::Acquire);
        if s1 != generation {
            // Being rewritten, or a newer one is about to be published.
            return None;
        }
        let locate = ScheduledLocate {
            at_micros: f64::from_bits(self.at_bits.load(Ordering::Relaxed)),
            position_seconds: f64::from_bits(self.position_bits.load(Ordering::Relaxed)),
            rate: f64::from_bits(self.rate_bits.load(Ordering::Relaxed)),
            playing: self.playing.load(Ordering::Relaxed),
        };
        fence(Ordering::Acquire);
        (self.seq.load(Ordering::Relaxed) == s1).then_some((generation, locate))
    }

    /// Mark generation `generation` landed — unless a newer locate was
    /// armed meanwhile, which then stays pending.
    pub(crate) fn consume(&self, generation: u64) {
        let _ = self
            .pending
            .compare_exchange(generation, 0, Ordering::AcqRel, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(at: f64) -> ScheduledLocate {
        ScheduledLocate {
            at_micros: at,
            position_seconds: 1.0,
            playing: true,
            rate: 1.0,
        }
    }

    #[test]
    fn armed_locate_is_seen_until_consumed() {
        let slot = LocateSlot::default();
        assert!(slot.peek().is_none());
        slot.arm(loc(5.0));
        let (generation, l) = slot.peek().unwrap();
        assert_eq!(l, loc(5.0));
        slot.consume(generation);
        assert!(slot.peek().is_none());
    }

    #[test]
    fn a_newer_arm_survives_consuming_the_older() {
        let slot = LocateSlot::default();
        slot.arm(loc(5.0));
        let (old, _) = slot.peek().unwrap();
        slot.arm(loc(9.0));
        slot.consume(old);
        assert_eq!(slot.peek().unwrap().1, loc(9.0));
    }

    #[test]
    fn cancel_disarms() {
        let slot = LocateSlot::default();
        slot.arm(loc(5.0));
        slot.cancel();
        assert!(slot.peek().is_none());
    }
}
