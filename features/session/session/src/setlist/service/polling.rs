//! Active indices polling/calculation, transport state tracking, subscriptions

use super::{
    ACTIVE_HYDRATION_POLL_MAX_MS, ACTIVE_HYDRATION_POLL_MS, ACTIVE_HYDRATION_TICK_MS,
    ACTIVE_INDICES_PROGRESS_EMIT_MS, PROJECT_SWITCH_DEBOUNCE_MS, SetlistServiceImpl,
    TRANSPORT_PROGRESS_EPSILON, TRANSPORT_TIME_EPSILON_SECS,
};
use architect::platform::Instant;
use daw::rpc::Daw;
use daw::service::transport::service::Transport;
use daw::service::{AudioEngine, ProjectContext, Projects};
use rustc_hash::FxHashMap;
use session_proto::{
    ActiveIndices, AdvanceMode, MeasureInfo, Section, SessionServiceError, SetlistEvent,
    SetlistService, Song, SongTransportState,
};
use smallvec::SmallVec;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError as BroadcastRecvError;
use tokio::sync::{broadcast, mpsc, watch};
// `MissedTickBehavior` + `tokio::time::interval` drive the native pump only;
// the wasm pump uses fused `architect::platform::sleep` timers instead.
#[cfg(not(target_arch = "wasm32"))]
use tokio::time::MissedTickBehavior;
use tracing::{debug, info, warn};
use vox::Tx;

/// A timestamp `dur` in the past, wasm-safe. `web_time::Instant` (what
/// `architect::platform::now()` returns) is relative to page load, so it starts
/// near zero and `now() - dur` UNDERFLOW-panics on wasm ("overflow when
/// subtracting duration from instant"). Native `Instant`s are huge and never
/// underflow, so `checked_sub` there is byte-for-byte the old `-`; on wasm we
/// saturate to `now()` (the first interval-gated action just waits one tick).
pub fn instant_ago(dur: Duration) -> Instant {
    architect::platform::now()
        .checked_sub(dur)
        .unwrap_or_else(architect::platform::now)
}

/// Loop-carried mutable state for the setlist subscription pump.
///
/// Every mutable the reactive `select!` branches touch lives here so the native
/// `tokio::select!` loop and the wasm `futures::select!` loop can drive the
/// *same* `on_*` branch handlers (see `run_subscription_stream`). Extracting
/// this made the native loop a pure refactor: the branch bodies moved verbatim,
/// only threading their mutables through `st`.
/// A project's transport state, as needed by
/// [`SetlistServiceImpl::calculate_song_transport`] to derive one song's
/// [`SongTransportState`].
pub struct ProjectTransportSnapshot {
    pub position: daw::service::Position,
    pub is_playing: bool,
    pub is_looping: bool,
    pub loop_region: Option<daw::service::LoopRegion>,
    pub tempo: f64,
    pub time_sig: (u32, u32),
}

struct StreamState {
    guid_to_indices: FxHashMap<String, Vec<usize>>,
    song_id_to_index: FxHashMap<String, usize>,
    last_indices: ActiveIndices,
    last_transport_by_song: FxHashMap<usize, SongTransportState>,
    last_section_by_song: FxHashMap<usize, Option<usize>>,
    last_current_project_guid: Option<String>,
    pending_project_switch: Option<(String, Instant)>,
    auto_advance_triggered_for: Option<usize>,
    chart_probe_inflight: Arc<AtomicBool>,
    chart_probe_tx: mpsc::UnboundedSender<(Duration, bool)>,
    transport_tick_count: u64,
    transport_events_sent: u64,
    transport_song_updates_sent: usize,
    transport_compute_total: Duration,
    chart_check_count: u64,
    chart_update_count: u64,
    chart_compute_total: Duration,
    last_indices_progress_emit: Instant,
    chart_probe_poll_ms: u64,
    chart_probe_last_started: Instant,
}

/// Subscription handles + the initial `StreamState`, produced by the shared
/// setup (`initialize_stream`). Both select loops own the receivers as locals
/// so their per-branch borrows stay disjoint.
struct StreamInit {
    setlist_updates: watch::Receiver<u64>,
    hydration_rx: broadcast::Receiver<(usize, Song)>,
    chart_hydration_rx: broadcast::Receiver<(usize, session_proto::SongChartHydration)>,
    transport_stream: daw::rpc::EventStream<daw::service::transport::TransportStreamEvent>,
    chart_probe_rx: mpsc::UnboundedReceiver<(Duration, bool)>,
    daw: Daw,
    state: StreamState,
}

impl<D> SetlistServiceImpl<D>
where
    D: AudioEngine
        + Clone
        + Projects
        + daw::service::TempoMap
        + Transport
        + architect::MaybeSendSync
        + 'static,
{
    pub(crate) fn active_indices_structural_changed(
        prev: &ActiveIndices,
        next: &ActiveIndices,
    ) -> bool {
        prev.song_index != next.song_index
            || prev.section_index != next.section_index
            || prev.slide_index != next.slide_index
            || prev.is_playing != next.is_playing
            || prev.looping != next.looping
            || prev.loop_selection != next.loop_selection
            || prev.queued_target != next.queued_target
    }

    /// Check if the transport has reached the queued target and clear it if so
    pub(crate) async fn check_and_clear_queue(
        &self,
        song_index: usize,
        section_index: Option<usize>,
        position_seconds: f64,
    ) {
        let queued = self.queued_target.read().await.clone();
        if let Some(target) = queued {
            let reached = match &target {
                session_proto::QueuedTarget::Section {
                    song_index: q_song,
                    section_index: q_section,
                    ..
                } => song_index == *q_song && section_index == Some(*q_section),
                session_proto::QueuedTarget::Time {
                    song_index: q_song,
                    position_seconds: q_pos,
                    ..
                }
                | session_proto::QueuedTarget::Comment {
                    song_index: q_song,
                    position_seconds: q_pos,
                    ..
                } => {
                    // Consider reached if within 0.1 seconds
                    song_index == *q_song && (position_seconds - q_pos).abs() < 0.1
                }
                session_proto::QueuedTarget::Measure {
                    song_index: q_song, ..
                } => {
                    // For measures, we'd need to check musical position - for now just check song
                    song_index == *q_song
                }
            };

            if reached {
                self.clear_queued_target().await;
            }
        }
    }

    /// Calculate transport state for a specific song based on its project's transport
    pub(crate) fn calculate_song_transport(
        song: &Song,
        song_index: usize,
        transport: ProjectTransportSnapshot,
    ) -> SongTransportState {
        let ProjectTransportSnapshot {
            position,
            is_playing,
            is_looping,
            loop_region,
            tempo,
            time_sig,
        } = transport;
        let song_duration = song.duration();
        let song_start = song.start_seconds();

        // Get position in seconds for progress calculations
        let position_seconds = position.time.map_or(0.0, |t| t.as_seconds());

        // Calculate progress within song
        let relative_pos = position_seconds - song_start;
        let progress = if song_duration > 0.0 {
            (relative_pos / song_duration).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Find section at position
        let (section_index, section_progress) = if let Some((sec_idx, section)) =
            song.section_at_position_with_index(position_seconds)
        {
            let sec_duration = section.duration();
            let sec_progress = if sec_duration > 0.0 {
                ((position_seconds - section.start_seconds) / sec_duration).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (Some(sec_idx), Some(sec_progress))
        } else {
            (None, None)
        };

        // Convert loop region from project-absolute to song-relative coordinates
        let song_loop_region = loop_region.and_then(|region| {
            // Only include loop region if it overlaps with the song
            let region_start_relative = region.start_seconds - song_start;
            let region_end_relative = region.end_seconds - song_start;

            // Check if loop region is within song bounds (with some tolerance)
            if region_end_relative > 0.0 && region_start_relative < song_duration {
                Some(daw::service::LoopRegion::new(
                    region_start_relative.max(0.0),
                    region_end_relative.min(song_duration),
                ))
            } else {
                None
            }
        });

        // Each song carries its own authored tempo / time signature (from its
        // chart). The shared-timeline standalone project reports one global
        // tempo (the first song's), so the project transport tempo is wrong for
        // every later song. Prefer the song's own value; fall back to the
        // project transport only when the song didn't author one. This is
        // model-agnostic: once each song is its own DAW project, `song.tempo`
        // still matches that project's tempo.
        let bpm = song.tempo.unwrap_or(tempo);
        let (time_sig_num, time_sig_denom) = song
            .time_signature
            .as_ref()
            .map_or(time_sig, |ts| (ts.numerator(), ts.denominator()));

        SongTransportState {
            song_index,
            position,
            progress,
            section_index,
            section_progress,
            is_playing,
            is_looping,
            loop_region: song_loop_region,
            bpm,
            time_sig_num,
            time_sig_denom,
        }
    }

    fn f64_changed(prev: f64, next: f64, epsilon: f64) -> bool {
        (prev - next).abs() > epsilon
    }

    fn option_f64_changed(prev: Option<f64>, next: Option<f64>, epsilon: f64) -> bool {
        match (prev, next) {
            (Some(a), Some(b)) => Self::f64_changed(a, b, epsilon),
            (None, None) => false,
            _ => true,
        }
    }

    fn loop_region_changed(
        prev: Option<&daw::service::LoopRegion>,
        next: Option<&daw::service::LoopRegion>,
    ) -> bool {
        match (prev, next) {
            (Some(a), Some(b)) => {
                Self::f64_changed(
                    a.start_seconds,
                    b.start_seconds,
                    TRANSPORT_TIME_EPSILON_SECS,
                ) || Self::f64_changed(a.end_seconds, b.end_seconds, TRANSPORT_TIME_EPSILON_SECS)
            }
            (None, None) => false,
            _ => true,
        }
    }

    pub(crate) fn transport_changed(prev: &SongTransportState, next: &SongTransportState) -> bool {
        if prev.song_index != next.song_index
            || prev.section_index != next.section_index
            || prev.is_playing != next.is_playing
            || prev.is_looping != next.is_looping
            || prev.time_sig_num != next.time_sig_num
            || prev.time_sig_denom != next.time_sig_denom
        {
            return true;
        }

        if Self::f64_changed(prev.bpm, next.bpm, 0.001)
            || Self::f64_changed(prev.progress, next.progress, TRANSPORT_PROGRESS_EPSILON)
            || Self::option_f64_changed(
                prev.section_progress,
                next.section_progress,
                TRANSPORT_PROGRESS_EPSILON,
            )
            || Self::loop_region_changed(prev.loop_region.as_ref(), next.loop_region.as_ref())
        {
            return true;
        }

        let prev_time = prev.position.time.map(|t| t.as_seconds());
        let next_time = next.position.time.map(|t| t.as_seconds());
        if Self::option_f64_changed(prev_time, next_time, TRANSPORT_TIME_EPSILON_SECS) {
            return true;
        }

        prev.position.musical != next.position.musical || prev.position.midi != next.position.midi
    }

    /// Get transport state for ALL songs by querying each project
    ///
    /// Uses native `get_state()` so in-process polling doesn't marshal through RPC.
    pub(crate) async fn get_all_song_transports(&self) -> Vec<SongTransportState> {
        let songs = {
            let setlist = self.setlist.read().await;
            setlist
                .as_ref()
                .map(|sl| sl.songs.clone())
                .unwrap_or_default()
        };
        if songs.is_empty() {
            return Vec::new();
        }

        // `Transport::get_state` on `daw_reaper::Reaper` hits REAPER's
        // main-thread-only FFI (EnumProjects to resolve each song's
        // project, current position, tempo/time-sig). Called directly from
        // this async fn it panics on a tokio worker
        // ("enum_projects must only be called from main thread"); bounce
        // the whole per-song batch through `main_thread::query` at once —
        // same pattern `calculate_active_indices`/
        // `build_from_open_projects_impl` already use.
        let daw = self.daw.clone();
        let guids: Vec<String> = songs.iter().map(|s| s.project_guid.clone()).collect();
        let states = daw_proto::main_thread::query(move || {
            guids
                .into_iter()
                .map(|guid| daw.get_state(ProjectContext::Project(guid)))
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        if states.len() != songs.len() {
            warn!("get_all_song_transports: main thread unavailable, skipping this poll");
            return Vec::new();
        }

        let mut transports = Vec::with_capacity(songs.len());
        for (song_index, (song, state)) in songs.into_iter().zip(states).enumerate() {
            let is_playing = state.play_state == daw::service::PlayState::Playing
                || state.play_state == daw::service::PlayState::Recording;
            let position = if is_playing {
                state.playhead_position.clone()
            } else {
                state.edit_position.clone()
            };
            let song_transport = Self::calculate_song_transport(
                &song,
                song_index,
                ProjectTransportSnapshot {
                    position,
                    is_playing,
                    is_looping: state.looping,
                    loop_region: state.loop_region.clone(),
                    tempo: state.tempo.bpm(),
                    time_sig: (
                        state.time_signature.numerator(),
                        state.time_signature.denominator(),
                    ),
                },
            );
            transports.push(song_transport);
        }
        for (index, transport) in transports.iter_mut().enumerate() {
            transport.song_index = index;
        }

        transports
    }

    /// Query the current transport for every song and publish a
    /// `TransportUpdate` on the events hub.
    ///
    /// The reactive polling loop only emits transport on playhead *position
    /// ticks*, which fire while playing. A seek while stopped moves the edit
    /// cursor without a tick, so the UI badges (playhead time, musical
    /// position, BPM) would stay frozen at the previous song. Call this after
    /// any stopped seek so the badges reflect the new position immediately.
    pub(crate) async fn publish_transport_snapshot(&self) {
        let transports = self.get_all_song_transports().await;
        if !transports.is_empty() {
            self.events_hub
                .publish(SetlistEvent::TransportUpdate(transports));
        }
    }

    /// Helper to calculate active indices for a given position
    fn calculate_indices_for_position(
        setlist: &session_proto::Setlist,
        current_project_guid: &str,
        position: f64,
        is_playing: bool,
        looping: bool,
    ) -> ActiveIndices {
        // Two setlist shapes share this code path: a single REAPER project
        // with every song laid out sequentially on one ruler (non-overlapping
        // `start_seconds..end_seconds` ranges, project_guid identical across
        // all songs — position alone disambiguates), and one-project-per-song
        // (`stamp_song_native`/`fixture_songs`, the standalone/web demo
        // backend) where EVERY song's local timeline starts at 0.0, so their
        // ranges all overlap at the origin. Matching by position alone in the
        // second shape always resolved to whichever song came first in the
        // list, no matter which project was actually selected — clicking any
        // other song's row seeked its project correctly (see
        // `seek_to_song_impl`) but the active indices snapped straight back
        // to song 0. Restrict the position match to the CURRENT project
        // first; only widen to every song (the single-project shape, where
        // this is a no-op — every song already shares that guid) if nothing
        // in the current project matches.
        let in_current_project = |song: &&Song| song.project_guid == current_project_guid;
        let song_data =
            setlist
                .songs
                .iter()
                .enumerate()
                .filter(|(_, song)| in_current_project(&song))
                .find(|(_, song)| position >= song.start_seconds && position < song.end_seconds)
                .or_else(|| {
                    setlist.songs.iter().enumerate().find(|(_, song)| {
                        position >= song.start_seconds && position < song.end_seconds
                    })
                })
                .or_else(|| {
                    // Fallback: nearest song, preferring one in the same project.
                    fn nearest<'a>(
                        songs: impl Iterator<Item = (usize, &'a Song)>,
                        position: f64,
                    ) -> Option<(usize, &'a Song)> {
                        songs.min_by(|(_, a), (_, b)| {
                            let dist_a = (position - a.start_seconds)
                                .abs()
                                .min((position - a.end_seconds).abs());
                            let dist_b = (position - b.start_seconds)
                                .abs()
                                .min((position - b.end_seconds).abs());
                            dist_a
                                .partial_cmp(&dist_b)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                    }
                    nearest(
                        setlist
                            .songs
                            .iter()
                            .enumerate()
                            .filter(|(_, song)| in_current_project(&song)),
                        position,
                    )
                    .or_else(|| nearest(setlist.songs.iter().enumerate(), position))
                });

        if let Some((song_index, song)) = song_data {
            // Calculate song progress
            let song_duration = song.duration();
            let song_relative_pos = position - song.start_seconds();
            let song_progress = if song_duration > 0.0 {
                Some((song_relative_pos / song_duration).clamp(0.0, 1.0))
            } else {
                None
            };

            // Find section at current position
            if let Some((section_index, section)) = song.section_at_position_with_index(position) {
                // Calculate section progress
                let section_duration = section.duration();
                let section_relative_pos = position - section.start_seconds;
                let section_progress = if section_duration > 0.0 {
                    Some((section_relative_pos / section_duration).clamp(0.0, 1.0))
                } else {
                    None
                };

                ActiveIndices {
                    song_index: Some(song_index),
                    section_index: Some(section_index),
                    slide_index: None,
                    song_progress,
                    section_progress,
                    is_playing,
                    looping,
                    loop_selection: None,
                    queued_target: None,
                }
            } else {
                // In song but not in a specific section
                ActiveIndices {
                    song_index: Some(song_index),
                    section_index: None,
                    slide_index: None,
                    song_progress,
                    section_progress: None,
                    is_playing,
                    looping,
                    loop_selection: None,
                    queued_target: None,
                }
            }
        } else {
            // No song found at position
            ActiveIndices {
                song_index: None,
                section_index: None,
                slide_index: None,
                song_progress: None,
                section_progress: None,
                is_playing,
                looping,
                loop_selection: None,
                queued_target: None,
            }
        }
    }

    /// Find the active song and section based on current DAW project
    pub(crate) async fn calculate_active_indices(&self) -> ActiveIndices {
        // `Projects::current` + `Transport::{get_position, is_playing, is_looping}`
        // on `daw_reaper::Reaper` hit REAPER's main-thread-only FFI. Called
        // from this async task on a tokio worker they hang on REAPER's
        // internal lock. Bounce once via `main_thread::query` for the whole
        // batch (same pattern as `build_from_open_projects_impl`).
        let daw = self.daw.clone();
        let Some(snapshot) = daw_proto::main_thread::query(move || {
            let current = daw.current()?;
            let ctx = ProjectContext::Project(current.guid.clone());
            let position = daw.get_position(ctx.clone());
            let is_playing = daw.is_playing(ctx.clone());
            let looping = daw.is_looping(ctx);
            Some((current.guid, position, is_playing, looping))
        })
        .await
        .flatten() else {
            warn!("Failed to get current project");
            return ActiveIndices::default();
        };

        let (current_project_guid, position, is_playing, looping) =
            (snapshot.0, snapshot.1, snapshot.2, snapshot.3);

        // Find which song corresponds to the current project
        let setlist_clone = self.setlist.read().await.clone();
        let Some(setlist) = setlist_clone else {
            return ActiveIndices {
                is_playing,
                looping,
                ..Default::default()
            };
        };

        Self::calculate_indices_for_position(
            &setlist,
            &current_project_guid,
            position,
            is_playing,
            looping,
        )
    }

    // =========================================================================
    // SetlistService trait: Subscriptions
    // =========================================================================

    /// Spawn the `#[subscribe]` stream pumps (setlist events + active
    /// indices) — call once after construction; every stream subscriber
    /// shares the hubs.
    pub fn start_stream_pumps(&self)
    where
        // `MaybeSendSync` = `Send + Sync` natively (what `platform::spawn`'s
        // tokio backend needs), empty on wasm (`spawn_local` needs neither).
        Self: Clone + architect::MaybeSendSync + 'static,
    {
        let pump = self.clone();
        architect::platform::spawn(async move {
            let _ = pump.subscribe_impl().await;
        });
        let pump = self.clone();
        architect::platform::spawn(async move {
            let _ = pump.subscribe_active_impl().await;
        });
    }

    /// One process-wide event pump: publishes every setlist change into
    /// the `#[subscribe]` hub (all stream subscribers share it). Spawn once.
    pub(crate) async fn subscribe_impl(&self) -> Result<(), SessionServiceError> {
        // Run the streaming loop INLINE so the subscribe operation stays
        // in-flight for the lifetime of the subscription — that keeps the Tx
        // channel bound. Returning early (spawn + Ok) completes the operation,
        // and the runtime's operation store then tears the channel down, so the
        // client receives one event and then EOF (the resubscribe spin).
        //
        // Because this never returns until the stream ends, the CLIENT must NOT
        // `await` subscribe() before reading the receiver — it must poll the
        // receiver concurrently with this call (see the desktop / web-server /
        // harness clients).
        self.run_subscription_stream().await
    }

    // ── Shared setup helpers ────────────────────────────────────────────────
    fn build_guid_maps(
        songs: &[Song],
    ) -> (FxHashMap<String, Vec<usize>>, FxHashMap<String, usize>) {
        let mut guid_to_indices: FxHashMap<String, Vec<usize>> = FxHashMap::default();
        for (idx, song) in songs.iter().enumerate() {
            guid_to_indices
                .entry(song.project_guid.clone())
                .or_default()
                .push(idx);
        }
        let song_id_to_index: FxHashMap<String, usize> = songs
            .iter()
            .enumerate()
            .map(|(idx, song)| (song.id.to_string(), idx))
            .collect();
        (guid_to_indices, song_id_to_index)
    }

    fn get_daw_handle() -> Daw {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Daw::get().clone()
        }
        #[cfg(target_arch = "wasm32")]
        {
            daw::get()
                .expect(
                    "wasm Daw not initialized; call daw::init_from_parts (build_in_process_daw) \
                     before starting the setlist pump",
                )
                .clone()
        }
    }

    // ── Shared setup ────────────────────────────────────────────────────────
    // The subscription setup (initial setlist/chart publish, guid maps,
    // transport snapshot, initial indices, chart-probe channel) is identical on
    // native and wasm; only the reactive loop differs (tokio vs futures
    // select). This produces the subscription handles + the initial
    // `StreamState` both loops drive.
    async fn initialize_stream(&self) -> StreamInit {
        let this = self;
        let setlist_updates = this.setlist_update_bus.subscribe();
        let hydration_rx = this.hydration_bus.subscribe();
        let chart_hydration_rx = this.chart_hydration_bus.subscribe();

        // Get songs for GUID -> index mapping
        // NOTE: subscribe does NOT build inline. Doing the full build inside
        // the streaming handler (before the select loop) wedges the
        // subscription response. The desktop calls build_from_open_projects
        // separately; the key fix is below — when there's no setlist yet we
        // stay subscribed (rather than bailing) so the revision loop
        // delivers SetlistChanged once a build populates it.
        let songs: Vec<Song> = {
            let setlist = this.setlist.read().await;
            setlist.as_ref().map_or_else(
                || {
                    // Build produced nothing (e.g. no open projects). Stay
                    // subscribed so a later build still delivers SetlistChanged.
                    info!("SetlistService::subscribe() - no setlist after build; awaiting updates");
                    Vec::new()
                },
                |sl| {
                    // Send initial setlist state.
                    self.events_hub
                        .publish(SetlistEvent::SetlistChanged(sl.clone()));
                    sl.songs.clone()
                },
            )
        };

        // Send cached chart payloads for current songs so subscribers can
        // render charts without bloating SetlistChanged payloads.
        {
            let chart_snapshot = this.chart_cache.snapshot().await;
            for (index, song) in songs.iter().enumerate() {
                if let Some(chart) = chart_snapshot.get(&song.project_guid).cloned() {
                    self.events_hub.publish(SetlistEvent::SongChartHydrated {
                        song_id: song.id.clone(),
                        index,
                        chart,
                    });
                }
            }
        }

        // Build project GUID -> song indices mapping (multiple songs may share a GUID).
        let (guid_to_indices, song_id_to_index) = Self::build_guid_maps(&songs);

        let daw = Self::get_daw_handle();
        let transport_stream = daw.transport_events();

        // Get initial active indices from REAPER's current project (makes RPC calls)
        // This ensures the UI shows the correct song/section on startup
        let last_indices = this.calculate_active_indices().await;

        // Update cached indices so navigation works correctly
        this.set_cached_indices(last_indices.clone()).await;

        // Also update active_song_id if we found a song
        if let Some(song_idx) = last_indices.song_index
            && let Some(song) = songs.get(song_idx)
        {
            *this.active_song_id.write().await = Some(song.id.to_string());
        }
        // Cursor delivery is owned by the active-indices pump
        // (`subscribe_active_impl` → `indices_hub`); this events pump only
        // carries setlist structure + transport. It still tracks
        // `last_indices` below for auto-advance / chart-probe bookkeeping.

        // Send initial transport state for all songs (one-time poll)
        let initial_transports = this.get_all_song_transports().await;
        let last_transport_by_song: FxHashMap<usize, SongTransportState> = initial_transports
            .iter()
            .cloned()
            .map(|transport| (transport.song_index, transport))
            .collect();
        self.events_hub
            .publish(SetlistEvent::TransportUpdate(initial_transports));

        // Track current song/section for enter/exit events
        // Keyed by song_index to track section per song
        let last_section_by_song: FxHashMap<usize, Option<usize>> = FxHashMap::default();

        // Track last known current project GUID for detecting tab switches.
        // `current()` hits REAPER main-thread FFI — bounce via main_thread::query
        // so the subscribe future doesn't hang on the tokio worker.
        let last_current_project_guid: Option<String> = {
            let daw = self.daw.clone();
            daw_proto::main_thread::query(move || daw.current().map(|p| p.guid))
                .await
                .flatten()
        };

        let (chart_probe_tx, chart_probe_rx) = mpsc::unbounded_channel::<(Duration, bool)>();
        let chart_probe_inflight = Arc::new(AtomicBool::new(false));

        let state = StreamState {
            guid_to_indices,
            song_id_to_index,
            last_indices,
            last_transport_by_song,
            last_section_by_song,
            last_current_project_guid,
            pending_project_switch: None,
            auto_advance_triggered_for: None,
            chart_probe_inflight,
            chart_probe_tx,
            transport_tick_count: 0,
            transport_events_sent: 0,
            transport_song_updates_sent: 0,
            transport_compute_total: Duration::ZERO,
            chart_check_count: 0,
            chart_update_count: 0,
            chart_compute_total: Duration::ZERO,
            last_indices_progress_emit: architect::platform::now(),
            chart_probe_poll_ms: ACTIVE_HYDRATION_POLL_MS,
            chart_probe_last_started: instant_ago(Duration::from_millis(ACTIVE_HYDRATION_POLL_MS)),
        };

        StreamInit {
            setlist_updates,
            hydration_rx,
            chart_hydration_rx,
            transport_stream,
            chart_probe_rx,
            daw,
            state,
        }
    }

    // ── Branch handlers ──────────────────────────────────────────────────────
    // One `async fn` per select branch, shared by both loops. Each returns
    // `ControlFlow::Break(())` where the original branch did `break` (ends the
    // pump) and `ControlFlow::Continue(())` where it did `continue` / fell
    // through (re-enter the loop). Bodies are the original branch bodies moved
    // verbatim, threading their mutables through `st`.

    async fn on_setlist_update(
        &self,
        st: &mut StreamState,
        changed: Result<(), watch::error::RecvError>,
    ) -> ControlFlow<()> {
        if changed.is_err() {
            return ControlFlow::Break(());
        }
        let setlist_clone = self.setlist.read().await.clone();
        if let Some(setlist) = setlist_clone {
            st.guid_to_indices = FxHashMap::default();
            for (idx, song) in setlist.songs.iter().enumerate() {
                st.guid_to_indices
                    .entry(song.project_guid.clone())
                    .or_default()
                    .push(idx);
            }
            st.song_id_to_index = setlist
                .songs
                .iter()
                .enumerate()
                .map(|(idx, song)| (song.id.to_string(), idx))
                .collect();
            self.events_hub
                .publish(SetlistEvent::SetlistChanged(setlist));
        }
        ControlFlow::Continue(())
    }

    fn on_hydration(&self, hydrated: Result<(usize, Song), BroadcastRecvError>) -> ControlFlow<()> {
        match hydrated {
            Ok((index, song)) => {
                self.events_hub.publish(SetlistEvent::SongHydrated {
                    song_id: song.id.clone(),
                    index,
                    song,
                });
            }
            Err(BroadcastRecvError::Lagged(skipped)) => {
                debug!("Hydration update lagged by {} messages", skipped);
            }
            Err(BroadcastRecvError::Closed) => return ControlFlow::Break(()),
        }
        ControlFlow::Continue(())
    }

    async fn on_chart_hydration(
        &self,
        chart_hydrated: Result<(usize, session_proto::SongChartHydration), BroadcastRecvError>,
    ) -> ControlFlow<()> {
        match chart_hydrated {
            Ok((index, chart)) => {
                let song_id = {
                    let setlist = self.setlist.read().await;
                    setlist
                        .as_ref()
                        .and_then(|sl| sl.songs.get(index))
                        .map_or_else(session_proto::SongId::new, |s| s.id.clone())
                };
                self.events_hub.publish(SetlistEvent::SongChartHydrated {
                    song_id,
                    index,
                    chart,
                });
            }
            Err(BroadcastRecvError::Lagged(skipped)) => {
                debug!("Chart hydration update lagged by {} messages", skipped);
            }
            Err(BroadcastRecvError::Closed) => return ControlFlow::Break(()),
        }
        ControlFlow::Continue(())
    }

    // Helper to resolve which song index based on position in seconds
    fn resolve_song_index_from_position(
        song_indices: &[usize],
        setlist: &session_proto::Setlist,
        pos_secs: f64,
    ) -> Option<usize> {
        song_indices
            .iter()
            .copied()
            .find(|&idx| {
                setlist
                    .songs
                    .get(idx)
                    .is_some_and(|s| pos_secs >= s.start_seconds && pos_secs < s.end_seconds)
            })
            // Fallback: nearest song in this project
            .or_else(|| {
                song_indices.iter().copied().min_by(|&a, &b| {
                    let dist_a = setlist.songs.get(a).map_or(f64::MAX, |s| {
                        (pos_secs - s.start_seconds)
                            .abs()
                            .min((pos_secs - s.end_seconds).abs())
                    });
                    let dist_b = setlist.songs.get(b).map_or(f64::MAX, |s| {
                        (pos_secs - s.start_seconds)
                            .abs()
                            .min((pos_secs - s.end_seconds).abs())
                    });
                    dist_a
                        .partial_cmp(&dist_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            })
    }

    // Helper to track section changes and emit events
    fn process_section_changes(
        st: &mut StreamState,
        song: &Song,
        song_index: usize,
        song_transport: &SongTransportState,
        pending_section_events: &mut SmallVec<[SetlistEvent; 8]>,
    ) {
        let current_section = song_transport.section_index;
        let last_section = st.last_section_by_song.get(&song_index).copied().flatten();

        if current_section != last_section {
            if let Some(sec_idx) = last_section {
                let section_id = song
                    .sections
                    .get(sec_idx)
                    .map_or_else(session_proto::SectionId::new, |s| s.section_id.clone());
                pending_section_events.push(SetlistEvent::SectionExited {
                    song_id: song.id.clone(),
                    song_index,
                    section_id,
                    section_index: sec_idx,
                });
            }

            if let Some(sec_idx) = current_section
                && let Some(section) = song.sections.get(sec_idx)
            {
                pending_section_events.push(SetlistEvent::SectionEntered {
                    song_id: song.id.clone(),
                    song_index,
                    section_id: section.section_id.clone(),
                    section_index: sec_idx,
                    section: section.clone(),
                });
            }

            st.last_section_by_song.insert(song_index, current_section);
        }
    }

    // Extract song transports from a transport event for a specific project.
    async fn process_transport_for_project(
        &self,
        st: &mut StreamState,
        daw: &Daw,
        project_guid: String,
        transport_state: daw::rpc::TransportState,
    ) -> (
        SmallVec<[SongTransportState; 16]>,
        SmallVec<[SetlistEvent; 8]>,
    ) {
        let mut song_transports: SmallVec<[SongTransportState; 16]> = SmallVec::new();
        let mut pending_section_events: SmallVec<[SetlistEvent; 8]> = SmallVec::new();

        let setlist_clone = self.setlist.read().await.clone();
        if let Some(setlist_ref) = setlist_clone {
            if let Some(song_indices_ref) = st.guid_to_indices.get(&project_guid) {
                let song_indices: SmallVec<[usize; 16]> =
                    song_indices_ref.iter().copied().collect();
                let is_playing = transport_state.play_state == daw::service::PlayState::Playing
                    || transport_state.play_state == daw::service::PlayState::Recording;

                let position = if is_playing {
                    transport_state.playhead_position.clone()
                } else {
                    transport_state.edit_position.clone()
                };

                let pos_secs = position
                    .time
                    .as_ref()
                    .map_or(0.0, daw_proto::PositionInSeconds::as_seconds);

                let resolved_index =
                    Self::resolve_song_index_from_position(&song_indices, &setlist_ref, pos_secs);

                for &song_index in &song_indices {
                    if let Some(song) = setlist_ref.songs.get(song_index) {
                        let song_transport = Self::calculate_song_transport(
                            song,
                            song_index,
                            ProjectTransportSnapshot {
                                position: position.clone(),
                                is_playing,
                                is_looping: transport_state.looping,
                                loop_region: transport_state.loop_region.clone(),
                                tempo: transport_state.tempo.bpm(),
                                time_sig: (
                                    transport_state.time_signature.numerator(),
                                    transport_state.time_signature.denominator(),
                                ),
                            },
                        );

                        if Some(song_index) == resolved_index {
                            Self::process_section_changes(
                                st,
                                song,
                                song_index,
                                &song_transport,
                                &mut pending_section_events,
                            );
                        }
                        song_transports.push(song_transport);
                    }
                }
            }
        }

        (song_transports, pending_section_events)
    }

    // Handle publishing updated transports and checking for auto-advance.
    async fn handle_transport_updates(
        &self,
        st: &mut StreamState,
        song_transports: SmallVec<[SongTransportState; 16]>,
        pending_section_events: SmallVec<[SetlistEvent; 8]>,
        active_song_index: Option<usize>,
    ) {
        for section_event in pending_section_events {
            self.events_hub.publish(section_event);
        }

        let active_transport = active_song_index
            .and_then(|idx| song_transports.iter().find(|t| t.song_index == idx))
            .or_else(|| song_transports.first());

        if let Some(transport) = active_transport {
            self.handle_active_transport_indices(st, transport).await;
        }

        let mut changed_transports: SmallVec<[SongTransportState; 16]> =
            SmallVec::with_capacity(song_transports.len());
        for transport in song_transports {
            let changed = st
                .last_transport_by_song
                .get(&transport.song_index)
                .is_none_or(|prev| Self::transport_changed(prev, &transport));
            if changed {
                st.last_transport_by_song
                    .insert(transport.song_index, transport.clone());
                changed_transports.push(transport);
            }
        }

        let changed_count = changed_transports.len();
        if changed_count > 0 {
            let changed_transports = changed_transports.into_vec();
            self.events_hub
                .publish(SetlistEvent::TransportUpdate(changed_transports));
            st.transport_events_sent = st.transport_events_sent.saturating_add(1);
            st.transport_song_updates_sent =
                st.transport_song_updates_sent.saturating_add(changed_count);
        }
    }

    /// The active-song-cursor half of [`Self::handle_transport_updates`]:
    /// derives [`ActiveIndices`] from the active song's transport, caches
    /// them, checks the queued-navigation target, runs auto-advance, and
    /// emits an `ActiveIndicesChanged` event when something structural (or
    /// throttled progress) changed.
    async fn handle_active_transport_indices(
        &self,
        st: &mut StreamState,
        transport: &SongTransportState,
    ) {
        let position_seconds = transport.position.time.map_or(0.0, |t| t.as_seconds());

        self.check_and_clear_queue(
            transport.song_index,
            transport.section_index,
            position_seconds,
        )
        .await;

        let queued_target = self.get_queued_target().await;

        let current_indices = ActiveIndices {
            song_index: Some(transport.song_index),
            section_index: transport.section_index,
            slide_index: None,
            song_progress: Some(transport.progress),
            section_progress: transport.section_progress,
            is_playing: transport.is_playing,
            looping: transport.is_looping,
            loop_selection: None,
            queued_target,
        };

        self.set_cached_indices(current_indices.clone()).await;

        let structural_change =
            Self::active_indices_structural_changed(&st.last_indices, &current_indices);
        let progress_change = current_indices.song_progress != st.last_indices.song_progress
            || current_indices.section_progress != st.last_indices.section_progress;
        let can_emit_progress = st.last_indices_progress_emit.elapsed()
            >= Duration::from_millis(ACTIVE_INDICES_PROGRESS_EMIT_MS);
        let song_changed = current_indices.song_index != st.last_indices.song_index;

        if let Some(song_idx) = current_indices.song_index {
            let near_end = current_indices.song_progress.is_some_and(|p| p >= 0.999);
            let stopped = !current_indices.is_playing;

            if song_changed {
                st.auto_advance_triggered_for = None;
            }

            if near_end && stopped && st.auto_advance_triggered_for != Some(song_idx) {
                let should_advance = {
                    let setlist_clone = self.setlist.read().await.clone();
                    setlist_clone.as_ref().and_then(|sl| {
                        let song = sl.songs.get(song_idx)?;
                        let mode = song.effective_advance_mode(sl.advance_mode);
                        if mode == AdvanceMode::AutoPlay {
                            let next_idx = song_idx.checked_add(1)?;
                            (next_idx < sl.songs.len()).then_some(next_idx)
                        } else {
                            None
                        }
                    })
                };

                if let Some(next_idx) = should_advance {
                    st.auto_advance_triggered_for = Some(song_idx);
                    info!(
                        "Auto-advance: song {} ended, advancing to song {}",
                        song_idx, next_idx
                    );
                    self.seek_to_song_internal(next_idx).await;
                }
            }
        }

        if structural_change || (progress_change && can_emit_progress) {
            if progress_change {
                st.last_indices_progress_emit = architect::platform::now();
            }
            st.last_indices = current_indices;
            if song_changed {
                st.chart_probe_poll_ms = ACTIVE_HYDRATION_POLL_MS;
                st.chart_probe_last_started =
                    instant_ago(Duration::from_millis(ACTIVE_HYDRATION_POLL_MS));
            }
        }
    }

    // Handle transport updates from the architect transport stream.
    async fn on_transport(
        &self,
        st: &mut StreamState,
        daw: &Daw,
        ev: Result<
            Option<vox::SelfRef<daw::service::transport::TransportStreamEvent>>,
            vox::RxError,
        >,
    ) -> ControlFlow<()> {
        // Only position ticks drive per-song updates; state-only
        // events (play/stop/tempo) are reflected by the fresh
        // snapshot query on the next tick. `None`/error ends the
        // stream — exit the pump.
        let project_guid = match ev {
            Ok(Some(ev_ref)) => match ev_ref.get() {
                daw::service::transport::TransportStreamEvent::Position(tick) => {
                    tick.project_guid.clone()
                }
                daw::service::transport::TransportStreamEvent::State(_) => {
                    return ControlFlow::Continue(());
                }
            },
            _ => return ControlFlow::Break(()),
        };
        // Full transport snapshot for this project (tempo / time
        // signature / loop / play-state / positions) — the same
        // struct the old SyncEngine delivered.
        let transport_state = match daw.project(project_guid.clone()).await {
            Ok(p) => match p.transport().get_state().await {
                Ok(s) => s,
                Err(_) => return ControlFlow::Continue(()),
            },
            Err(_) => return ControlFlow::Continue(()),
        };
        let transport_started = architect::platform::now();
        let active_song_id = self.active_song_id.read().await.clone();
        let active_song_index = active_song_id
            .as_ref()
            .and_then(|id| st.song_id_to_index.get(id).copied());

        let (song_transports, pending_section_events) = self
            .process_transport_for_project(st, daw, project_guid, transport_state)
            .await;

        if !song_transports.is_empty() {
            self.handle_transport_updates(
                st,
                song_transports,
                pending_section_events,
                active_song_index,
            )
            .await;
        }

        st.transport_tick_count = st.transport_tick_count.saturating_add(1);
        st.transport_compute_total = st
            .transport_compute_total
            .saturating_add(transport_started.elapsed());
        ControlFlow::Continue(())
    }

    // Periodically check if the current REAPER project tab has changed
    async fn on_project_check(&self, st: &mut StreamState, daw: &Daw) -> ControlFlow<()> {
        let this = self;
        // Avoid tab-switch churn while transport is active.
        if this.get_cached_indices().await.is_playing {
            st.pending_project_switch = None;
            return ControlFlow::Continue(());
        }

        let current_project_guid = {
            let daw = self.daw.clone();
            daw_proto::main_thread::query(move || daw.current().map(|p| p.guid))
                .await
                .flatten()
        };
        if let Some(current_guid) = current_project_guid {
            // No change.
            if st.last_current_project_guid.as_ref() == Some(&current_guid) {
                st.pending_project_switch = None;
                return ControlFlow::Continue(());
            }

            // Debounce tab switches so transient backend jitter doesn't thrash active state.
            let switch_stable = match st.pending_project_switch.as_ref() {
                Some((pending_guid, started)) if pending_guid == &current_guid => {
                    started.elapsed() >= Duration::from_millis(PROJECT_SWITCH_DEBOUNCE_MS)
                }
                _ => false,
            };
            if !switch_stable {
                st.pending_project_switch = Some((current_guid, architect::platform::now()));
                return ControlFlow::Continue(());
            }

            let (confirmed_guid, _) = st
                .pending_project_switch
                .take()
                .unwrap_or_else(|| (current_guid, architect::platform::now()));
            debug!(
                "Project tab changed from {:?} to {}",
                st.last_current_project_guid, confirmed_guid
            );

            // Check if previously active project was paused - if so, stop it
            if let Some(prev_guid) = &st.last_current_project_guid
                && let Ok(prev_project) = daw.project(prev_guid).await
            {
                let transport = prev_project.transport();
                if let Ok(state) = transport.get_play_state().await
                    && state == daw::service::PlayState::Paused
                {
                    // Stop the paused project
                    debug!("Stopping paused project {}", prev_guid);
                    let _ = transport.stop().await;
                }
            }

            // Find the song that matches the new current project
            if st.guid_to_indices.contains_key(&confirmed_guid) {
                // Let calculate_active_indices resolve the correct
                // song by position (handles multi-song projects).
                let new_indices = this.calculate_active_indices().await;
                this.set_cached_indices(new_indices.clone()).await;

                // Update active song ID from resolved index
                if let Some(song_idx) = new_indices.song_index {
                    let song_id_str = {
                        let setlist_guard = this.setlist.read().await;
                        setlist_guard
                            .as_ref()
                            .and_then(|sl| sl.songs.get(song_idx))
                            .map(|song| song.id.to_string())
                    };
                    if let Some(song_id_str) = song_id_str {
                        *this.active_song_id.write().await = Some(song_id_str);
                    }
                }

                if new_indices != st.last_indices {
                    // Cursor published by the active-indices pump;
                    // keep the chart-probe scheduling bookkeeping.
                    if new_indices.song_index != st.last_indices.song_index {
                        st.chart_probe_poll_ms = ACTIVE_HYDRATION_POLL_MS;
                        st.chart_probe_last_started =
                            instant_ago(Duration::from_millis(ACTIVE_HYDRATION_POLL_MS));
                    }
                    st.last_indices = new_indices;
                }
            }

            st.last_current_project_guid = Some(confirmed_guid);
        }
        ControlFlow::Continue(())
    }

    async fn on_active_hydration(&self, st: &mut StreamState) -> ControlFlow<()> {
        let this = self;
        if this.get_cached_indices().await.is_playing {
            return ControlFlow::Continue(());
        }
        if st.chart_probe_last_started.elapsed() < Duration::from_millis(st.chart_probe_poll_ms) {
            return ControlFlow::Continue(());
        }
        // Keep chart probing off the hot transport/select loop.
        // If a prior probe is still running, skip this tick.
        if !st.chart_probe_inflight.swap(true, Ordering::SeqCst) {
            st.chart_probe_last_started = architect::platform::now();
            let this_clone = this.clone();
            let chart_probe_tx_clone = st.chart_probe_tx.clone();
            let inflight = st.chart_probe_inflight.clone();
            let probe = async move {
                let chart_started = architect::platform::now();
                let updated = this_clone.refresh_active_song_chart_if_changed().await;
                let _ = chart_probe_tx_clone.send((chart_started.elapsed(), updated));
                inflight.store(false, Ordering::SeqCst);
            };
            // Native uses `tokio::spawn` (byte-for-byte with the original loop);
            // wasm has no tokio runtime, so drive the probe via the portable
            // `spawn_local`-backed `architect::platform::spawn`.
            #[cfg(not(target_arch = "wasm32"))]
            tokio::spawn(probe);
            #[cfg(target_arch = "wasm32")]
            architect::platform::spawn(probe);
        }
        ControlFlow::Continue(())
    }

    fn on_chart_probe(
        st: &mut StreamState,
        chart_probe: Option<(Duration, bool)>,
    ) -> ControlFlow<()> {
        if let Some((elapsed, updated)) = chart_probe {
            st.chart_check_count = st.chart_check_count.saturating_add(1);
            if updated {
                st.chart_update_count = st.chart_update_count.saturating_add(1);
                st.chart_probe_poll_ms = ACTIVE_HYDRATION_POLL_MS;
            } else if elapsed >= Duration::from_millis(25) {
                st.chart_probe_poll_ms =
                    (st.chart_probe_poll_ms.saturating_mul(2)).min(ACTIVE_HYDRATION_POLL_MAX_MS);
            } else {
                // Keep lightweight probes responsive while idle.
                st.chart_probe_poll_ms =
                    (st.chart_probe_poll_ms.saturating_add(1000)).min(ACTIVE_HYDRATION_POLL_MAX_MS);
            }
            st.chart_compute_total = st.chart_compute_total.saturating_add(elapsed);
        } else {
            warn!("Chart probe channel closed");
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }

    async fn on_perf_log(&self, st: &mut StreamState) -> ControlFlow<()> {
        let this = self;
        if st.transport_tick_count == 0 && st.chart_check_count == 0 {
            return ControlFlow::Continue(());
        }

        let transport_avg_ms = if st.transport_tick_count > 0 {
            (st.transport_compute_total.as_secs_f64() * 1000.0)
                / f64::from(u32::try_from(st.transport_tick_count).unwrap_or(u32::MAX))
        } else {
            0.0
        };
        let chart_avg_ms = if st.chart_check_count > 0 {
            (st.chart_compute_total.as_secs_f64() * 1000.0)
                / f64::from(u32::try_from(st.chart_check_count).unwrap_or(u32::MAX))
        } else {
            0.0
        };
        let fingerprint_support_val = *this.fingerprint_method_supported.read().await;
        let fingerprint_support = match fingerprint_support_val {
            Some(true) => "supported",
            Some(false) => "unsupported",
            None => "unknown",
        };

        debug!(
            "Setlist perf (5s): transport_ticks={}, transport_events={}, transport_song_updates={}, transport_avg_ms={:.2}, chart_checks={}, chart_updates={}, chart_avg_ms={:.2}, fingerprint={}",
            st.transport_tick_count,
            st.transport_events_sent,
            st.transport_song_updates_sent,
            transport_avg_ms,
            st.chart_check_count,
            st.chart_update_count,
            chart_avg_ms,
            fingerprint_support
        );

        st.transport_tick_count = 0;
        st.transport_events_sent = 0;
        st.transport_song_updates_sent = 0;
        st.transport_compute_total = Duration::ZERO;
        st.chart_check_count = 0;
        st.chart_update_count = 0;
        st.chart_compute_total = Duration::ZERO;
        ControlFlow::Continue(())
    }

    // ── Native reactive loop ─────────────────────────────────────────────────
    // A pure refactor of the original select loop: same `tokio::select!` + three
    // `tokio::time::interval` timers (MissedTickBehavior::Skip), now dispatching
    // to the shared `on_*` handlers. Behavior is identical to the pre-refactor
    // loop.
    #[cfg(not(target_arch = "wasm32"))]
    async fn run_subscription_stream(&self) -> Result<(), SessionServiceError> {
        debug!("SetlistService::subscribe() - starting fully reactive event stream");
        let StreamInit {
            mut setlist_updates,
            mut hydration_rx,
            mut chart_hydration_rx,
            mut transport_stream,
            mut chart_probe_rx,
            daw,
            mut state,
        } = self.initialize_stream().await;

        // Timer for checking project tab switches (every 500ms)
        // This is separate from transport updates which come at ~30Hz
        let mut project_check_interval = tokio::time::interval(Duration::from_millis(500));
        project_check_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut active_hydration_interval =
            tokio::time::interval(Duration::from_millis(ACTIVE_HYDRATION_TICK_MS));
        active_hydration_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut perf_log_interval = tokio::time::interval(Duration::from_secs(5));
        perf_log_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        // Fully reactive loop — architect transport stream + periodic checks.
        loop {
            tokio::select! {
                changed = setlist_updates.changed() => {
                    if self.on_setlist_update(&mut state, changed).await.is_break() {
                        break;
                    }
                }
                hydrated = hydration_rx.recv() => {
                    if self.on_hydration(hydrated).is_break() {
                        break;
                    }
                }
                chart_hydrated = chart_hydration_rx.recv() => {
                    if self.on_chart_hydration(chart_hydrated).await.is_break() {
                        break;
                    }
                }
                ev = transport_stream.recv() => {
                    if self.on_transport(&mut state, &daw, ev).await.is_break() {
                        break;
                    }
                }
                _ = project_check_interval.tick() => {
                    if self.on_project_check(&mut state, &daw).await.is_break() {
                        break;
                    }
                }
                _ = active_hydration_interval.tick() => {
                    if self.on_active_hydration(&mut state).await.is_break() {
                        break;
                    }
                }
                chart_probe = chart_probe_rx.recv() => {
                    if Self::on_chart_probe(&mut state, chart_probe).is_break() {
                        break;
                    }
                }
                _ = perf_log_interval.tick() => {
                    if self.on_perf_log(&mut state).await.is_break() {
                        break;
                    }
                }
            }
        }

        info!("SetlistService::subscribe() - stream ended");
        Ok(())
    }

    // ── Wasm reactive loop ───────────────────────────────────────────────────
    // Same StreamState + shared `on_*` handlers as native, driven by
    // `futures::select!` on the browser's single-threaded runtime (no tokio
    // runtime, no `tokio::time::interval`, no `tokio::select!`). The
    // broadcast/watch/transport/chart-probe receiver futures are freshly fused
    // each iteration (cancel-safe, exactly like `tokio::select!` recreates its
    // branch futures); the three `tokio::time::interval` timers become
    // re-arming `architect::platform::sleep` futures.
    //
    // Timer caveat: unlike `MissedTickBehavior::Skip` (fixed wall-clock cadence
    // regardless of handler duration), each `sleep(period)` here is re-armed
    // from the END of its handler, so a slow handler shifts the next tick's
    // phase. Wasm timer resolution is also millisecond-grained. Neither matters
    // for the browser pump (the live-rig pump is the native path above).
    #[cfg(target_arch = "wasm32")]
    async fn run_subscription_stream(&self) -> Result<(), SessionServiceError> {
        use futures::FutureExt;

        debug!("SetlistService::subscribe() - starting fully reactive event stream (wasm)");
        let StreamInit {
            mut setlist_updates,
            mut hydration_rx,
            mut chart_hydration_rx,
            mut transport_stream,
            mut chart_probe_rx,
            daw,
            mut state,
        } = self.initialize_stream().await;

        // Re-arming periodic timers (boxed so the fused future is `Unpin` and can
        // be reassigned in place after each fire).
        let mut project_check =
            Box::pin(architect::platform::sleep(Duration::from_millis(500))).fuse();
        let mut active_hydration = Box::pin(architect::platform::sleep(Duration::from_millis(
            ACTIVE_HYDRATION_TICK_MS,
        )))
        .fuse();
        let mut perf_log = Box::pin(architect::platform::sleep(Duration::from_secs(5))).fuse();

        loop {
            // Freshly fuse the receiver futures each iteration and pin them on
            // the stack for `futures::select!` (they are cancel-safe, so
            // recreating them per iteration matches `tokio::select!`).
            let setlist_fut = setlist_updates.changed().fuse();
            let hydration_fut = hydration_rx.recv().fuse();
            let chart_hydration_fut = chart_hydration_rx.recv().fuse();
            let transport_fut = transport_stream.recv().fuse();
            let chart_probe_fut = chart_probe_rx.recv().fuse();
            futures::pin_mut!(
                setlist_fut,
                hydration_fut,
                chart_hydration_fut,
                transport_fut,
                chart_probe_fut
            );

            let flow = futures::select! {
                changed = setlist_fut => self.on_setlist_update(&mut state, changed).await,
                hydrated = hydration_fut => self.on_hydration(hydrated),
                chart_hydrated = chart_hydration_fut => self.on_chart_hydration(chart_hydrated).await,
                ev = transport_fut => self.on_transport(&mut state, &daw, ev).await,
                chart_probe = chart_probe_fut => Self::on_chart_probe(&mut state, chart_probe),
                _ = project_check => {
                    project_check =
                        Box::pin(architect::platform::sleep(Duration::from_millis(500))).fuse();
                    self.on_project_check(&mut state, &daw).await
                }
                _ = active_hydration => {
                    active_hydration = Box::pin(architect::platform::sleep(
                        Duration::from_millis(ACTIVE_HYDRATION_TICK_MS),
                    ))
                    .fuse();
                    self.on_active_hydration(&mut state).await
                }
                _ = perf_log => {
                    perf_log =
                        Box::pin(architect::platform::sleep(Duration::from_secs(5))).fuse();
                    self.on_perf_log(&mut state).await
                }
            };
            if let ControlFlow::Break(()) = flow {
                break;
            }
        }

        info!("SetlistService::subscribe() - stream ended");
        Ok(())
    }

    /// One process-wide active-indices pump: 60 Hz cursor changes into
    /// the `#[subscribe]` hub. Spawn once.
    pub(crate) async fn subscribe_active_impl(&self) -> Result<(), SessionServiceError> {
        info!("SetlistService: starting active-indices pump");
        let mut last_indices = self.calculate_active_indices().await;
        self.indices_hub.publish(last_indices.clone());
        loop {
            architect::platform::sleep(Duration::from_micros(16667)).await;
            let current_indices = self.calculate_active_indices().await;
            if current_indices != last_indices {
                self.indices_hub.publish(current_indices.clone());
                last_indices = current_indices;
            }
        }
    }

    pub(crate) fn get_audio_latency_impl(&self) -> f64 {
        self.daw.output_latency_seconds()
    }

    pub(crate) fn get_audio_latency_info_impl(&self) -> session_proto::AudioLatencyInfo {
        let state = self.daw.state();
        session_proto::AudioLatencyInfo {
            input_samples: state.latency.input_samples,
            output_samples: state.latency.output_samples,
            output_seconds: state.latency.output_seconds,
            sample_rate: state.latency.sample_rate,
            is_running: state.is_running,
        }
    }
}

// =============================================================================
// SetlistService trait implementation — delegates to sub-module methods
// =============================================================================

impl<D> SetlistService for SetlistServiceImpl<D>
where
    D: AudioEngine
        + Clone
        + daw::service::ExtState
        + Projects
        + daw::service::TempoMap
        + Transport
        + daw::service::Tracks
        // `Send + Sync` natively (byte-for-byte), empty on wasm — the browser
        // in-process engine serves this with a `!Send` Standalone backend.
        + architect::MaybeSendSync
        + 'static,
{
    // =========================================================================
    // Query Methods
    // =========================================================================

    fn setlist(&self) -> Result<session_proto::Setlist, SessionServiceError> {
        let setlist = self
            .setlist
            .try_read()
            .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?;
        setlist
            .clone()
            .ok_or_else(|| SessionServiceError::not_found("Setlist", "current"))
    }

    fn songs(&self) -> Result<Vec<Song>, SessionServiceError> {
        let setlist = self
            .setlist
            .try_read()
            .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?;
        Ok(setlist
            .as_ref()
            .map(|sl| sl.songs.clone())
            .unwrap_or_default())
    }

    fn song(&self, index: usize) -> Result<Song, SessionServiceError> {
        let setlist = self
            .setlist
            .try_read()
            .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?;
        setlist
            .as_ref()
            .and_then(|s| s.songs.get(index).cloned())
            .ok_or_else(|| SessionServiceError::not_found("Song", &index))
    }

    fn sections(&self, song_index: usize) -> Result<Vec<Section>, SessionServiceError> {
        self.setlist
            .try_read()
            .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?
            .as_ref()
            .and_then(|sl| sl.songs.get(song_index))
            .map(|song| song.sections.clone())
            .ok_or_else(|| SessionServiceError::not_found("Song", &song_index))
    }

    fn section(
        &self,
        song_index: usize,
        section_index: usize,
    ) -> Result<Section, SessionServiceError> {
        self.setlist
            .try_read()
            .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?
            .as_ref()
            .and_then(|sl| sl.songs.get(song_index))
            .ok_or_else(|| SessionServiceError::not_found("Song", &song_index))
            .and_then(|song| {
                song.sections
                    .get(section_index)
                    .cloned()
                    .ok_or_else(|| SessionServiceError::not_found("Section", &section_index))
            })
    }

    async fn song_chart(
        &self,
        song_index: usize,
    ) -> Result<Option<session_proto::SongChartHydration>, SessionServiceError> {
        let project_guid = self
            .setlist
            .read()
            .await
            .as_ref()
            .and_then(|sl| sl.songs.get(song_index))
            .map(|song| song.project_guid.clone())
            .ok_or_else(|| SessionServiceError::not_found("Song", &song_index))?;
        Ok(self.chart_cache.get(&project_guid).await)
    }

    fn measures(&self, song_index: usize) -> Result<Vec<MeasureInfo>, SessionServiceError> {
        let song = {
            let setlist = self
                .setlist
                .try_read()
                .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?;
            setlist
                .as_ref()
                .and_then(|s| s.songs.get(song_index))
                .cloned()
                .ok_or_else(|| SessionServiceError::not_found("Song", &song_index))?
        };

        // If we have pre-calculated measure positions, use them
        if !song.measure_positions.is_empty() {
            let ts = song
                .time_signature
                .unwrap_or_else(|| daw::service::TimeSignature::new(4, 4));
            return Ok(song
                .measure_positions
                .iter()
                .enumerate()
                .map(|(idx, pos)| MeasureInfo {
                    measure: i32::try_from(idx).unwrap_or(i32::MAX),
                    time_seconds: pos
                        .time
                        .as_ref()
                        .map_or(0.0, daw_proto::PositionInSeconds::as_seconds),
                    time_sig_numerator: ts.numerator().cast_signed(),
                    time_sig_denominator: ts.denominator().cast_signed(),
                })
                .collect());
        }

        // Otherwise, calculate measures from tempo and time signature
        let ts = song
            .time_signature
            .unwrap_or_else(|| daw::service::TimeSignature::new(4, 4));
        let tempo = song.tempo.unwrap_or(120.0);

        // Calculate measure duration in seconds
        let beats_per_measure = f64::from(ts.numerator());
        let seconds_per_beat = 60.0 / tempo;
        let measure_duration = beats_per_measure * seconds_per_beat;

        // Generate measures for the song duration
        let song_duration = song.duration();
        let count_f64 = (song_duration / measure_duration).ceil();
        let measure_count = if count_f64 < 0.0 || !count_f64.is_finite() {
            i32::MAX
        } else if count_f64 > f64::from(i32::MAX) {
            i32::MAX
        } else {
            // Already range-checked above (0.0..=f64::from(i32::MAX)) and
            // finite; std has no non-`as` float-to-int conversion.
            #[allow(
                clippy::as_conversions,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss
            )]
            {
                count_f64 as i32
            }
        };

        Ok((0..measure_count)
            .map(|idx| MeasureInfo {
                measure: idx,
                time_seconds: f64::from(idx).mul_add(measure_duration, song.start_seconds),
                time_sig_numerator: ts.numerator().cast_signed(),
                time_sig_denominator: ts.denominator().cast_signed(),
            })
            .collect())
    }

    fn active_song(&self) -> Result<Song, SessionServiceError> {
        // Use cached indices for instant response (updated at 60Hz by polling loop)
        let active = self
            .cached_indices
            .try_read()
            .map_err(|_| SessionServiceError::Internal("active indices are busy".to_string()))?
            .clone();
        let song_index = active
            .song_index
            .ok_or_else(|| SessionServiceError::not_found("Song", "active"))?;
        let setlist = self
            .setlist
            .try_read()
            .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?;
        setlist
            .as_ref()
            .and_then(|s| s.songs.get(song_index).cloned())
            .ok_or_else(|| SessionServiceError::not_found("Song", &song_index))
    }

    fn active_section(&self) -> Result<Section, SessionServiceError> {
        // Use cached indices for instant response (updated at 60Hz by polling loop)
        let active = self
            .cached_indices
            .try_read()
            .map_err(|_| SessionServiceError::Internal("active indices are busy".to_string()))?
            .clone();
        let song_index = active
            .song_index
            .ok_or_else(|| SessionServiceError::not_found("Song", "active"))?;
        let section_index = active
            .section_index
            .ok_or_else(|| SessionServiceError::not_found("Section", "active"))?;
        self.setlist
            .try_read()
            .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?
            .as_ref()
            .and_then(|sl| sl.songs.get(song_index))
            .ok_or_else(|| SessionServiceError::not_found("Song", &song_index))
            .and_then(|song| {
                song.sections
                    .get(section_index)
                    .cloned()
                    .ok_or_else(|| SessionServiceError::not_found("Section", &section_index))
            })
    }

    fn song_at(&self, seconds: f64) -> Result<Song, SessionServiceError> {
        self.setlist
            .try_read()
            .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?
            .as_ref()
            .and_then(|sl| sl.song_at(seconds))
            .map(|(_, song)| song.clone())
            .ok_or_else(|| SessionServiceError::not_found("Song", &format!("at {seconds}s")))
    }

    fn section_at(&self, seconds: f64) -> Result<Section, SessionServiceError> {
        self.setlist
            .try_read()
            .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?
            .as_ref()
            .and_then(|sl| sl.song_at(seconds))
            .ok_or_else(|| SessionServiceError::not_found("Song", &format!("at {seconds}s")))
            .and_then(|(_, song)| {
                song.section_at_position_with_index(seconds)
                    .map(|(_, section)| section.clone())
                    .ok_or_else(|| {
                        SessionServiceError::not_found("Section", &format!("at {seconds}s"))
                    })
            })
    }

    // =========================================================================
    // Navigation Commands — delegate to navigation.rs
    // =========================================================================

    async fn go_to_song(&self, index: usize) -> Result<(), SessionServiceError> {
        self.go_to_song_impl(index).await
    }

    async fn next_song(&self) -> Result<(), SessionServiceError> {
        self.next_song_impl().await
    }

    async fn previous_song(&self) -> Result<(), SessionServiceError> {
        self.previous_song_impl().await
    }

    async fn go_to_section(&self, index: usize) -> Result<(), SessionServiceError> {
        self.go_to_section_impl(index).await
    }

    async fn next_section(&self) -> Result<(), SessionServiceError> {
        self.next_section_impl().await
    }

    async fn previous_section(&self) -> Result<(), SessionServiceError> {
        self.previous_section_impl().await
    }

    async fn seek_to(&self, seconds: f64) -> Result<(), SessionServiceError> {
        self.seek_to_impl(seconds).await
    }

    async fn seek_to_time(
        &self,
        song_index: usize,
        seconds: f64,
    ) -> Result<(), SessionServiceError> {
        self.seek_to_time_impl(song_index, seconds).await
    }

    async fn seek_to_song(&self, song_index: usize) -> Result<(), SessionServiceError> {
        self.seek_to_song_impl(song_index).await
    }

    async fn seek_to_section(
        &self,
        song_index: usize,
        section_index: usize,
    ) -> Result<(), SessionServiceError> {
        self.seek_to_section_impl(song_index, section_index).await
    }

    async fn seek_to_musical_position(
        &self,
        song_index: usize,
        position: daw::service::MusicalPosition,
    ) -> Result<(), SessionServiceError> {
        self.seek_to_musical_position_impl(song_index, position)
            .await
    }

    async fn goto_measure(
        &self,
        song_index: usize,
        measure: i32,
    ) -> Result<(), SessionServiceError> {
        self.goto_measure_impl(song_index, measure).await
    }

    // =========================================================================
    // Playback Commands — delegate to playback.rs
    // =========================================================================

    async fn toggle_playback(&self) -> Result<(), SessionServiceError> {
        self.toggle_playback_impl().await
    }

    async fn play(&self) -> Result<(), SessionServiceError> {
        self.play_impl().await
    }

    async fn pause(&self) -> Result<(), SessionServiceError> {
        self.pause_impl().await
    }

    async fn stop(&self) -> Result<(), SessionServiceError> {
        self.stop_impl().await
    }

    async fn toggle_song_loop(&self) -> Result<(), SessionServiceError> {
        self.toggle_song_loop_impl();
        Ok(())
    }

    async fn toggle_section_loop(&self) -> Result<(), SessionServiceError> {
        Self::toggle_section_loop_impl();
        Ok(())
    }

    async fn set_loop_region(
        &self,
        start_seconds: f64,
        end_seconds: f64,
    ) -> Result<(), SessionServiceError> {
        Self::set_loop_region_impl(start_seconds, end_seconds);
        Ok(())
    }

    async fn clear_loop(&self) -> Result<(), SessionServiceError> {
        self.clear_loop_impl();
        Ok(())
    }

    // =========================================================================
    // Recording — delegate to record.rs
    // =========================================================================

    async fn record(&self) -> Result<(), SessionServiceError> {
        self.record_impl().await
    }

    async fn stop_recording(&self) -> Result<(), SessionServiceError> {
        self.stop_recording_impl().await
    }

    async fn toggle_recording(&self) -> Result<(), SessionServiceError> {
        self.toggle_recording_impl().await
    }

    async fn song_review(
        &self,
        song_index: usize,
    ) -> Result<Vec<session_proto::review::Pass>, SessionServiceError> {
        self.song_review_impl(song_index).await
    }

    async fn mark_take(
        &self,
        song_index: usize,
        pass: u32,
        mark: session_proto::review::Mark,
    ) -> Result<(), SessionServiceError> {
        self.mark_take_impl(song_index, pass, mark).await
    }

    async fn set_song_record_arm(&self, armed: bool) -> Result<(), SessionServiceError> {
        self.set_song_record_arm_impl(armed).await
    }

    // =========================================================================
    // Build/Refresh — delegate to build.rs
    // =========================================================================

    async fn build_from_open_projects(&self) -> Result<(), SessionServiceError> {
        self.build_from_open_projects_impl().await
    }

    async fn refresh(&self) -> Result<(), SessionServiceError> {
        self.refresh_impl().await
    }

    async fn load_demo_setlist(&self) -> Result<(), SessionServiceError> {
        super::demo::stamp_demo_setlist().await?;
        self.build_from_open_projects_impl().await
    }

    async fn generate_combined_setlist(
        &self,
        gap_measures: u32,
    ) -> Result<String, SessionServiceError> {
        self.generate_combined_setlist_impl(gap_measures).await
    }

    // =========================================================================
    // Subscriptions — delegate to polling.rs
    // =========================================================================

    async fn get_audio_latency(&self) -> Result<f64, SessionServiceError> {
        Ok(self.get_audio_latency_impl())
    }

    async fn get_audio_latency_info(
        &self,
    ) -> Result<session_proto::AudioLatencyInfo, SessionServiceError> {
        Ok(self.get_audio_latency_info_impl())
    }
}

impl<D> session_proto::services::setlist_service::SetlistServiceStreamSource
    for SetlistServiceImpl<D>
where
    D: AudioEngine
        + Clone
        + daw::service::ExtState
        + Projects
        + daw::service::TempoMap
        + Transport
        + daw::service::Tracks
        // `Send + Sync` natively (byte-for-byte), empty on wasm.
        + architect::MaybeSendSync
        + 'static,
{
    fn events_hub(&self) -> &architect::PubSub<SetlistEvent> {
        &self.events_hub
    }

    fn active_indices_hub(&self) -> &architect::PubSub<ActiveIndices> {
        &self.indices_hub
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw::service::Position;
    use daw_proto::{PositionInSeconds, TimeSignature};
    use daw_standalone::Standalone;
    use session_proto::SongId;

    fn song_at(name: &str, start: f64, end: f64, tempo: f64, ts: (u32, u32)) -> Song {
        Song {
            id: SongId::new(),
            name: name.to_string(),
            project_guid: "shared-timeline".to_string(),
            start_seconds: start,
            end_seconds: end,
            count_in_seconds: None,
            sections: vec![],
            comments: vec![],
            tempo: Some(tempo),
            time_signature: Some(TimeSignature::new(ts.0, ts.1)),
            measure_positions: vec![],
            chart_text: None,
            parsed_chart: None,
            detected_chords: vec![],
            chart_fingerprint: None,
            advance_mode: None,
            color: None,
        }
    }

    /// The demo stamps every song onto ONE standalone project, so the project
    /// transport reports a single global tempo (the first song's, 127). Each
    /// later song must still show ITS OWN authored tempo/time signature in the
    /// badges — `calculate_song_transport` overrides the project transport with
    /// the song's own values. This guards the "tempo badge stuck at 127" bug.
    #[test]
    fn song_transport_reports_songs_own_tempo_not_project_tempo() {
        // A later song authored at 72 BPM, 6/8 — while the project transport
        // (shared timeline) reports the first song's 127 BPM, 4/4.
        let song = song_at("God I'm Just Grateful", 300.0, 480.0, 72.0, (6, 8));
        let position = Position::from_time(PositionInSeconds::from_seconds(360.0));

        let t = SetlistServiceImpl::<Standalone>::calculate_song_transport(
            &song,
            1,
            ProjectTransportSnapshot {
                position,
                is_playing: false,
                is_looping: false,
                loop_region: None,
                tempo: 127.0,     // project transport tempo (WRONG for this song)
                time_sig: (4, 4), // project transport time sig (WRONG for this song)
            },
        );

        assert!(
            (t.bpm - 72.0).abs() < f64::EPSILON,
            "badge must show the song's own tempo, not 127"
        );
        assert_eq!(t.time_sig_num, 6);
        assert_eq!(t.time_sig_denom, 8);
        // Position is carried through so the time/musical badges move on seek.
        let pos_secs = t.position.time.map(|p| p.as_seconds());
        assert!(
            (pos_secs.unwrap_or(0.0) - 360.0).abs() < f64::EPSILON,
            "seek position must reach the transport state for the badge"
        );
    }

    /// When a song hasn't authored a tempo (no chart), fall back to the project
    /// transport tempo rather than losing it.
    #[test]
    fn song_transport_falls_back_to_project_tempo_when_unauthored() {
        let mut song = song_at("Untitled", 0.0, 60.0, 0.0, (4, 4));
        song.tempo = None;
        song.time_signature = None;
        let position = Position::from_time(PositionInSeconds::from_seconds(1.0));

        let t = SetlistServiceImpl::<Standalone>::calculate_song_transport(
            &song,
            0,
            ProjectTransportSnapshot {
                position,
                is_playing: false,
                is_looping: false,
                loop_region: None,
                tempo: 140.0,
                time_sig: (3, 4),
            },
        );

        assert!((t.bpm - 140.0).abs() < f64::EPSILON);
        assert_eq!(t.time_sig_num, 3);
        assert_eq!(t.time_sig_denom, 4);
    }
}
