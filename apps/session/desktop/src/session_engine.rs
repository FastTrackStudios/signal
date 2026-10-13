//! In-process session engine — the daw-standalone setlist player.
//!
//! Embeds the session domain plus a `daw-standalone` backend so the
//! setlist is DATA this app can PLAY — transport over songs/sections
//! without REAPER. Construction replicates session's
//! `standalone_setlist_harness` (the proven REAPER-free path), minus any
//! seeded songs — the engine boots empty; [`SessionEngine::load_setlist`]
//! is what puts real songs into it (called from the Home page):
//!
//! 1. `Standalone::new()` — no projects seeded yet.
//! 2. `build_in_process_daw` serves Standalone's service bundle over a
//!    vox memory link and wires the global `daw::` facade to it
//!    (`daw::init_from_parts`) — the setlist builder + polling loops
//!    resolve the daw through `daw::get()`.
//! 3. `SetlistServiceImpl::with_daw(standalone)` + `start_stream_pumps()`
//!    — one process-wide pump per `#[subscribe]` hub.
//! 4. An `architect::LocalServer` hosts the setlist RPC behind a
//!    `LayerRouter`; the resulting `SetlistServiceClient` becomes
//!    session-ui's `Session` singleton (the same client the desktop app
//!    builds over its REAPER socket — here it's a memory link).
//! 5. The UI bridge (see `session_view`) attaches straight to the
//!    service's `events_hub()` and folds events into session-ui's
//!    global signals — the in-process flavor of the web remote's
//!    subscription.
//!
//! Audio: `attach_audio_engine` opens the default cpal output so the
//! audio callback drives the playhead sample-accurately, following
//! whichever project is current — including "none yet" at boot, before
//! a setlist has been loaded. If no device is available the soft clock
//! is re-enabled and transport runs silently — the player still works.
//!
//! **Audio decode is lazy, per song.** `load_setlist` only parses each
//! song's RPP structure (`load_rpp_text` — tracks/markers/tempo, no
//! audio) up front; that's fast even for a long setlist. Decoding a
//! song's actual audio (`materialize_via_bay`) happens on the audio
//! thread, right before it attaches — the moment a song *becomes*
//! current, not when the setlist is built. Loading N songs used to
//! decode all N up front, which could exceed `daw-standalone`'s preload
//! budget (each multitrack song is 3-5 GB of PCM) and froze the UI for
//! however long that decode took; now only the one song actually being
//! played is ever resident. `song_folders`/`materialized` are what let
//! the audio thread do this: the former remembers which folder each
//! project's stems live in (for the media bay's file resolver), the
//! latter is a decoded-once cache so re-visiting a song doesn't re-decode.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use daw::service::Projects;
use daw_standalone::audio_engine::materialize::materialize_via_bay;
use daw_standalone::bootstrap::{InProcessDaw, build_in_process_daw};
use daw_standalone::media_bay::ProjectRelativeResolver;
use daw_standalone::project_loader::load_rpp_text;
use daw_standalone::sync::Standalone;
use session::services::setlist_service::{
    SetlistServiceStreamClient, setlist_service_stream_service_descriptor,
    stream_serve as setlist_service_stream_serve,
};
use session::{
    SetlistServiceClient, SetlistServiceImpl, serve_setlist_service,
    setlist_service_service_descriptor,
};
use session_vault_sync::library::LibrarySong;
use session_vault_sync::live_bus;

/// Which folder each open project's stems live in — set at
/// `load_setlist` time (structure only, no decode yet), read by the
/// audio thread right before it materializes+attaches a song.
type SongFolders = Arc<Mutex<HashMap<String, PathBuf>>>;
/// Project guids whose audio has already been decoded — the audio
/// thread checks this before materializing so switching back to a
/// previously-played song in the setlist doesn't re-decode it.
type Materialized = Arc<Mutex<HashSet<String>>>;

/// Per-song outcome of [`SessionEngine::load_setlist`], for the Home
/// page to report back to the user. Structural only (track count) —
/// audio decode is lazy now, so there's nothing to report about it yet
/// at load time; see the module doc.
pub struct SongLoadReport {
    pub title: String,
    pub track_count: usize,
}

pub struct SessionEngine {
    /// Shared service handle — the UI bridge attaches to its
    /// `events_hub()` directly (in-process, no wire).
    pub setlist: SetlistServiceImpl<Standalone>,
    /// RPC client over the in-process LocalServer — installed as
    /// session-ui's `Session` singleton for transport commands.
    pub client: SetlistServiceClient,
    /// Stream client for the `#[subscribe]` events + active_indices streams.
    /// The UI bridge drives `events(tx)` / `active_indices(tx)` on this so the
    /// vox lane pumps them (raw in-process hub attach is never drained).
    pub stream_client: SetlistServiceStreamClient,
    /// The standalone daw backend itself (kept for future direct
    /// native-trait access; the audio thread holds its own clone).
    #[allow(dead_code)]
    pub standalone: Standalone,
    /// Which folder each open project's stems live in — see the module
    /// doc. Shared with the audio thread, which reads it to materialize
    /// a song's audio lazily right before attaching.
    song_folders: SongFolders,
    /// Project guids already decoded — shared with the audio thread.
    materialized: Materialized,
    /// Keeps the daw-facade memory link's acceptor alive.
    _daw: InProcessDaw,
    /// Keeps the setlist RPC LocalServer's acceptor + lanes alive.
    _scope: Arc<architect::Scope>,
}

impl SessionEngine {
    /// A fresh `LayerRouter` serving THIS engine's setlist service — the RPC
    /// layer plus its `#[subscribe]` stream sibling (events +
    /// active_indices), both over the same `SetlistServiceImpl` (shared
    /// PubSub hubs). Engine mode merges this onto the network `/vox` router
    /// so browser remotes drive the same transport/setlist the in-process
    /// UI reads; the serve layers here are additional instances over the
    /// same impl as the in-process LocalServer's — layers are cheap, the
    /// state is shared.
    pub fn router(&self) -> daw::LayerRouter {
        daw::LayerRouter::new()
            .with(
                setlist_service_service_descriptor(),
                serve_setlist_service(self.setlist.clone()),
            )
            .with(
                setlist_service_stream_service_descriptor(),
                setlist_service_stream_serve(self.setlist.clone()),
            )
    }

    /// Tear down whatever's currently open (the demo, or a previously
    /// loaded setlist) and load `songs` as the new one. Structure only —
    /// each song's Master Setlist Template is built in-memory and its
    /// tracks/markers/tempo imported via `load_rpp_text`; audio decode is
    /// deferred to the audio thread, per song, right before it's needed
    /// (see the module doc — this is what keeps loading a setlist fast
    /// and its memory use bounded to one song's audio at a time).
    /// Returns a per-song report for the Home page to show the user.
    pub async fn load_setlist(&self, songs: Vec<LibrarySong>) -> eyre::Result<Vec<SongLoadReport>> {
        // 1. Close every currently-open project — whatever was there
        //    before, `build_from_open_projects` below only ever sees
        //    "every project currently open," so this is what scopes the
        //    rebuild to just the new setlist.
        for info in Projects::list(&self.standalone) {
            Projects::close(&self.standalone, &info.guid);
        }
        // Stale guids from the previous setlist would otherwise linger
        // in these maps forever (closed projects never get an explicit
        // teardown notice) — a fresh setlist starts both fresh.
        self.song_folders.lock().unwrap().clear();
        self.materialized.lock().unwrap().clear();

        // 2. Load each song's structure (fast — no audio decode here).
        let mut reports = Vec::with_capacity(songs.len());
        let mut first_guid: Option<String> = None;
        for song in songs {
            let rpp_text = live_bus::build_live_rpp(&song);
            let synthetic_path = song.folder.join(format!("{}.rpp", song.title));
            match load_rpp_text(
                &self.standalone,
                &song.title,
                synthetic_path.to_string_lossy().as_ref(),
                &rpp_text,
            ) {
                Ok(proj) => {
                    self.song_folders
                        .lock()
                        .unwrap()
                        .insert(proj.project_guid.clone(), song.folder.clone());
                    if first_guid.is_none() {
                        first_guid = Some(proj.project_guid.clone());
                    }
                    reports.push(SongLoadReport {
                        title: song.title.clone(),
                        track_count: proj.track_count,
                    });
                }
                Err(e) => {
                    tracing::warn!("failed to load {}: {e}", song.title);
                    reports.push(SongLoadReport {
                        title: song.title.clone(),
                        track_count: 0,
                    });
                }
            }
        }

        // 3. Focus the first song so the audio thread's poll picks it up,
        //    materializes ITS audio (only), and the setlist build below
        //    centers on song 0.
        if let Some(guid) = &first_guid {
            self.standalone.set_current_project(guid);
        }

        // 4. Rebuild the setlist structure from the newly-open projects
        //    (republishes `SetlistChanged` — the always-running
        //    `SessionEventBridge` picks it up with no changes needed there).
        self.client
            .build_from_open_projects()
            .await
            .map_err(|e| eyre::eyre!("build_from_open_projects: {e:?}"))?;

        // 5. Reset the cursor onto the new setlist's first song/section —
        //    the previous cursor position doesn't mean anything here.
        if first_guid.is_some() {
            self.client
                .seek_to_section(0, 0)
                .await
                .map_err(|e| eyre::eyre!("seek_to_section: {e:?}"))?;
        }

        Ok(reports)
    }
}

static ENGINE: OnceLock<SessionEngine> = OnceLock::new();

/// The engine, once [`bootstrap_blocking`] has succeeded.
pub fn engine() -> Option<&'static SessionEngine> {
    ENGINE.get()
}

/// Build the whole in-process stack before the UI launches. Blocking:
/// runs on a dedicated leaked runtime that then keeps hosting the
/// stream pumps, the memory-link acceptors, and the soft transport
/// clocks for the life of the process.
pub fn bootstrap_blocking() -> eyre::Result<()> {
    // 16 MiB worker stacks: vox 0.10's debug-build channel encode
    // recurses deeply on Setlist payloads and overflows tokio's default
    // 2 MiB workers (see session's standalone_setlist_harness).
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("fts-session-engine")
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()?;
    let rt: &'static tokio::runtime::Runtime = Box::leak(Box::new(rt));

    let engine = rt.block_on(bootstrap(rt.handle().clone()))?;

    // Session singleton for session-ui components (transport buttons,
    // sidebar seeks) — same client type the REAPER desktop installs.
    session_ui::Session::init(engine.client.clone())
        .map_err(|e| eyre::eyre!("Session::init: {e:?}"))?;

    ENGINE
        .set(engine)
        .map_err(|_| eyre::eyre!("session engine initialized twice"))?;
    Ok(())
}

async fn bootstrap(engine_rt: tokio::runtime::Handle) -> eyre::Result<SessionEngine> {
    // 1. Standalone backend — no projects seeded. The Home page's "Load &
    //    Play" is what puts real songs into it via `load_setlist`.
    let standalone = Standalone::new();

    // 2. In-process daw facade over a vox memory link. The setlist
    //    service's build/hydration path goes through `daw::get()`, so
    //    install the global facade exactly like the harness does.
    let bundle = build_in_process_daw(standalone.clone()).await?;
    // Dedicated current-thread runtime for `daw::block_on` (sync
    // contexts only — everything here is async). Kept separate so
    // block_on can't be called on the engine runtime from within it.
    let block_on_rt = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
    );
    daw::init_from_parts(bundle.daw.clone(), block_on_rt);

    // 3. The setlist service over the standalone backend.
    let setlist = SetlistServiceImpl::with_daw(standalone.clone());

    // 4. In-process RPC client (architect::LocalServer over a memory
    //    link) — the same conduit shape every remote uses.
    let router = daw::LayerRouter::new()
        .with(
            setlist_service_service_descriptor(),
            serve_setlist_service(setlist.clone()),
        )
        // The `#[subscribe]` stream sibling (events + active_indices), served
        // from the impl's PubSub hubs. Without this the stream client's
        // subscribe calls return `UnknownMethod`.
        .with(
            setlist_service_stream_service_descriptor(),
            setlist_service_stream_serve(setlist.clone()),
        );
    let scope = architect::Scope::new();
    let server = architect::LocalServer::serve(router, Arc::clone(&scope));
    let caller = server
        .caller()
        .await
        .map_err(|e| eyre::eyre!("local setlist caller: {e:?}"))?;
    let client = SetlistServiceClient::new(caller);

    // Stream client for the `#[subscribe]` streams (events + active_indices).
    // Subscriptions MUST be consumed through this client so the vox lane pumps
    // them — attaching a raw `vox::Tx` to the hub in-process is never drained.
    let stream_client = server
        .establish::<SetlistServiceStreamClient>()
        .await
        .map_err(|e| eyre::eyre!("local setlist stream client: {e:?}"))?;

    // Nothing is open yet — no initial build. The stream pumps start now
    // regardless, so the very first `load_setlist`'s `SetlistChanged`
    // republish (from `build_from_open_projects`) reaches the UI bridge.
    setlist.start_stream_pumps();

    // 5. Audio — graceful. cpal streams are !Send, so the engine lives on
    //    its own parked thread. Nothing is open yet; the thread waits for
    //    `load_setlist` to make a project current, and lazily decodes
    //    each song's audio the first time it becomes current (see the
    //    module doc).
    let song_folders: SongFolders = Arc::new(Mutex::new(HashMap::new()));
    let materialized: Materialized = Arc::new(Mutex::new(HashSet::new()));
    spawn_audio_thread(
        standalone.clone(),
        song_folders.clone(),
        materialized.clone(),
        engine_rt,
    );

    Ok(SessionEngine {
        setlist,
        client,
        stream_client,
        standalone,
        song_folders,
        materialized,
        _daw: bundle,
        _scope: scope,
    })
}

/// Open the default cpal output and let the audio callback drive the playhead
/// of the ACTIVE project — re-attaching whenever the current project changes.
///
/// Per-song-project model: each song is its own standalone project, and the
/// audio engine renders exactly one project's graph. So the engine must FOLLOW
/// the active song: when the user seeks to another song (which selects that
/// song's project), drop the current engine and attach to the new project.
/// `attach_audio_engine` disables the attached project's soft clock (the audio
/// callback drives `advance()` instead); on the project we switch AWAY from we
/// re-enable the soft clock so its playhead still responds to seeks.
///
/// The re-attach on a song switch happens between songs, never mid-render, so
/// the brief device close/reopen is inaudible during a song.
///
/// It ALSO supervises the device: if the output stream dies mid-playback (e.g.
/// PipeWire drops the connection), the engine's error callback re-enables the
/// soft clock immediately (the playhead never freezes) and latches
/// `stream_errored()`. This loop notices that and reopens the device so audio
/// returns on its own — with backoff so a persistently-missing device doesn't
/// spam re-open attempts.
fn spawn_audio_thread(
    standalone: Standalone,
    song_folders: SongFolders,
    materialized: Materialized,
    rt: tokio::runtime::Handle,
) {
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let result = std::thread::Builder::new()
        .name("fts-audio".into())
        .spawn(move || {
            // Enter the engine runtime: transport_engine_for lazily
            // spawns the per-project soft clock task.
            let _rt_guard = rt.enter();

            // Decode a song's audio the first time it's about to become
            // current — see the module doc. A no-op for a guid that's
            // already decoded, or one `load_setlist` never registered a
            // folder for (nothing to resolve stems against).
            let materialize_if_needed = |guid: &str| {
                if materialized.lock().unwrap().contains(guid) {
                    return;
                }
                let Some(folder) = song_folders.lock().unwrap().get(guid).cloned() else {
                    return;
                };
                standalone
                    .media_bay()
                    .set_file_resolver(Box::new(ProjectRelativeResolver::new(folder)));
                match materialize_via_bay(&standalone, guid) {
                    Ok(report) => {
                        tracing::info!(
                            "decoded audio for '{guid}': {} source(s), {} failed",
                            report.loaded,
                            report.failed.len()
                        );
                        for (take, err) in &report.failed {
                            tracing::warn!("  ! {take}: {err}");
                        }
                    }
                    Err(e) => tracing::warn!("materialize '{guid}': {e}"),
                }
                materialized.lock().unwrap().insert(guid.to_string());
            };

            // Attach to a project: returns the live engine on success. On
            // failure, re-enable that project's soft clock so play still
            // advances the playhead silently.
            let attach = |guid: &str| -> Option<daw_standalone::audio_engine::AudioEngine> {
                materialize_if_needed(guid);
                match standalone.attach_audio_engine(guid) {
                    Ok(engine) => {
                        // Guide (click / count-in / section cues): built at the
                        // device rate, mixed in via the aux post-render hook.
                        crate::guide::install(&engine);
                        tracing::info!("audio engine attached to project '{guid}'");
                        Some(engine)
                    }
                    Err(e) => {
                        tracing::warn!(
                            "no audio output for project '{guid}' ({e}); transport runs silently"
                        );
                        standalone
                            .transport_engine_for(guid)
                            .soft_clock_enabled
                            .store(true, Ordering::SeqCst);
                        None
                    }
                }
            };

            // Nothing is open at boot — `attached_guid` starts empty (never
            // a real project guid) so the first real `current()` is always
            // treated as a switch.
            let mut attached_guid = String::new();
            let mut engine: Option<daw_standalone::audio_engine::AudioEngine> = None;
            // Ticks (×150 ms) to wait before retrying after a failed/absent
            // device, so a persistently-missing output doesn't spam re-opens.
            // Reset to 0 on success; capped so recovery stays reasonably prompt.
            let mut retry_backoff = 0u32;

            // Follow the active project + supervise the device. Polling (not a
            // subscription) keeps the audio thread self-contained; 150 ms is
            // well below song-switch cadence and adds no cost while a song plays.
            loop {
                std::thread::sleep(Duration::from_millis(150));
                let current = standalone.current().map(|p| p.guid);

                let Some(current) = current else {
                    // Nothing loaded (yet, or a setlist was just torn down
                    // mid-reload) — make sure no stale device stays open.
                    if engine.take().is_some() {
                        tracing::info!("no active project; audio engine detached");
                    }
                    attached_guid.clear();
                    continue;
                };

                // 1. Song switch: re-attach to the newly-active project.
                if current != attached_guid {
                    tracing::info!(
                        "active project changed '{attached_guid}' → '{current}'; re-attaching audio"
                    );
                    // Restore the soft clock on the project we're leaving so its
                    // playhead still moves on seeks while it's not the audio target
                    // (a no-op the first time, when `attached_guid` is empty).
                    if !attached_guid.is_empty() {
                        standalone
                            .transport_engine_for(&attached_guid)
                            .soft_clock_enabled
                            .store(true, Ordering::SeqCst);
                    }
                    // Drop the old engine (closes its cpal stream) BEFORE opening
                    // the new one so the single output device is free.
                    drop(engine.take());
                    engine = attach(&current);
                    attached_guid = current;
                    retry_backoff = 0;
                    continue;
                }

                // 2. Device died on the current project: the engine already
                //    re-enabled the soft clock (playhead keeps moving); reopen
                //    the device so audio comes back.
                let dead = engine.as_ref().map(|e| e.stream_errored()).unwrap_or(true);
                if dead {
                    if retry_backoff > 0 {
                        retry_backoff -= 1;
                        continue;
                    }
                    if engine.is_some() {
                        tracing::warn!(
                            "audio stream on '{attached_guid}' died; reopening output device"
                        );
                    }
                    drop(engine.take()); // close the dead stream before reopening
                    engine = attach(&attached_guid);
                    // On success clear backoff; on failure wait ~2s before retry.
                    retry_backoff = if engine.is_some() { 0 } else { 13 };
                }
            }
        });
    if let Err(e) = result {
        tracing::warn!("could not spawn audio thread: {e}");
    }
}
