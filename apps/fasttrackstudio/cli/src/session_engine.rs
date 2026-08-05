//! `fts session engine` — the standalone session engine, in-process.
//!
//! Session's engines normally run inside the app (see
//! `apps/fasttrackstudio/src/session_engine.rs`); there is no standalone
//! session binary. This module builds the exact same stack — a seeded
//! `daw-standalone` backend, the global daw facade over a vox memory
//! link, `SetlistServiceImpl` + its stream pumps — and serves the setlist
//! RPC + `#[subscribe]` stream layers over `architect::axum_ws` at
//! `ws://:3030/ws` (the session gateway shape the web remotes expect).

use std::sync::Arc;

use axum::Router;
use axum::extract::{State, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use daw::service::ProjectInfo;
use daw_standalone::bootstrap::build_in_process_daw;
use daw_standalone::sync::Standalone;
use eyre::Result;
use session::setlist::service::demo::stamp_demo_setlist_with;
use session::{
    SetlistServiceClient, SetlistServiceImpl, serve_setlist_service,
    setlist_service_service_descriptor,
};

/// Project GUID the demo setlist is stamped into.
const DEMO_PROJECT_GUID: &str = "fts-demo";
const DEFAULT_ADDR: &str = "0.0.0.0:3030";

#[derive(Clone)]
struct AppState {
    router: architect::LayerRouter,
}

/// One vox connection per WebSocket upgrade; the shared LayerRouter
/// dispatches the setlist RPC + stream services by method id.
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| architect::axum_ws::serve_router(socket, state.router.clone()))
        .into_response()
}

/// Build the engine and serve it. Blocks for the life of the process.
///
/// 16 MiB worker stacks: vox 0.10's debug-build channel encode recurses
/// deeply on Setlist payloads (same workaround as the app's engine).
pub fn run_blocking(addr: Option<String>) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("fts-session-engine")
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()?;
    let addr = addr
        .or_else(|| std::env::var("SESSION_ENGINE_ADDR").ok())
        .unwrap_or_else(|| DEFAULT_ADDR.to_string());
    rt.block_on(run(addr))
}

async fn run(addr: String) -> Result<()> {
    // 1. Standalone backend seeded with the playable demo setlist.
    let standalone = Standalone::new();
    let guid = standalone.seed_project(ProjectInfo {
        guid: DEMO_PROJECT_GUID.into(),
        name: "FTS Demo Setlist".into(),
        path: String::new(),
    });
    stamp_demo_setlist_with(&standalone).map_err(|e| eyre::eyre!("stamp demo setlist: {e:?}"))?;

    // 2. In-process daw facade over a vox memory link (the setlist
    //    builder resolves the daw through `daw::get()`).
    let bundle = build_in_process_daw(standalone.clone()).await?;
    let block_on_rt = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
    );
    daw::init_from_parts(bundle.daw.clone(), block_on_rt);

    // 3. Setlist service + one process-wide pump per `#[subscribe]` hub.
    let setlist = SetlistServiceImpl::with_daw(standalone.clone());
    setlist.start_stream_pumps();

    // 4. Initial build over an in-process LocalServer client (the same
    //    conduit shape every remote uses).
    let router = daw::LayerRouter::new()
        .with(
            setlist_service_service_descriptor(),
            serve_setlist_service(setlist.clone()),
        )
        .with(
            session::services::setlist_service::setlist_service_stream_service_descriptor(),
            session::services::setlist_service::stream_serve(setlist.clone()),
        );
    let scope = architect::Scope::new();
    let server = architect::LocalServer::serve(router.clone(), Arc::clone(&scope));
    let caller = server
        .caller()
        .await
        .map_err(|e| eyre::eyre!("local setlist caller: {e:?}"))?;
    let client = SetlistServiceClient::new(caller);
    client
        .build_from_open_projects()
        .await
        .map_err(|e| eyre::eyre!("build_from_open_projects: {e:?}"))?;
    tracing::info!("setlist built from standalone project '{guid}'");

    // 5. Audio — graceful. cpal streams are !Send; on failure re-enable
    //    the soft clock so transport still advances silently.
    spawn_audio_thread(standalone, guid, tokio::runtime::Handle::current());

    // 6. The gateway: ws://<addr>/ws, one vox connection per upgrade.
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/ws", get(ws_handler))
        .with_state(AppState { router });
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("session engine serving ws://{addr}/ws");
    println!("session engine — ws://{addr}/ws");
    axum::serve(listener, app).await?;
    let _ = bundle; // keep the memory-link acceptor alive for the process life
    let _ = scope;
    Ok(())
}

/// Open the default cpal output so the audio callback drives the playhead
/// (same shape as the app's engine, minus the guide renderer).
fn spawn_audio_thread(standalone: Standalone, guid: String, rt: tokio::runtime::Handle) {
    let result = std::thread::Builder::new()
        .name("fts-audio".into())
        .spawn(move || {
            let _rt_guard = rt.enter();
            match standalone.attach_audio_engine(&guid) {
                Ok(_engine) => {
                    tracing::info!("audio engine attached (default cpal output)");
                    loop {
                        std::thread::park();
                    }
                }
                Err(e) => {
                    tracing::warn!("no audio output ({e}); transport runs silently");
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
