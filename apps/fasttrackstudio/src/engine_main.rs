//! `fasttrackstudio --engine` — the headless signal engine.
//!
//! The former `apps/signal-engine` binary, folded into the app: opens the
//! live audio engine (guitar in → NAM chain → out), loads the profile, and
//! mounts the rig's vox `LayerRouter` at `ws://<host>:4040/vox`. Remotes
//! (the fts web build served by this same process, other machines on the
//! network, the desktop app) drive it through the exact same generated
//! clients — the core cannot tell the difference. This mode never touches
//! dioxus/GUI and never bootstraps the in-process session engine.
//!
//! The transport/serving boilerplate — runtime, tracing, panic logger,
//! `/health`, `/vox`, iroh p2p, and the embedded SPA fallback — all lives in
//! [`architect::host`]; this file is just "build the rigs, mount the router,
//! serve." The browser remote is the fasttrackstudio WEB build, embedded with
//! `--features embed-web` (staged by `just rig-install` into `web-dist/`), so
//! the deployed engine is ONE artifact.

use architect::host::{self, EngineHost, WebBundle};
use architect::rig::RigBackend as _;
use signal_guitar::proto::rig::Rig as _;
use signal_guitar::GuitarRigBackend;

/// Default bind address; override with `SIGNAL_ENGINE_ADDR` (or the legacy
/// `RIGD_ADDR`, still honored so existing live setups keep working).
const DEFAULT_ADDR: &str = "0.0.0.0:4040";

/// Bind address: `SIGNAL_ENGINE_ADDR` wins, then the legacy `RIGD_ADDR`,
/// then the default.
fn bind_addr() -> String {
    std::env::var("SIGNAL_ENGINE_ADDR")
        .or_else(|_| std::env::var("RIGD_ADDR"))
        .unwrap_or_else(|_| DEFAULT_ADDR.to_string())
}

/// The staged web bundle, compiled into the binary. `just rig-install`
/// copies `target/dx/fasttrackstudio/release/web/public` →
/// `apps/fasttrackstudio/web-dist/` before the native build; without the
/// feature no staged dir is required.
#[cfg(feature = "embed-web")]
static EMBEDDED_WEB: include_dir::Dir<'static> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/web-dist");

/// Locate the browser-remote bundle so the engine serves it itself — any
/// device on the LAN opens `http://<host>:4040/` and gets the control UI.
/// First match wins:
///
/// 1. `SIGNAL_WEB_DIST` env var (explicit override)
/// 2. assets embedded in the binary (feature `embed-web` — deployed layout)
/// 3. `<exe_dir>/signal-web` (legacy deployed layout — bundle beside the binary)
/// 4. dx dev build output relative to the workspace `target/`:
///    `target/dx/fasttrackstudio/{release,debug}/web/public`
///
/// A directory candidate only counts if it contains an `index.html`. `None`
/// means headless (only `/health` + `/vox`).
fn web_bundle() -> Option<WebBundle> {
    if let Ok(dir) = std::env::var("SIGNAL_WEB_DIST") {
        let p = std::path::PathBuf::from(&dir);
        if p.join("index.html").is_file() {
            return Some(WebBundle::Dir(p));
        }
        tracing::warn!("SIGNAL_WEB_DIST={dir} has no index.html — ignoring");
    }

    #[cfg(feature = "embed-web")]
    {
        return Some(WebBundle::Embedded(&EMBEDDED_WEB));
    }

    #[cfg(not(feature = "embed-web"))]
    {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))?;
        let candidates = [
            // Legacy deployed layout: bundle beside the binary.
            exe_dir.join("signal-web"),
            // Dev: exe lives in target/{debug,release}; dx output is a
            // sibling under target/dx/.
            exe_dir.join("../dx/fasttrackstudio/release/web/public"),
            exe_dir.join("../dx/fasttrackstudio/debug/web/public"),
        ];
        candidates
            .into_iter()
            .find(|p| p.join("index.html").is_file())
            .map(WebBundle::Dir)
    }
}

/// Entry point for `fasttrackstudio --engine`: builds the multi-thread tokio
/// runtime and never returns until the server dies.
pub fn run() {
    host::init_tracing("info");
    // Log every panic loudly (thread name + backtrace). Panics stay
    // unwinding — control-plane panics are caught and survived (the rig's
    // meter pump self-heals; audio keeps playing) — but none die silently.
    host::install_panic_logger();

    // Session player: bring up the in-process daw-standalone setlist engine
    // (demo setlist + audio + guide) BEFORE the server runtime exists — it
    // owns its own runtime (bootstrap_blocking), and async_main merges its
    // SetlistService router onto the shared `/vox` router so browser
    // remotes drive the ENGINE's transport. Audio stays here on the engine
    // host; failure is non-fatal (the rig serves without a setlist).
    #[cfg(feature = "session")]
    match crate::session_engine::bootstrap_blocking() {
        Ok(()) => tracing::info!("session engine ready (in-process daw-standalone)"),
        Err(e) => tracing::error!("session engine failed to start: {e:?}"),
    }

    host::block_on(async_main());
}

async fn async_main() {
    // ── Rigs ──────────────────────────────────────────────────────────────
    // Every rig backend mounts its own service bundle onto ONE shared router
    // (`merge_router`) served at `/vox`; adding a rig (keys, …) is one more
    // backend + `.merge_router(rig.router())` here, plus a matching view in the
    // app. Each rig keeps its own dispatcher/PubSub — only the routing is
    // shared. Per-rig start policy differs (the worship guitar auto-opens
    // audio; other rigs stay dormant until a remote starts them), but the
    // wiring is identical.
    let guitar = GuitarRigBackend::new();
    guitar.start(); // worship rig: open audio + load profile off-thread
    let bass = signal_bass::BassRigBackend::new(); // dormant until started
    let drums = signal_drums::DrumRigBackend::new(); // dormant until started
    let keys = signal_keys::KeysRigBackend::new(); // dormant until started
    let synth = signal_synth_rig::SynthRigBackend::new(); // dormant until started

    let router = guitar
        .router()
        .merge_router(bass.router())
        .merge_router(drums.router())
        .merge_router(keys.router())
        .merge_router(synth.router());

    // ── Session (the setlist player) ─────────────────────────────────────
    // Mount SetlistService (+ its `#[subscribe]` stream sibling) from the
    // in-process session engine bootstrapped in `run()`, so browser/desktop
    // remotes reach the setlist over the SAME `/vox` (and iroh) endpoint as
    // the rigs. The transport plays HERE — remotes only command and render.
    #[cfg(feature = "session")]
    let router = match crate::session_engine::engine() {
        Some(session) => {
            // Open the setlist on song 0 / section 0 so remotes land on an
            // active cursor (demo stamping leaves the edit cursor at the
            // timeline end — nothing would be active until a user seeks).
            let client = session.client.clone();
            tokio::spawn(async move {
                match client.seek_to_section(0, 0).await {
                    Ok(_) => tracing::info!("opened setlist on song 0 / section 0"),
                    Err(e) => tracing::warn!("initial setlist seek failed: {e:?}"),
                }
            });
            router.merge_router(session.router())
        }
        None => router, // bootstrap failed — the rigs still serve
    };

    // ── Watch bridge ──────────────────────────────────────────────────────
    // watchOS can't speak vox over WebSocket (TN3135), so the watch remote
    // gets a thin HTTP+SSE JSON surface (engine_watch.rs). The bridge is an
    // ordinary vox client over an in-process LocalServer against the SAME
    // router — the core can't tell a watch from a browser.
    let watch_scope = architect::Scope::new();
    let local = architect::LocalServer::serve(router.clone(), watch_scope.clone());
    let watch_routes = crate::engine_watch::router(local).await;

    // Serve the router over axum (`/vox` + `/health`) and iroh p2p, with the
    // browser remote as the HTTP fallback — all of it in `architect::host`.
    let config_dir = signal_sampler::rig_prefs::signal_config_dir();
    let mut engine_host = EngineHost::new(router, bind_addr());
    if let Some(routes) = watch_routes {
        engine_host = engine_host.extend(routes);
        tracing::info!("watch bridge mounted at /watch/v1");
    }
    engine_host
        .iroh(
            config_dir.join("iroh.key"),
            Some(config_dir.join("iroh-endpoint-id")),
        )
        .web(web_bundle())
        .serve()
        .await;
}
