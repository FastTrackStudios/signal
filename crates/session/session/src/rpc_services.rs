//! Vox RPC service implementations for session control surfaces.
//!
//! One service today: [`SessionModeServiceImpl`] — get / set / list FTS
//! session modes. (Take-ranking and record-control moved to
//! `daw_actions`, next to the handlers they drive.)
//!
//! Every call bounces to REAPER's main thread via
//! `daw_reaper::main_thread::query` because the underlying handlers
//! call raw REAPER FFI that's main-thread-only. The async RPC
//! dispatcher runs on the tokio runtime; without the bounce, calling
//! e.g. `set_mode` from a remote client would touch REAPER from the
//! wrong thread.

use daw_proto::main_thread;
use session_proto::SessionServiceError;
use session_proto::services::SessionModeService;
use tokio::sync::broadcast::error::RecvError;

use crate::modes::{self as mode_actions, Mode};

// ─── SessionModeService ─────────────────────────────────────────────────

#[derive(Clone)]
pub struct SessionModeServiceImpl {
    /// `#[subscribe]` hub for mode changes; a pump bridges the internal
    /// broadcast into it (replay 1 = late subscribers get current mode).
    modes_hub: architect::PubSub<String>,
}

impl Default for SessionModeServiceImpl {
    fn default() -> Self {
        let hub = architect::PubSub::sliding(1);
        // Seed + pump: current mode first, then every flip.
        hub.publish(mode_actions::current_mode().slug().to_string());
        let pump = hub.clone();
        let mut rx = mode_actions::mode_broadcast().subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(slug) => {
                        pump.publish(slug);
                    }
                    Err(RecvError::Closed) => return,
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "SessionMode pump lagged");
                    }
                }
            }
        });
        Self { modes_hub: hub }
    }
}

impl session_proto::services::session_mode_service::SessionModeServiceStreamSource
    for SessionModeServiceImpl
{
    fn mode_changes_hub(&self) -> &architect::PubSub<String> {
        &self.modes_hub
    }
}

impl SessionModeService for SessionModeServiceImpl {
    async fn current_mode(&self) -> Result<String, SessionServiceError> {
        Ok(mode_actions::current_mode().slug().to_string())
    }

    async fn set_mode(&self, slug: String) -> Result<(), SessionServiceError> {
        let mode = Mode::from_slug(&slug).ok_or_else(|| SessionServiceError::NotFound {
            entity: "Mode".to_string(),
            id: slug.clone(),
        })?;
        main_thread::query(move || mode_actions::set_mode(mode))
            .await
            .ok_or_else(|| {
                SessionServiceError::DawError("TaskSupport not initialised".to_string())
            })?;
        Ok(())
    }

    async fn list_modes(&self) -> Result<Vec<String>, SessionServiceError> {
        Ok(Mode::ALL.iter().map(|m| m.slug().to_string()).collect())
    }
}
