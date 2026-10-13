//! Type definitions for REAPER default preset assignment files.

use serde::{Deserialize, Serialize};

/// A single mapping from a plugin identity string to a default preset name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefPresetEntry {
    /// Plugin identity string as used by REAPER (e.g. `"VST3: My Synth (Vendor)"`).
    pub plugin_id: String,
    /// Name of the preset to load by default.
    pub preset_name: String,
}

/// The complete contents of `reaper-defpresets.ini`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefPresets {
    /// All entries in file order.
    pub entries: Vec<DefPresetEntry>,
}
