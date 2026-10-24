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
