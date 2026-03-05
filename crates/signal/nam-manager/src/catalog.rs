use crate::gain_group::GainStageGroup;
use crate::nam_file::{NamFileEntry, NamFileKind};
use crate::NamError;
use serde::{Deserialize, Serialize};
use signal_proto::tagging::TagCategory;
use std::collections::HashMap;
use std::path::Path;

/// Top-level catalog container. Serialized as `nam/catalog.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NamCatalog {
    /// Catalog format version
    pub version: u32,
    /// Content hash → file entry
    pub entries: HashMap<String, NamFileEntry>,
    /// Group ID → gain stage group
    pub groups: HashMap<String, GainStageGroup>,
    /// IR pairing associations
    pub ir_pairings: Vec<IrPairing>,
}

/// Associates a NAM amp model with a recommended IR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrPairing {
    /// Hash of the NAM model file
    pub model_hash: String,
    /// Hash of the IR WAV file
    pub ir_hash: String,
    /// Optional descriptive label
    pub label: Option<String>,
}

impl NamCatalog {
    pub fn new() -> Self {
        Self {
            version: 1,
            ..Default::default()
        }
    }

    /// Load catalog from a JSON file.
    pub fn load(path: &Path) -> Result<Self, NamError> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| NamError::Io(format!("reading catalog {}: {}", path.display(), e)))?;
        let catalog: Self = serde_json::from_str(&contents)
            .map_err(|e| NamError::Parse(format!("parsing catalog: {}", e)))?;
        Ok(catalog)
    }

    /// Save catalog to a JSON file (pretty-printed).
    pub fn save(&self, path: &Path) -> Result<(), NamError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| NamError::Io(format!("creating catalog dir: {}", e)))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| NamError::Parse(format!("serializing catalog: {}", e)))?;
        std::fs::write(path, json)
            .map_err(|e| NamError::Io(format!("writing catalog {}: {}", path.display(), e)))?;
        Ok(())
    }

    /// Look up an entry by its content hash.
    pub fn get_entry(&self, hash: &str) -> Option<&NamFileEntry> {
        self.entries.get(hash)
    }

    /// Return all entries of a given kind.
    pub fn entries_by_kind(&self, kind: NamFileKind) -> Vec<&NamFileEntry> {
        self.entries
            .values()
            .filter(|e| e.kind == kind)
            .collect()
    }

    /// Return all entries that have a tag matching the given category and value.
    pub fn entries_by_tag(&self, category: TagCategory, value: &str) -> Vec<&NamFileEntry> {
        let key = format!("{}:{}", category.as_str(), value);
        self.entries
            .values()
            .filter(|e| e.tags.contains_key(&key))
            .collect()
    }

    /// Return all amp model entries (convenience).
    pub fn amp_models(&self) -> Vec<&NamFileEntry> {
        self.entries_by_kind(NamFileKind::AmpModel)
    }

    /// Return all IR entries (convenience).
    pub fn impulse_responses(&self) -> Vec<&NamFileEntry> {
        self.entries_by_kind(NamFileKind::ImpulseResponse)
    }

    /// Add an IR pairing.
    pub fn add_ir_pairing(&mut self, model_hash: String, ir_hash: String, label: Option<String>) {
        self.ir_pairings.push(IrPairing {
            model_hash,
            ir_hash,
            label,
        });
    }

    /// Get recommended IRs for a given model hash.
    pub fn ir_pairings_for_model(&self, model_hash: &str) -> Vec<&IrPairing> {
        self.ir_pairings
            .iter()
            .filter(|p| p.model_hash == model_hash)
            .collect()
    }

    /// Summary stats for display.
    pub fn stats(&self) -> CatalogStats {
        let amp_count = self
            .entries
            .values()
            .filter(|e| e.kind == NamFileKind::AmpModel)
            .count();
        let ir_count = self
            .entries
            .values()
            .filter(|e| e.kind == NamFileKind::ImpulseResponse)
            .count();
        CatalogStats {
            total_entries: self.entries.len(),
            amp_models: amp_count,
            impulse_responses: ir_count,
            groups: self.groups.len(),
            ir_pairings: self.ir_pairings.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CatalogStats {
    pub total_entries: usize,
    pub amp_models: usize,
    pub impulse_responses: usize,
    pub groups: usize,
    pub ir_pairings: usize,
}

impl std::fmt::Display for CatalogStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Catalog: {} entries ({} amp models, {} IRs), {} groups, {} IR pairings",
            self.total_entries,
            self.amp_models,
            self.impulse_responses,
            self.groups,
            self.ir_pairings
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_proto::tagging::TagSet;

    #[test]
    fn catalog_round_trip() {
        let mut catalog = NamCatalog::new();
        catalog.entries.insert(
            "abc123".into(),
            NamFileEntry {
                hash: "abc123".into(),
                kind: NamFileKind::AmpModel,
                relative_path: "amps/test.nam".into(),
                filename: "test.nam".into(),
                nam_version: Some("0.5.1".into()),
                architecture: Some("LSTM".into()),
                sample_rate: Some(48000),
                gain: Some(7.0),
                loudness: None,
                gear_type: Some("amp".into()),
                gear_make: Some("ENGL".into()),
                gear_model: None,
                tone_type: None,
                modeled_by: None,
                ir_channels: None,
                ir_sample_rate: None,
                ir_duration_ms: None,
                tags: TagSet::default(),
            },
        );

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.json");
        catalog.save(&path).unwrap();
        let loaded = NamCatalog::load(&path).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.version, 1);

        let entry = loaded.get_entry("abc123").unwrap();
        assert_eq!(entry.gain, Some(7.0));
    }

    #[test]
    fn query_by_kind() {
        let mut catalog = NamCatalog::new();
        catalog.entries.insert(
            "amp1".into(),
            NamFileEntry {
                hash: "amp1".into(),
                kind: NamFileKind::AmpModel,
                relative_path: "amps/a.nam".into(),
                filename: "a.nam".into(),
                nam_version: None,
                architecture: None,
                sample_rate: None,
                gain: None,
                loudness: None,
                gear_type: None,
                gear_make: None,
                gear_model: None,
                tone_type: None,
                modeled_by: None,
                ir_channels: None,
                ir_sample_rate: None,
                ir_duration_ms: None,
                tags: TagSet::default(),
            },
        );
        catalog.entries.insert(
            "ir1".into(),
            NamFileEntry {
                hash: "ir1".into(),
                kind: NamFileKind::ImpulseResponse,
                relative_path: "ir/b.wav".into(),
                filename: "b.wav".into(),
                nam_version: None,
                architecture: None,
                sample_rate: None,
                gain: None,
                loudness: None,
                gear_type: None,
                gear_make: None,
                gear_model: None,
                tone_type: None,
                modeled_by: None,
                ir_channels: None,
                ir_sample_rate: None,
                ir_duration_ms: None,
                tags: TagSet::default(),
            },
        );

        assert_eq!(catalog.amp_models().len(), 1);
        assert_eq!(catalog.impulse_responses().len(), 1);
    }
}
