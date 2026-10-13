//! Serialization logic for REAPER external state persistence files.

use super::types::*;

/// Serialize an `ExtState` back to the `.ini` file format.
pub fn serialize(state: &ExtState) -> String {
    let mut out = String::new();
    for section in &state.sections {
        out.push_str(&format!("[{}]\n", section.name));
        for entry in &section.entries {
            out.push_str(&format!("{}={}\n", entry.key, entry.value));
        }
        out.push('\n');
    }
    out
}
