//! Parsing logic for REAPER external state persistence files.

use super::types::*;
use thiserror::Error;

/// Errors that can occur while parsing `reaper-extstate.ini`.
#[derive(Debug, Error)]
pub enum ParseError {
    // Currently all errors are handled leniently; this variant is reserved
    // for future stricter validation.
    #[error("parse error on line {line}: {content}")]
    InvalidLine { line: usize, content: String },
}

/// Parse the full contents of `reaper-extstate.ini`.
pub fn parse(input: &str) -> Result<ExtState, ParseError> {
    let mut sections: Vec<ExtStateSection> = Vec::new();
    let mut current: Option<ExtStateSection> = None;

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if let Some(sec) = current.take() {
                sections.push(sec);
            }
            current = Some(ExtStateSection {
                name: trimmed[1..trimmed.len() - 1].to_string(),
                entries: Vec::new(),
            });
        } else if let Some(eq) = trimmed.find('=') {
            // Use splitn so values containing '=' are preserved intact.
            let key = trimmed[..eq].to_string();
            let value = trimmed[eq + 1..].to_string();
            if let Some(sec) = current.as_mut() {
                sec.entries.push(ExtStateEntry { key, value });
            }
            // Lines before any section header are silently ignored.
        }
    }
    if let Some(sec) = current {
        sections.push(sec);
    }

    Ok(ExtState { sections })
}
