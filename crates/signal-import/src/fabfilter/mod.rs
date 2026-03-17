//! FabFilter preset importer.
//!
//! Scans `~/Documents/FabFilter/Presets/<PluginName>/` for `.ffp` files,
//! extracts metadata from text-format presets, and produces
//! `ImportedPresetCollection` values ready for the orchestrator.

pub mod parser;
pub mod registry;
pub mod rig_presets;
pub mod tags;

use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr};
use tracing::debug;

use crate::types::{DiscoveredPlugin, ImportedPresetCollection, ImportedSnapshot};
use registry::{FabFilterPluginEntry, FfpFormat, FABFILTER_PLUGINS};

/// Default location for FabFilter presets on macOS.
const DEFAULT_PRESETS_ROOT: &str = "~/Documents/FabFilter/Presets";

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

fn default_presets_root() -> PathBuf {
    expand_tilde(DEFAULT_PRESETS_ROOT)
}

/// FabFilter preset importer.
pub struct FabFilterImporter {
    presets_root: PathBuf,
}

impl FabFilterImporter {
    /// Create an importer using the default FabFilter preset location.
    pub fn new() -> Self {
        Self {
            presets_root: default_presets_root(),
        }
    }

    /// Create an importer with a custom root directory (useful for testing).
    pub fn with_root(root: PathBuf) -> Self {
        Self { presets_root: root }
    }

    /// Discover all FabFilter plugins that have preset directories on disk.
    pub fn discover_plugins(&self) -> Result<Vec<DiscoveredPlugin>> {
        let mut plugins = Vec::new();

        for entry in FABFILTER_PLUGINS {
            let dir = self.presets_root.join(entry.name);
            if !dir.is_dir() {
                continue;
            }

            let count = count_ffp_files(&dir);
            if count == 0 {
                continue;
            }

            plugins.push(DiscoveredPlugin {
                plugin_name: entry.name.to_string(),
                preset_dir: dir,
                block_type: entry.block_type,
                preset_count: count,
                is_text_format: entry.format == FfpFormat::Text,
            });
        }

        Ok(plugins)
    }

    /// Scan a specific plugin's preset directory and produce an `ImportedPresetCollection`.
    pub fn scan(&self, plugin_name: &str) -> Result<ImportedPresetCollection> {
        let entry = registry::lookup_plugin(plugin_name)
            .ok_or_else(|| eyre::eyre!("Unknown FabFilter plugin: {plugin_name}"))?;

        let dir = self.presets_root.join(entry.name);
        if !dir.is_dir() {
            eyre::bail!("Preset directory not found: {}", dir.display());
        }

        let snapshots = scan_directory(&dir, &dir, entry)?;

        Ok(ImportedPresetCollection {
            plugin_name: entry.name.to_string(),
            vendor: "FabFilter".to_string(),
            block_type: entry.block_type,
            snapshots,
        })
    }
}

/// Recursively count `.ffp` files in a directory.
fn count_ffp_files(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_ffp_files(&path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ffp"))
            {
                count += 1;
            }
        }
    }
    count
}

/// Recursively scan a directory for `.ffp` files and build `ImportedSnapshot` values.
fn scan_directory(
    dir: &Path,
    root: &Path,
    entry: &FabFilterPluginEntry,
) -> Result<Vec<ImportedSnapshot>> {
    let mut snapshots = Vec::new();

    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .wrap_err_with(|| format!("Failed to read directory: {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for fs_entry in entries {
        let path = fs_entry.path();

        if path.is_dir() {
            snapshots.extend(scan_directory(&path, root, entry)?);
            continue;
        }

        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ffp"))
        {
            continue;
        }

        let raw_bytes = std::fs::read(&path)
            .wrap_err_with(|| format!("Failed to read preset: {}", path.display()))?;

        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Compute folder from relative path (e.g. "Guitar/Clean")
        let folder = path
            .parent()
            .and_then(|p| p.strip_prefix(root).ok())
            .filter(|rel| !rel.as_os_str().is_empty())
            .map(|rel| rel.to_string_lossy().to_string());

        // Extract metadata and parameters from text-format presets
        let (author, description, vendor_tags, parameters) = if entry.format == FfpFormat::Text {
            match parser::parse_ffp_text(&String::from_utf8_lossy(&raw_bytes)) {
                Ok(parsed) => {
                    let params =
                        parser::extract_block_parameters(&parsed.signature, &parsed.parameters);
                    (parsed.author, parsed.description, parsed.tags, params)
                }
                Err(e) => {
                    debug!("Failed to parse text preset {}: {e}", path.display());
                    (None, None, Vec::new(), Vec::new())
                }
            }
        } else {
            (None, None, Vec::new(), Vec::new())
        };

        snapshots.push(ImportedSnapshot {
            name,
            folder,
            author,
            description,
            vendor_tags,
            raw_bytes,
            parameters,
            source_plugin: Some(entry.reaper_name.to_string()),
            store_raw_as_state: entry.format == FfpFormat::Binary, // binary .ffp = valid CLAP state
        });
    }

    Ok(snapshots)
}
