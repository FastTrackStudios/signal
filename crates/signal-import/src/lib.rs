//! Signal preset import system.
//!
//! Converts vendor plugin presets (FabFilter, rfxchain, etc.) into Signal's
//! `Preset` / `Snapshot` model. Each vendor importer produces an
//! `ImportedPresetCollection` which the orchestrator converts and persists.

pub mod fabfilter;
pub mod rfxchain;
pub mod types;

use eyre::Result;
use signal_controller::SignalController;
use signal_proto::metadata::Metadata;
use signal_proto::tagging::{StructuredTag, TagCategory, TagSet, TagSource};
use signal_proto::{Block, Preset, PresetId, Snapshot, SnapshotId};
use uuid::Uuid;

use types::{ImportReport, ImportedPresetCollection};

/// UUID namespace for deterministic import IDs.
///
/// All imported presets/snapshots derive their IDs from this namespace via UUID v5,
/// making re-imports idempotent — the same vendor+plugin+file always produces the
/// same ID, so a second import overwrites rather than duplicates.
const IMPORT_NAMESPACE: Uuid = Uuid::from_bytes([
    0x73, 0x69, 0x67, 0x6e, 0x61, 0x6c, 0x2d, 0x69,
    0x6d, 0x70, 0x6f, 0x72, 0x74, 0x2d, 0x6e, 0x73,
]);

/// Import a collection of vendor presets into Signal's library.
///
/// Creates (or replaces) a `Preset` for the plugin, with one `Snapshot` per
/// imported file. Uses deterministic UUIDs so re-running the import is safe.
pub async fn import_presets(
    signal: &SignalController,
    collection: ImportedPresetCollection,
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
        if let Some(folder) = &imported.folder {
            metadata = metadata.with_folder(folder.clone());
        }
        if let Some(desc) = &imported.description {
            metadata = metadata.with_description(desc.clone());
        }

        let snapshot = Snapshot::new(snap_id, &imported.name, Block::default())
            .with_metadata(metadata)
            .with_state_data(imported.raw_bytes.clone());

        snapshots.push(snapshot);
    }

    let snapshots_imported = snapshots.len();

    // First snapshot becomes the default
    let default_snapshot = snapshots.remove(0);

    // Build preset-level metadata
    let mut preset_tags = TagSet::new();
    preset_tags.insert(
        StructuredTag::new(TagCategory::Vendor, &collection.vendor.to_ascii_lowercase())
            .with_source(TagSource::Imported),
    );
    preset_tags.insert(
        StructuredTag::new(TagCategory::Plugin, &plugin_tag_value)
            .with_source(TagSource::Imported),
    );
    let mut preset_metadata = Metadata::new().with_description(format!(
        "Imported {} presets from {}",
        collection.plugin_name, collection.vendor
    ));
    preset_metadata.tags = preset_tags.to_tags();

    let preset = Preset::new(
        preset_id,
        &collection.plugin_name,
        collection.block_type,
        default_snapshot,
        snapshots,
    )
    .with_metadata(preset_metadata);

    // Persist — save() does delete+insert, so re-import is handled
    signal.block_presets().save(preset).await?;

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
    out.push_str(&format!(
        "Snapshots: {}\n",
        collection.snapshots.len()
    ));

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
