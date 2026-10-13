//! In-process [`Daw`](daw_control::Daw) construction backed by
//! [`Standalone`] — thin wrapper over [`architect::LocalServer`].
//!
//! Serves `Standalone`'s [`architect::Services`] bundle over a vox
//! in-memory link and wraps the resulting `Caller` in
//! `daw_control::Daw` for any consumer that speaks the proto traits —
//! `SynchronizationEngine`, UI shells, tests, etc.
//!
//! ```ignore
//! use daw_standalone::sync::Standalone;
//! use daw_standalone::bootstrap::build_in_process_daw;
//!
//! let standalone = Standalone::new();
//! standalone.seed_project(ProjectInfo { ... });
//! let daw = build_in_process_daw(standalone).await?.daw;
//! daw.current_project().await?.transport().play().await?;
//! ```

use std::sync::Arc;

use architect::{LocalServer, Scope, Services};
use vox::Caller;

use crate::sync::Standalone;

/// Bundle of an in-process [`daw_control::Daw`] + the resources
/// keeping its server side alive. Clone the inner `Daw` freely; drop
/// the bundle to release the in-proc link.
#[derive(Clone)]
pub struct InProcessDaw {
    pub daw: daw_control::Daw,
    pub standalone: Standalone,
    /// Owns the acceptor task behind the in-memory link.
    _scope: Arc<Scope>,
}

impl InProcessDaw {
    pub fn caller(&self) -> &Caller {
        self.daw.caller()
    }
}

/// Build an in-process `Daw` client wired to `standalone`'s service
/// bundle (`architect::Services::into_router()`).
pub async fn build_in_process_daw(standalone: Standalone) -> eyre::Result<InProcessDaw> {
    let scope = Scope::new();
    let server = LocalServer::serve(standalone.clone().into_router(), Arc::clone(&scope));
    let caller = server.caller().await?;
    Ok(InProcessDaw {
        daw: daw_control::Daw::new(caller),
        standalone,
        _scope: scope,
    })
}
