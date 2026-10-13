//! Serialization logic for REAPER default preset assignment files.

use super::types::*;

/// Serialize a `DefPresets` back to the `.ini` file format.
pub fn serialize(dp: &DefPresets) -> String {
    let mut out = String::new();
    out.push_str("[defpresets]\n");
    for entry in &dp.entries {
        out.push_str(&format!("{}={}\n", entry.plugin_id, entry.preset_name));
    }
    out
}
