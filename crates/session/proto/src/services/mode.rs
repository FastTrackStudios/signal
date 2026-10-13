//! Session mode control (Organize / Write / Produce / Record / …).

use super::error::SessionServiceError;
use vox::Tx;

// ─── Mode control ──────────────────────────────────────────────────────
//
// FTS session modes (Organize / Write / Produce / Record / …). The data
// enum lives in the `session` crate (it carries layout / toolbar logic
// that doesn't belong in -proto). On the wire we use the stable
// lowercase slug — every implementation calls `Mode::from_slug` to
// reconstitute.

/// Service for inspecting + switching the active FTS session mode.
///
/// Mounted by `fts-extensions` and consumed by the CLI / desktop /
/// any other Vox peer (e.g. a mobile app). State is host-process-
/// global; per-project mode is a future extension.
pub mod session_mode_service {
    use super::{SessionServiceError, Tx};

    #[architect::rpc]
    pub trait SessionModeService {
        /// Lowercase slug of the currently active mode.
        /// E.g. `"organize"`, `"record"`.
        async fn current_mode(&self) -> Result<String, SessionServiceError>;

        /// Switch the active mode by slug. Idempotent if `slug`
        /// matches the current mode. Errors with `NotFound` if the
        /// slug doesn't map to a known mode.
        async fn set_mode(&self, slug: String) -> Result<(), SessionServiceError>;

        /// All known mode slugs in declaration order, for menus / CLI
        /// completion. Returned even when no mode change has happened
        /// yet (static set).
        async fn list_modes(&self) -> Result<Vec<String>, SessionServiceError>;

        /// Mode changes, as they happen: the new slug each time the
        /// active mode flips — `set_mode` over RPC, the in-REAPER hotkey
        /// action, or `restore_persisted_mode` at startup. Zero polling.
        #[subscribe]
        fn mode_changes(&self) -> String;
    }
}

pub use session_mode_service::{
    Service as SessionModeServiceLayer, SessionModeService, SessionModeServiceClient,
    SessionModeServiceDispatcher, layer as session_mode_service_layer,
    serve as serve_session_mode_service, session_mode_service_rpc_service_descriptor,
    session_mode_service_service_descriptor,
};
