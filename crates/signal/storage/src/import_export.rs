//! JSON import/export for bulk data transfer.
//!
//! Serializes/deserializes complete storage bundles for backup, sharing,
//! and device synchronization.

use serde::{Deserialize, Serialize};
use signal_proto::{
    engine::Engine, layer::Layer, profile::Profile, rig::Rig, setlist::Setlist, song::Song,
    ModulePreset, Preset,
};

use crate::StorageResult;

// ---------------------------------------------------------------------------
// Export bundle
// ---------------------------------------------------------------------------

/// A complete export of all storage data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportBundle {
    /// Format version for forward compatibility.
    pub format_version: u32,

    /// Optional label for this export.
    #[serde(default)]
    pub label: Option<String>,

    /// Timestamp when the export was created (Unix epoch seconds).
    pub exported_at: u64,

    /// Device ID that produced this export.
    #[serde(default)]
    pub device_id: Option<String>,

    /// Block preset collections.
    #[serde(default)]
    pub block_presets: Vec<Preset>,

    /// Module preset collections.
    #[serde(default)]
    pub module_presets: Vec<ModulePreset>,

    /// Layer definitions.
    #[serde(default)]
    pub layers: Vec<Layer>,

    /// Engine definitions.
    #[serde(default)]
    pub engines: Vec<Engine>,

    /// Rig definitions.
    #[serde(default)]
    pub rigs: Vec<Rig>,

    /// Profile definitions.
    #[serde(default)]
    pub profiles: Vec<Profile>,

    /// Song definitions.
    #[serde(default)]
    pub songs: Vec<Song>,

    /// Setlist definitions.
    #[serde(default)]
    pub setlists: Vec<Setlist>,
}

impl ExportBundle {
    /// Current format version.
    pub const CURRENT_FORMAT: u32 = 1;

    /// Create an empty bundle with current format version.
    pub fn new() -> Self {
        Self {
            format_version: Self::CURRENT_FORMAT,
            label: None,
            exported_at: 0,
            device_id: None,
            block_presets: Vec::new(),
            module_presets: Vec::new(),
            layers: Vec::new(),
            engines: Vec::new(),
            rigs: Vec::new(),
            profiles: Vec::new(),
            songs: Vec::new(),
            setlists: Vec::new(),
        }
    }

    /// Total number of entities in this bundle.
    pub fn entity_count(&self) -> usize {
        self.block_presets.len()
            + self.module_presets.len()
            + self.layers.len()
            + self.engines.len()
            + self.rigs.len()
            + self.profiles.len()
            + self.songs.len()
            + self.setlists.len()
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> StorageResult<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| crate::StorageError::Data(format!("JSON serialization error: {e}")))
    }

    /// Deserialize from JSON string.
    pub fn from_json(json: &str) -> StorageResult<Self> {
        let bundle: Self = serde_json::from_str(json)
            .map_err(|e| crate::StorageError::Data(format!("JSON parse error: {e}")))?;

        if bundle.format_version > Self::CURRENT_FORMAT {
            return Err(crate::StorageError::Data(format!(
                "Unsupported format version {} (max supported: {})",
                bundle.format_version,
                Self::CURRENT_FORMAT
            )));
        }

        Ok(bundle)
    }

    /// Serialize to compact JSON bytes (for transmission).
    pub fn to_json_bytes(&self) -> StorageResult<Vec<u8>> {
        serde_json::to_vec(self)
            .map_err(|e| crate::StorageError::Data(format!("JSON serialization error: {e}")))
    }

    /// Deserialize from JSON bytes.
    pub fn from_json_bytes(bytes: &[u8]) -> StorageResult<Self> {
        let bundle: Self = serde_json::from_slice(bytes)
            .map_err(|e| crate::StorageError::Data(format!("JSON parse error: {e}")))?;

        if bundle.format_version > Self::CURRENT_FORMAT {
            return Err(crate::StorageError::Data(format!(
                "Unsupported format version {} (max supported: {})",
                bundle.format_version,
                Self::CURRENT_FORMAT
            )));
        }

        Ok(bundle)
    }
}

impl Default for ExportBundle {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Import options
// ---------------------------------------------------------------------------

/// Strategy for handling conflicts during import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStrategy {
    /// Skip entities that already exist (by ID).
    Skip,
    /// Overwrite existing entities with imported ones.
    Overwrite,
    /// Import as new entities with fresh IDs.
    Duplicate,
}

/// Options controlling import behavior.
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// How to handle ID conflicts.
    pub conflict_strategy: ConflictStrategy,

    /// If true, only validate without actually importing.
    pub dry_run: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            conflict_strategy: ConflictStrategy::Skip,
            dry_run: false,
        }
    }
}

/// Result of an import operation.
#[derive(Debug, Clone)]
pub struct ImportResult {
    /// Number of entities imported.
    pub imported: usize,
    /// Number of entities skipped (conflict).
    pub skipped: usize,
    /// Number of entities overwritten.
    pub overwritten: usize,
    /// Any errors encountered (non-fatal).
    pub errors: Vec<String>,
}

impl ImportResult {
    pub fn new() -> Self {
        Self {
            imported: 0,
            skipped: 0,
            overwritten: 0,
            errors: Vec::new(),
        }
    }

    pub fn total_processed(&self) -> usize {
        self.imported + self.skipped + self.overwritten
    }
}

impl Default for ImportResult {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_bundle_round_trip() {
        let bundle = ExportBundle::new();
        let json = bundle.to_json().unwrap();
        let parsed = ExportBundle::from_json(&json).unwrap();
        assert_eq!(parsed.format_version, ExportBundle::CURRENT_FORMAT);
        assert_eq!(parsed.entity_count(), 0);
    }

    #[test]
    fn bundle_with_label() {
        let mut bundle = ExportBundle::new();
        bundle.label = Some("My Backup".to_string());
        bundle.exported_at = 1700000000;
        bundle.device_id = Some("device-123".to_string());

        let json = bundle.to_json().unwrap();
        let parsed = ExportBundle::from_json(&json).unwrap();
        assert_eq!(parsed.label.as_deref(), Some("My Backup"));
        assert_eq!(parsed.exported_at, 1700000000);
        assert_eq!(parsed.device_id.as_deref(), Some("device-123"));
    }

    #[test]
    fn unsupported_format_version_rejected() {
        let json = r#"{"format_version": 999, "exported_at": 0}"#;
        let err = ExportBundle::from_json(json).unwrap_err();
        assert!(err.to_string().contains("Unsupported format version"));
    }

    #[test]
    fn bytes_round_trip() {
        let bundle = ExportBundle::new();
        let bytes = bundle.to_json_bytes().unwrap();
        let parsed = ExportBundle::from_json_bytes(&bytes).unwrap();
        assert_eq!(parsed.format_version, ExportBundle::CURRENT_FORMAT);
    }

    #[test]
    fn import_result_counts() {
        let mut result = ImportResult::new();
        result.imported = 5;
        result.skipped = 2;
        result.overwritten = 1;
        assert_eq!(result.total_processed(), 8);
    }
}
