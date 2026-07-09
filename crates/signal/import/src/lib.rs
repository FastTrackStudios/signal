//! Signal preset import system -- converts vendor plugin presets into the signal domain.
//!
//! Converts vendor plugin presets (FabFilter, rfxchain, etc.) into Signal's
//! `Preset` / `Snapshot` model. Each vendor importer produces an
//! `ImportedPresetCollection` which the orchestrator converts and persists
//! via [`SignalController`](signal_controller::SignalController).
//!
//! # Architecture position
//!
//! ```text
//! signal-proto + signal-controller
//!           |
//!           v
//!   signal-import (this crate)
//!           |
//!           v
//!   signal-daw-bridge, signal (facade)
//! ```
//!
//! **Depends on**: `signal-proto`, `signal-controller`
//!
//! **Depended on by**: `signal-daw-bridge`, `signal` (facade, via dev/test path)
//!
//! # Key types and functions
//!
//! - [`import_presets`] / [`import_presets_with_library`] -- orchestrator that
//!   converts an `ImportedPresetCollection` into persisted `Preset`/`Snapshot` entities
//! - [`import_preset_id`] -- deterministic UUID v5 generation for idempotent re-imports
//! - [`dry_run_report`] -- preview what an import would produce without persisting
//! - [`types::ImportedPresetCollection`] -- vendor-agnostic intermediate representation
//! - [`types::ImportReport`] -- summary of an import run
//!
//! # Vendor modules
//!
//! - [`fabfilter`] -- FabFilter Pro-Q, Pro-R, Saturn, etc. preset parsing and tag mapping
//! - [`rfxchain`] -- REAPER `.RfxChain` file parsing

pub mod fabfilter;
pub mod library_writer;
pub mod rfxchain;
pub mod types;

use std::path::Path;

use eyre::Result;
use signal_controller::SignalController;
use signal_proto::metadata::Metadata;
use signal_proto::tagging::{StructuredTag, TagCategory, TagSet, TagSource};
use signal_proto::{Block, BlockParameter, Preset, PresetId, Snapshot, SnapshotId};
use uuid::Uuid;

use types::{ImportReport, ImportedPresetCollection};

/// UUID namespace for deterministic import IDs.
///
/// All imported presets/snapshots derive their IDs from this namespace via UUID v5,
/// making re-imports idempotent — the same vendor+plugin+file always produces the
/// same ID, so a second import overwrites rather than duplicates.
pub const IMPORT_NAMESPACE: Uuid = Uuid::from_bytes([
    0x73, 0x69, 0x67, 0x6e, 0x61, 0x6c, 0x2d, 0x69, 0x6d, 0x70, 0x6f, 0x72, 0x74, 0x2d, 0x6e, 0x73,
]);

/// Compute the deterministic preset ID for a vendor+plugin combination.
pub fn import_preset_id(vendor: &str, plugin_name: &str) -> PresetId {
    let uuid = Uuid::new_v5(
        &IMPORT_NAMESPACE,
        format!("{vendor}:{plugin_name}").as_bytes(),
    );
    PresetId::from(uuid.to_string())
}

/// Import a collection of vendor presets into Signal's library.
///
/// Creates (or replaces) a `Preset` for the plugin, with one `Snapshot` per
/// imported file. Uses deterministic UUIDs so re-running the import is safe.
///
/// If `library_root` is provided, also writes preset files to the library
/// directory structure (the DB acts as a queryable cache).
pub async fn import_presets(
    signal: &SignalController,
    collection: ImportedPresetCollection,
) -> Result<ImportReport> {
    import_presets_with_library(signal, collection, None).await
}

/// Import presets with optional file-based library writing.
pub async fn import_presets_with_library(
    signal: &SignalController,
    collection: ImportedPresetCollection,
    library_root: Option<&Path>,
) -> Result<ImportReport> {
    if collection.snapshots.is_empty() {
        return Ok(ImportReport {
            preset_name: collection.plugin_name.clone(),
            snapshots_imported: 0,
            snapshots_skipped: 0,
        });
    }

    // Deterministic preset ID: same vendor+plugin always gets the same UUID
    let preset_uuid = Uuid::new_v5(
        &IMPORT_NAMESPACE,
        format!("{}:{}", collection.vendor, collection.plugin_name).as_bytes(),
    );
    let preset_id = PresetId::from(preset_uuid.to_string());

    // Normalize plugin name for tag value (lowercase, underscores)
    let plugin_tag_value = collection
        .plugin_name
        .to_ascii_lowercase()
        .replace(' ', "_")
        .replace('-', "_");

    let mut snapshots: Vec<Snapshot> = Vec::with_capacity(collection.snapshots.len());

    for imported in &collection.snapshots {
        // Deterministic snapshot ID: scoped under the preset namespace
        let snap_key = match &imported.folder {
            Some(folder) => format!("{}/{}", folder, imported.name),
            None => imported.name.clone(),
        };
        let snap_uuid = Uuid::new_v5(&preset_uuid, snap_key.as_bytes());
        let snap_id = SnapshotId::from(snap_uuid.to_string());

        // Build structured tags
        let mut tag_set = TagSet::new();

        // Workflow origin tag — distinguishes imported presets from user-created ones
        tag_set.insert(
            StructuredTag::new(TagCategory::Workflow, "imported").with_source(TagSource::Imported),
        );

        // Vendor + plugin tags
        tag_set.insert(
            StructuredTag::new(TagCategory::Vendor, &collection.vendor.to_ascii_lowercase())
                .with_source(TagSource::Imported),
        );
        tag_set.insert(
            StructuredTag::new(TagCategory::Plugin, &plugin_tag_value)
                .with_source(TagSource::Imported),
        );

        // Map vendor-specific tags
        for raw_tag in &imported.vendor_tags {
            tag_set.insert(fabfilter::tags::map_fabfilter_tag(raw_tag));
        }

        // Also infer tags from the snapshot name
        let inferred = signal_proto::tagging::infer_tags_from_name(&imported.name);
        tag_set.merge(&inferred);

        // Build metadata
        let mut metadata = Metadata::new();
        metadata.tags = tag_set.to_tags();
        // Source tag as raw string — matches fx_capture.rs format "source:{reaper_name}"
        if let Some(ref source) = imported.source_plugin {
            metadata.tags.add(format!("source:{source}"));
        }
        if let Some(folder) = &imported.folder {
            metadata = metadata.with_folder(folder.clone());
        }
        if let Some(desc) = &imported.description {
            metadata = metadata.with_description(desc.clone());
        }

        // Build block with actual parsed parameters (if available),
        // otherwise fall back to the default 3-param block.
        let block = if imported.parameters.is_empty() {
            Block::default()
        } else {
            let params: Vec<BlockParameter> = imported
                .parameters
                .iter()
                .map(|p| {
                    let id = p.name.to_lowercase().replace(' ', "_");
                    let mut bp = BlockParameter::new(id, &p.name, p.value);
                    if let Some(ref daw_name) = p.daw_name {
                        bp = bp.with_daw_name(daw_name);
                    }
                    bp
                })
                .collect();
            Block::from_parameters(params)
        };

        let mut snapshot = Snapshot::new(snap_id, &imported.name, block).with_metadata(metadata);
        if imported.store_raw_as_state {
            snapshot = snapshot.with_state_data(imported.raw_bytes.clone());
        }

        snapshots.push(snapshot);
    }

    let snapshots_imported = snapshots.len();

    // First snapshot becomes the default
    let default_snapshot = snapshots.remove(0);

    // Build preset-level metadata
    let mut preset_tags = TagSet::new();
    preset_tags.insert(
        StructuredTag::new(TagCategory::Workflow, "imported").with_source(TagSource::Imported),
    );
    preset_tags.insert(
        StructuredTag::new(TagCategory::Vendor, &collection.vendor.to_ascii_lowercase())
            .with_source(TagSource::Imported),
    );
    preset_tags.insert(
        StructuredTag::new(TagCategory::Plugin, &plugin_tag_value).with_source(TagSource::Imported),
    );
    let mut preset_metadata = Metadata::new().with_description(format!(
        "Imported {} presets from {}",
        collection.plugin_name, collection.vendor
    ));
    preset_metadata.tags = preset_tags.to_tags();
    // Add source tag at preset level too (grab from first snapshot)
    if let Some(ref source) = collection
        .snapshots
        .first()
        .and_then(|s| s.source_plugin.as_ref())
    {
        preset_metadata.tags.add(format!("source:{source}"));
    }

    let preset = Preset::new(
        preset_id,
        &collection.plugin_name,
        collection.block_type,
        default_snapshot,
        snapshots,
    )
    .with_metadata(preset_metadata);

    // Persist — save() does delete+insert, so re-import is handled
    signal.block_presets().save(preset.clone()).await?;

    // Write to file-based library if a root path was provided
    if let Some(root) = library_root {
        library_writer::write_preset_to_library(root, &collection.vendor, &preset)?;
    }

    Ok(ImportReport {
        preset_name: collection.plugin_name,
        snapshots_imported,
        snapshots_skipped: 0,
    })
}

/// Dry-run: show what would be imported without persisting.
pub fn dry_run_report(collection: &ImportedPresetCollection) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Preset: {} ({})\n",
        collection.plugin_name,
        collection.block_type.display_name()
    ));
    out.push_str(&format!("Vendor: {}\n", collection.vendor));
    out.push_str(&format!("Snapshots: {}\n", collection.snapshots.len()));

    // Group by folder
    let mut folders = std::collections::BTreeMap::<String, usize>::new();
    for snap in &collection.snapshots {
        let key = snap.folder.clone().unwrap_or_else(|| "(root)".to_string());
        *folders.entry(key).or_default() += 1;
    }
    if !folders.is_empty() {
        out.push_str("Folders:\n");
        for (folder, count) in &folders {
            out.push_str(&format!("  {folder}: {count} presets\n"));
        }
    }

    out
}
