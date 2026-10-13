// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! Backend-agnostic DAW state synchronization engine.
//!
//! Replicates DAW state between peers by subscribing to the
//! streaming service traits in `daw-proto`, wrapping each event in a
//! [`SyncEvent`] envelope with origin peer + sequence, broadcasting
//! outbound, and applying remote envelopes back through the daw
//! mutation API.
//!
//! Backend-agnostic by construction: any backend that implements the
//! streaming traits (`TransportStream`, `MarkersStream`, …) gets
//! synchronization for free. REAPER↔REAPER is the canonical use
//! case; REAPER↔Standalone (testing), REAPER↔ProTools, or any other
//! pair work without engine changes.
//!
//! # Architecture
//!
//! ```text
//! Local DAW (DawEventHub broadcasts at ~30Hz)
//!   ├─ TrackStreamEvent ─┐
//!   ├─ TransportEvent ───┤
//!   ├─ MarkerStreamEvent ┤
//!   └─ … ────────────────┤
//!                         ▼
//!                SynchronizationEngine
//!                   ├─ wrap in SyncEvent { origin: self, seq: N }
//!                   ├─ check suppression (skip if echo)
//!                   └─ broadcast outbound (consumed by sync-repo
//!                      network adapter, applied to remote peer)
//!
//! Remote SyncEvent arrives (via `apply_remote`)
//!   ├─ filter: origin == self → skip
//!   ├─ record in suppression set
//!   └─ apply via daw mutation API
//! ```
//!
//! Network transport (peer discovery, mDNS, Ableton Link, the actual
//! socket between two peers) is **not** in this crate. That lives in
//! the FastTrackStudio/sync repo, which depends on this crate and
//! wires `outbound()` → peer connection → `apply_remote()` on the
//! other side.

pub mod apply;
pub mod daw_module;
pub mod drift;
pub mod engine;
pub mod heartbeat;
pub mod subscriptions;
pub mod suppression;

pub use engine::SynchronizationEngine;

// ── Sync types (formerly sync-proto) ─────────────────────────────────
//
// Lifted from sync-proto since these are domain types the engine
// owns. The sync repo (network adapter) imports from here when it
// needs to serialize the envelope over the wire.

use daw::service::{
    FxEvent, ItemEvent, MarkerEvent, ProjectEvent, RegionEvent, RoutingEvent, TakeEvent,
    TempoMapEvent, TrackEvent, Transport,
};
use facet::Facet;

/// Identifies a sync session and the local peer within it.
#[derive(Clone, Debug, Facet)]
pub struct SyncSession {
    /// Unique identifier for the sync session (shared by all peers).
    pub session_id: String,
    /// Unique identifier for this peer within the session.
    pub peer_id: String,
    /// Human-readable name for this peer (e.g. "Studio A").
    pub display_name: String,
}

/// State of a connected peer in the sync session.
#[derive(Clone, Debug, Facet)]
pub struct SyncPeer {
    pub peer_id: String,
    pub display_name: String,
    /// Whether this peer is the session host (first to join).
    pub is_host: bool,
    pub config: SyncConfig,
}

/// What to synchronize and how to resolve conflicts.
#[derive(Clone, Debug, Facet)]
pub struct SyncConfig {
    pub transport: bool,
    pub tracks: bool,
    pub fx: bool,
    pub items: bool,
    pub routing: bool,
    pub tempo_map: bool,
    pub markers: bool,
    pub regions: bool,
    pub conflict_policy: ConflictPolicy,
}

impl SyncConfig {
    /// Sync everything with last-write-wins.
    pub fn all() -> Self {
        Self {
            transport: true,
            tracks: true,
            fx: true,
            items: true,
            routing: true,
            tempo_map: true,
            markers: true,
            regions: true,
            conflict_policy: ConflictPolicy::LastWriteWins,
        }
    }

    /// Transport-only — simplest mode.
    pub fn transport_only() -> Self {
        Self {
            transport: true,
            tracks: false,
            fx: false,
            items: false,
            routing: false,
            tempo_map: false,
            markers: false,
            regions: false,
            conflict_policy: ConflictPolicy::LastWriteWins,
        }
    }
}

/// How to resolve conflicting edits from multiple peers.
#[repr(u8)]
#[derive(Clone, Debug, Facet)]
pub enum ConflictPolicy {
    /// Most recent edit wins (using sequence numbers).
    LastWriteWins,
    /// Host peer's edits always take priority.
    HostPriority,
}

/// Discriminated union of all DAW event domains. Each variant wraps
/// the corresponding daw-proto event type.
#[repr(u8)]
#[derive(Clone, Debug, Facet)]
pub enum SyncDomain {
    Transport(Transport),
    Track(TrackEvent),
    Fx(FxEvent),
    Item(ItemEvent),
    Take(TakeEvent),
    Routing(RoutingEvent),
    TempoMap(TempoMapEvent),
    Marker(MarkerEvent),
    Region(RegionEvent),
    Project(ProjectEvent),
}

/// A sync event envelope. Every change detected locally gets wrapped
/// before broadcast; remote peers use `origin_peer` to filter echoes
/// and `sequence` for ordering / conflict resolution.
#[derive(Clone, Debug, Facet)]
pub struct SyncEvent {
    /// Peer that originated this change.
    pub origin_peer: String,
    /// Monotonically increasing sequence number from the origin peer.
    pub sequence: u64,
    /// Project GUID this event applies to.
    pub project_guid: String,
    /// The domain-specific event payload.
    pub domain: SyncDomain,
    /// Wall-clock timestamp (ms since Unix epoch) when created. Used
    /// by the drift corrector to estimate the master's current
    /// position at the time the follower processes a heartbeat,
    /// compensating for network + RPC latency.
    pub created_at_ms: u64,
}

impl SyncEvent {
    /// Current wall-clock time in milliseconds since Unix epoch.
    pub fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Current status of the synchronization engine.
#[repr(u8)]
#[derive(Clone, Debug, Facet)]
pub enum SyncStatus {
    /// Not in a sync session.
    Disconnected,
    /// Joining a session (handshake in progress).
    Connecting,
    /// Connected and actively syncing.
    Connected,
    /// Performing initial state sync (late join).
    InitialSync,
}
