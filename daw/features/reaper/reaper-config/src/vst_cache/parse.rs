//! Parsing logic for REAPER VST/DX plugin cache files.

use super::types::*;
use thiserror::Error;

/// Errors that can occur while parsing a VST plugin cache file.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("missing [vstcache] section")]
    MissingSection,
    #[error("malformed entry on line {line}: '{content}'")]
    MalformedEntry { line: usize, content: String },
    #[error("invalid timestamp on line {line}: {source}")]
    InvalidTimestamp {
        line: usize,
        #[source]
        source: std::num::ParseIntError,
    },
}

/// Parse the full contents of a `reaper-vstplugins*.ini` file.
pub fn parse(input: &str) -> Result<VstPluginsCache, ParseError> {
    let mut in_vstcache = false;
    let mut entries = Vec::new();

    for (line_idx, line) in input.lines().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_vstcache = trimmed == "[vstcache]";
            continue;
        }
        if !in_vstcache {
            continue;
        }
        // key=value line
        let eq = trimmed
            .find('=')
            .ok_or_else(|| ParseError::MalformedEntry {
                line: line_num,
                content: trimmed.to_string(),
            })?;
        let filename = trimmed[..eq].to_string();
        let value = &trimmed[eq + 1..];

        // value format: timestamp,vst_id,Name[!!!vsti]
        let mut parts = value.splitn(3, ',');
        let ts_str = parts.next().ok_or_else(|| ParseError::MalformedEntry {
            line: line_num,
            content: value.to_string(),
        })?;
        let vst_id = parts.next().ok_or_else(|| ParseError::MalformedEntry {
            line: line_num,
            content: value.to_string(),
        })?;
        let name_raw = parts.next().ok_or_else(|| ParseError::MalformedEntry {
            line: line_num,
            content: value.to_string(),
        })?;

        let timestamp = ts_str
            .parse::<u64>()
            .map_err(|e| ParseError::InvalidTimestamp {
                line: line_num,
                source: e,
            })?;

        let (name, is_instrument) = if let Some(base) = name_raw.strip_suffix("!!!vsti") {
            (base.to_string(), true)
        } else {
            (name_raw.to_string(), false)
        };

        entries.push(VstPlugin {
            filename,
            timestamp,
            vst_id: vst_id.to_string(),
            name,
            is_instrument,
        });
    }

    Ok(VstPluginsCache { entries })
}
