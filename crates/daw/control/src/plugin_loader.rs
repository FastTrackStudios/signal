//! Plugin Loader — eager plugin loading through the DAW service API
//!
//! Client-side handle for requesting REAPER to load plugins into its address space.

use crate::DawClients;
use std::sync::Arc;

/// Handle for loading plugins into the host DAW process.
pub struct PluginLoader {
    clients: Arc<DawClients>,
}

impl PluginLoader {
    pub(crate) fn new(clients: Arc<DawClients>) -> Self {
        Self { clients }
    }

    /// Load a plugin from the given filesystem path.
    ///
    /// The path should point to the actual binary inside the plugin bundle.
    /// Returns `Ok` on success, `AlreadyLoaded` if idempotent, or `Error`.
    pub async fn load_plugin(
        &self,
        plugin_path: &str,
    ) -> crate::Result<daw_proto::PluginLoadResult> {
        Ok(self
            .clients
            .plugin_loader
            .load(plugin_path.to_string())
            .await?)
    }

    /// List all currently loaded plugins.
    pub async fn list_loaded(&self) -> crate::Result<Vec<daw_proto::LoadedPluginInfo>> {
        Ok(self.clients.plugin_loader.list_loaded().await?)
    }

    /// Check if a plugin at the given path is already loaded.
    pub async fn is_loaded(&self, plugin_path: &str) -> crate::Result<bool> {
        Ok(self
            .clients
            .plugin_loader
            .is_loaded(plugin_path.to_string())
            .await?)
    }
}
