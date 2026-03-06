//! Write imported presets to the signal-library file structure.
//!
//! Produces a directory layout under `Library/blocks/plugin/{vendor}/{plugin}/`:
//!
//! ```text
//! manifest.json          — preset-level metadata
//! snapshots/
//!   {snapshot_id}.json   — per-snapshot metadata + parameters
//!   {snapshot_id}.bin    — binary state data (only for binary-format presets)
//! ```
//!
//! These files are the source of truth; the SQLite DB is a queryable cache.

use std::path::{Path, PathBuf};

use eyre::{Context, Result};
use serde::{Deserialize, Serialize};
use signal_proto::{Preset, Snapshot};

/// Manifest written to `manifest.json` at the preset directory root.
#[derive(Debug, Serialize, Deserialize)]
pub struct PresetManifest {
    pub id: String,
    pub name: String,
    pub block_type: String,
    pub vendor: String,
    pub source_plugin: Option<String>,
    pub default_snapshot_id: String,
    pub metadata: ManifestMetadata,
}

/// Metadata section of the manifest.
#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Per-snapshot JSON file.
#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotFile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    pub metadata: SnapshotFileMetadata,
    pub parameters: Vec<SnapshotParameter>,
    pub has_state_data: bool,
}

/// Metadata section of a snapshot file.
#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotFileMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// A parameter entry in the snapshot file.
#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotParameter {
    pub id: String,
    pub name: String,
    pub value: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daw_name: Option<String>,
}

/// Extract the `source:` tag value from a metadata tag list.
fn extract_source_tag(tags: &[String]) -> Option<String> {
    tags.iter()
        .find_map(|t| t.strip_prefix("source:").map(|s| s.to_string()))
}

/// Extract the `folder:` value from metadata, if present.
fn extract_folder(metadata: &signal_proto::metadata::Metadata) -> Option<String> {
    metadata.folder.clone()
}

/// Write a single snapshot to `snapshots/{id}.json` (and optionally `.bin`).
fn write_snapshot(snapshots_dir: &Path, snapshot: &Snapshot) -> Result<()> {
    let id_str = snapshot.id().to_string();
    let meta = snapshot.metadata();

    let snap_file = SnapshotFile {
        id: id_str.clone(),
        name: snapshot.name().to_string(),
        folder: extract_folder(meta),
        metadata: SnapshotFileMetadata {
            tags: meta.tags.as_slice().to_vec(),
        },
        parameters: snapshot
            .block()
            .parameters()
            .iter()
            .map(|p| SnapshotParameter {
                id: p.id().to_string(),
                name: p.name().to_string(),
                value: p.value().get(),
                daw_name: p.daw_name().map(|s| s.to_string()),
            })
            .collect(),
        has_state_data: snapshot.state_data().is_some(),
    };

    let json_path = snapshots_dir.join(format!("{id_str}.json"));
    let json_bytes = serde_json::to_string_pretty(&snap_file)
        .wrap_err("Failed to serialize snapshot JSON")?;
    std::fs::write(&json_path, json_bytes)
        .wrap_err_with(|| format!("Failed to write {}", json_path.display()))?;

    // Write binary state data if present
    if let Some(state_data) = snapshot.state_data() {
        let bin_path = snapshots_dir.join(format!("{id_str}.bin"));
        std::fs::write(&bin_path, state_data)
            .wrap_err_with(|| format!("Failed to write {}", bin_path.display()))?;
    }

    Ok(())
}

/// Write a complete preset to the library directory structure.
///
/// Creates:
/// ```text
/// {library_root}/blocks/plugin/{vendor}/{plugin_name}/
///   manifest.json
///   snapshots/
///     {default_snapshot_id}.json
///     {default_snapshot_id}.bin  (if binary)
///     {snapshot_id}.json
///     ...
/// ```
pub fn write_preset_to_library(
    library_root: &Path,
    vendor: &str,
    preset: &Preset,
) -> Result<()> {
    let vendor_lower = vendor.to_ascii_lowercase();
    let preset_dir = library_root
        .join("blocks")
        .join("plugin")
        .join(&vendor_lower)
        .join(preset.name());

    let snapshots_dir = preset_dir.join("snapshots");
    std::fs::create_dir_all(&snapshots_dir)
        .wrap_err_with(|| format!("Failed to create {}", snapshots_dir.display()))?;

    // Extract source plugin from preset metadata tags
    let source_plugin = extract_source_tag(preset.metadata().tags.as_slice());

    // Build manifest
    let default_snapshot = preset.default_snapshot();
    let manifest = PresetManifest {
        id: preset.id().to_string(),
        name: preset.name().to_string(),
        block_type: preset.block_type().display_name().to_string(),
        vendor: vendor.to_string(),
        source_plugin,
        default_snapshot_id: default_snapshot.id().to_string(),
        metadata: ManifestMetadata {
            tags: preset.metadata().tags.as_slice().to_vec(),
            description: preset.metadata().description.clone(),
        },
    };

    let manifest_path = preset_dir.join("manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .wrap_err("Failed to serialize manifest")?;
    std::fs::write(&manifest_path, manifest_json)
        .wrap_err_with(|| format!("Failed to write {}", manifest_path.display()))?;

    // Write default snapshot
    write_snapshot(&snapshots_dir, &default_snapshot)?;

    // Write additional snapshots
    for snapshot in preset.snapshots() {
        write_snapshot(&snapshots_dir, snapshot)?;
    }

    Ok(())
}

/// Compute the library directory path for a preset.
pub fn preset_library_path(library_root: &Path, vendor: &str, preset_name: &str) -> PathBuf {
    library_root
        .join("blocks")
        .join("plugin")
        .join(vendor.to_ascii_lowercase())
        .join(preset_name)
}
