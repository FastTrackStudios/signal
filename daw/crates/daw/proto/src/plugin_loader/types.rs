//! Plugin loader data types.

use facet::Facet;

/// Result of a plugin load operation.
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum PluginLoadResult {
    /// Plugin loaded and initialized successfully.
    Ok,
    /// Plugin loading failed.
    Error(String),
    /// Plugin was already loaded (idempotent).
    AlreadyLoaded,
}

/// Information about a loaded plugin.
#[derive(Debug, Clone, Facet)]
pub struct LoadedPluginInfo {
    /// Filesystem path the plugin was loaded from.
    pub path: String,
    /// Human-readable name derived from the path.
    pub name: String,
}
