//! In-process session engine — the daw-standalone setlist player.
//!
//! Embeds the session domain plus a `daw-standalone` backend so the
//! setlist is DATA this app can PLAY — transport over songs/sections
//! without REAPER. Construction replicates session's
//! `standalone_setlist_harness` (the proven REAPER-free path):
//!
//! 1. `Standalone::new()` + `seed_project` + `stamp_demo_setlist_with`
//!    seeds a playable 3-song demo setlist into the in-memory project.
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
//! audio callback drives the playhead sample-accurately. If no device
//! is available the soft clock is re-enabled and transport runs
//! silently — the player still works.

use std::sync::{Arc, OnceLock};

use daw::service::ProjectInfo;
use daw_standalone::bootstrap::{InProcessDaw, build_in_process_daw};
use daw_standalone::sync::Standalone;
use session::services::setlist_service::{
    SetlistServiceStreamClient, setlist_service_stream_service_descriptor,
    stream_serve as setlist_service_stream_serve,
};
use session::setlist_service::demo::stamp_demo_setlist_with;
use session::{
    SetlistServiceClient, SetlistServiceImpl, serve_setlist_service,
    setlist_service_service_descriptor,
};

/// Project GUID the demo setlist is stamped into.
const DEMO_PROJECT_GUID: &str = "fts-demo";

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
    /// Keeps the daw-facade memory link's acceptor alive.
    _daw: InProcessDaw,
    /// Keeps the setlist RPC LocalServer's acceptor + lanes alive.
    _scope: Arc<architect::Scope>,
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

/// Elevation Worship "Praise" multitrack stems, grouped for the mixer. The
/// audio lives outside the repo; if the folder is absent we skip seeding and
/// the setlist still loads (markers/sections only). Stems are mmap'd, so this
/// is near-instant even for 23×78 MB files.
fn seed_praise_media(standalone: &Standalone, project_guid: &str) {
    use daw_standalone::media_seed::{StemSpec, seed_media_tracks};

    // (display name, filename, group)
    const STEMS: &[(&str, &str, &str)] = &[
        ("Click", "01 - Click.wav", "Guide"),
        ("Cue", "02 - Cue.wav", "Guide"),
        (
            "Original Track",
            "03 - Elevation Worship - Praise (Original Track).wav",
            "Reference",
        ),
        ("BGVS", "04 - BGVS.wav", "Vocals"),
        ("BGVS 2", "05 - BGVS 2.wav", "Vocals"),
        ("Choir", "06 - Choir.wav", "Vocals"),
        ("Organ", "07 - Organ.wav", "Keys"),
        ("Keys", "08 - Keys.wav", "Keys"),
        ("Piano", "09 - Piano.wav", "Keys"),
        ("Electric Bass 1", "10 - Electric Bass 1.wav", "Bass"),
        ("Electric Bass 2", "11 - Electric Bass 2.wav", "Bass"),
        ("Synth Bass", "12 - Synth Bass.wav", "Bass"),
        ("Acoustic Guitar", "13 - Acoustic Guitar.wav", "Guitars"),
        ("Electric Guitar 1", "14 - Electric Guitar 1.wav", "Guitars"),
        ("Electric Guitar 2", "15 - Electric Guitar 2.wav", "Guitars"),
        ("Electric Guitar 3", "16 - Electric Guitar 3.wav", "Guitars"),
        ("Electric Guitar 4", "17 - Electric Guitar 4.wav", "Guitars"),
        ("Electric Guitar 5", "18 - Electric Guitar 5.wav", "Guitars"),
        ("Electric Guitar 6", "19 - Electric Guitar 6.wav", "Guitars"),
        ("Electric Guitar 7", "20 - Electric Guitar 7.wav", "Guitars"),
        ("Loop", "21 - Loop.wav", "Tracks"),
        ("Hand Percussion", "22 - Hand Percussion.wav", "Percussion"),
        ("Percussion", "23 - Percussion.wav", "Percussion"),
    ];

    // Resolve the stems dir: env override, else the known Downloads location.
    let dir = std::env::var("FTS_PRAISE_STEMS").map(std::path::PathBuf::from).unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_default();
        std::path::PathBuf::from(home).join(
            "Downloads/Elevation Worship - Praise-20260712T200150Z-2-001/Elevation Worship - Praise/- MultiTracks",
        )
    });
    if !dir.is_dir() {
        tracing::info!("Praise stems not found at {dir:?} — seeding markers only");
        return;
    }

    let stems: Vec<StemSpec> = STEMS
        .iter()
        .map(|(name, file, group)| {
            StemSpec::new(*name, dir.join(file).to_string_lossy().to_string(), Some(group))
        })
        .collect();

    // Praise is the first song: region starts at t=0, stems aligned to it.
    // Length generously covers the ~276 s song (render clips to source frames).
    let report = seed_media_tracks(standalone, project_guid, &stems, 0.0, 300.0);
    tracing::info!(
        "seeded Praise media: {} tracks / {} folders, {} sources loaded, {} failed",
        report.tracks_created,
        report.folders_created,
        report.materialize.loaded,
        report.materialize.failed.len(),
    );
}

async fn bootstrap(engine_rt: tokio::runtime::Handle) -> eyre::Result<SessionEngine> {
    // 1. Standalone backend seeded with a playable demo setlist
    //    (3 songs with count-ins, sections, markers — see
    //    session::setlist_service::demo).
    let standalone = Standalone::new();
    let guid = standalone.seed_project(ProjectInfo {
        guid: DEMO_PROJECT_GUID.into(),
        name: "FTS Demo Setlist".into(),
        path: String::new(),
    });
    stamp_demo_setlist_with(&standalone).map_err(|e| eyre::eyre!("stamp demo setlist: {e:?}"))?;
    tracing::info!("demo setlist stamped into standalone project '{guid}'");

    // Seed the real Praise multitrack stems as grouped, playable tracks (when
    // the audio is present on this machine — the demo still works without it).
    seed_praise_media(&standalone, &guid);

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

    // Initial build from the seeded project. Later builds (UI-driven)
    // republish through the hub.
    client
        .build_from_open_projects()
        .await
        .map_err(|e| eyre::eyre!("build_from_open_projects: {e:?}"))?;
    tracing::info!("setlist built from standalone project");

    // Start the `#[subscribe]` stream pumps AFTER the build so the events
    // pump captures the populated setlist (its song→transport mapping is
    // snapshotted once at start; starting before the build leaves it empty
    // and no TransportUpdate is ever emitted → a frozen playhead in the UI).
    setlist.start_stream_pumps();

    // Dev/verification affordance: FTS_AUTOPLAY=1 starts the transport
    // immediately after the setlist is built (e.g. for headless-ish
    // audio smoke runs).
    if std::env::var("FTS_AUTOPLAY").map(|v| v == "1").unwrap_or(false) {
        // Demo stamping leaves the edit cursor at the timeline end —
        // rewind to the first song's first measure before rolling.
        if let Err(e) = client.goto_measure(0, 0).await {
            tracing::warn!("FTS_AUTOPLAY rewind failed: {e:?}");
        }
        match client.play().await {
            Ok(_) => tracing::info!("FTS_AUTOPLAY=1: transport started"),
            Err(e) => tracing::warn!("FTS_AUTOPLAY play failed: {e:?}"),
        }
    }

    // 5. Audio — graceful. cpal streams are !Send, so the engine lives
    //    on its own parked thread. On failure the soft clock is
    //    re-enabled and the transport runs silently.
    spawn_audio_thread(standalone.clone(), guid, engine_rt);

    Ok(SessionEngine {
        setlist,
        client,
        stream_client,
        standalone,
        _daw: bundle,
        _scope: scope,
    })
}

/// Try to open the default cpal output and let the audio callback drive
/// the playhead. `attach_audio_engine` disables the project's soft
/// clock before opening the device, so on failure we re-enable it —
/// transport keeps advancing (timer-driven) with no audio.
fn spawn_audio_thread(standalone: Standalone, guid: String, rt: tokio::runtime::Handle) {
    let result = std::thread::Builder::new()
        .name("fts-audio".into())
        .spawn(move || {
            // Enter the engine runtime: transport_engine_for lazily
            // spawns the per-project soft clock task.
            let _rt_guard = rt.enter();
            match standalone.attach_audio_engine(&guid) {
                Ok(engine) => {
                    tracing::info!("audio engine attached (default cpal output)");
                    // Guide (click / count-in / section cues): built at the
                    // device rate and mixed into the output via the aux
                    // post-render hook.
                    crate::guide::install(&engine);
                    // The cpal stream lives on this thread; park forever.
                    loop {
                        std::thread::park();
                    }
                }
                Err(e) => {
                    tracing::warn!("no audio output ({e}); transport runs silently");
                    // attach disabled the soft clock before failing —
                    // restore it so play still advances the playhead.
                    standalone
                        .transport_engine_for(&guid)
                        .soft_clock_enabled
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
        });
    if let Err(e) = result {
        tracing::warn!("could not spawn audio thread: {e}");
    }
}
