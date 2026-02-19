//! Import Neural DSP catalog from disk into domain `Preset`/`Snapshot` types.
//!
//! Reads the file-based catalog at `~/Music/FastTrackStudio/Library/` (produced
//! by `cargo xtask catalog`) and converts each NDSP plugin into a `Preset`
//! with `BlockType::Custom`, one `Snapshot` per factory preset.
//!
//! If the catalog directory is missing, returns an empty `Vec` gracefully.

use std::path::{Path, PathBuf};

use signal_proto::catalog::{BlockMetadata, Catalog, SnapshotMetadata};
use signal_proto::metadata::Metadata;
use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

/// Read the catalog from `library_path` and return one `Preset` per NDSP plugin.
///
/// Each plugin becomes a `Preset` with `BlockType::Custom`. Each factory preset
/// on disk becomes a `Snapshot` within that collection.
///
/// Returns an empty `Vec` if the catalog directory doesn't exist.
pub fn catalog_block_collections(library_path: &Path) -> Vec<Preset> {
    let catalog_path = library_path.join("catalog.json");
    if !catalog_path.exists() {
        return Vec::new();
    }

    let catalog_json = match std::fs::read_to_string(&catalog_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[signal-storage] Failed to read catalog.json: {e}");
            return Vec::new();
        }
    };

    let catalog: Catalog = match serde_json::from_str(&catalog_json) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[signal-storage] Failed to parse catalog.json: {e}");
            return Vec::new();
        }
    };

    let mut presets = Vec::new();

    for plugin in &catalog.plugins {
        let block_dir = library_path
            .join("blocks/plugin/neural-dsp")
            .join(&plugin.slug);

        // Read block.json for plugin metadata
        let block_json_path = block_dir.join("block.json");
        let block_meta: Option<BlockMetadata> = std::fs::read_to_string(&block_json_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());

        let plugin_name = block_meta
            .as_ref()
            .map(|m| m.name.clone())
            .unwrap_or_else(|| plugin.name.clone());

        // Collect all snapshot JSONs recursively
        let snapshots_dir = block_dir.join("snapshots");
        let mut snapshot_metas = Vec::new();
        if snapshots_dir.exists() {
            collect_snapshot_metas(&snapshots_dir, &mut snapshot_metas);
        }

        if snapshot_metas.is_empty() {
            continue;
        }

        // Sort alphabetically by name for stable ordering
        snapshot_metas.sort_by(|(a, _), (b, _)| a.name.cmp(&b.name));

        // Convert to domain Snapshots
        let domain_snapshots: Vec<Snapshot> = snapshot_metas
            .iter()
            .map(|(meta, meta_dir)| {
                // Include folder in seed key to avoid collisions — multiple
                // folders can contain presets with the same name (e.g., "Clean"
                // in both "Artists/Plini" and "Artists/Ryan Lerman").
                let seed_key = if meta.folder.is_empty() {
                    format!("ndsp-{}-{}", plugin.slug, meta.id)
                } else {
                    format!(
                        "ndsp-{}-{}-{}",
                        plugin.slug,
                        signal_proto::catalog::slugify(&meta.folder),
                        meta.id
                    )
                };
                let snapshot_id = seed_id(&seed_key);
                let mut metadata = Metadata::new().with_tag("Neural DSP");

                if !meta.folder.is_empty() {
                    metadata = metadata.with_folder(&meta.folder);
                }
                for tag in &meta.tags {
                    metadata = metadata.with_tag(tag);
                }

                // Convert fingerprint params to BlockParameter values.
                // Numeric values → f32, booleans → 0.0/1.0, unparseable → skip.
                let block_params: Vec<BlockParameter> = meta
                    .fingerprint
                    .params
                    .iter()
                    .filter_map(|(name, value_str)| {
                        let value: f32 = if value_str == "true" {
                            1.0
                        } else if value_str == "false" {
                            0.0
                        } else {
                            value_str.parse::<f64>().ok()? as f32
                        };
                        Some(BlockParameter::new(name, name, value))
                    })
                    .collect();

                // Prefer REAPER VST chunk (from harvest) over raw NDSP binary.
                // The .chunk file contains the exact blob that set_vst_chunk expects.
                let state_data = meta
                    .reaper_chunk_file
                    .as_ref()
                    .and_then(|f| std::fs::read(meta_dir.join(f)).ok())
                    .or_else(|| {
                        if !meta.state_file.is_empty() {
                            std::fs::read(meta_dir.join(&meta.state_file)).ok()
                        } else {
                            None
                        }
                    });

                let snapshot = Snapshot::new(
                    snapshot_id,
                    &meta.name,
                    Block::from_parameters(block_params),
                )
                .with_metadata(metadata);

                if let Some(data) = state_data {
                    snapshot.with_state_data(data)
                } else {
                    snapshot
                }
            })
            .collect();

        // First snapshot is default, rest are additional
        let default = domain_snapshots[0].clone();
        let additional: Vec<Snapshot> = domain_snapshots.into_iter().skip(1).collect();

        let preset_id = seed_id(&format!("ndsp-{}", plugin.slug));
        let preset_metadata = Metadata::new()
            .with_tag("Neural DSP")
            .with_tag(&plugin_name);

        let preset = Preset::new(
            preset_id,
            &plugin_name,
            BlockType::Custom,
            default,
            additional,
        )
        .with_metadata(preset_metadata);

        presets.push(preset);
    }

    presets
}

/// Import `.RfxChain` files from the library into `Preset`/`Snapshot` types.
///
/// Walks two locations:
/// - `blocks/plugin/neural-dsp/<Plugin Name>/` — per-plugin presets
/// - `profiles/<profile-name>/` — profile-level patches
///
/// Each plugin folder becomes a `Preset`, each `.RfxChain` file becomes a
/// `Snapshot` with raw file bytes as `state_data`. No JSON metadata needed —
/// the rfxchain IS the data.
///
/// Returns an empty `Vec` if no rfxchain files are found.
pub fn rfxchain_block_collections(library_path: &Path) -> Vec<Preset> {
    let mut presets = Vec::new();

    // ── blocks/plugin/neural-dsp/<Plugin Name>/*.RfxChain ──
    let ndsp_dir = library_path.join("blocks/plugin/neural-dsp");
    if ndsp_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&ndsp_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let plugin_name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                let plugin_slug = signal_proto::catalog::slugify(&plugin_name);

                if let Some(preset) =
                    rfxchain_preset_from_dir(&path, &plugin_name, &plugin_slug, "ndsp")
                {
                    presets.push(preset);
                }
            }
        }
    }

    // ── presets/<name>/*.RfxChain ──
    let presets_dir = library_path.join("presets");
    if presets_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&presets_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let preset_name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                let preset_slug = signal_proto::catalog::slugify(&preset_name);

                if let Some(preset) =
                    rfxchain_preset_from_dir(&path, &preset_name, &preset_slug, "preset")
                {
                    presets.push(preset);
                }
            }
        }
    }

    presets
}

/// Build a `Preset` from all `.RfxChain` files in a directory.
///
/// `prefix` is either `"ndsp"` or `"profile"` — used to namespace seed keys:
/// - ndsp: `seed_id("ndsp-<plugin-slug>")` / `seed_id("ndsp-<plugin-slug>-<preset-slug>")`
/// - profile: `seed_id("profile-<profile-slug>")` / `seed_id("profile-<profile-slug>-<preset-slug>")`
fn rfxchain_preset_from_dir(
    dir: &Path,
    display_name: &str,
    slug: &str,
    prefix: &str,
) -> Option<Preset> {
    let mut rfx_files: Vec<(String, PathBuf)> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ext == "rfxchain" {
                let name = path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Unknown")
                    .to_string();
                rfx_files.push((name, path));
            }
        }
    }

    if rfx_files.is_empty() {
        return None;
    }

    // Sort alphabetically for stable ordering
    rfx_files.sort_by(|(a, _), (b, _)| a.cmp(b));

    let snapshots: Vec<Snapshot> = rfx_files
        .iter()
        .map(|(name, path)| {
            let preset_slug = signal_proto::catalog::slugify(name);
            let seed_key = format!("{prefix}-{slug}-{preset_slug}");
            let snapshot_id = seed_id(&seed_key);
            let state_data = std::fs::read(path).ok();

            let snapshot = Snapshot::new(snapshot_id, name, Block::from_parameters(vec![]))
                .with_metadata(Metadata::new().with_tag("RfxChain"));

            if let Some(data) = state_data {
                snapshot.with_state_data(data)
            } else {
                snapshot
            }
        })
        .collect();

    let default = snapshots[0].clone();
    let additional: Vec<Snapshot> = snapshots.into_iter().skip(1).collect();

    let preset_id = seed_id(&format!("{prefix}-{slug}"));
    let preset = Preset::new(
        preset_id,
        display_name,
        BlockType::Custom,
        default,
        additional,
    )
    .with_metadata(Metadata::new().with_tag("RfxChain").with_tag(display_name));

    Some(preset)
}

/// Recursively walk a directory collecting `SnapshotMetadata` + parent dir from `*.json` files.
fn collect_snapshot_metas(dir: &Path, out: &mut Vec<(SnapshotMetadata, PathBuf)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_snapshot_metas(&path, out);
        } else if path.extension().map_or(false, |ext| ext == "json") {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(meta) = serde_json::from_str::<SnapshotMetadata>(&contents) {
                    let parent = path.parent().unwrap_or(dir).to_path_buf();
                    out.push((meta, parent));
                }
            }
        }
    }
}
