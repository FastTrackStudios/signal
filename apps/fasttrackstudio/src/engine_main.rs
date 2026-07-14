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
//! The browser remote is the fasttrackstudio WEB build (`dx build
//! --platform web --no-default-features --features signal`), embedded into
//! the binary when compiled with `--features embed-web` (staged by `just
//! rig-install` into `web-dist/`), so the deployed engine is ONE artifact.

use axum::extract::{State, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use architect::axum_ws;
use signal_guitar::GuitarRigBackend;
use signal_guitar::proto::rig::Rig as _;
use tower_http::services::{ServeDir, ServeFile};

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

/// Where the browser remote's assets come from.
enum WebBundle {
    /// A directory on disk (env override or legacy deployed layout).
    Dir(std::path::PathBuf),
    /// Compiled into this binary (`embed-web`).
    #[cfg(feature = "embed-web")]
    Embedded,
}

/// Locate the built web bundle (the browser remote) so the engine can serve
/// it itself — any device on the LAN opens `http://<host>:4040/` and gets
/// the control UI. First match wins:
///
/// 1. `SIGNAL_WEB_DIST` env var (explicit override)
/// 2. assets embedded in the binary (feature `embed-web` — deployed layout)
/// 3. `<exe_dir>/signal-web` (legacy deployed layout — bundle beside the binary)
/// 4. dx dev build output relative to the workspace `target/` this binary
///    was built into: `target/dx/fasttrackstudio/{release,debug}/web/public`
///
/// A directory candidate only counts if it contains an `index.html`.
/// Returns `None` when no bundle exists — the engine runs fine headless,
/// serving only `/health` + `/vox`.
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
        return Some(WebBundle::Embedded);
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

/// Content type by file extension — enough for the dx web bundle without
/// pulling a mime crate into the app.
#[cfg(feature = "embed-web")]
fn content_type_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "wasm" => "application/wasm",
        "json" | "map" => "application/json",
        "webmanifest" => "application/manifest+json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Serve the embedded bundle from memory, `index.html` fallback for
/// client-side routes (anything that isn't an existing file).
#[cfg(feature = "embed-web")]
async fn embedded_asset(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let (path, file) = match EMBEDDED_WEB.get_file(path) {
        Some(f) if !path.is_empty() => (path, f),
        _ => match EMBEDDED_WEB.get_file("index.html") {
            Some(f) => ("index.html", f),
            None => {
                return (axum::http::StatusCode::NOT_FOUND, "no index.html in embedded bundle")
                    .into_response();
            }
        },
    };
    (
        [(axum::http::header::CONTENT_TYPE, content_type_for(path))],
        file.contents(),
    )
        .into_response()
}

#[derive(Clone)]
struct AppState {
    /// The one shared [`architect::LayerRouter`] serving *every* rig. Each rig
    /// backend merges its own service bundle into it (`merge_router`), so all
    /// rigs — guitar, drums, and future ones (keys, …) — are reached over the
    /// single `/vox` endpoint, dispatched by method id.
    router: architect::LayerRouter,
}

/// One vox connection per WebSocket upgrade; the shared [`architect::LayerRouter`]
/// dispatches every rig's services (Rig, RigStream, AudioSettings, DrumRig, …)
/// by method id.
async fn vox_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| async move {
        let router = state.router.clone();
        let acceptor = axum_ws::lane_acceptor_fn(move |_req, connection| {
            connection.handle_with(router.clone());
            Ok(())
        });
        axum_ws::serve(socket, acceptor).await;
    })
    .into_response()
}

/// Serve the same router over an iroh p2p endpoint — remotes dial the
/// engine by bare endpoint id from any network, no port forwarding. The
/// endpoint's secret key persists at `<config>/iroh.key` so the id is
/// stable across restarts; the id itself is logged and written to
/// `<config>/iroh-endpoint-id` for other devices/agents to read.
async fn serve_iroh(router: architect::LayerRouter) {
    use architect::iroh_link;

    let config_dir = signal_sampler::rig_prefs::signal_config_dir();
    let secret_key = match iroh_link::load_or_create_secret_key(&config_dir.join("iroh.key")) {
        Ok(k) => k,
        Err(e) => {
            tracing::error!(error = %e, "iroh secret key unavailable; p2p transport disabled");
            return;
        }
    };
    let endpoint = match iroh_link::bind_endpoint(secret_key).await {
        Ok(ep) => ep,
        Err(e) => {
            tracing::error!(error = %e, "iroh endpoint bind failed; p2p transport disabled");
            return;
        }
    };
    tracing::info!("iroh endpoint id: {}", endpoint.id());
    if let Err(e) = std::fs::write(
        config_dir.join("iroh-endpoint-id"),
        format!("{}\n", endpoint.id()),
    ) {
        tracing::warn!(error = %e, "could not write iroh-endpoint-id");
    }

    let acceptor = iroh_link::lane_acceptor_fn(move |_req, connection| {
        connection.handle_with(router.clone());
        Ok(())
    });
    iroh_link::serve_endpoint(&endpoint, acceptor).await;
}

/// Entry point for `fasttrackstudio --engine`: builds the multi-thread
/// tokio runtime and never returns until the server dies.
pub fn run() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(async_main());
}

async fn async_main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Log every panic loudly (thread name + backtrace) before the default
    // hook runs. Panics stay unwinding — control-plane panics are caught
    // and survived (the rig's meter pump self-heals; audio keeps playing)
    // — but none may die silently mid-service.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let backtrace = std::backtrace::Backtrace::force_capture();
        tracing::error!(
            thread = thread.name().unwrap_or("<unnamed>"),
            "panic: {info}\n{backtrace}"
        );
        default_hook(info);
    }));

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
    let drums = signal_drums::DrumRigBackend::new(); // dormant until started
    let keys = signal_keys::KeysRigBackend::new(); // dormant until started
    let synth = signal_synth_rig::SynthRigBackend::new(); // dormant until started

    let router = guitar
        .router()
        .merge_router(drums.router())
        .merge_router(keys.router())
        .merge_router(synth.router());
    let state = AppState { router };

    tokio::spawn(serve_iroh(state.router.clone()));

    let mut app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/vox", get(vox_handler))
        .with_state(state);

    let addr = bind_addr();

    // Serve the browser remote (the fts web build) from the same router,
    // with an index.html fallback so client-side routes deep-link.
    let web = web_bundle();
    match &web {
        Some(WebBundle::Dir(dist)) => {
            let serve = ServeDir::new(dist).fallback(ServeFile::new(dist.join("index.html")));
            app = app.fallback_service(serve);
        }
        #[cfg(feature = "embed-web")]
        Some(WebBundle::Embedded) => {
            app = app.fallback(get(embedded_asset));
        }
        None => {
            tracing::warn!(
                "web bundle not found (SIGNAL_WEB_DIST, embed-web, <exe_dir>/signal-web, \
                 target/dx/fasttrackstudio) — serving /health + /vox only"
            );
        }
    }

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("bind {addr}: {e}"));
    tracing::info!("signal engine serving ws://{addr}/vox");
    match &web {
        Some(WebBundle::Dir(dist)) => {
            tracing::info!("web remote: http://{addr}/ (bundle: {})", dist.display());
        }
        #[cfg(feature = "embed-web")]
        Some(WebBundle::Embedded) => {
            tracing::info!("web remote: http://{addr}/ (embedded bundle)");
        }
        None => {}
    }
    axum::serve(listener, app).await.expect("axum serve");
}
