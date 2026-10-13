//! Resources — REAPER's resource, ini and color-theme paths.
//!
//! Also the theme-development reload: REAPER re-reads `rtconfig.txt` and the
//! image folder every time a theme is opened, so re-opening the active theme
//! *is* a reload. There is no REAPER action for this — only the tweak
//! window's button — so this service is the way to drive it from a script.

use crate::DawClients;
use std::path::PathBuf;
use std::sync::Arc;

/// Handle for REAPER's resource paths and theme loading.
pub struct Resources {
    clients: Arc<DawClients>,
}

impl Resources {
    pub(crate) fn new(clients: Arc<DawClients>) -> Self {
        Self { clients }
    }

    /// REAPER's resource directory.
    pub async fn resource_path(&self) -> crate::Result<PathBuf> {
        Ok(self.clients.resource.resource_path().await?.into())
    }

    /// Path to `reaper.ini`.
    pub async fn ini_file_path(&self) -> crate::Result<PathBuf> {
        Ok(self.clients.resource.ini_file_path().await?.into())
    }

    /// The active color theme, or `None` when REAPER's default is in use.
    pub async fn color_theme_path(&self) -> crate::Result<Option<PathBuf>> {
        Ok(self
            .clients
            .resource
            .color_theme_path()
            .await?
            .map(PathBuf::from))
    }

    /// Load a color theme from disk.
    pub async fn load_color_theme(&self, path: impl Into<PathBuf>) -> crate::Result<bool> {
        let path = path.into().display().to_string();
        Ok(self.clients.resource.load_color_theme(Some(path)).await?)
    }

    /// Reload whatever theme is already active — the edit/see loop.
    pub async fn reload_color_theme(&self) -> crate::Result<bool> {
        Ok(self.clients.resource.load_color_theme(None).await?)
    }
}
