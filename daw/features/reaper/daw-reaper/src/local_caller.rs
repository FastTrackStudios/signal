//! In-process vox caller — thin wrapper over [`architect::LocalServer`].
//!
//! Any in-process consumer (plugins, extensions, desktop apps) gets a
//! `Caller` against a `LayerRouter` without a socket: the router is
//! served over a vox in-memory link by architect's in-process
//! transport, and the raw caller fans out into per-service clients
//! (`daw_control::Daw`).

use std::sync::Arc;

use architect::{LayerRouter, LocalServer, Scope};
use vox::Caller;

/// In-process vox caller backed by memory channels.
///
/// # Example
///
/// ```ignore
/// let local = LocalCaller::new(crate::plugin_services::create_daw_handler()).await?;
/// let daw = Daw::new(local.caller());
/// ```
#[derive(Clone)]
pub struct LocalCaller {
    caller: Caller,
    /// Owns the acceptor task; the link lives as long as any clone.
    _scope: Arc<Scope>,
}

impl LocalCaller {
    /// Serve `router` in-process and establish a caller against it.
    pub async fn new(router: LayerRouter) -> eyre::Result<Self> {
        let scope = Scope::new();
        let server = LocalServer::serve(router, Arc::clone(&scope));
        let caller = server.caller().await?;
        tracing::debug!("LocalCaller established (architect::LocalServer, in-memory link)");
        Ok(Self {
            caller,
            _scope: scope,
        })
    }

    /// Get the `Caller` for use with `Daw::new()` or `Daw::init()`.
    pub fn caller(&self) -> Caller {
        self.caller.clone()
    }
}
