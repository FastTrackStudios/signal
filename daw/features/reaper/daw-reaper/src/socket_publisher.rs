//! Unix-socket Vox publisher.
//!
//! REAPER extensions that want external apps (CLI, desktop, mobile,
//! audio-sync peers, …) to call their service surface over Vox RPC use
//! [`publish_extension_socket`]. The host owns a fully-populated
//! [`architect::LayerRouter`] (built via `Reaper::layers()` plus any
//! extension-specific layers) and hands it to the publisher; the
//! publisher opens a Unix socket on `socket_path()`, accepts virtual
//! connections, and routes every request to the supplied router.
//!
//! Extracted from `daw-bridge` so any extension can stand up its own
//! external surface without duplicating socket plumbing.

use std::path::PathBuf;
use std::sync::Arc;

use architect::LayerRouter;
use tokio::net::UnixListener;
use tracing::{debug, info, warn};
use vox::MetadataExt as _;

/// Vox lane acceptor that hands every inbound lane to a single shared
/// `LayerRouter`. The router contains the mounted dispatchers for every
/// service exposed by the extension.
#[derive(Clone)]
pub struct ExtensionConnectionAcceptor {
    handler: Arc<LayerRouter>,
}

impl ExtensionConnectionAcceptor {
    pub fn new(handler: LayerRouter) -> Self {
        Self {
            handler: Arc::new(handler),
        }
    }
}

/// Resolve the Unix socket path for this REAPER process.
///
/// - `$FTS_SOCKET` overrides if set (used by tests / forced paths).
/// - Default: `/tmp/fts-daw-{pid}.sock` so multiple REAPER instances on
///   the same machine never collide. External discovery enumerates
///   `/tmp/fts-daw-*.sock`.
pub fn socket_path() -> PathBuf {
    std::env::var("FTS_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let pid = std::process::id();
            PathBuf::from(format!("/tmp/fts-daw-{pid}.sock"))
        })
}

/// Spawn the Unix-socket listener and start accepting Vox connections
/// against `router`. Returns nothing — the listener task is detached
/// onto the tokio runtime and lives for the rest of the process. Any
/// failure to bind logs a warning but does not panic; the in-process
/// service surface remains usable.
///
/// Idempotent: stale socket files from a previous run are removed
/// before binding. Safe to call once per `plugin_main`.
pub fn publish_extension_socket(router: LayerRouter) {
    let path = socket_path();
    let _ = std::fs::remove_file(&path);

    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            warn!(
                path = %path.display(),
                error = %e,
                "Failed to bind extension Unix socket"
            );
            return;
        }
    };

    info!(path = %path.display(), "Extension Unix socket listening");

    let acceptor = ExtensionConnectionAcceptor::new(router);
    tokio::task::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    debug!("Client connected via Unix socket");
                    let acceptor = acceptor.clone();
                    tokio::task::spawn(async move {
                        let link = vox_stream::StreamLink::unix(stream);
                        // vox 0.10 lane model: hand every inbound lane to the
                        // shared LayerRouter (it dispatches by method id).
                        let router = acceptor.handler.as_ref().clone();
                        let lane_acceptor = vox::lane_acceptor_fn(move |req, connection| {
                            let role = req.metadata().meta_str("role").unwrap_or("unknown");
                            info!(role = role, "Accepting lane");
                            connection.handle_with(router.clone());
                            Ok(())
                        });
                        match vox::acceptor_on(link)
                            .on_lane(lane_acceptor)
                            .establish_connection()
                            .await
                        {
                            Ok(_connection) => {
                                debug!("Unix socket session established");
                                std::future::pending::<()>().await;
                            }
                            Err(e) => {
                                warn!(error = ?e, "Unix socket handshake failed");
                            }
                        }
                    });
                }
                Err(e) => {
                    warn!(error = %e, "Unix socket accept error");
                }
            }
        }
    });
}
