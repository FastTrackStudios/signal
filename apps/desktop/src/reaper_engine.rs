//! Recording Mode — drive a real, running REAPER instead of playing the
//! setlist ourselves.
//!
//! Live Mode ([`crate::session_engine`]) embeds a `daw-standalone` backend
//! and plays the setlist's audio itself. Recording Mode is the same
//! `SetlistService` wire surface — same `SetlistServiceClient` type, same
//! `session_ui::Session::init` call, same performance view — just dialed
//! at a REAPER extension's `SetlistServiceImpl<daw_reaper::Reaper>`
//! instead of an in-process one. `fts-extensions` already mounts that
//! service (`session::daw_services::layer_services_with_daw`) — along
//! with every other `daw::service` trait `Reaper` implements, via
//! `Reaper.into_router()` — onto the `LayerRouter` its REAPER-hosted
//! `daw-reaper` publishes over a local Unix-domain-socket vox link
//! (`/tmp/fts-daw-{pid}.sock`).
//!
//! Two client connections share that one socket, opened separately
//! because they need different client types:
//! - [`daw::cli`] (already-written REAPER launcher + socket discovery +
//!   the generic `Projects`/`Tracks`/etc. surface) handles "find or
//!   launch REAPER" and "open each song's project".
//! - This module's own [`connect`] opens the `SetlistService` lanes
//!   directly — `daw::cli`'s `Daw` type doesn't expose those (they're
//!   session's own services, not core `daw` ones).
//!
//! No audio thread here: REAPER is the one making sound.

use std::path::{Path, PathBuf};

use session::SetlistServiceClient;
use session::services::setlist_service::SetlistServiceStreamClient;
use session_vault_sync::library::LibrarySong;

const SOCKET_DIR: &str = "/tmp";
const SOCKET_PREFIX: &str = "fts-daw-";
const SOCKET_SUFFIX: &str = ".sock";

/// The dev-rig REAPER profile (`~/fts-dev`, per `daw::cli::daw_profiles()`)
/// — the one this whole workflow (organizing songs, testing Recording
/// Mode) has been running against. A profile picker is future work; this
/// is the one that matters today.
const REAPER_PROFILE: &str = "fts-dev";

/// A live REAPER this app could attach to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveReaper {
    /// The REAPER process's pid, as encoded in its socket filename.
    pub pid: u32,
    /// Its DAW socket, ready to hand to [`connect_to`].
    pub socket: PathBuf,
}

/// Every live REAPER publishing a DAW socket, newest first.
///
/// Stale sockets (REAPER exited without cleanup) are skipped, not deleted;
/// this app has no business unlinking another process's files.
///
/// The extension behind the socket is not identified here, and can't be:
/// the filename carries a pid and nothing else. Anything mounting
/// `daw-reaper` publishes one — `fts-extensions` in normal use,
/// `session-extension` under the test harness — and both mount
/// `session::daw_services::layer_services_with_daw`, so either is a valid
/// target. A REAPER whose extension *doesn't* mount `SetlistService` is
/// only discoverable as a connect that opens the lane and fails, which
/// [`connect_to`] reports as a handshake error.
#[must_use]
pub fn discover_all() -> Vec<LiveReaper> {
    let Ok(entries) = std::fs::read_dir(SOCKET_DIR) else {
        return Vec::new();
    };
    let mut sockets: Vec<LiveReaper> = entries
        .filter_map(|entry| {
            let socket = entry.ok()?.path();
            let filename = socket.file_name()?.to_str()?;
            let pid: u32 = filename
                .strip_prefix(SOCKET_PREFIX)?
                .strip_suffix(SOCKET_SUFFIX)?
                .parse()
                .ok()?;
            process_alive(pid).then_some(LiveReaper { pid, socket })
        })
        .collect();
    sockets.sort_by_key(|s| std::cmp::Reverse(s.pid));
    sockets
}

/// The most recently started live REAPER's socket, if any — the highest
/// pid among sockets whose owning process is still alive.
fn discover_socket() -> Option<PathBuf> {
    discover_all().into_iter().next().map(|s| s.socket)
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    // Signal 0: no-op existence check (see `kill(2)`) — this never
    // actually signals the process, just probes whether it's ours to see.
    unsafe { libc::kill(i32::try_from(pid).unwrap_or(-1), 0) == 0 }
}

/// A live connection to a REAPER extension's `SetlistService`. Keeps
/// `session_ui`'s `Session` singleton pointed at real REAPER for the rest
/// of the process — dropping this would close the link, so it's leaked
/// into a static the same way [`crate::session_engine::SessionEngine`] is.
pub struct ReaperEngine {
    pub client: SetlistServiceClient,
    pub stream_client: SetlistServiceStreamClient,
    pub socket: PathBuf,
    /// Keeps the vox connection (and its socket) open for the engine's
    /// lifetime — both clients above borrow it implicitly through their
    /// lanes, and dropping it tears down the whole link.
    _connection: vox_core::ConnectionHandle,
}

/// Reason Recording Mode couldn't attach — surfaced to the mode-switch UI
/// rather than just logged, since "REAPER isn't running" is the expected
/// first thing a user sees, not a bug report.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("no REAPER socket found in /tmp — is REAPER running with the FTS extension loaded?")]
    NoSocket,
    #[error("connecting to {}: {source}", .path.display())]
    Connect {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("vox handshake with {}: {source:?}", .path.display())]
    Handshake {
        path: PathBuf,
        source: vox_core::ConnectionError,
    },
}

/// Connect to the most recently started live REAPER and hand back a
/// `SetlistService` client pair — the same client type
/// [`session_ui::Session::init`] takes in Live Mode, just backed by real
/// REAPER instead of the standalone player.
pub async fn connect() -> Result<ReaperEngine, ConnectError> {
    connect_to(&discover_socket().ok_or(ConnectError::NoSocket)?).await
}

/// [`connect`] against an explicit socket path — for the future connect
/// UI (picking among several sockets) and for tests that spawn their own
/// isolated REAPER instance.
pub async fn connect_to(socket: &Path) -> Result<ReaperEngine, ConnectError> {
    let stream = tokio::net::UnixStream::connect(socket)
        .await
        .map_err(|source| ConnectError::Connect {
            path: socket.to_path_buf(),
            source,
        })?;
    let link = vox_stream::StreamLink::unix(stream);
    let connection = vox_core::initiator_on(link)
        .establish_connection()
        .await
        .map_err(|source| ConnectError::Handshake {
            path: socket.to_path_buf(),
            source,
        })?;

    let client = connection
        .open_lane::<SetlistServiceClient>()
        .await
        .map_err(|source| ConnectError::Handshake {
            path: socket.to_path_buf(),
            source,
        })?;
    let stream_client = connection
        .open_lane::<SetlistServiceStreamClient>()
        .await
        .map_err(|source| ConnectError::Handshake {
            path: socket.to_path_buf(),
            source,
        })?;

    Ok(ReaperEngine {
        client,
        stream_client,
        socket: socket.to_path_buf(),
        _connection: connection,
    })
}

/// The live connection, if there is one.
///
/// An `RwLock` rather than a `OnceLock` because Recording Mode is now
/// connectable from the UI: REAPER usually isn't running when the app
/// starts, and a user who launches it then presses Connect must not have
/// to restart the app. Replaceable, not just settable, so a REAPER that
/// was quit and reopened (a new pid, a new socket) can be attached to in
/// the same session.
static ENGINE: std::sync::RwLock<Option<std::sync::Arc<ReaperEngine>>> =
    std::sync::RwLock::new(None);

/// The Recording Mode connection, if one is established. `None` in Live
/// Mode, before the first successful connect, or if REAPER isn't reachable.
///
/// Returns an owned handle: the connection can be replaced by a later
/// reconnect, so there is no `&'static` to hand out.
#[must_use]
pub fn engine() -> Option<std::sync::Arc<ReaperEngine>> {
    ENGINE.read().ok()?.clone()
}

/// Whether a connection is currently established — the cheap check, for
/// UI that only needs to know connected/not.
#[must_use]
pub fn is_connected() -> bool {
    ENGINE.read().is_ok_and(|e| e.is_some())
}

/// Attach to a running REAPER, from the UI, without blocking it.
///
/// The vox connection has to live on this module's own leaked runtime (it
/// hosts the link for the rest of the process), but the caller is a Dioxus
/// event handler on the UI thread — so the work is spawned there and the
/// outcome comes back over a oneshot the caller can await on whatever
/// runtime it likes.
///
/// The error is a `String` rather than [`ConnectError`]: by the time it
/// crosses this boundary it is a message to show someone, and the variants
/// carry non-`Send`-friendly vox internals that the UI has no use for.
pub fn spawn_connect(
    socket: Option<PathBuf>,
) -> tokio::sync::oneshot::Receiver<Result<(), String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    runtime().spawn(async move {
        let outcome = ensure_connected_to(socket)
            .await
            .map_err(|e| format!("{e:#}"));
        // A closed receiver just means the pane went away mid-connect; the
        // connection itself is already installed either way.
        let _ = tx.send(outcome);
    });
    rx
}

/// Bumped every time the connection state changes — attached, or lost.
///
/// The UI cannot poll `is_connected()` on its own: Dioxus re-renders on
/// signal changes, and this is a plain static. Anything that shows
/// connection state reads this to subscribe.
pub static CONNECTION_EPOCH: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn bump_epoch() {
    CONNECTION_EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// How often the supervisor checks whether the REAPER it is attached to is
/// still alive, and whether a new one has appeared to attach to.
const SUPERVISE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// Keep Recording Mode attached to *a* running REAPER, forever.
///
/// Two failures this handles, both of which used to leave the app wedged:
///
/// - **REAPER quits while connected.** Nothing noticed. `ENGINE` was only
///   ever written, never cleared, so `is_connected()` stayed true, the
///   workspace kept rendering a player wired to a dead vox link, and the
///   subscription futures just logged "stream ended" and exited. The user
///   saw a frozen setlist with no indication anything was wrong.
/// - **REAPER comes back.** Nothing re-dialled, so the only recovery was
///   restarting the app.
///
/// Liveness is a `kill(pid, 0)` on the socket's owning process, not an RPC
/// round trip: it needs no timeout policy, cannot block on a REAPER that is
/// alive but busy (loading a large project happily blocks its main thread
/// for seconds), and is the same probe [`discover_all`] already uses.
///
/// Reconnects are attempted whenever a socket is available and we hold none.
/// A failure is not fatal — REAPER publishes its socket slightly before the
/// extension has finished mounting its services, so the first dial after a
/// launch routinely loses that race and the next tick simply wins it.
pub fn spawn_supervisor() {
    runtime().spawn(async move {
        loop {
            tokio::time::sleep(SUPERVISE_INTERVAL).await;

            let attached = engine().and_then(|e| socket_pid(&e.socket));
            match supervise(
                attached,
                |pid| process_alive(pid),
                || discover_socket().is_some(),
            ) {
                Action::Idle => {}
                Action::Drop => {
                    tracing::warn!(pid = attached, "REAPER exited; dropping the connection");
                    disconnect();
                }
                Action::Attach => match ensure_connected_to(None).await {
                    Ok(()) => tracing::info!("reattached to REAPER"),
                    // Expected while REAPER is still coming up — it
                    // publishes its socket before the extension behind it
                    // has finished mounting services, so the first dial
                    // after a launch routinely loses that race and the next
                    // tick wins it.
                    Err(e) => tracing::debug!("reattach attempt failed: {e:#}"),
                },
            }
        }
    });
}

/// What one supervisor tick should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    /// Attached and healthy, or detached with nothing to attach to.
    Idle,
    /// The REAPER we were attached to is gone.
    Drop,
    /// We hold no connection and one is available.
    Attach,
}

/// The supervisor's decision, as a pure function of what it can observe.
///
/// Split out from the loop so it can be tested at all: the loop itself needs
/// a tokio runtime, a real vox handshake and a live REAPER, none of which a
/// unit test can produce — but every *decision* it makes is this.
fn supervise(
    attached: Option<u32>,
    alive: impl Fn(u32) -> bool,
    any_available: impl Fn() -> bool,
) -> Action {
    match attached {
        Some(pid) if alive(pid) => Action::Idle,
        // Attached to something that is gone. Drop before attaching: the
        // replacement is dialled on the *next* tick, so the UI passes
        // through a truthful "not connected" state rather than appearing to
        // hold a connection it doesn't have.
        Some(_) => Action::Drop,
        None if any_available() => Action::Attach,
        None => Action::Idle,
    }
}

/// Drop the current connection and tell the UI.
///
/// `session_ui::Session` is cleared too: it holds its own copy of the client,
/// and a workspace that kept rendering from it would be driving a dead link.
/// The vox connection itself closes when the last `Arc<ReaperEngine>` goes —
/// `ReaperEngine` owns the `ConnectionHandle`.
fn disconnect() {
    if let Ok(mut slot) = ENGINE.write() {
        *slot = None;
    }
    session_ui::Session::clear();
    // Recording Mode owns `daw::get()` while it is connected (Live Mode
    // installs its own and never reaches this path), so a dead REAPER should
    // not stay reachable through it — the armed-track poll waits for a live
    // one instead of reading a corpse.
    daw::rpc::Daw::clear();
    bump_epoch();
}

/// The pid encoded in a DAW socket's filename.
fn socket_pid(socket: &Path) -> Option<u32> {
    socket
        .file_name()?
        .to_str()?
        .strip_prefix(SOCKET_PREFIX)?
        .strip_suffix(SOCKET_SUFFIX)?
        .parse()
        .ok()
}

static RUNTIME: std::sync::OnceLock<std::sync::Arc<tokio::runtime::Runtime>> =
    std::sync::OnceLock::new();

fn runtime() -> &'static std::sync::Arc<tokio::runtime::Runtime> {
    RUNTIME.get_or_init(|| {
        std::sync::Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("fts-reaper-engine")
                .enable_all()
                .build()
                .expect("build the reaper-engine tokio runtime"),
        )
    })
}

/// Install `daw::get()`'s global singleton against this Recording Mode
/// connection, once. Live Mode installs it at boot
/// (`session_engine::bootstrap_blocking` -> `daw::init_from_parts`); this
/// module never did, so anything that self-connects through `daw::get()`
/// — `daw_ui::MixerPanel` (the app's actual Mixer tab), `mixer_view.rs`'s
/// "Open in REAPER" — silently found no DAW and rendered nothing in
/// Recording Mode. Idempotent: safe to call every time `ensure_reaper_running`
/// runs (app boot with REAPER already up, and again after `load_playlist`
/// launches it fresh).
fn install_daw_singleton(daw: daw::rpc::Daw) {
    // No early return when one is already installed: `daw::init_from_parts`
    // now *replaces* the global handle, which is exactly what a reconnect to
    // a restarted REAPER needs. Skipping it left the Mixer panel, "Open in
    // REAPER" and the armed-track poll all holding the dead connection after
    // a bounce — connected UI, silent underneath.
    daw::init_from_parts(daw, runtime().clone());
}

/// Connect to REAPER and install the client as session-ui's `Session`
/// singleton, if that hasn't already happened. Idempotent — a no-op once
/// `engine()` is `Some`, so it's safe to call both at boot (REAPER may
/// already be running from a previous session) and after
/// [`load_playlist`] launches/attaches to REAPER later.
async fn ensure_connected() -> eyre::Result<()> {
    ensure_connected_to(None).await
}

/// [`ensure_connected`] against a specific REAPER, or the newest one when
/// `socket` is `None`. The UI's Connect button picks a socket from
/// [`discover_all`]; boot and `load_playlist` pass `None`.
///
/// Already being connected wins over `socket`: this returns `Ok` without
/// touching an existing link, so it cannot be used to *switch* REAPERs. The
/// connect pane only appears while disconnected, so that never comes up
/// today — but a "attach to a different REAPER" affordance would need to
/// drop the current engine first, not just call this with another path.
async fn ensure_connected_to(socket: Option<PathBuf>) -> eyre::Result<()> {
    if is_connected() {
        return Ok(());
    }
    let engine = match socket {
        Some(ref path) => connect_to(path).await,
        None => connect().await,
    }
    .map_err(|e| eyre::eyre!("connect to REAPER: {e}"))?;

    // REAPER was already running (this is the "app started after REAPER"
    // path) — `load_playlist` isn't guaranteed to run again this session,
    // so this is the only chance to install `daw::get()`'s singleton
    // before the Mixer tab (`daw_ui::MixerPanel`) tries to self-connect.
    if let Ok(daw_connection) = daw::cli::connect(Some(engine.socket.clone())).await {
        install_daw_singleton(daw_connection.daw);
    }

    session_ui::Session::init(engine.client.clone())
        .map_err(|e| eyre::eyre!("Session::init: {e:?}"))?;

    // `--engine` mode's LAN control surface (engine_server.rs) serves
    // Recording Mode through this proxy — installed here, not there, so
    // it's ready the moment REAPER is regardless of whether the LAN
    // server is even running in this process (the GUI app installs it
    // too; it's cheap — two subscriber tasks — and unused if nothing ever
    // dials /vox).
    crate::reaper_lan_proxy::install(engine.client.clone(), engine.stream_client.clone());

    *ENGINE
        .write()
        .map_err(|_| eyre::eyre!("the reaper engine lock is poisoned"))? =
        Some(std::sync::Arc::new(engine));
    bump_epoch();
    Ok(())
}

/// The Recording Mode counterpart to
/// [`crate::session_engine::bootstrap_blocking`] — called once at app
/// startup. Unlike Live Mode, failure here is the *expected* common case
/// (REAPER usually isn't running yet): it just means the performance view
/// shows "not connected" until [`load_playlist`] launches REAPER and
/// connects for real. Blocking: runs on a dedicated leaked runtime that
/// then keeps hosting the vox connection for the life of the process.
pub fn bootstrap_blocking() -> eyre::Result<()> {
    // Leak the runtime handle now so it's ready for `load_playlist`'s
    // later async calls even if this first connect attempt fails.
    let rt = runtime();
    let outcome = rt.block_on(ensure_connected());
    // Start supervising regardless of whether that first attempt worked:
    // "REAPER isn't running yet" is the common case at app start, and the
    // supervisor is what turns that into "attaches by itself when it is".
    spawn_supervisor();
    match outcome {
        Ok(()) => Ok(()),
        Err(e) => {
            tracing::info!("REAPER not reachable yet ({e}); the supervisor will keep trying");
            Ok(())
        }
    }
}

/// Open every song's REAPER project as its own tab — launching REAPER
/// first (the `fts-dev` dev-rig profile) if nothing is running yet — then
/// rebuild the setlist from whatever's now open, exactly like Live Mode's
/// `SessionEngine::load_setlist` does for the standalone backend.
///
/// This is the "open the playlist/album in REAPER" entry point: each
/// `LibrarySong` names a folder holding an already-organized `.RPP` (the
/// same files `dynamic-template --apply-buses` / `convert-markers`
/// produce) — REAPER opens the real project, not a synthesized one.
pub async fn load_playlist(songs: &[LibrarySong]) -> eyre::Result<()> {
    let daw = ensure_reaper_running().await?;

    for song in songs {
        let Some(rpp_path) = find_rpp(&song.folder) else {
            tracing::warn!(
                "no .RPP found under {} for '{}'; skipping",
                song.folder.display(),
                song.title
            );
            continue;
        };
        daw.open_project(rpp_path.to_string_lossy().into_owned())
            .await
            .map_err(|e| eyre::eyre!("opening '{}' ({rpp_path:?}): {e:?}", song.title))?;
    }

    // REAPER is confirmed running now (we just opened projects on it) —
    // (re)connect the persistent SetlistService link + Session singleton
    // if `bootstrap_blocking` couldn't at app startup.
    ensure_connected().await?;
    let engine = engine().ok_or_else(|| {
        eyre::eyre!("REAPER is running but Recording Mode's own SetlistService link isn't up")
    })?;
    engine
        .client
        .build_from_open_projects()
        .await
        .map_err(|e| eyre::eyre!("build_from_open_projects: {e:?}"))?;
    engine
        .client
        .seek_to_section(0, 0)
        .await
        .map_err(|e| eyre::eyre!("seek_to_section: {e:?}"))?;
    Ok(())
}

/// Connect to a running REAPER, or launch the `fts-dev` profile and wait
/// for it to come up if nothing's listening yet.
async fn ensure_reaper_running() -> eyre::Result<daw::rpc::Daw> {
    if let Some(socket) = discover_socket() {
        let daw = daw::cli::connect(Some(socket)).await?.daw;
        install_daw_singleton(daw.clone());
        return Ok(daw);
    }
    tracing::info!("no live REAPER found; launching the '{REAPER_PROFILE}' profile");
    let (connection, pid, socket) = daw::cli::launch_and_connect(REAPER_PROFILE).await?;
    tracing::info!(pid, socket = %socket.display(), "REAPER launched");
    install_daw_singleton(connection.daw.clone());
    Ok(connection.daw)
}

/// The `.RPP` an already-organized song folder holds — there's exactly
/// one per song by convention (this repo's own `dynamic-template
/// --apply-buses` never produces more than one `.organized.RPP` per
/// input), but prefer an `.organized.RPP` over a bare one if both exist,
/// since that's the file this whole pipeline has been building toward.
fn find_rpp(folder: &Path) -> Option<PathBuf> {
    let entries: Vec<PathBuf> = std::fs::read_dir(folder)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("rpp"))
        })
        .collect();
    entries
        .iter()
        .find(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.ends_with(".organized"))
        })
        .or_else(|| entries.first())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn socket(pid: u32) -> PathBuf {
        PathBuf::from(format!("{SOCKET_DIR}/{SOCKET_PREFIX}{pid}{SOCKET_SUFFIX}"))
    }

    #[test]
    fn socket_pid_round_trips_the_naming_convention() {
        // `socket_pid` and `discover_all` parse the same filenames; if they
        // ever disagree the supervisor checks liveness of the wrong process
        // and either drops a healthy connection or clings to a dead one.
        assert_eq!(socket_pid(&socket(4242)), Some(4242));
        assert_eq!(socket_pid(Path::new("/tmp/fts-daw-.sock")), None);
        assert_eq!(socket_pid(Path::new("/tmp/fts-daw-abc.sock")), None);
        assert_eq!(socket_pid(Path::new("/tmp/something-else.sock")), None);
        assert_eq!(socket_pid(Path::new("/tmp/fts-daw-7")), None);
    }

    #[test]
    fn a_healthy_connection_is_left_alone() {
        assert_eq!(supervise(Some(7), |_| true, || true), Action::Idle);
    }

    #[test]
    fn a_dead_reaper_is_dropped_even_when_another_is_available() {
        // Drop first, attach next tick — never swap straight from one dead
        // connection to a new one, so the UI shows the truth in between.
        assert_eq!(supervise(Some(7), |_| false, || true), Action::Drop);
    }

    #[test]
    fn detached_attaches_only_when_something_is_there() {
        assert_eq!(supervise(None, |_| true, || true), Action::Attach);
        assert_eq!(supervise(None, |_| true, || false), Action::Idle);
    }

    #[test]
    fn liveness_is_asked_about_the_attached_pid_specifically() {
        // Not "is any REAPER alive" — the one we hold. A second REAPER
        // running must not keep a dead connection looking healthy.
        assert_eq!(supervise(Some(7), |pid| pid == 9, || true), Action::Drop);
        assert_eq!(supervise(Some(9), |pid| pid == 9, || true), Action::Idle);
    }

    #[test]
    fn discovery_skips_processes_that_are_gone() {
        // pid 1 is always alive; u32::MAX never is. `discover_all` reads the
        // real /tmp, so assert on the property rather than exact contents.
        let found = discover_all();
        assert!(
            found.iter().all(|r| process_alive(r.pid)),
            "discover_all returned a socket whose process is gone: {found:?}"
        );
        let mut pids: Vec<u32> = found.iter().map(|r| r.pid).collect();
        let sorted = {
            let mut p = pids.clone();
            p.sort_unstable_by(|a, b| b.cmp(a));
            p
        };
        pids.dedup();
        assert_eq!(
            found.iter().map(|r| r.pid).collect::<Vec<_>>(),
            sorted,
            "newest-first ordering is what `connect()` relies on to pick a REAPER"
        );
    }
}
