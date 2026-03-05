//! Intermediate representations for imported presets.
//!
//! These types decouple vendor-specific parsing from Signal's domain model,
//! allowing each importer to produce a uniform structure that the orchestrator
//! converts into `Preset` / `Snapshot` entities.

use std::path::PathBuf;

use signal_proto::block::BlockType;

/// A single extracted parameter from a vendor preset.
#[derive(Debug, Clone)]
pub struct ImportedParameter {
    /// Display name (may be shortened, e.g. `"B1 Freq"`).
    pub name: String,
    /// Normalized value in `[0.0, 1.0]`.
    pub value: f32,
    /// Original DAW plugin parameter name, if it differs from `name`.
    ///
    /// For example, FabFilter Pro-Q renames `"Band 1 Frequency"` to `"B1 Freq"` for
    /// display — this field preserves `"Band 1 Frequency"` so we can send it back
    /// to REAPER's `set_parameter_by_name`.
    pub daw_name: Option<String>,
}

/// A single preset variation parsed from a vendor file.
#[derive(Debug, Clone)]
pub struct ImportedSnapshot {
    /// Display name (typically filename without extension).
    pub name: String,
    /// Subfolder path relative to the plugin preset root (e.g. "Guitar/Clean").
    pub folder: Option<String>,
    /// Author extracted from preset metadata, if available.
    pub author: Option<String>,
    /// Description extracted from preset metadata, if available.
    pub description: Option<String>,
    /// Raw tags from the vendor preset file (e.g. FabFilter comma-separated tags).
    pub vendor_tags: Vec<String>,
    /// Entire file contents — may or may not be stored as `Snapshot.state_data`
    /// depending on `store_raw_as_state`.
    pub raw_bytes: Vec<u8>,
    /// Extracted parameters (normalized 0-1) for UI display and DAW apply.
    /// Empty for binary-format presets where parameters can't be parsed.
    pub parameters: Vec<ImportedParameter>,
    /// Full REAPER plugin identifier (e.g. `"VST3: FabFilter Pro-Q 4 (FabFilter)"`).
    /// When present, a `source:{value}` tag is added to enable dedup matching,
    /// non-template display, and "Add to FX Chain".
    pub source_plugin: Option<String>,
    /// Whether to store `raw_bytes` as `Snapshot.state_data`.
    /// Set to `false` for vendor-native formats (e.g. FabFilter `.ffp`) that REAPER
    /// can't load via `set_state_chunk`. Set to `true` for REAPER-native formats
    /// (e.g. rfxchain) where the bytes ARE valid state chunks.
    pub store_raw_as_state: bool,
}

/// A collection of snapshots for one plugin, ready for conversion to a `Preset`.
#[derive(Debug, Clone)]
pub struct ImportedPresetCollection {
    /// Plugin display name (e.g. "Pro-Q 4").
    pub plugin_name: String,
    /// Vendor name (e.g. "FabFilter").
    pub vendor: String,
    /// Signal block type this plugin maps to.
    pub block_type: BlockType,
    /// All parsed snapshots.
    pub snapshots: Vec<ImportedSnapshot>,
}

/// Summary of a single import operation.
#[derive(Debug, Clone)]
pub struct ImportReport {
    /// Name of the preset that was created/updated.
    pub preset_name: String,
    /// Number of snapshots successfully imported.
    pub snapshots_imported: usize,
    /// Number of snapshots skipped (e.g. duplicates on re-import).
    pub snapshots_skipped: usize,
}

/// A plugin discovered on disk that can be imported.
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    /// Plugin display name.
    pub plugin_name: String,
    /// Path to the preset directory on disk.
    pub preset_dir: PathBuf,
    /// Signal block type this plugin maps to.
    pub block_type: BlockType,
    /// Number of preset files found.
    pub preset_count: usize,
    /// Whether the preset files are text-parseable (vs binary).
    pub is_text_format: bool,
}
