//! Parsing logic for REAPER default preset assignment files.

use super::types::*;
use thiserror::Error;

/// Errors that can occur while parsing `reaper-defpresets.ini`.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("missing [defpresets] section")]
    MissingSection,
}

/// Parse the full contents of `reaper-defpresets.ini`.
pub fn parse(input: &str) -> Result<DefPresets, ParseError> {
    let mut in_section = false;
    let mut entries = Vec::new();

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = trimmed == "[defpresets]";
            continue;
        }
        if !in_section {
            continue;
        }
        // Split on first '=' only so preset names may contain '='.
        if let Some(eq) = trimmed.find('=') {
            let plugin_id = trimmed[..eq].to_string();
            let preset_name = trimmed[eq + 1..].to_string();
            entries.push(DefPresetEntry {
                plugin_id,
                preset_name,
            });
        }
    }

    Ok(DefPresets { entries })
}
