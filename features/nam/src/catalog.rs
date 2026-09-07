use crate::NamError;
use crate::gain_group::GainStageGroup;
use crate::nam_file::{NamFileEntry, NamFileKind};
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: 1,
            ..Default::default()
        }
    }

    /// Load catalog from a JSON file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or the JSON is invalid.
    pub fn load(path: &Path) -> Result<Self, NamError> {
        let contents = std::fs::read_to_string(path).map_err(|e| {
            NamError::CatalogError(format!("reading catalog {}: {}", path.display(), e))
        })?;
        let catalog: Self = serde_json::from_str(&contents).map_err(|e| {
            NamError::CatalogError(format!("parsing catalog {}: {}", path.display(), e))
        })?;
        Ok(catalog)
    }

    /// Save catalog to a JSON file (pretty-printed).
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be created or written, or if serialization fails.
    pub fn save(&self, path: &Path) -> Result<(), NamError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| NamError::CatalogError(format!("serializing catalog: {e}")))?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Look up an entry by its content hash.
    #[must_use]
    pub fn get_entry(&self, hash: &str) -> Option<&NamFileEntry> {
        self.entries.get(hash)
    }

    /// Return all entries of a given kind.
    #[must_use]
    pub fn entries_by_kind(&self, kind: NamFileKind) -> Vec<&NamFileEntry> {
        self.entries.values().filter(|e| e.kind == kind).collect()
    }

    /// Return all entries that have a tag matching the given category and value.
    #[must_use]
    pub fn entries_by_tag(&self, category: TagCategory, value: &str) -> Vec<&NamFileEntry> {
        let key = format!("{}:{}", category.as_str(), value);
        self.entries
            .values()
            .filter(|e| e.tags.contains_key(&key))
            .collect()
    }

    /// Return all amp model entries (convenience).
    #[must_use]
    pub fn amp_models(&self) -> Vec<&NamFileEntry> {
        self.entries_by_kind(NamFileKind::AmpModel)
    }

    /// Return all IR entries (convenience).
    #[must_use]
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
    #[must_use]
    pub fn ir_pairings_for_model(&self, model_hash: &str) -> Vec<&IrPairing> {
        self.ir_pairings
            .iter()
            .filter(|p| p.model_hash == model_hash)
            .collect()
    }

    /// Summary stats for display.
    #[must_use]
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
                provenance: None,
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
                provenance: None,
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
                provenance: None,
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

    /// A minimal entry, so the provenance tests state only what they are about.
    fn sample_entry(hash: &str, filename: &str) -> NamFileEntry {
        NamFileEntry {
            provenance: None,
            hash: hash.into(),
            kind: NamFileKind::AmpModel,
            relative_path: format!("amps/{filename}"),
            filename: filename.into(),
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
            tags: Default::default(),
        }
    }

    /// A catalog written before provenance existed must still load. The field
    /// is `serde(default)` precisely so an upgrade does not strand a user's
    /// existing `nam/catalog.json` — this pins that.
    ///
    /// The legacy document is derived by serializing a current catalog and
    /// deleting the key, rather than hand-written: a literal fixture would
    /// drift out of date the next time an unrelated field is added, and would
    /// then be testing nothing.
    #[test]
    fn a_catalog_without_provenance_still_loads() {
        let mut catalog = NamCatalog::new();
        catalog
            .entries
            .insert("abc123".into(), sample_entry("abc123", "old.nam"));

        let mut doc: serde_json::Value =
            serde_json::to_value(&catalog).expect("serialize current catalog");
        let entry = doc["entries"]["abc123"]
            .as_object_mut()
            .expect("entry object");
        assert!(
            entry.remove("provenance").is_some(),
            "field was there to remove"
        );

        let legacy: NamCatalog =
            serde_json::from_value(doc).expect("a catalog with no provenance key still loads");
        let back = legacy.entries.get("abc123").expect("entry survived");
        assert_eq!(back.filename, "old.nam");
        assert!(back.provenance.is_none(), "absent provenance reads as None");
    }

    /// Provenance survives the round trip it exists for: the attribution terms
    /// are only met if creator and licence are still there next session.
    #[test]
    fn provenance_round_trips() {
        use crate::nam_file::Provenance;
        let mut catalog = NamCatalog::new();
        let mut entry = sample_entry("h", "t.nam");
        entry.provenance = Some(Provenance {
            source: "tone3000".into(),
            tone_id: Some("1234".into()),
            model_id: Some("5678".into()),
            tone_url: Some("https://www.tone3000.com/tones/1234".into()),
            creator: Some("someone".into()),
            creator_url: Some("https://www.tone3000.com/users/someone".into()),
            license: Some("cc-by".into()),
        });
        catalog.entries.insert("h".into(), entry);

        let json = serde_json::to_string(&catalog).expect("serialize");
        let back: NamCatalog = serde_json::from_str(&json).expect("deserialize");
        let p = back.entries["h"]
            .provenance
            .as_ref()
            .expect("provenance kept");
        assert_eq!(p.creator.as_deref(), Some("someone"));
        assert_eq!(p.license.as_deref(), Some("cc-by"));
        assert_eq!(
            p.tone_url.as_deref(),
            Some("https://www.tone3000.com/tones/1234")
        );
    }
}
