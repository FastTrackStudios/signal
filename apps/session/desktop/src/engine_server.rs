//! `session-desktop --engine`: a headless Session engine — a **Remote
//! target** for the Session UI, and the LAN control surface the phone /
//! tablet / browser remotes drive.
//!
//! In Live Mode it serves **the whole daw facade** of its in-process
//! `daw-standalone` — every service `Standalone`'s `architect::Services`
//! bundle mounts (transport, projects, tracks, items, takes, markers,
//! regions, tempo, routing, peaks, the `#[subscribe]` stream siblings, …),
//! the very set `build_in_process_daw` serves to the engine itself — plus
//! the setlist service the browser remote already speaks. A UI dials it
//! with `remote::connect_engine_daw` and attaches exactly as it attaches to
//! REAPER. Recording Mode serves only `reaper_lan_proxy`'s setlist (that
//! module's doc says why it is a proxy rather than a re-mounted router).
//!
//! Two transports, one router:
//!
//! - WebSocket `ws://0.0.0.0:<port>/vox` (default 4040) — binds every
//!   interface, since the point is reachability from other devices.
//! - iroh — an endpoint whose secret key persists under
//!   `~/.config/fts/session-engine-iroh.key` (`$XDG_CONFIG_HOME` honoured),
//!   so its endpoint id, and the `fts-engine:<id>` connect string logged at
//!   start-up, survive restarts. Native peers dial it across NATs with no
//!   port forwarding.
//!
//! Arguments (after `--engine`): `--port N`, `--project PATH` (`.rpp` or
//! `.session`), `--setlist PATH` (a setlist note, or a folder of song
//! folders), `--no-iroh`, `--iroh-key PATH`. See [`EngineArgs`].

use std::path::PathBuf;

use architect::axum_ws;
use architect::iroh_link::{self, iroh};
use axum::Router;
use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use daw::LayerRouter;
use daw::Services as _;

const DEFAULT_PORT: u16 = 4040;

/// File name of the engine's iroh secret key, under `<config>/fts/`.
const IROH_KEY_FILE: &str = "session-engine-iroh.key";

/// What `--engine` was asked to do.
#[derive(Debug, Default, PartialEq)]
pub struct EngineArgs {
    /// `--port N`: the WebSocket port (default 4040).
    pub port: Option<u16>,
    /// `--project PATH` / `--setlist PATH`: what to open before serving.
    pub open: Option<OpenTarget>,
    /// `--no-iroh`: serve the WebSocket only.
    pub no_iroh: bool,
    /// `--iroh-key PATH`: where the endpoint's identity lives, instead of
    /// the default under the config dir.
    pub iroh_key: Option<PathBuf>,
}

/// Something to open into the engine at start-up.
#[derive(Debug, PartialEq)]
pub enum OpenTarget {
    /// A REAPER `.rpp`, or a `.session` project directory.
    Project(PathBuf),
    /// A setlist note (`.md`, songs resolved against the track libraries),
    /// or a folder of song folders (every song in it, in title order).
    Setlist(PathBuf),
}

impl EngineArgs {
    /// Read the `--engine` arguments out of the process's command line
    /// (anything unrecognised is left for other parsers).
    pub fn parse(args: &[String]) -> Self {
        let value = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1))
                .filter(|v| !v.starts_with("--"))
        };
        let open = value("--project")
            .map(|p| OpenTarget::Project(PathBuf::from(p)))
            .or_else(|| value("--setlist").map(|p| OpenTarget::Setlist(PathBuf::from(p))));
        Self {
            port: value("--port").and_then(|p| p.parse().ok()),
            open,
            no_iroh: args.iter().any(|a| a == "--no-iroh"),
            iroh_key: value("--iroh-key").map(PathBuf::from),
        }
    }
}

/// The daw facade of `standalone`: every service its `architect::Services`
/// bundle mounts — the same router `build_in_process_daw` serves over the
/// engine's own memory link, so a remote sees what the engine sees.
pub fn daw_facade_router(standalone: &daw_standalone::sync::Standalone) -> LayerRouter {
    standalone.clone().into_router()
}

/// The router a running engine serves: Live Mode's full daw facade plus
/// its setlist, or Recording Mode's setlist proxy. `None` until one of the
/// two has booted.
pub fn engine_router() -> Option<LayerRouter> {
    if let Some(engine) = crate::session_engine::engine() {
        return Some(daw_facade_router(&engine.standalone).merge_router(engine.router()));
    }
    crate::reaper_lan_proxy::proxy().map(crate::reaper_lan_proxy::router)
}

#[derive(Clone)]
struct EngineState {
    router: LayerRouter,
}

async fn vox_handler(ws: WebSocketUpgrade, State(state): State<EngineState>) -> Response {
    ws.on_upgrade(move |socket| axum_ws::serve_router(socket, state.router))
        .into_response()
}

async fn index_handler() -> Response {
    // `web-dist/` (see the `web-stage` just recipe) isn't staged into this
    // build yet, or `embed-web` wasn't enabled — either way there's no
    // browser UI to serve, but the /vox endpoint above still works with
    // any vox client (a native app, a test harness, a future page).
    axum::response::Html(
        "<!doctype html><title>session-desktop --engine</title>\
         <body style=\"font-family:system-ui;padding:2rem;max-width:40rem\">\
         <h1>session-desktop --engine</h1>\
         <p>Serving the setlist and the daw facade over <code>/vox</code>. \
         No browser UI is staged into this build yet — build one with \
         <code>just web-stage</code> and rebuild with <code>--features \
         embed-web</code> to serve it here.</p>\
         </body>",
    )
    .into_response()
}

/// Serve `router` over `/vox` (+ a placeholder `/`) on an already-bound
/// listener, until the listener fails.
pub async fn serve_ws(listener: tokio::net::TcpListener, router: LayerRouter) -> eyre::Result<()> {
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/vox", get(vox_handler))
        .with_state(EngineState { router });
    axum::serve(listener, app)
        .await
        .map_err(|e| eyre::eyre!("engine server: {e}"))
}

/// Serve `router` on an iroh endpoint — each accepted bi-stream is one vox
/// connection — until the endpoint closes.
pub async fn serve_iroh(endpoint: &iroh::Endpoint, router: LayerRouter) {
    iroh_link::serve_router(endpoint, router).await;
}

/// Where the engine's iroh identity lives by default:
/// `<$XDG_CONFIG_HOME or ~/.config>/fts/session-engine-iroh.key`.
pub fn default_iroh_key_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(xdg) => PathBuf::from(xdg),
        None => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("fts").join(IROH_KEY_FILE))
}

/// Bind the engine's iroh endpoint on its persistent identity (created on
/// first run), so the endpoint id is the same on every start.
pub async fn bind_iroh(key_path: &std::path::Path) -> eyre::Result<iroh::Endpoint> {
    let key = iroh_link::load_or_create_secret_key(key_path)
        .map_err(|e| eyre::eyre!("iroh key {}: {e}", key_path.display()))?;
    iroh_link::bind_endpoint(key)
        .await
        .map_err(|e| eyre::eyre!("iroh bind: {e}"))
}

/// Serve the running engine's router over the WebSocket on `args.port`
/// (default 4040, every interface) and, unless `--no-iroh`, over iroh.
///
/// The caller boots the engine first (`session_engine::bootstrap_blocking`
/// or `reaper_engine::bootstrap_blocking`) — each manages its own runtime
/// and must not be called from inside this one.
///
/// Never returns under normal operation — the server runs until the
/// process is killed, same as any other long-running daemon. A failure to
/// bring iroh up is not fatal: the WebSocket keeps serving.
pub async fn run(args: &EngineArgs) -> eyre::Result<()> {
    let router = engine_router().ok_or_else(|| {
        eyre::eyre!("neither engine (Live Mode or Recording Mode) finished booting")
    })?;
    let serves_daw = crate::session_engine::engine().is_some();

    let port = args.port.unwrap_or(DEFAULT_PORT);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .map_err(|e| eyre::eyre!("binding 0.0.0.0:{port}: {e}"))?;

    // The endpoint must outlive the server, which never returns.
    let mut iroh_endpoint = None;
    let mut connect = None;
    if !args.no_iroh {
        let key_path = args.iroh_key.clone().or_else(default_iroh_key_path);
        let bound = match key_path {
            Some(path) => bind_iroh(&path).await,
            None => Err(eyre::eyre!("no config dir for the iroh key (HOME unset)")),
        };
        match bound {
            Ok(endpoint) => {
                connect = Some(crate::remote::EngineAddr::connect_string(&endpoint.id()));
                let serving = endpoint.clone();
                let router = router.clone();
                tokio::spawn(async move { serve_iroh(&serving, router).await });
                iroh_endpoint = Some(endpoint);
            }
            Err(e) => {
                tracing::warn!(error = %e, "--engine: iroh unavailable; serving WebSocket only")
            }
        }
    }

    tracing::info!(
        engine.ws = %format!("ws://0.0.0.0:{port}/vox"),
        engine.lan = ?lan_addresses(port),
        engine.iroh_connect = connect.as_deref().unwrap_or("off"),
        engine.serves_daw = serves_daw,
        "session-desktop --engine: serving (open http://<lan addr> on another device; \
         dial the iroh connect string from a native Session)"
    );

    let served = serve_ws(listener, router).await;
    drop(iroh_endpoint);
    served
}

/// Every non-loopback IPv4 address this machine has, each paired with
/// `port` — usually just one (the LAN interface), occasionally more (VPN,
/// a second NIC). Best-effort: falls back to `localhost` alone if enumerating
/// interfaces fails, so the server still prints *something* useful.
fn lan_addresses(port: u16) -> Vec<String> {
    let mut addrs: Vec<String> = Vec::new();
    if let Ok(interfaces) = local_ip_interfaces() {
        for ip in interfaces {
            addrs.push(format!("{ip}:{port}"));
        }
    }
    if addrs.is_empty() {
        addrs.push(format!("localhost:{port}"));
    }
    addrs
}

/// Non-loopback IPv4 addresses via `getifaddrs(3)` — no extra crate needed
/// for something this small. Unix-only, matching the rest of Recording
/// Mode's own Unix-only socket-discovery code in this app.
#[cfg(unix)]
fn local_ip_interfaces() -> std::io::Result<Vec<std::net::Ipv4Addr>> {
    use std::net::Ipv4Addr;

    let mut result = Vec::new();
    let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: `ifap` is a valid out-pointer for getifaddrs; freeifaddrs is
    // called on every path once it succeeds, matching the man page's
    // ownership contract.
    if unsafe { libc::getifaddrs(&mut ifap) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut cursor = ifap;
    while !cursor.is_null() {
        // SAFETY: `cursor` is non-null and was populated by getifaddrs;
        // `ifa_addr` may legitimately be null for some interface types.
        let ifa = unsafe { &*cursor };
        if !ifa.ifa_addr.is_null() {
            // SAFETY: sockaddr fields are readable for the lifetime of
            // this loop iteration; only sockaddr_in (AF_INET) is
            // interpreted, matching its actual size.
            let family = unsafe { (*ifa.ifa_addr).sa_family };
            if i32::from(family) == libc::AF_INET {
                let sockaddr_in = ifa.ifa_addr.cast::<libc::sockaddr_in>();
                let ip = unsafe { (*sockaddr_in).sin_addr.s_addr };
                let ip = Ipv4Addr::from(u32::from_be(ip));
                if !ip.is_loopback() {
                    result.push(ip);
                }
            }
        }
        cursor = ifa.ifa_next;
    }
    // SAFETY: `ifap` was successfully populated by the getifaddrs call
    // above and hasn't been freed yet.
    unsafe { libc::freeifaddrs(ifap) };
    Ok(result)
}

#[cfg(not(unix))]
fn local_ip_interfaces() -> std::io::Result<Vec<std::net::Ipv4Addr>> {
    Ok(Vec::new())
}

#[cfg(test)]
#[path = "engine_server_tests.rs"]
mod tests;
