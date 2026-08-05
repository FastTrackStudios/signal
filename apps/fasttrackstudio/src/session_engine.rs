//! In-process session engine — the daw-standalone setlist player.
//!
//! Embeds the session domain plus a `daw-standalone` backend so the
//! setlist is DATA this app can PLAY — transport over songs/sections
//! without REAPER. Construction replicates session's
//! `standalone_setlist_harness` (the proven REAPER-free path):
//!
//! 1. `Standalone::new()` + one `seed_project` per song +
//!    `stamp_song_with_default_tempo_native` seeds a playable demo setlist as
//!    ONE standalone project per song (each 0-based, its own tempo/time-sig).
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
use session::setlist::service::demo::{demo_songs_base, stamp_song_with_default_tempo_native};
use session::{
    SetlistServiceClient, SetlistServiceImpl, serve_setlist_service,
    setlist_service_service_descriptor,
};

/// GUID for the demo song at setlist index `i` (one project per song).
fn demo_song_guid(i: usize) -> String {
    format!("demo-song-{i:02}")
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
fn seed_praise_media(
    standalone: &Standalone,
    project_guid: &str,
    start: f64,
    len: f64,
) -> daw_standalone::media_seed::SeedReport {
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
        return daw_standalone::media_seed::SeedReport::default();
    }

    let stems: Vec<StemSpec> = STEMS
        .iter()
        .map(|(name, file, group)| {
            StemSpec::new(*name, dir.join(file).to_string_lossy().to_string(), Some(group))
        })
        .collect();

    let report = seed_media_tracks(standalone, project_guid, &stems, start, len);
    tracing::info!(
        "seeded Praise media: {} tracks / {} folders, {} sources loaded, {} failed",
        report.tracks_created,
        report.folders_created,
        report.materialize.loaded,
        report.materialize.failed.len(),
    );
    report
}

/// A rough instrument-family group for a stem, from its (prefix-stripped) name.
/// Cosmetic — drives the mixer's folder buses; approximate is fine for the demo.
fn group_for(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_lowercase();
    let any = |ks: &[&str]| ks.iter().any(|k| n.contains(k));
    if any(&["click", "cue", "guide"]) {
        Some("Guide")
    } else if any(&["loop"]) {
        Some("Tracks")
    } else if any(&["bass"]) {
        Some("Bass")
    } else if any(&["drum", "perc"]) {
        Some("Drums")
    } else if any(&["organ", "key", "piano", "rhodes", "synth", "pad"]) {
        Some("Keys")
    } else if any(&["guitar", "gtr", "acoustic", "electric", "ag", "eg"]) {
        Some("Guitars")
    } else if any(&["vocal", "vox", "bgv", "choir"]) {
        Some("Vocals")
    } else if any(&["sax", "horn", "brass", "string", "orch"]) {
        Some("Orchestra")
    } else if any(&["fx"]) {
        Some("FX")
    } else {
        None
    }
}

/// Seed a song's stems by globbing its folder's `*.wav` files. Display names
/// strip a leading "Song - " prefix ("Holy Forever - EG 2" → "EG 2"); groups
/// are inferred from the name. Missing folder ⇒ skip (markers-only).
fn seed_globbed_media(
    standalone: &Standalone,
    project_guid: &str,
    song: &str,
    dir: &std::path::Path,
    start: f64,
    len: f64,
) -> daw_standalone::media_seed::SeedReport {
    use daw_standalone::media_seed::{SeedReport, StemSpec, seed_media_tracks};

    if !dir.is_dir() {
        tracing::info!("{song} stems not found at {dir:?} — seeding markers only");
        return SeedReport::default();
    }
    let mut wavs: Vec<std::path::PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .map(|x| x.eq_ignore_ascii_case("wav"))
                    .unwrap_or(false)
            })
            .collect(),
        Err(e) => {
            tracing::warn!("{song}: read stems dir {dir:?}: {e}");
            return SeedReport::default();
        }
    };
    wavs.sort();
    let stems: Vec<StemSpec> = wavs
        .iter()
        .map(|p| {
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            // "Holy Forever - EG 2" → "EG 2"; "01 - Click" → "Click".
            let name = stem.rsplit(" - ").next().unwrap_or(&stem).trim().to_string();
            let group = group_for(&name);
            StemSpec::new(name, p.to_string_lossy().to_string(), group)
        })
        .collect();
    if stems.is_empty() {
        return SeedReport::default();
    }
    let report = seed_media_tracks(standalone, project_guid, &stems, start, len);
    tracing::info!(
        "seeded {song} media: {} tracks / {} folders, {} sources loaded, {} failed",
        report.tracks_created,
        report.folders_created,
        report.materialize.loaded,
        report.materialize.failed.len(),
    );
    report
}

/// On-disk folder (under `FTS_WORSHIP_STEMS`) for each globbed worship song.
const WORSHIP_FOLDERS: &[(&str, &str)] = &[
    ("God, I'm Just Grateful", "God, I_m Just Grateful _ Elevation Worship _ D"),
    ("Holy Forever", "Holy Forever _ Bethel Music _ Bb"),
    ("Thank God I'm Free", "Thank God I_m Free _ Elevation Rhythm _ E"),
    ("Washed", "Washed _ Elevation Rhythm _ B"),
    ("Who Else", "Who Else _ Gateway Worship _ Ab"),
];

/// Seed one demo song's multitrack stems into ITS OWN project, 0-based (media
/// downbeat at t=0 so it lines up with the project's own song-start marker).
/// Praise uses its curated grouped stem table; the others glob their folder.
/// Missing audio is skipped (the song still loads, markers/sections only).
fn seed_song_media(standalone: &Standalone, project_guid: &str, name: &str, len: f64) {
    // Every song lives on its own project timeline starting at 0.
    let start = 0.0;
    let report = if name == "Praise" {
        seed_praise_media(standalone, project_guid, start, len)
    } else if let Some((_, folder)) = WORSHIP_FOLDERS.iter().find(|(n, _)| *n == name) {
        let base = std::env::var("FTS_WORSHIP_STEMS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default())
                    .join("Downloads/Worship MultiTracks")
            });
        seed_globbed_media(standalone, project_guid, name, &base.join(folder), start, len)
    } else {
        return;
    };
    decorate_seeded_tracks(standalone, project_guid, &report);
}

/// After seeding, colour every track/folder by its instrument family and mute
/// the multitrack's own baked-in Click/Guide/Cue/Count stems by default — we
/// synthesize our own click + guide via the guide engine, so the shipped ones
/// must be silent (they stay in the project, visible). Runs on the native
/// `Tracks` trait (in-process, no RPC — this is before the daw facade exists).
fn decorate_seeded_tracks(
    standalone: &Standalone,
    project_guid: &str,
    report: &daw_standalone::media_seed::SeedReport,
) {
    use daw::service::{ProjectContext, TrackRef, Tracks};

    let ctx = ProjectContext::Project(project_guid.to_string());
    let mut muted = 0usize;
    for t in &report.tracks {
        let color = category_color(t.group.as_deref(), &t.name);
        let _ = Tracks::set_color(standalone, ctx.clone(), TrackRef::Guid(t.guid.clone()), color);
        if is_click_or_guide(t.group.as_deref(), &t.name) {
            let _ =
                Tracks::set_muted(standalone, ctx.clone(), TrackRef::Guid(t.guid.clone()), true);
            muted += 1;
        }
    }
    if muted > 0 {
        tracing::info!("{project_guid}: default-muted {muted} baked-in click/guide stem(s)");
    }
}

/// `true` when a track's name or group marks it as the multitrack's own
/// click/guide/cue/count — the stems we replace with our synthesized guide.
fn is_click_or_guide(group: Option<&str>, name: &str) -> bool {
    let hay = format!("{} {}", group.unwrap_or(""), name).to_ascii_lowercase();
    ["click", "guide", "cue", "count"]
        .iter()
        .any(|k| hay.contains(k))
}

/// Worship-palette colour (0xRRGGBB) for a track, by group first then name.
fn category_color(group: Option<&str>, name: &str) -> u32 {
    let hay = format!("{} {}", group.unwrap_or(""), name).to_ascii_lowercase();
    let has = |ks: &[&str]| ks.iter().any(|k| hay.contains(k));
    // Order matters: click/guide before the instrument families.
    if has(&["click", "guide", "cue", "count"]) {
        0x6B_7280 // muted gray
    } else if has(&["reference", "original", "master mix"]) {
        0x64_748B // slate (reference/original track)
    } else if has(&["vocal", "vox", "bgv", "choir", "lead"]) {
        0xD4_A017 // gold
    } else if has(&["drum", "kick", "snare", "tom", "hat", "cymbal", "perc"]) {
        0xC0_2942 // crimson
    } else if has(&["bass"]) {
        0x7C_3AED // purple
    } else if has(&["guitar", "gtr", " ag", " eg", "acoustic", "electric"]) {
        0xEA_7317 // orange
    } else if has(&["synth", "pad"]) {
        0x0E_9488 // teal
    } else if has(&["key", "piano", "organ", "rhodes", "wurli"]) {
        0x0025_63EB // blue
    } else if has(&["fx", "loop", "track", "swell", "riser"]) {
        0x4B_6B5A // gray-green
    } else if has(&["string", "orch", "horn", "brass", "sax"]) {
        0xB2_5C2E // burnt orange (orchestra)
    } else {
        0x52_5252 // neutral fallback
    }
}

async fn bootstrap(engine_rt: tokio::runtime::Handle) -> eyre::Result<SessionEngine> {
    // 1. Standalone backend seeded with a playable demo setlist — ONE project
    //    per song (each 0-based: count-in near t=0, first downbeat at that
    //    song's measure 1, its own default tempo/time-signature). Project names
    //    are zero-padded (`00 Praise`, `01 …`) because `Projects::list()` sorts
    //    by name and `build_from_open_projects` follows that order — the padding
    //    keeps the setlist in authored order (see session demo::ORDER).
    let standalone = Standalone::new();
    // Built-in FX (EQ/comp/reverb/…): `Effects::add("Reverb")` on any
    // session track instantiates real signal-fx DSP that the render
    // path processes in that track's chain.
    standalone.set_fx_factory(std::sync::Arc::new(signal_fx::NativeFxFactory));
    let songs = demo_songs_base();
    let mut song_guids: Vec<String> = Vec::with_capacity(songs.len());
    for (i, song) in songs.iter().enumerate() {
        let guid = demo_song_guid(i);
        standalone.seed_project(ProjectInfo {
            guid: guid.clone(),
            name: format!("{i:02} {}", song.name),
            path: String::new(),
        });
        stamp_song_with_default_tempo_native(
            &standalone,
            daw::service::ProjectContext::Project(guid.clone()),
            song,
        )
        .map_err(|e| eyre::eyre!("stamp song {} ({}): {e:?}", i, song.name))?;
        // Attach the bundled keyflow chart to the project (ext-state
        // `FTS/chart_text`) so setlist hydration serves it to remotes —
        // the browser chart pane renders from exactly this text.
        if let Some(chart) = session::setlist::service::demo::demo_chart_for(song.name) {
            use daw::service::ExtState as _;
            standalone
                .set_project(
                    daw::service::ProjectContext::Project(guid.clone()),
                    session::setlist::service::CHART_EXT_STATE_SECTION,
                    session::setlist::service::CHART_EXT_STATE_KEY,
                    chart,
                )
                .map_err(|e| eyre::eyre!("stamp chart for {}: {e:?}", song.name))?;
        }
        // Seed this song's multitrack stems 0-based into its own project (when
        // the audio is present on this machine — the demo still works without).
        let len = song.region_end - song.region_start;
        seed_song_media(&standalone, &guid, song.name, len);
        song_guids.push(guid);
    }
    // Focus the first song's project so the initial build centers on song 0.
    let first_guid = song_guids
        .first()
        .cloned()
        .ok_or_else(|| eyre::eyre!("demo setlist produced no songs"))?;
    standalone.set_current_project(&first_guid);
    tracing::info!(
        "demo setlist stamped into {} per-song projects ('{}' … )",
        song_guids.len(),
        first_guid,
    );
    let guid = first_guid;

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
fn spawn_audio_thread(standalone: Standalone, initial_guid: String, rt: tokio::runtime::Handle) {
    use daw::service::Projects;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let result = std::thread::Builder::new()
        .name("fts-audio".into())
        .spawn(move || {
            // Enter the engine runtime: transport_engine_for lazily
            // spawns the per-project soft clock task.
            let _rt_guard = rt.enter();

            // Attach to a project: returns the live engine on success. On
            // failure, re-enable that project's soft clock so play still
            // advances the playhead silently.
            let attach = |guid: &str| -> Option<daw_standalone::audio_engine::AudioEngine> {
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

            let mut attached_guid = initial_guid.clone();
            // Holds the live cpal stream for the currently-attached project;
            // dropping it (on switch) closes the device before we reopen it.
            let mut engine = attach(&initial_guid);
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

                // 1. Song switch: re-attach to the newly-active project.
                if let Some(current) = current
                    && current != attached_guid
                {
                    tracing::info!(
                        "active project changed '{attached_guid}' → '{current}'; re-attaching audio"
                    );
                    // Restore the soft clock on the project we're leaving so its
                    // playhead still moves on seeks while it's not the audio target.
                    standalone
                        .transport_engine_for(&attached_guid)
                        .soft_clock_enabled
                        .store(true, Ordering::SeqCst);
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
