//! WindowManager — high-level client wrapper around [`WindowManagerService`].
//!
//! Layouts are stored in REAPER's native `reaper-screensets.ini` so REAPER's
//! own `Screenset: Load #N` action drives the actual window/dock work. This
//! handle is the typed facade for naming + applying those layouts from
//! application code.

use crate::DawClients;
use daw_proto::window_manager::{
    WindowLayout, WindowLayoutOptions, WindowLayoutResult, WindowLayoutSummary,
};
use std::sync::Arc;

/// Handle for applying and inspecting named workspace layouts.
pub struct WindowManager {
    clients: Arc<DawClients>,
}

impl WindowManager {
    pub(crate) fn new(clients: Arc<DawClients>) -> Self {
        Self { clients }
    }

    /// Apply a layout by name. Resolves the name against the layouts
    /// stored in `reaper-screensets.ini` and fires REAPER's native
    /// `Screenset: Load #N` action for the matching slot.
    pub async fn apply(
        &self,
        name: &str,
        options: WindowLayoutOptions,
    ) -> crate::Result<WindowLayoutResult> {
        Ok(self
            .clients
            .window_manager
            .apply_layout(name.to_string(), options)
            .await?)
    }

    /// All layouts currently stored in `reaper-screensets.ini`.
    pub async fn list(&self) -> crate::Result<Vec<WindowLayoutSummary>> {
        Ok(self.clients.window_manager.list_layouts().await?)
    }

    /// Most recently applied layout, tracked Rust-side. `None` until the
    /// first `apply` call in this process lifetime.
    pub async fn current(&self) -> crate::Result<Option<WindowLayoutSummary>> {
        Ok(self.clients.window_manager.current_layout().await?)
    }

    /// Fetch a full layout definition by name.
    pub async fn get(&self, name: &str) -> crate::Result<Option<WindowLayout>> {
        Ok(self
            .clients
            .window_manager
            .get_layout(name.to_string())
            .await?)
    }

    /// Persist a layout definition. Captures the toolbar metadata on our
    /// side; the REAPER screenset body is whatever REAPER had at capture
    /// time (caller fires "Screenset: Save #N" alongside this to snapshot
    /// live state).
    pub async fn save(&self, layout: WindowLayout) -> crate::Result<WindowLayoutResult> {
        Ok(self.clients.window_manager.save_layout(layout).await?)
    }

    /// Remove a layout by name. Frees the underlying REAPER screenset slot.
    pub async fn delete(&self, name: &str) -> crate::Result<WindowLayoutResult> {
        Ok(self
            .clients
            .window_manager
            .delete_layout(name.to_string())
            .await?)
    }
}
