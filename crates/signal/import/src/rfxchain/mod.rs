//! RfxChain importer for signal-library preset directories.
//!
//! Each signal-library block directory contains:
//! - `preset.rfxchain` — REAPER FX chain chunk
//! - `raw.json` — metadata JSON with name, description, etc.
//!
//! Subdirectories under a block type represent individual presets/variations.

use std::path::Path;

use eyre::{Result, WrapErr};
use signal_proto::block::BlockType;

use crate::types::{ImportedPresetCollection, ImportedSnapshot};

/// RfxChain preset importer for signal-library directories.
pub struct RfxChainImporter;

impl RfxChainImporter {
    /// Scan a signal-library block directory.
    ///
    /// Expects a structure like:
    /// ```text
    /// source_dir/
    ///   preset_name_a/
    ///     preset.rfxchain
    ///     raw.json
    ///   preset_name_b/
    ///     preset.rfxchain
    ///     raw.json
    /// ```
    pub fn scan(
        source_dir: &Path,
        block_type: BlockType,
        plugin_name: Option<&str>,
    ) -> Result<ImportedPresetCollection> {
        if !source_dir.is_dir() {
            eyre::bail!("Source directory not found: {}", source_dir.display());
        }

        let collection_name = plugin_name
            .map(String::from)
            .or_else(|| {
                source_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| "rfxchain-import".to_string());

        let mut snapshots = Vec::new();

        let mut entries: Vec<_> = std::fs::read_dir(source_dir)
            .wrap_err("Failed to read source directory")?
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let rfxchain_path = path.join("preset.rfxchain");
            if !rfxchain_path.exists() {
                continue;
            }

            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let raw_bytes = std::fs::read(&rfxchain_path)
                .wrap_err_with(|| format!("Failed to read {}", rfxchain_path.display()))?;

            // Try to read description from raw.json
            let description = read_raw_json_description(&path.join("raw.json"));

            snapshots.push(ImportedSnapshot {
                name,
                folder: None,
                author: None,
                description,
                vendor_tags: Vec::new(),
                raw_bytes,
                parameters: Vec::new(),
                source_plugin: None,
                store_raw_as_state: true, // rfxchain bytes ARE REAPER state chunks
            });
        }

        Ok(ImportedPresetCollection {
            plugin_name: collection_name,
            vendor: "signal-library".to_string(),
            block_type,
            snapshots,
        })
    }
}

/// Try to extract a description from a `raw.json` file.
fn read_raw_json_description(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("description")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
}
