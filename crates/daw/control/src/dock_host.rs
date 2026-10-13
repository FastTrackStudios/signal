//! Dock Host — High-level client wrapper around [`DockHostService`].
//!
//! Provides ergonomic access for tests and extensions that need to drive
//! the dock host (register/show/hide/persist) over the same vox client
//! transport they already use for ActionRegistry / Track / etc.

use crate::DawClients;
use daw_proto::dock_host::{DockHandle, DockKind, PanelPixels, UiEventDto};
use std::sync::Arc;

/// Handle for registering and toggling docks via the host adapter.
pub struct DockHost {
    clients: Arc<DawClients>,
}

impl DockHost {
    pub(crate) fn new(clients: Arc<DawClients>) -> Self {
        Self { clients }
    }

    /// Register (or look up) a dock by stable string id. Re-registering
    /// the same id returns the existing handle.
    pub async fn register_dock(
        &self,
        id: &str,
        title: &str,
        kind: DockKind,
    ) -> crate::Result<DockHandle> {
        Ok(self
            .clients
            .dock_host
            .register_dock(id.to_string(), title.to_string(), kind)
            .await?)
    }

    /// Drop a previously-registered dock. Returns false if the handle
    /// was already gone.
    pub async fn unregister_dock(&self, handle: DockHandle) -> crate::Result<bool> {
        Ok(self.clients.dock_host.unregister_dock(handle).await?)
    }

    pub async fn show(&self, handle: DockHandle) -> crate::Result<()> {
        self.clients.dock_host.show(handle).await?;
        Ok(())
    }

    pub async fn hide(&self, handle: DockHandle) -> crate::Result<()> {
        self.clients.dock_host.hide(handle).await?;
        Ok(())
    }

    /// Toggle visibility. Returns the new state.
    pub async fn toggle(&self, handle: DockHandle) -> crate::Result<bool> {
        Ok(self.clients.dock_host.toggle(handle).await?)
    }

    pub async fn is_visible(&self, handle: DockHandle) -> crate::Result<bool> {
        Ok(self.clients.dock_host.is_visible(handle).await?)
    }

    /// Persist the layout to whatever store the adapter uses (for the
    /// REAPER adapter this is `ExtState`; the returned blob may be empty).
    pub async fn save_layout(&self) -> crate::Result<Vec<u8>> {
        Ok(self.clients.dock_host.save_layout().await?)
    }

    /// Restore a previously-saved layout. Returns `false` if the blob
    /// was unrecognized.
    pub async fn restore_layout(&self, blob: Vec<u8>) -> crate::Result<bool> {
        Ok(self.clients.dock_host.restore_layout(blob).await?)
    }

    // `subscribe_events` retired with the architect::rpc port —
    // dock event streaming lives on a sibling trait.

    /// Capture the current rendered pixels of a panel.
    ///
    /// Returns `None` if no panel is mounted for `handle` or if it has
    /// not completed its first render tick yet (no readback bytes).
    pub async fn capture_pixels(&self, handle: DockHandle) -> crate::Result<Option<PanelPixels>> {
        Ok(self.clients.dock_host.capture_panel_pixels(handle).await?)
    }

    /// Inject a synthetic UI event into a panel for interaction tests.
    /// Returns `false` if the handle has no live panel mounted.
    pub async fn inject_event(&self, handle: DockHandle, event: UiEventDto) -> crate::Result<bool> {
        Ok(self
            .clients
            .dock_host
            .inject_ui_event(handle, event)
            .await?)
    }
}
