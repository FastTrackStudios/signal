//! Type definitions for REAPER VST/DX plugin cache files.

use serde::{Deserialize, Serialize};

/// A single plugin cache entry from the `[vstcache]` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VstPlugin {
    /// Absolute path or filename of the plugin binary.
    pub filename: String,
    /// File modification timestamp as stored by REAPER.
    pub timestamp: u64,
    /// VST unique identifier (hex string).
    pub vst_id: String,
    /// Human-readable plugin name.
    pub name: String,
    /// `true` if the plugin is an instrument (value ends with `!!!vsti`).
    pub is_instrument: bool,
}

/// The complete contents of a `reaper-vstplugins*.ini` file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VstPluginsCache {
    /// All plugin entries in file order.
    pub entries: Vec<VstPlugin>,
}
