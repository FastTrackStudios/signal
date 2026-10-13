//! Parsing logic for REAPER plugin preset files.

use super::types::*;
use thiserror::Error;

/// Errors that can occur while parsing a plugin preset file.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("missing [General] section")]
    MissingGeneral,
    #[error("missing required key '{key}' in [General]")]
    MissingGeneralKey { key: &'static str },
    #[error("invalid integer value for '{key}': {source}")]
    InvalidInt {
        key: String,
        #[source]
        source: std::num::ParseIntError,
    },
    #[error("missing required key '{key}' in preset [{index}]")]
    MissingPresetKey { key: &'static str, index: usize },
}

/// Parse the full contents of a plugin preset INI file.
pub fn parse(input: &str) -> Result<PluginPresets, ParseError> {
    // Split into sections at `[SectionHeader]` lines.
    let mut sections: Vec<(&str, Vec<(&str, &str)>)> = Vec::new();
    let mut current_name: Option<&str> = None;
    let mut current_kv: Vec<(&str, &str)> = Vec::new();

    for line in input.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            if let Some(name) = current_name.take() {
                sections.push((name, std::mem::take(&mut current_kv)));
            }
            current_name = Some(&line[1..line.len() - 1]);
        } else if let Some(eq) = line.find('=') {
            let k = &line[..eq];
            let v = &line[eq + 1..];
            current_kv.push((k, v));
        }
    }
    if let Some(name) = current_name {
        sections.push((name, current_kv));
    }

    // Find [General].
    let general_kv = sections
        .iter()
        .find(|(name, _)| *name == "General")
        .map(|(_, kv)| kv)
        .ok_or(ParseError::MissingGeneral)?;

    let last_def_imp_time = find_key(general_kv, "LastDefImpTime")
        .ok_or(ParseError::MissingGeneralKey {
            key: "LastDefImpTime",
        })?
        .parse::<u64>()
        .map_err(|e| ParseError::InvalidInt {
            key: "LastDefImpTime".into(),
            source: e,
        })?;

    let nb_presets = find_key(general_kv, "NbPresets")
        .ok_or(ParseError::MissingGeneralKey { key: "NbPresets" })?
        .parse::<u32>()
        .map_err(|e| ParseError::InvalidInt {
            key: "NbPresets".into(),
            source: e,
        })?;

    // Parse numbered preset sections.
    let mut presets = Vec::new();
    for i in 0..nb_presets as usize {
        let section_name = i.to_string();
        let kv = sections
            .iter()
            .find(|(name, _)| *name == section_name.as_str())
            .map(|(_, kv)| kv)
            .ok_or(ParseError::MissingPresetKey {
                key: "section",
                index: i,
            })?;

        let name = find_key(kv, "Name")
            .ok_or(ParseError::MissingPresetKey {
                key: "Name",
                index: i,
            })?
            .to_string();
        let len = find_key(kv, "Len")
            .ok_or(ParseError::MissingPresetKey {
                key: "Len",
                index: i,
            })?
            .parse::<u32>()
            .map_err(|e| ParseError::InvalidInt {
                key: "Len".into(),
                source: e,
            })?;
        let data = find_key(kv, "Data")
            .ok_or(ParseError::MissingPresetKey {
                key: "Data",
                index: i,
            })?
            .to_string();

        presets.push(Preset { name, len, data });
    }

    Ok(PluginPresets {
        last_def_imp_time,
        nb_presets,
        presets,
    })
}

fn find_key<'a>(kv: &[(&str, &'a str)], key: &str) -> Option<&'a str> {
    kv.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}
