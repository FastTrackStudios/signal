//! Diagnostic probes that execute entirely on REAPER's main thread.
//!
//! Architect's sync bridge dispatches the whole method to the main
//! thread once, so any tight loop inside the impl runs in-process
//! with zero IPC overhead. Useful for measuring intrinsic latency of
//! csurf / hub / forwarder paths without the cross-process noise the
//! standard subscribe path picks up.

use crate::{DawResult, ProjectContext};
use facet::Facet;

/// One per-project position frame received from a peer. Mirrors
/// `daw_audio_sync::clock_sync::RemotePosition` on the wire.
#[derive(Clone, Debug, Default, Facet)]
pub struct PeerProjectPosition {
    /// Hex-encoded 16-byte project id (process-local on the peer).
    pub project_id_hex: String,
    /// Peer's audio host clock when broadcast (microseconds).
    pub host_micros: i64,
    pub playhead_seconds: f64,
    pub sample_rate: f64,
    pub playrate: f64,
    pub is_playing: bool,
    /// Milliseconds since this frame was received locally.
    pub received_age_ms: u64,
}

/// Wire-format snapshot of a peer the local ClockSync session knows
/// about. Mirrors `daw_audio_sync::clock_sync::PeerInfo` but with
/// owned types (`String` instead of `Instant`/`SocketAddr`) so it
/// crosses the vox boundary cleanly.
#[derive(Clone, Debug, Default, Facet)]
pub struct PeerSummary {
    /// Stable peer id (UUID string).
    pub id: String,
    /// `host:port` of the peer's listen socket.
    pub addr: String,
    /// Remote clock − local clock, microseconds.
    pub offset_us: i64,
    /// One-way network delay estimate, microseconds.
    pub delay_us: i64,
    /// Milliseconds since the last successful round-trip.
    pub rtt_age_ms: u64,
    /// Milliseconds since the last announce.
    pub announce_age_ms: u64,
    /// Latest broadcast playhead in seconds, if known. `f64::NAN`
    /// when no position has been received yet. For multi-project
    /// peers this is whatever frame arrived first (typically the
    /// peer's current project); use `audio_sync_peer_projects` for
    /// the full per-project list.
    pub remote_playhead_seconds: f64,
    /// Whether the remote was playing at last broadcast.
    pub remote_is_playing: bool,
}

/// Audio-thread snapshot — mirrors `daw_audio_sync::AudioSnapshot` (the
/// `daw_transport_sync` core's) on the wire so cross-process consumers can
/// observe per-buffer state. `host_micros` is rounded to whole µs here;
/// the snapshot's `playrate` is not carried.
#[derive(Clone, Copy, Debug, Default, Facet)]
pub struct AudioSyncSnapshot {
    pub sequence: u64,
    pub host_micros: u64,
    pub playhead_seconds: f64,
    pub sample_rate: f64,
    pub buffer_len: u32,
    pub is_playing: bool,
}

#[architect::rpc]
pub trait Diagnostics {
    /// Measure the in-process event-bus publish→receive floor.
    ///
    /// Subscribes to the hub's broadcast receiver, then for each
    /// sample synthesizes a `VolumeChanged` event, calls
    /// `hub().publish_track(...)`, and spins on `try_recv` until it
    /// arrives. Returns microseconds per sample.
    ///
    /// This measures what an **in-process consumer** (a bridge-side
    /// OSC/MIDI task, an audio-graph node, an inspector tab) pays
    /// once a csurf event has already fired. It excludes:
    ///   - REAPER's csurf callback dispatch (deferred to next main
    ///     loop tick, outside our control)
    ///   - vox encode + IPC (the cross-process subscribe test
    ///     measures that path)
    ///   - tokio worker scheduling (try_recv is synchronous)
    ///
    /// Result demonstrates that the architect facade supports
    /// in-process subscribers at broadcast-channel speed (~1-50µs)
    /// using the same backend impl that serves cross-process
    /// subscribers over vox.
    fn hub_publish_latency_us(&self, project: ProjectContext, samples: u32) -> Vec<u64>;

    /// Latest [`AudioSyncSnapshot`] from REAPER's audio thread, or
    /// `None` if the audio hook hasn't fired yet (no audio engine
    /// running, hook registration failed, REAPER just started).
    fn audio_sync_snapshot(&self) -> Option<AudioSyncSnapshot>;

    /// Sample `count` consecutive [`AudioSyncSnapshot`]s spaced
    /// `interval_us` microseconds apart. Useful for measuring audio
    /// buffer rate from the test side without flooding RPC channels.
    fn audio_sync_observe(&self, count: u32, interval_us: u64) -> Vec<AudioSyncSnapshot>;

    /// Current ClockSync peer table. Empty if the clock-sync layer
    /// wasn't brought up (FTS_AUDIO_SYNC_PORT not set) or if no
    /// peers have announced yet. Each entry summarises a peer the
    /// local session is tracking — clock offset, network delay,
    /// last-known sample position.
    fn audio_sync_peers(&self) -> Vec<PeerSummary>;

    /// Local peer id of this ClockSync session, or empty string if
    /// the layer isn't running. Useful for tests to identify which
    /// peer is which without having to bind their own session.
    fn audio_sync_self_peer_id(&self) -> String;

    /// Manually insert a peer into the local ClockSync table. Used
    /// by tests + by environments where multicast discovery is
    /// blocked (corporate LANs, CI loopback). The peer id is the
    /// UUID string the remote reports via `audio_sync_self_peer_id`;
    /// the addr is `host:port` of the remote's ClockSync socket.
    fn audio_sync_seed_peer(&self, peer_id: &str, addr: &str) -> DawResult<()>;

    /// Latest decision from the drift corrector. Returns the empty
    /// struct (all zero, no leader) when the corrector isn't
    /// running (FTS_AUDIO_SYNC_DRIFT not set, or controller hasn't
    /// ticked yet).
    fn audio_sync_drift_decision(&self) -> DriftDecisionSummary;

    /// Every per-project position frame the local ClockSync has
    /// received from a given peer. Empty when the peer is unknown
    /// or hasn't broadcast yet. Use this when FTS-session needs to
    /// see every project a peer is tracking, not just one.
    fn audio_sync_peer_projects(&self, peer_id: &str) -> Vec<PeerProjectPosition>;

    /// Per-project snapshots from the LOCAL multi-project registry.
    /// Each entry is (project_id_hex, AudioSyncSnapshot). Empty
    /// when the multi-project hook hasn't observed any project
    /// yet (REAPER not playing, or no audio engine).
    fn audio_sync_local_projects(&self) -> Vec<LocalProjectSnapshot>;
}

/// One entry in `audio_sync_local_projects` — the LOCAL multi-project
/// registry's per-project snapshot. Use this to see what the local
/// audio thread is observing for each open project tab.
#[derive(Clone, Debug, Default, Facet)]
pub struct LocalProjectSnapshot {
    /// Hex-encoded 16-byte project id assigned by the bridge.
    pub project_id_hex: String,
    pub snapshot: AudioSyncSnapshot,
}

/// Wire-format mirror of `daw_audio_sync::drift::DriftDecision`.
/// `leader_peer_id` is an empty string when no leader is elected;
/// `drift_seconds` is `f64::NAN` when no leader.
#[derive(Clone, Debug, Default, Facet)]
pub struct DriftDecisionSummary {
    pub sequence: u64,
    pub leader_peer_id: String,
    pub drift_seconds: f64,
    pub target_rate: f64,
}
