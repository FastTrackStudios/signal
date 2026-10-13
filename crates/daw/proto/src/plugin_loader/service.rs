//! Plugin loader service — eagerly load REAPER plugins.

use super::{LoadedPluginInfo, PluginLoadResult};

#[architect::rpc]
pub trait PluginLoading {
    /// Load a plugin from the given filesystem path.
    fn load(&self, path: &str) -> PluginLoadResult;

    /// List all currently loaded plugins.
    fn list_loaded(&self) -> Vec<LoadedPluginInfo>;

    /// Check if a plugin at the given path is already loaded.
    fn is_loaded(&self, path: &str) -> bool;
}
