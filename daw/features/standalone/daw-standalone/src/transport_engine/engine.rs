//! `TransportEngine` — lock-free shared sample clock.
//!
//! Hands an `Arc<TransportShared>` to both the audio callback (writes
//! `playhead_samples` via [`advance`](TransportShared::advance), reads
//! the rest) and the control thread (writes everything else, snapshots
//! the playhead for the proto `Transport` mirror + the UI tick stream).
//!
//! All state is `core::sync::atomic` — no `Mutex`, no allocation, no
//! blocking. Safe to call `advance` from a cpal/AudioWorklet callback.
//!
//! Scope of this first slice:
//! - Single static tempo (BPM via [`set_tempo_bpm`]).
//! - Loop wrap with Firewheel-style frame clamping.
//! - Varispeed via fixed-point sub-sample accumulator so non-integer
//!   `playrate` doesn't drift.
//!
//! Out of scope (deferred): [`DynamicTempoMap`](super::tempo_map::DynamicTempoMap),
//! count-in / pre-roll, sub-tick automation, `RecordMode::Item` punch.

use core::sync::atomic::{AtomicBool, AtomicI64, AtomicU8, AtomicU32, AtomicU64, Ordering};

use daw_transport_sync::{AudioSnapshot, SnapshotCell};

use super::block::{BlockPlan, BlockSegment, LocateSlot, ScheduledLocate};
use super::clock::{InstantMusical, InstantSamples, InstantSeconds, SampleClock};
use super::tempo_map::StaticTempoMap;

/// Fractional bits of the fixed-point playhead: 40.24 — ~130 days of
/// timeline at 48 kHz, to 6e-8 of a sample. Varispeed advances a block by
/// `frames * rate` samples, which is rarely whole; carrying the fraction
/// (instead of rounding every block) keeps the audio continuous across
/// blocks and a drift-correction rate like 1.00008 exact.
const FRAC_BITS: u32 = 24;
const FRAC_ONE: f64 = (1u64 << FRAC_BITS) as f64;

#[inline]
fn fixed_from_samples(samples: f64) -> i64 {
    (samples * FRAC_ONE).round() as i64
}

#[inline]
fn fixed_to_samples(fixed: i64) -> f64 {
    fixed as f64 / FRAC_ONE
}

/// The sync machinery a transport carries for
/// [`crate::transport_sync::SyncBackend`]: the per-buffer snapshot the
/// audio thread publishes, the scheduled-locate slot it lands, and the
/// buffer counter.
struct SyncCells {
    snapshot: SnapshotCell,
    locate: LocateSlot,
    sequence: AtomicU64,
}

impl core::fmt::Debug for SyncCells {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SyncCells")
            .field("snapshot", &self.snapshot.load())
            .field("sequence", &self.sequence.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

/// `repr(u8)` view of `daw_proto::PlayState`. Kept separate so this
/// module compiles without proto.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayStateRepr {
    Stopped = 0,
    Playing = 1,
    Paused = 2,
    Recording = 3,
}

impl PlayStateRepr {
    #[inline]
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Playing,
            2 => Self::Paused,
            3 => Self::Recording,
            _ => Self::Stopped,
        }
    }
    #[inline]
    pub fn is_advancing(self) -> bool {
        matches!(self, Self::Playing | Self::Recording)
    }
}

/// Loop region in samples. `end == 0` (or `end <= start`) disables.
#[derive(Debug, Clone, Copy)]
pub struct LoopRegionSamples {
    pub start: InstantSamples,
    pub end: InstantSamples,
}

impl LoopRegionSamples {
    #[inline]
    pub fn is_valid(self) -> bool {
        self.end.0 > self.start.0
    }
}

/// Lock-free shared state. Field naming = `_bits` for f64-as-u64.
#[derive(Debug)]
pub struct TransportShared {
    /// Output sample rate. Set once at engine creation.
    sample_rate: AtomicU32,

    /// Current playhead, in output-rate samples, fixed point
    /// ([`FRAC_BITS`] fractional bits).
    ///
    /// Only [`advance`](Self::advance), [`begin_block`](Self::begin_block)
    /// and the `set_playhead*` setters write this. Readers can use
    /// `Relaxed` — UI ticks tolerate staleness on the order of one block.
    playhead_fixed: AtomicI64,

    /// [`PlayStateRepr`] as `u8`.
    play_state: AtomicU8,

    /// Loop enabled.
    looping: AtomicBool,
    metronome: AtomicBool,
    loop_start_samples: AtomicI64,
    loop_end_samples: AtomicI64,

    /// `playrate` f64 bits. 1.0 = realtime, 2.0 = double speed.
    playrate_bits: AtomicU64,

    /// Static BPM, f64 bits. Updated when proto `Transport.tempo`
    /// changes.
    tempo_bpm_bits: AtomicU64,

    sync: SyncCells,
}

impl TransportShared {
    pub fn new(sample_rate: u32, initial_bpm: f64) -> Self {
        Self {
            sample_rate: AtomicU32::new(sample_rate),
            playhead_fixed: AtomicI64::new(0),
            play_state: AtomicU8::new(PlayStateRepr::Stopped as u8),
            looping: AtomicBool::new(false),
            metronome: AtomicBool::new(false),
            loop_start_samples: AtomicI64::new(0),
            loop_end_samples: AtomicI64::new(0),
            playrate_bits: AtomicU64::new(1.0f64.to_bits()),
            tempo_bpm_bits: AtomicU64::new(initial_bpm.to_bits()),
            sync: SyncCells {
                snapshot: SnapshotCell::new(),
                locate: LocateSlot::default(),
                sequence: AtomicU64::new(0),
            },
        }
    }

    // ── Audio-thread API ────────────────────────────────────────────

    /// Advance the playhead by `frames` output-rate samples. **Must
    /// only be called from the audio callback.** Honors `playrate` and
    /// loop wrap.
    ///
    /// Returns the playhead value (in samples) at the **start** of the
    /// processed block — what a node would tag its block with.
    ///
    /// `frames` is the buffer size; on loop wrap the function still
    /// reports having consumed the full block (sample-accurate
    /// loop-end node behavior is a future concern — see the Firewheel
    /// `clamp + teleport` pattern noted in tempo_map.rs).
    #[inline]
    pub fn advance(&self, frames: u32) -> InstantSamples {
        let state = PlayStateRepr::from_u8(self.play_state.load(Ordering::Relaxed));
        let start = self.playhead_fixed.load(Ordering::Relaxed);
        if !state.is_advancing() || frames == 0 {
            return InstantSamples(start >> FRAC_BITS);
        }

        let playrate = f64::from_bits(self.playrate_bits.load(Ordering::Relaxed));
        // Frames consumed at output rate → timeline samples advanced
        // (playrate > 1 advances faster), fraction carried.
        let end = self.advanced(start, frames as usize, playrate);
        self.playhead_fixed.store(end, Ordering::Relaxed);
        InstantSamples(start >> FRAC_BITS)
    }

    /// `start` (fixed point) moved on by `frames` output frames at
    /// `rate`, wrapped by the loop region.
    #[inline]
    fn advanced(&self, start: i64, frames: usize, rate: f64) -> i64 {
        let mut end = start + (frames as f64 * rate * FRAC_ONE).round() as i64;
        if self.looping.load(Ordering::Relaxed) {
            let loop_start = self.loop_start_samples.load(Ordering::Relaxed) << FRAC_BITS;
            let loop_end = self.loop_end_samples.load(Ordering::Relaxed) << FRAC_BITS;
            if loop_end > loop_start && end >= loop_end {
                // Teleport: wrap modulo loop span, mapped from
                // distance past loop_start. Handles huge buffer sizes
                // that span >1 loop iteration.
                let span = loop_end - loop_start;
                let offset = (end - loop_start).rem_euclid(span);
                end = loop_start + offset;
            }
        }
        end
    }

    /// Open one audio buffer of `frames` output frames: land a scheduled
    /// locate that falls in it, commit where the playhead ends up, and
    /// publish the buffer's sync snapshot. **Audio thread only** (one
    /// driver at a time — the callback, or the soft clock when no device
    /// runs); lock- and allocation-free.
    ///
    /// `stamp_micros` is when the buffer's first frame starts, in the sync
    /// clock ([`crate::transport_sync::now_micros`], filtered by a
    /// [`daw_transport_sync::BufferClock`]); `period_micros` is how long
    /// the buffer lasts in that clock.
    ///
    /// Returns what to render: each [`BlockSegment`] is the timeline from
    /// `start_samples` at `rate`, or silence. The playhead is committed
    /// *before* rendering, so a control-thread seek made while the block
    /// renders is kept (it is never overwritten by this block's advance).
    pub fn begin_block(&self, frames: u32, stamp_micros: f64, period_micros: f64) -> BlockPlan {
        let n = frames as usize;
        let sr = f64::from(self.sample_rate().max(1));
        let state = self.play_state();
        let playing0 = state.is_advancing();
        let rate0 = self.playrate();
        let fixed0 = self.playhead_fixed.load(Ordering::Relaxed);
        let seg0 = BlockSegment {
            offset: 0,
            frames: n,
            start_samples: fixed_to_samples(fixed0),
            rate: rate0,
            playing: playing0,
        };

        let period = if period_micros.is_finite() && period_micros > 0.0 {
            period_micros
        } else {
            n as f64 / sr * 1e6
        };
        let landing = self.sync.locate.peek().and_then(|(generation, loc)| {
            // Where in this buffer the locate's moment falls, in frames
            // (negative: already late; -inf: "at the next buffer").
            let offset_f = if loc.at_micros.is_finite() {
                (loc.at_micros - stamp_micros) / period * n as f64
            } else {
                f64::NEG_INFINITY
            };
            // Nearer the next buffer's first frame than this one's last:
            // the next buffer lands it.
            (offset_f < n as f64 - 0.5).then_some((generation, loc, offset_f))
        });

        let (plan, end_fixed, landed) = match landing {
            None => {
                let end = if playing0 && n > 0 {
                    self.advanced(fixed0, n, rate0)
                } else {
                    fixed0
                };
                (BlockPlan::single(seg0), end, None)
            }
            Some((generation, loc, offset_f)) => {
                let rate = loc.rate.clamp(0.25, 4.0);
                let k = if offset_f > 0.0 {
                    (offset_f.round() as usize).min(n)
                } else {
                    0
                };
                let target = loc.position_seconds * sr;
                // The playhead is `target` exactly at the locate's moment:
                // frame k sits (k - offset_f) frames from it (sub-sample
                // when the moment falls between frames; the lateness when
                // it has passed).
                let start = if loc.playing && offset_f.is_finite() {
                    (k as f64 - offset_f).mul_add(rate, target)
                } else {
                    target
                };
                let seg1 = BlockSegment {
                    offset: k,
                    frames: n - k,
                    start_samples: start,
                    rate,
                    playing: loc.playing,
                };
                let start_fixed = fixed_from_samples(start);
                let end = if loc.playing && n > k {
                    self.advanced(start_fixed, n - k, rate)
                } else {
                    start_fixed
                };
                let plan = if k == 0 {
                    BlockPlan::single(seg1)
                } else {
                    BlockPlan::split(BlockSegment { frames: k, ..seg0 }, seg1)
                };
                (plan, end, Some((generation, loc, rate)))
            }
        };

        if let Some((generation, loc, rate)) = landed {
            self.playrate_bits.store(rate.to_bits(), Ordering::Relaxed);
            let next = match (loc.playing, playing0) {
                (true, true) => state, // keep Recording a recording
                (true, false) => PlayStateRepr::Playing,
                (false, true) => PlayStateRepr::Stopped,
                (false, false) => state, // Paused stays paused
            };
            self.play_state.store(next as u8, Ordering::Relaxed);
            // The locate wins over anything written since this block
            // began: it is the newest instruction.
            self.playhead_fixed.store(end_fixed, Ordering::Relaxed);
            self.sync.locate.consume(generation);
        } else if end_fixed != fixed0 {
            // A seek from a control thread since `fixed0` was read wins
            // over this block's advance.
            let _ = self.playhead_fixed.compare_exchange(
                fixed0,
                end_fixed,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }

        let first = plan.first();
        let sequence = self.sync.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        self.sync.snapshot.store(&AudioSnapshot {
            sequence,
            project_id: [0; 16],
            host_micros: stamp_micros,
            playhead_seconds: first.start_samples / sr,
            playrate: first.rate,
            sample_rate: sr,
            buffer_len: frames,
            is_playing: first.playing,
        });
        plan
    }

    // ── Control-thread reads ────────────────────────────────────────

    #[inline]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }
    /// The playhead, whole output-rate samples (floored).
    #[inline]
    pub fn playhead_samples(&self) -> InstantSamples {
        InstantSamples(self.playhead_fixed.load(Ordering::Relaxed) >> FRAC_BITS)
    }
    /// The playhead, output-rate samples with the varispeed fraction kept.
    #[inline]
    pub fn playhead_samples_f64(&self) -> f64 {
        fixed_to_samples(self.playhead_fixed.load(Ordering::Relaxed))
    }
    /// The latest buffer's sync snapshot; `None` until a driver has run
    /// one buffer through [`begin_block`](Self::begin_block).
    #[inline]
    pub fn sync_snapshot(&self) -> Option<AudioSnapshot> {
        self.sync.snapshot.load()
    }
    #[inline]
    pub fn play_state(&self) -> PlayStateRepr {
        PlayStateRepr::from_u8(self.play_state.load(Ordering::Relaxed))
    }
    #[inline]
    pub fn is_looping(&self) -> bool {
        self.looping.load(Ordering::Relaxed)
    }
    #[inline]
    pub fn loop_region(&self) -> Option<LoopRegionSamples> {
        let s = self.loop_start_samples.load(Ordering::Relaxed);
        let e = self.loop_end_samples.load(Ordering::Relaxed);
        let r = LoopRegionSamples {
            start: InstantSamples(s),
            end: InstantSamples(e),
        };
        r.is_valid().then_some(r)
    }
    #[inline]
    pub fn playrate(&self) -> f64 {
        f64::from_bits(self.playrate_bits.load(Ordering::Relaxed))
    }
    #[inline]
    pub fn tempo_bpm(&self) -> f64 {
        f64::from_bits(self.tempo_bpm_bits.load(Ordering::Relaxed))
    }

    // ── Control-thread writes ───────────────────────────────────────

    #[inline]
    pub fn set_play_state(&self, s: PlayStateRepr) {
        self.play_state.store(s as u8, Ordering::Relaxed);
    }
    #[inline]
    pub fn set_playhead(&self, p: InstantSamples) {
        self.playhead_fixed
            .store(p.0 << FRAC_BITS, Ordering::Relaxed);
    }
    /// Seek to a fractional sample position.
    #[inline]
    pub fn set_playhead_samples_f64(&self, samples: f64) {
        self.playhead_fixed
            .store(fixed_from_samples(samples), Ordering::Relaxed);
    }
    /// Arm a scheduled locate (replacing any not yet landed): the driver
    /// lands it on the exact frame of the buffer its moment falls in.
    /// Any thread; never blocks the audio thread.
    pub fn schedule_locate(&self, locate: ScheduledLocate) {
        self.sync.locate.arm(locate);
    }
    /// Drop a scheduled locate that has not landed yet.
    pub fn cancel_scheduled_locate(&self) {
        self.sync.locate.cancel();
    }
    #[inline]
    pub fn metronome(&self) -> bool {
        self.metronome.load(Ordering::Relaxed)
    }

    pub fn set_metronome(&self, v: bool) {
        self.metronome.store(v, Ordering::Relaxed);
    }

    pub fn set_looping(&self, v: bool) {
        self.looping.store(v, Ordering::Relaxed);
    }
    /// Set loop region. Pass `None` to disable (also clears the bool).
    pub fn set_loop_region(&self, r: Option<LoopRegionSamples>) {
        match r {
            Some(r) if r.is_valid() => {
                self.loop_start_samples.store(r.start.0, Ordering::Relaxed);
                self.loop_end_samples.store(r.end.0, Ordering::Relaxed);
            }
            _ => {
                self.loop_start_samples.store(0, Ordering::Relaxed);
                self.loop_end_samples.store(0, Ordering::Relaxed);
                self.looping.store(false, Ordering::Relaxed);
            }
        }
    }
    /// Set varispeed. Clamps to the same 0.25..=4.0 range the proto
    /// service uses.
    #[inline]
    pub fn set_playrate(&self, rate: f64) {
        let clamped = rate.clamp(0.25, 4.0);
        self.playrate_bits
            .store(clamped.to_bits(), Ordering::Relaxed);
    }
    #[inline]
    pub fn set_tempo_bpm(&self, bpm: f64) {
        self.tempo_bpm_bits.store(bpm.to_bits(), Ordering::Relaxed);
    }
    /// Rewrite the sample rate — used by an audio engine when it
    /// attaches and discovers the device's actual rate. Should only
    /// be called while the engine is stopped (or before the soft
    /// clock starts) to avoid playhead-in-samples becoming
    /// inconsistent under a unit change.
    #[inline]
    pub fn set_sample_rate(&self, sr: u32) {
        debug_assert!(sr > 0);
        self.sample_rate.store(sr, Ordering::Relaxed);
    }
}

/// Convenience handle = `Arc<TransportShared>` plus derived snapshots.
#[derive(Debug, Clone)]
pub struct TransportEngine {
    pub shared: alloc::sync::Arc<TransportShared>,
}

impl TransportEngine {
    pub fn new(sample_rate: u32, initial_bpm: f64) -> Self {
        Self {
            shared: alloc::sync::Arc::new(TransportShared::new(sample_rate, initial_bpm)),
        }
    }

    /// Derive a snapshot suitable for emitting a [`PositionTick`] or
    /// mirroring into proto `Transport.playhead_position`.
    pub fn snapshot(&self) -> TransportSnapshot {
        let s = &*self.shared;
        let clock = SampleClock::new(s.sample_rate());
        let samples = s.playhead_samples();
        let seconds = clock.samples_to_seconds(samples);
        let tempo = StaticTempoMap::new(s.tempo_bpm().max(1e-3));
        let musical = tempo.samples_to_musical(samples, s.playrate(), &clock);
        TransportSnapshot {
            samples,
            seconds,
            musical,
            play_state: s.play_state(),
        }
    }

    /// Seek by seconds, converting via the sample rate.
    pub fn seek_seconds(&self, seconds: f64) {
        let clock = SampleClock::new(self.shared.sample_rate());
        self.shared
            .set_playhead(clock.seconds_to_samples(InstantSeconds(seconds)));
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TransportSnapshot {
    pub samples: InstantSamples,
    pub seconds: InstantSeconds,
    pub musical: InstantMusical,
    pub play_state: PlayStateRepr,
}

extern crate alloc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_when_stopped_is_noop() {
        let e = TransportEngine::new(48_000, 120.0);
        e.shared.set_play_state(PlayStateRepr::Stopped);
        let start = e.shared.advance(512);
        assert_eq!(start.0, 0);
        assert_eq!(e.shared.playhead_samples().0, 0);
    }

    #[test]
    fn advance_when_playing_bumps_playhead() {
        let e = TransportEngine::new(48_000, 120.0);
        e.shared.set_play_state(PlayStateRepr::Playing);
        let _ = e.shared.advance(512);
        let _ = e.shared.advance(512);
        assert_eq!(e.shared.playhead_samples().0, 1024);
    }

    #[test]
    fn varispeed_doubles_advance() {
        let e = TransportEngine::new(48_000, 120.0);
        e.shared.set_play_state(PlayStateRepr::Playing);
        e.shared.set_playrate(2.0);
        let _ = e.shared.advance(512);
        assert_eq!(e.shared.playhead_samples().0, 1024);
    }

    #[test]
    fn loop_wraps_at_end() {
        let e = TransportEngine::new(48_000, 120.0);
        e.shared.set_play_state(PlayStateRepr::Playing);
        e.shared.set_loop_region(Some(LoopRegionSamples {
            start: InstantSamples(0),
            end: InstantSamples(1000),
        }));
        e.shared.set_looping(true);
        // 3 blocks of 400 = 1200 advance, should wrap to 200.
        for _ in 0..3 {
            let _ = e.shared.advance(400);
        }
        assert_eq!(e.shared.playhead_samples().0, 200);
    }

    #[test]
    fn loop_wraps_when_block_overshoots_by_multiple_iterations() {
        let e = TransportEngine::new(48_000, 120.0);
        e.shared.set_play_state(PlayStateRepr::Playing);
        e.shared.set_loop_region(Some(LoopRegionSamples {
            start: InstantSamples(100),
            end: InstantSamples(200),
        }));
        e.shared.set_looping(true);
        e.shared.set_playhead(InstantSamples(150));
        // Advance 1050 samples: from 150 → 1200; (1200-100) % 100 = 0
        // → playhead = 100.
        let _ = e.shared.advance(1050);
        assert_eq!(e.shared.playhead_samples().0, 100);
    }

    #[test]
    fn snapshot_seconds_and_musical_track_playhead() {
        let e = TransportEngine::new(48_000, 120.0);
        e.shared.set_playhead(InstantSamples(24_000)); // 0.5 s, 1 beat @ 120
        let snap = e.snapshot();
        assert!((snap.seconds.0 - 0.5).abs() < 1e-9);
        assert!((snap.musical.0 - 1.0).abs() < 1e-9);
    }
}
