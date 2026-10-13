//! Serialization logic for REAPER VST/DX plugin cache files.

use super::types::*;

/// Serialize a `VstPluginsCache` back to the `.ini` file format.
pub fn serialize(cache: &VstPluginsCache) -> String {
    let mut out = String::new();
    out.push_str("[vstcache]\n");
    for entry in &cache.entries {
        let suffix = if entry.is_instrument { "!!!vsti" } else { "" };
        out.push_str(&format!(
            "{}={},{},{}{}\n",
            entry.filename, entry.timestamp, entry.vst_id, entry.name, suffix
        ));
    }
    out
}
