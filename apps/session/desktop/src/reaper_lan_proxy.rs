//! Recording Mode's LAN control surface: a `SetlistService` implementation
//! that forwards every call to the real `SetlistServiceImpl<daw_reaper::
//! Reaper>` running inside the REAPER extension, over the same
//! `SetlistServiceClient`/`SetlistServiceStreamClient` connection
//! `reaper_engine.rs` already opens.
//!
//! Why a proxy and not just re-mounting the extension's own router:
//! session-desktop only ever gets a *client* to that service (it dials a
//! Unix-domain socket the extension already publishes) — there is no
//! local backend to build a fresh `SetlistServiceImpl` from. Serving the
//! trait for LAN clients means implementing it by hand: `SetlistService`
//! mixes plain `fn` query methods (must return synchronously — REAPER's
//! own impl reads them from an in-memory cache, not from a live query)
//! with `async fn` commands. The commands forward straight to the client;
//! the queries read from a local cache kept current by a background task
//! consuming the client's own `events`/`active_indices` streams — the
//! exact same shape `SetlistServiceImpl`'s own cache/pump already uses
//! (see `crates/session/session/src/setlist/service/polling.rs`), just
//! fed from a remote connection instead of an in-process backend.

use std::sync::Arc;

use session::services::setlist_service::{SetlistServiceStreamClient, SetlistServiceStreamSource};
use session::{SessionServiceError, SetlistServiceClient};
use session_proto::{
    ActiveIndices, AudioLatencyInfo, MeasureInfo, MusicalPosition, Section, Setlist, SetlistEvent,
    Song, SongChartHydration,
};
use tokio::sync::RwLock;

/// Every generated client method returns `Result<T, vox::VoxError<E>>` —
/// `VoxError::User` carries the real application error the remote handler
/// returned; every other variant is a transport-level failure (connection
/// dropped, timed out, …) with no `SessionServiceError` equivalent, so
/// those collapse into `Internal` with the debug text for diagnosis.
fn map_vox_err(e: vox::VoxError<SessionServiceError>) -> SessionServiceError {
    match e {
        vox::VoxError::User(inner) => *inner,
        other => SessionServiceError::Internal(format!("LAN proxy: {other:?}")),
    }
}

#[derive(Clone)]
pub struct ReaperLanProxy {
    client: SetlistServiceClient,
    setlist: Arc<RwLock<Option<Setlist>>>,
    cached_indices: Arc<RwLock<ActiveIndices>>,
    events_hub: architect::PubSub<SetlistEvent>,
    indices_hub: architect::PubSub<ActiveIndices>,
}

impl ReaperLanProxy {
    /// Build the proxy and start the background cache pump. `stream_client`
    /// is consumed — its `events`/`active_indices` subscriptions run for
    /// the life of the process, mirroring `SessionEventBridge`'s own
    /// subscribe-once-forever pattern.
    pub fn new(client: SetlistServiceClient, stream_client: SetlistServiceStreamClient) -> Self {
        let proxy = Self {
            client,
            setlist: Arc::new(RwLock::new(None)),
            cached_indices: Arc::new(RwLock::new(ActiveIndices::default())),
            events_hub: architect::PubSub::sliding(64),
            indices_hub: architect::PubSub::sliding(16).with_replay(1),
        };
        proxy.spawn_pump(stream_client);
        proxy
    }

    /// Fetch the setlist directly and update the cache immediately,
    /// rather than waiting on the `events` stream's `SetlistChanged`
    /// republish. Building/refreshing the setlist on the *real* backend
    /// completing does not mean the stream pump has already processed the
    /// resulting event — that's a second, independent async hop — so a
    /// caller that seeks or reads the setlist right after
    /// `build_from_open_projects` returns can otherwise race an empty
    /// cache. Best-effort: a failed read here just leaves the cache as it
    /// was; the stream will still catch up shortly after.
    async fn refresh_setlist_cache(&self) {
        if let Ok(setlist) = self.client.setlist().await {
            *self.setlist.write().await = Some(setlist);
        }
    }

    fn spawn_pump(&self, stream_client: SetlistServiceStreamClient) {
        // Seed the cache from a direct snapshot before the stream pump
        // starts — the same deterministic-initial-state reasoning
        // `SessionEventBridge` documents for the in-process case: don't
        // rely solely on the stream's first republish, which may not have
        // fired yet (e.g. Recording Mode connects after
        // `build_from_open_projects` already ran once).
        let client = self.client.clone();
        let setlist = self.setlist.clone();
        tokio::spawn(async move {
            if let Ok(initial) = client.setlist().await {
                *setlist.write().await = Some(initial);
            }
        });

        let setlist = self.setlist.clone();
        let events_hub = self.events_hub.clone();
        tokio::spawn({
            let stream_client = stream_client.clone();
            async move {
                let (tx, mut rx) = vox::channel::<SetlistEvent>();
                tokio::spawn(async move {
                    let _ = stream_client.events(tx).await;
                });
                while let Ok(Some(event_ref)) = rx.recv().await {
                    let event = event_ref.get().clone();
                    if let SetlistEvent::SetlistChanged(new_setlist) = &event {
                        *setlist.write().await = Some(new_setlist.clone());
                    }
                    events_hub.publish(event);
                }
            }
        });

        let cached_indices = self.cached_indices.clone();
        let indices_hub = self.indices_hub.clone();
        tokio::spawn(async move {
            let (tx, mut rx) = vox::channel::<ActiveIndices>();
            tokio::spawn(async move {
                let _ = stream_client.active_indices(tx).await;
            });
            while let Ok(Some(indices_ref)) = rx.recv().await {
                let indices = indices_ref.get().clone();
                *cached_indices.write().await = indices.clone();
                indices_hub.publish(indices);
            }
        });
    }
}

impl session::SetlistService for ReaperLanProxy {
    fn setlist(&self) -> Result<Setlist, SessionServiceError> {
        self.setlist
            .try_read()
            .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?
            .clone()
            .ok_or_else(|| SessionServiceError::not_found("Setlist", "current"))
    }

    fn songs(&self) -> Result<Vec<Song>, SessionServiceError> {
        Ok(self
            .setlist
            .try_read()
            .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?
            .as_ref()
            .map(|sl| sl.songs.clone())
            .unwrap_or_default())
    }

    fn song(&self, index: usize) -> Result<Song, SessionServiceError> {
        self.setlist
            .try_read()
            .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?
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

    fn measures(&self, song_index: usize) -> Result<Vec<MeasureInfo>, SessionServiceError> {
        self.setlist
            .try_read()
            .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?
            .as_ref()
            .and_then(|s| s.songs.get(song_index))
            .map(|song| {
                let ts = song
                    .time_signature
                    .unwrap_or_else(|| daw::service::TimeSignature::new(4, 4));
                song.measure_positions
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
                    .collect()
            })
            .ok_or_else(|| SessionServiceError::not_found("Song", &song_index))
    }

    async fn song_chart(
        &self,
        song_index: usize,
    ) -> Result<Option<SongChartHydration>, SessionServiceError> {
        self.client
            .song_chart(song_index)
            .await
            .map_err(map_vox_err)
    }

    fn active_song(&self) -> Result<Song, SessionServiceError> {
        let song_index = self
            .cached_indices
            .try_read()
            .map_err(|_| SessionServiceError::Internal("active indices are busy".to_string()))?
            .song_index
            .ok_or_else(|| SessionServiceError::not_found("Song", "active"))?;
        self.setlist
            .try_read()
            .map_err(|_| SessionServiceError::Internal("setlist state is busy".to_string()))?
            .as_ref()
            .and_then(|s| s.songs.get(song_index).cloned())
            .ok_or_else(|| SessionServiceError::not_found("Song", &song_index))
    }

    fn active_section(&self) -> Result<Section, SessionServiceError> {
        let (song_index, section_index) = {
            let active = self.cached_indices.try_read().map_err(|_| {
                SessionServiceError::Internal("active indices are busy".to_string())
            })?;
            (
                active
                    .song_index
                    .ok_or_else(|| SessionServiceError::not_found("Song", "active"))?,
                active
                    .section_index
                    .ok_or_else(|| SessionServiceError::not_found("Section", "active"))?,
            )
        };
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

    async fn go_to_song(&self, index: usize) -> Result<(), SessionServiceError> {
        self.client.go_to_song(index).await.map_err(map_vox_err)
    }

    async fn next_song(&self) -> Result<(), SessionServiceError> {
        self.client.next_song().await.map_err(map_vox_err)
    }

    async fn previous_song(&self) -> Result<(), SessionServiceError> {
        self.client.previous_song().await.map_err(map_vox_err)
    }

    async fn go_to_section(&self, index: usize) -> Result<(), SessionServiceError> {
        self.client.go_to_section(index).await.map_err(map_vox_err)
    }

    async fn next_section(&self) -> Result<(), SessionServiceError> {
        self.client.next_section().await.map_err(map_vox_err)
    }

    async fn previous_section(&self) -> Result<(), SessionServiceError> {
        self.client.previous_section().await.map_err(map_vox_err)
    }

    async fn seek_to(&self, seconds: f64) -> Result<(), SessionServiceError> {
        self.client.seek_to(seconds).await.map_err(map_vox_err)
    }

    async fn seek_to_time(
        &self,
        song_index: usize,
        seconds: f64,
    ) -> Result<(), SessionServiceError> {
        self.client
            .seek_to_time(song_index, seconds)
            .await
            .map_err(map_vox_err)
    }

    async fn seek_to_song(&self, song_index: usize) -> Result<(), SessionServiceError> {
        self.client
            .seek_to_song(song_index)
            .await
            .map_err(map_vox_err)
    }

    async fn seek_to_section(
        &self,
        song_index: usize,
        section_index: usize,
    ) -> Result<(), SessionServiceError> {
        self.client
            .seek_to_section(song_index, section_index)
            .await
            .map_err(map_vox_err)
    }

    async fn seek_to_musical_position(
        &self,
        song_index: usize,
        position: MusicalPosition,
    ) -> Result<(), SessionServiceError> {
        self.client
            .seek_to_musical_position(song_index, position)
            .await
            .map_err(map_vox_err)
    }

    async fn goto_measure(
        &self,
        song_index: usize,
        measure: i32,
    ) -> Result<(), SessionServiceError> {
        self.client
            .goto_measure(song_index, measure)
            .await
            .map_err(map_vox_err)
    }

    async fn toggle_playback(&self) -> Result<(), SessionServiceError> {
        self.client.toggle_playback().await.map_err(map_vox_err)
    }

    async fn play(&self) -> Result<(), SessionServiceError> {
        self.client.play().await.map_err(map_vox_err)
    }

    async fn pause(&self) -> Result<(), SessionServiceError> {
        self.client.pause().await.map_err(map_vox_err)
    }

    async fn stop(&self) -> Result<(), SessionServiceError> {
        self.client.stop().await.map_err(map_vox_err)
    }

    async fn toggle_song_loop(&self) -> Result<(), SessionServiceError> {
        self.client.toggle_song_loop().await.map_err(map_vox_err)
    }

    async fn toggle_section_loop(&self) -> Result<(), SessionServiceError> {
        self.client.toggle_section_loop().await.map_err(map_vox_err)
    }

    async fn set_loop_region(
        &self,
        start_seconds: f64,
        end_seconds: f64,
    ) -> Result<(), SessionServiceError> {
        self.client
            .set_loop_region(start_seconds, end_seconds)
            .await
            .map_err(map_vox_err)
    }

    async fn clear_loop(&self) -> Result<(), SessionServiceError> {
        self.client.clear_loop().await.map_err(map_vox_err)
    }

    async fn record(&self) -> Result<(), SessionServiceError> {
        self.client.record().await.map_err(map_vox_err)
    }

    async fn stop_recording(&self) -> Result<(), SessionServiceError> {
        self.client.stop_recording().await.map_err(map_vox_err)
    }

    async fn song_review(
        &self,
        song_index: usize,
    ) -> Result<Vec<session_proto::review::Pass>, SessionServiceError> {
        self.client
            .song_review(song_index)
            .await
            .map_err(map_vox_err)
    }

    async fn mark_take(
        &self,
        song_index: usize,
        pass: u32,
        mark: session_proto::review::Mark,
    ) -> Result<(), SessionServiceError> {
        self.client
            .mark_take(song_index, pass, mark)
            .await
            .map_err(map_vox_err)
    }

    async fn toggle_recording(&self) -> Result<(), SessionServiceError> {
        self.client.toggle_recording().await.map_err(map_vox_err)
    }

    async fn set_song_record_arm(&self, armed: bool) -> Result<(), SessionServiceError> {
        self.client
            .set_song_record_arm(armed)
            .await
            .map_err(map_vox_err)
    }

    async fn build_from_open_projects(&self) -> Result<(), SessionServiceError> {
        self.client
            .build_from_open_projects()
            .await
            .map_err(map_vox_err)?;
        self.refresh_setlist_cache().await;
        Ok(())
    }

    async fn refresh(&self) -> Result<(), SessionServiceError> {
        self.client.refresh().await.map_err(map_vox_err)?;
        self.refresh_setlist_cache().await;
        Ok(())
    }

    async fn load_demo_setlist(&self) -> Result<(), SessionServiceError> {
        self.client.load_demo_setlist().await.map_err(map_vox_err)?;
        self.refresh_setlist_cache().await;
        Ok(())
    }

    async fn generate_combined_setlist(
        &self,
        gap_measures: u32,
    ) -> Result<String, SessionServiceError> {
        self.client
            .generate_combined_setlist(gap_measures)
            .await
            .map_err(map_vox_err)
    }

    async fn get_audio_latency(&self) -> Result<f64, SessionServiceError> {
        self.client.get_audio_latency().await.map_err(map_vox_err)
    }

    async fn get_audio_latency_info(&self) -> Result<AudioLatencyInfo, SessionServiceError> {
        self.client
            .get_audio_latency_info()
            .await
            .map_err(map_vox_err)
    }
}

impl architect::HasDispatcher for ReaperLanProxy {
    type Dispatcher = architect::dispatch::CurrentThreadDispatcher;

    fn dispatcher(&self) -> Self::Dispatcher {
        architect::dispatch::CurrentThreadDispatcher
    }
}

impl SetlistServiceStreamSource for ReaperLanProxy {
    fn events_hub(&self) -> &architect::PubSub<SetlistEvent> {
        &self.events_hub
    }

    fn active_indices_hub(&self) -> &architect::PubSub<ActiveIndices> {
        &self.indices_hub
    }
}

static PROXY: std::sync::OnceLock<ReaperLanProxy> = std::sync::OnceLock::new();

/// Install the proxy once Recording Mode has a real connection — called
/// from `reaper_engine::ensure_connected` alongside `Session::init`, so
/// it's ready the moment REAPER is, whether or not `--engine`'s LAN
/// server is even running in this process.
pub fn install(client: SetlistServiceClient, stream_client: SetlistServiceStreamClient) {
    let _ = PROXY.set(ReaperLanProxy::new(client, stream_client));
}

/// The proxy, once Recording Mode has connected.
pub fn proxy() -> Option<&'static ReaperLanProxy> {
    PROXY.get()
}

/// A fresh `LayerRouter` serving this proxy's `SetlistService` — the
/// Recording Mode counterpart to `session_engine::SessionEngine::router()`.
pub fn router(proxy: &ReaperLanProxy) -> daw::LayerRouter {
    use session::services::setlist_service::{
        setlist_service_stream_service_descriptor, stream_serve as setlist_service_stream_serve,
    };
    use session::{serve_setlist_service, setlist_service_service_descriptor};

    daw::LayerRouter::new()
        .with(
            setlist_service_service_descriptor(),
            serve_setlist_service(proxy.clone()),
        )
        .with(
            setlist_service_stream_service_descriptor(),
            setlist_service_stream_serve(proxy.clone()),
        )
}
