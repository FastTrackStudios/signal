//! `SetlistService` implementation
//!
//! Split into focused sub-modules:
//! - `hydration` — Song hydration, chart extraction, caching
//! - `navigation` — Song/section navigation and seeking
//! - `playback` — Play, pause, stop, loop controls
//! - `build` — Setlist assembly from open DAW projects
//! - `polling` — Active indices calculation, transport streaming, subscriptions

mod build;
// Combined-setlist RPP export drives `daw::file` (REAPER project combining) —
// native-only. wasm gets a stub (see the bottom of this file).
#[cfg(not(target_arch = "wasm32"))]
mod combined;
pub mod demo;
// Watches a `.base` setlist file on disk (native std::fs + a spawned
// JoinHandle-held watcher) — native-only.
#[cfg(not(target_arch = "wasm32"))]
pub mod file_watcher;
mod hydration;
// The SynchronizationEngine is REAPER-linked (daw-synchronization pulls the
// reaper daw backend), so the live daw-sync bridge is native-only.
#[cfg(not(target_arch = "wasm32"))]
#[cfg(feature = "reaper")]
pub mod live_daw_sync;
// Holds a spawned JoinHandle forwarding a `.base` file's changes — native-only.
#[cfg(not(target_arch = "wasm32"))]
pub mod live_sync;
mod navigation;
mod playback;
mod polling;
pub mod position_sync;
mod record;
mod review;

use crate::cache::Cache;
use crate::event_bus::{EventBus, WatchBus};
use architect::platform::Instant;
use session_proto::{ActiveIndices, QueuedTarget, Setlist, Song, SongChartHydration};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;

use position_sync::PositionSyncBridge;

pub(crate) use hydration::SongCacheEntry;

pub(crate) const HYDRATION_CONCURRENCY: usize = 12;
pub(crate) const MIDI_TRACK_TAG: &str = "CHORDS";
/// Project ext-state section/key carrying a song's keyflow chart text.
///
/// Written by hosts that know a song's chart (the demo engine stamps the
/// bundled charts; an organize/combine flow can stamp imported ones) and
/// read by hydration as the chart source when no MIDI chart-analysis
/// service is mounted (e.g. the daw-standalone network engine).
pub const CHART_EXT_STATE_SECTION: &str = "FTS";
pub const CHART_EXT_STATE_KEY: &str = "chart_text";
pub(crate) const ACTIVE_HYDRATION_POLL_MS: u64 = 2000;
pub(crate) const ACTIVE_HYDRATION_TICK_MS: u64 = 500;
pub(crate) const ACTIVE_HYDRATION_POLL_MAX_MS: u64 = 15000;
pub(crate) const ACTIVE_INDICES_PROGRESS_EMIT_MS: u64 = 100;
pub(crate) const CHART_REFRESH_FALLBACK_POLL_MS: u64 = 5000;
pub(crate) const PROJECT_SWITCH_DEBOUNCE_MS: u64 = 900;
pub(crate) const TRANSPORT_TIME_EPSILON_SECS: f64 = 0.002;
pub(crate) const TRANSPORT_PROGRESS_EPSILON: f64 = 0.0005;

/// Implementation of `SetlistService`
pub struct SetlistServiceImpl<D = ()> {
    /// Native in-process DAW backend. Host crates provide the concrete backend
    /// (for example daw-reaper); session only calls daw-proto service traits.
    pub(crate) daw: D,
    /// Current setlist state
    pub(crate) setlist: Arc<RwLock<Option<Setlist>>>,
    /// Currently active song ID (cached locally to avoid RPC calls)
    /// Using ID instead of index ensures stability when songs are reordered
    pub(crate) active_song_id: Arc<RwLock<Option<String>>>,
    /// Cached active indices (updated by polling loop at 60Hz)
    /// Used for instant navigation without RPC calls
    pub(crate) cached_indices: Arc<RwLock<ActiveIndices>>,
    /// Queued navigation target (flashes in UI until transport reaches it)
    /// Only one target can be queued at a time
    pub(crate) queued_target: Arc<RwLock<Option<QueuedTarget>>>,
    /// `#[subscribe]` hubs — the stream layer serves subscribers from these;
    /// the pumps (`subscribe_impl` / `subscribe_active_impl`) publish into them.
    pub(crate) events_hub: architect::PubSub<session_proto::SetlistEvent>,
    pub(crate) indices_hub: architect::PubSub<session_proto::ActiveIndices>,
    /// Monotonic setlist revision stream for subscribers (hydration/build updates)
    pub(crate) setlist_update_bus: Arc<WatchBus<u64>>,
    /// Last setlist revision value
    pub(crate) setlist_revision: Arc<AtomicU64>,
    /// Full-song cache keyed by project GUID
    pub(crate) song_cache: Cache<String, SongCacheEntry>,
    /// Delta updates for individual hydrated songs
    pub(crate) hydration_bus: Arc<EventBus<(usize, Song)>>,
    /// Delta updates for hydrated chart payloads
    pub(crate) chart_hydration_bus: Arc<EventBus<(usize, SongChartHydration)>>,
    /// Chart payload cache keyed by project GUID
    pub(crate) chart_cache: Cache<String, SongChartHydration>,
    /// Whether the connected DAW backend supports `source_fingerprint()`.
    /// None = unknown, Some(true) = supported, Some(false) = unsupported.
    pub(crate) fingerprint_method_supported: Arc<RwLock<Option<bool>>>,
    /// Last fallback chart refresh attempt when fingerprint API is unavailable.
    pub(crate) last_chart_refresh_attempt: Cache<String, Instant>,
    /// Monotonic build generation to cancel stale background hydration tasks
    pub(crate) build_generation: Arc<AtomicU64>,
    /// Bidirectional position sync between song tabs and setlist tab.
    /// None until a combined setlist is generated.
    pub(crate) position_sync: Arc<RwLock<Option<PositionSyncBridge>>>,
    /// Every song's takes and what the room thought of them, by project
    /// guid. Loaded from the song's ext state on demand and written
    /// back on every mark, so it survives the evening.
    pub(crate) reviews: Arc<RwLock<HashMap<String, session_proto::review::Review>>>,
    /// Where the take being recorded started, in the song's own
    /// seconds, and in which project.
    ///
    /// Held only between record and stop: a pass exists once it has
    /// both ends, and a recording that is still running is not a take
    /// anybody can rate.
    pub(crate) recording_from: Arc<RwLock<Option<(String, f64)>>>,
}

impl<D> Clone for SetlistServiceImpl<D>
where
    D: Clone,
{
    fn clone(&self) -> Self {
        Self {
            daw: self.daw.clone(),
            setlist: self.setlist.clone(),
            active_song_id: self.active_song_id.clone(),
            cached_indices: self.cached_indices.clone(),
            queued_target: self.queued_target.clone(),
            events_hub: self.events_hub.clone(),
            indices_hub: self.indices_hub.clone(),
            setlist_update_bus: self.setlist_update_bus.clone(),
            setlist_revision: self.setlist_revision.clone(),
            song_cache: self.song_cache.clone(),
            hydration_bus: self.hydration_bus.clone(),
            chart_hydration_bus: self.chart_hydration_bus.clone(),
            chart_cache: self.chart_cache.clone(),
            fingerprint_method_supported: self.fingerprint_method_supported.clone(),
            last_chart_refresh_attempt: self.last_chart_refresh_attempt.clone(),
            build_generation: self.build_generation.clone(),
            position_sync: self.position_sync.clone(),
            reviews: self.reviews.clone(),
            recording_from: self.recording_from.clone(),
        }
    }
}

impl<D> SetlistServiceImpl<D> {
    pub fn with_daw(daw: D) -> Self {
        Self {
            daw,
            setlist: Arc::new(RwLock::new(None)),
            active_song_id: Arc::new(RwLock::new(None)),
            cached_indices: Arc::new(RwLock::new(ActiveIndices::default())),
            queued_target: Arc::new(RwLock::new(None)),
            events_hub: architect::PubSub::sliding(64),
            // Replay the last cursor to every new subscriber: a remote that
            // joins mid-show (the browser session player over the network
            // engine) sees the active song/section immediately instead of
            // waiting for the next change to publish.
            indices_hub: architect::PubSub::sliding(16).with_replay(1),
            setlist_update_bus: Arc::new(WatchBus::new("session.setlist.updates", 0_u64)),
            setlist_revision: Arc::new(AtomicU64::new(0)),
            song_cache: Cache::named("session.setlist.song_cache"),
            hydration_bus: Arc::new(EventBus::new("session.setlist.hydration", 1024)),
            chart_hydration_bus: Arc::new(EventBus::new("session.setlist.chart_hydration", 1024)),
            chart_cache: Cache::named("session.setlist.chart_cache"),
            fingerprint_method_supported: Arc::new(RwLock::new(None)),
            last_chart_refresh_attempt: Cache::named("session.setlist.chart_refresh_attempts"),
            build_generation: Arc::new(AtomicU64::new(0)),
            position_sync: Arc::new(RwLock::new(None)),
            reviews: Arc::new(RwLock::new(HashMap::new())),
            recording_from: Arc::new(RwLock::new(None)),
        }
    }
}

impl SetlistServiceImpl<()> {
    #[must_use]
    pub fn new() -> Self {
        Self::with_daw(())
    }
}

impl Default for SetlistServiceImpl<()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: architect::MaybeSendSync> SetlistServiceImpl<D> {
    /// Get the cached active indices (updated by polling loop, no RPC calls)
    pub(crate) async fn get_cached_indices(&self) -> ActiveIndices {
        self.cached_indices.read().await.clone()
    }

    /// Update cached indices (called by polling loop)
    pub(crate) async fn set_cached_indices(&self, indices: ActiveIndices) {
        *self.cached_indices.write().await = indices;
    }

    /// Set a queued navigation target
    pub(crate) async fn queue_target(&self, target: QueuedTarget) {
        *self.queued_target.write().await = Some(target);
    }

    /// Clear the queued navigation target
    pub(crate) async fn clear_queued_target(&self) {
        *self.queued_target.write().await = None;
    }

    /// Get the current queued target
    pub(crate) async fn get_queued_target(&self) -> Option<QueuedTarget> {
        self.queued_target.read().await.clone()
    }

    /// Get a specific song by index (internal helper)
    pub(crate) async fn get_song_internal(&self, index: usize) -> Option<Song> {
        let setlist = self.setlist.read().await;
        setlist.as_ref()?.songs.get(index).cloned()
    }

    /// Get a specific song by ID (internal helper)
    pub(crate) async fn get_song_by_id(&self, id: &str) -> Option<Song> {
        let setlist = self.setlist.read().await;
        setlist
            .as_ref()?
            .songs
            .iter()
            .find(|song| song.id.as_str() == id)
            .cloned()
    }

    /// Get the active song from cached local state (no RPC calls)
    pub(crate) async fn get_cached_active_song(&self) -> Option<Song> {
        let song_id = self.active_song_id.read().await.clone();
        let song_id = song_id?;
        self.get_song_by_id(&song_id).await
    }

    /// Set the active song by ID (called when navigating)
    pub(crate) async fn set_active_song_id(&self, id: &str) {
        *self.active_song_id.write().await = Some(id.to_string());
    }

    pub(crate) fn notify_setlist_changed(&self) {
        let revision = self
            .setlist_revision
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        self.setlist_update_bus.send(revision);
    }
}

impl<D> architect::HasDispatcher for SetlistServiceImpl<D> {
    type Dispatcher = architect::dispatch::CurrentThreadDispatcher;

    fn dispatcher(&self) -> Self::Dispatcher {
        architect::dispatch::CurrentThreadDispatcher
    }
}

// wasm stub for the native combined-setlist RPP export (see `mod combined`,
// gated native-only). Keeps the `SetlistService::generate_combined_setlist`
// trait method total on wasm; RPP file combining is inherently native.
#[cfg(target_arch = "wasm32")]
impl<D> SetlistServiceImpl<D> {
    pub(crate) async fn generate_combined_setlist_impl(
        &self,
        _gap_measures: u32,
    ) -> Result<String, session_proto::SessionServiceError> {
        Err(session_proto::SessionServiceError::Internal(
            "combined-setlist RPP export is not available in the browser build".to_string(),
        ))
    }
}
