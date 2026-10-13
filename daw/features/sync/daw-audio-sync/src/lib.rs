// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! The REAPER adapter for [`daw_transport_sync`]: sample-accurate
//! multi-machine playback for N REAPER instances playing the same project.
//!
//! The maths lives in the core — the per-buffer snapshot
//! ([`AudioSnapshot`]/[`SnapshotCell`], re-exported here), the NTP clock
//! estimator, [`daw_transport_sync::Position`] projection and the PI
//! [`daw_transport_sync::DriftController`]. This crate is what REAPER
//! needs around it:
//!
//! - **Hooks** ([`AudioSyncHook`], [`registry::MultiProjectHook`]):
//!   reaper-medium `OnAudioBuffer` impls that fill the core's snapshot on
//!   REAPER's real-time audio thread, stamped in REAPER's `time_precise`
//!   clock.
//! - **Carrier** ([`clock_sync`]): UDP multicast discovery, unicast
//!   ping/pong feeding one core `ClockEstimator` per peer, and position
//!   frames.
//! - **Actuator** ([`drift`]): a tokio loop running the core's
//!   `DriftController` against the elected leader, applying its rate via
//!   `CSurf_OnPlayRateChange` on the main thread.
//!
//! # Realtime discipline
//!
//! Everything on the audio thread must be:
//! - **alloc-free** — no `Vec::push`, `Box::new`, `String::from`, etc.
//! - **lock-free** — no `Mutex`, no `RwLock`, no blocking syscalls.
//! - **bounded latency** — short, predictable work per callback.
//!
//! The core's [`SnapshotCell`] is a seqlock the audio thread writes without
//! waiting; readers detect torn reads via its sequence.

use std::sync::Arc;

pub mod clock_sync;
pub mod drift;
pub mod registry;

/// The snapshot types are the core's: one definition, whatever the
/// backend. `host_micros` is `f64` microseconds in REAPER's
/// `time_precise` clock; `project_id` is assigned by the
/// [`registry`] (`[0; 16]` is "the current project" for single-project
/// consumers).
pub use daw_transport_sync::{AudioSnapshot, ProjectId, SnapshotCell};

#[inline]
pub(crate) fn split_project_id(id: ProjectId) -> (u64, u64) {
    let mut hi = [0u8; 8];
    let mut lo = [0u8; 8];
    hi.copy_from_slice(&id[0..8]);
    lo.copy_from_slice(&id[8..16]);
    (u64::from_le_bytes(hi), u64::from_le_bytes(lo))
}

#[inline]
pub(crate) fn combine_project_id(hi: u64, lo: u64) -> ProjectId {
    let mut out = [0u8; 16];
    out[0..8].copy_from_slice(&hi.to_le_bytes());
    out[8..16].copy_from_slice(&lo.to_le_bytes());
    out
}

use reaper_medium::{
    OnAudioBuffer, OnAudioBufferArgs, ProjectContext, RealTimeAudioThreadScope,
    Reaper as MediumReaper,
};

/// REAPER's sync clock now, microseconds: `time_precise` (a monotonic
/// high-resolution clock, seconds) scaled. Every snapshot this crate
/// publishes is stamped in it, so clock-sync offsets are between REAPER
/// instances' `time_precise` clocks.
#[inline]
pub(crate) fn reaper_clock_micros(reaper: &MediumReaper<RealTimeAudioThreadScope>) -> f64 {
    clock_micros(reaper.low())
}

/// REAPER's sync clock now, microseconds, from any thread — the clock
/// every snapshot's `host_micros` is in (see `reaper_clock_micros`).
/// What a remote follower pings to learn its offset to this REAPER.
#[inline]
pub fn clock_micros(low: &reaper_low::Reaper) -> f64 {
    // Only exposed at the low binding level (medium hasn't wrapped it).
    // `time_precise` reads a monotonic OS clock; it is callable from any
    // thread (the audio hook calls it on the real-time thread).
    low.time_precise() * 1_000_000.0
}

/// The play rate to publish with a snapshot.
///
/// There is no audio-thread-safe way to read it: `Master_GetPlayRate` is
/// `MainThreadOnly` in reaper-medium, and REAPER does not document it as
/// safe from the audio thread. So the snapshot carries 1.0 (nominal) — the
/// same assumption the position frames always made. Consequence: a
/// follower treats the leader as playing at nominal rate; a project whose
/// play rate is deliberately off 1.0 is not yet followed at that rate.
/// (An audio-thread measurement is possible — playhead advance per buffer
/// over buffer duration — if that is ever needed.)
pub(crate) const PUBLISHED_PLAYRATE: f64 = 1.0;

/// REAPER audio hook. Registered once at extension load via
/// `ReaperSession::audio_reg_hardware_hook_add`. Writes a fresh
/// [`AudioSnapshot`] to the shared [`SnapshotCell`] on every callback's
/// pre-buffer phase.
///
/// Holds a `MediumReaper<RealTimeAudioThreadScope>` so it can call
/// `get_play_position_2_ex` (audio-thread-safe variant) and
/// `time_precise` for the host clock.
pub struct AudioSyncHook {
    cell: Arc<SnapshotCell>,
    reaper: MediumReaper<RealTimeAudioThreadScope>,
    counter: u64,
}

impl AudioSyncHook {
    pub fn new(cell: Arc<SnapshotCell>, reaper: MediumReaper<RealTimeAudioThreadScope>) -> Self {
        Self {
            cell,
            reaper,
            counter: 0,
        }
    }
}

impl OnAudioBuffer for AudioSyncHook {
    fn call(&mut self, args: OnAudioBufferArgs) {
        // We sample on the pre-buffer phase only — keeps the snapshot
        // rate at one per audio buffer instead of two.
        if args.is_post {
            return;
        }
        self.counter = self.counter.wrapping_add(1);

        let host_micros = reaper_clock_micros(&self.reaper);

        // get_play_position_2_ex: position of next audio block —
        // matches the audio thread's notion of "now". get_play_position
        // is for the displayed cursor which lags by latency.
        let pos_value = self
            .reaper
            .get_play_position_2_ex(ProjectContext::CurrentProject)
            .get();

        let is_playing = self
            .reaper
            .get_play_state_ex(ProjectContext::CurrentProject)
            .is_playing;

        self.cell.store(&AudioSnapshot {
            sequence: self.counter,
            project_id: [0u8; 16],
            host_micros,
            playhead_seconds: pos_value,
            playrate: PUBLISHED_PLAYRATE,
            sample_rate: args.srate.get(),
            buffer_len: args.len,
            is_playing,
        });
    }
}

/// Convenience: build the cell + hook pair and return the cell handle
/// for the reader side. Caller registers the returned hook via
/// `ReaperSession::audio_reg_hardware_hook_add`.
pub fn build_hook(
    reaper: MediumReaper<RealTimeAudioThreadScope>,
) -> (Arc<SnapshotCell>, AudioSyncHook) {
    let cell = Arc::new(SnapshotCell::new());
    let hook = AudioSyncHook::new(cell.clone(), reaper);
    (cell, hook)
}

// ── Process-global cell ────────────────────────────────────────────
//
// The audio hook is registered in the REAPER extension (daw-bridge),
// but the Diagnostics RPC impl that exposes snapshots lives in
// daw-reaper. To bridge them without circular deps, we publish the
// cell as a global here. daw-bridge sets it on extension load;
// daw-reaper reads it from the Diagnostics impl.

static GLOBAL_CELL: std::sync::OnceLock<Arc<SnapshotCell>> = std::sync::OnceLock::new();

/// Publish the cell so other crates can read snapshots. Call once at
/// extension load. Subsequent calls are silently ignored.
pub fn set_global_cell(cell: Arc<SnapshotCell>) {
    let _ = GLOBAL_CELL.set(cell);
}

/// Read the latest snapshot from the process-global cell. Returns
/// `None` if the cell hasn't been published yet (extension not
/// loaded) or if the audio hook hasn't fired yet (no audio engine).
pub fn global_snapshot() -> Option<AudioSnapshot> {
    GLOBAL_CELL.get()?.load()
}

// ── Process-global ClockSync ───────────────────────────────────────
//
// Same pattern as `GLOBAL_CELL`: daw-bridge publishes the live
// session on startup; consumers (daw-reaper's Diagnostics impl, etc.)
// read peer-table snapshots without re-binding sockets.

static GLOBAL_CLOCK_SYNC: std::sync::OnceLock<Arc<clock_sync::ClockSync>> =
    std::sync::OnceLock::new();

pub fn set_global_clock_sync(session: Arc<clock_sync::ClockSync>) {
    let _ = GLOBAL_CLOCK_SYNC.set(session);
}

pub fn global_clock_sync() -> Option<&'static Arc<clock_sync::ClockSync>> {
    GLOBAL_CLOCK_SYNC.get()
}

// ── Process-global DriftCorrector ──────────────────────────────────

static GLOBAL_DRIFT_CORRECTOR: std::sync::OnceLock<Arc<drift::DriftCorrector>> =
    std::sync::OnceLock::new();

pub fn set_global_drift_corrector(corrector: Arc<drift::DriftCorrector>) {
    let _ = GLOBAL_DRIFT_CORRECTOR.set(corrector);
}

pub fn global_drift_corrector() -> Option<&'static Arc<drift::DriftCorrector>> {
    GLOBAL_DRIFT_CORRECTOR.get()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn snapshot_round_trip() {
        let cell = SnapshotCell::new();
        let snap = AudioSnapshot {
            sequence: 42,
            project_id: [7u8; 16],
            host_micros: 1_000_000.25,
            playhead_seconds: 3.5,
            playrate: PUBLISHED_PLAYRATE,
            sample_rate: 48000.0,
            buffer_len: 256,
            is_playing: true,
        };
        cell.store(&snap);
        let read = cell.load().expect("stored");
        assert_eq!(read, snap);
    }

    #[test]
    fn empty_cell_returns_none() {
        let cell = SnapshotCell::new();
        assert!(cell.load().is_none());
    }

    // Tore on arm64 (a few runs in a hundred) until the core's seqlock
    // gained its fences: fence(Release) after the odd `seq` store,
    // fence(Acquire) before the re-read.
    #[test]
    fn seqlock_under_contention() {
        let cell = Arc::new(SnapshotCell::new());
        let writer_cell = cell.clone();
        let writer = thread::spawn(move || {
            for i in 0..10_000u64 {
                writer_cell.store(&AudioSnapshot {
                    sequence: i,
                    host_micros: i as f64 * 1000.0,
                    playhead_seconds: i as f64 * 0.01,
                    buffer_len: 256,
                    is_playing: i % 2 == 0,
                    ..AudioSnapshot::default()
                });
            }
        });
        let mut last_seq = 0u64;
        for _ in 0..100_000 {
            if let Some(s) = cell.load() {
                // Cross-field consistency: playhead and host_micros
                // were written together, so they must match.
                assert!((s.playhead_seconds - s.sequence as f64 * 0.01).abs() < 1e-9);
                assert_eq!(s.host_micros, s.sequence as f64 * 1000.0);
                assert!(s.sequence >= last_seq);
                last_seq = s.sequence;
            }
        }
        writer.join().unwrap();
    }
}
