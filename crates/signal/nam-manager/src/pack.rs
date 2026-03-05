use crate::NamError;
use serde::{Deserialize, Serialize};
use signal_proto::tagging::{StructuredTag, TagCategory, TagSet, TagSource};
use std::collections::HashMap;
use std::path::Path;

/// A curated pack definition — one per amp, pedal, or capture collection.
///
/// Lives as a JSON file in `signal-library/nam/packs/`.
/// Declares the vendor, model, category, default tags, and per-file overrides.
///
/// Example:
/// ```json
/// {
///   "id": "engl-fireball",
///   "label": "ENGL Fireball 100",
///   "vendor": "ENGL",
///   "category": "amp",
///   "gear_model": "Fireball 100",
///   "default_tone": "high-gain",
///   "characters": ["tight", "aggressive"],
///   "files": {
///     "ENGL Fireball.nam": {},
///     "ENGL Fireball+Ts9.nam": { "tone": "high-gain", "boost_pedal": "TS9" },
///     "ENGL Fireball+Ts808.nam": { "tone": "high-gain", "boost_pedal": "TS808" }
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackDefinition {
    /// Unique slug identifier (matches JSON filename without extension)
    pub id: String,
    /// Human-readable label
    pub label: String,
    /// Manufacturer / brand name (e.g. "ENGL", "Revv", "ML Sound Labs")
    pub vendor: String,
    /// Top-level category
    pub category: PackCategory,
    /// Specific gear model (e.g. "Fireball 100", "Generator 120 MK III")
    #[serde(default)]
    pub gear_model: Option<String>,
    /// Person or studio who captured/modeled this
    #[serde(default)]
    pub modeled_by: Option<String>,
    /// Default tone for all files in this pack (can be overridden per-file)
    #[serde(default)]
    pub default_tone: Option<String>,
    /// Character descriptors that apply to the whole pack
    #[serde(default)]
    pub characters: Vec<String>,
    /// Per-file metadata overrides. Key = filename (not path).
    #[serde(default)]
    pub files: HashMap<String, FileOverride>,
    /// The subdirectory under amps/, drives/, etc. where these files live.
    /// If not set, derived from the pack id.
    #[serde(default)]
    pub directory: Option<String>,
}

/// Top-level category for a pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackCategory {
    Amp,
    Drive,
    Ir,
    Archetype,
}

impl PackCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            PackCategory::Amp => "amp",
            PackCategory::Drive => "drive",
            PackCategory::Ir => "ir",
            PackCategory::Archetype => "archetype",
        }
    }

    /// The subdirectory in signal-library/nam/ for this category.
    pub fn directory(&self) -> &'static str {
        match self {
            PackCategory::Amp => "amps",
            PackCategory::Drive => "drives",
            PackCategory::Ir => "ir",
            PackCategory::Archetype => "archetypes",
        }
    }
}

/// Per-file overrides within a pack. All fields optional — only set what differs
/// from the pack defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileOverride {
    /// Override the tone (e.g. "clean" for a specific capture in a high-gain pack)
    #[serde(default)]
    pub tone: Option<String>,
    /// Boost pedal used in this capture
    #[serde(default)]
    pub boost_pedal: Option<String>,
    /// Additional character tags for this specific file
    #[serde(default)]
    pub characters: Vec<String>,
    /// Custom tags (category:value pairs)
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Load all pack definitions from a directory.
pub fn load_packs(packs_dir: &Path) -> Result<Vec<PackDefinition>, NamError> {
    let mut packs = Vec::new();

    if !packs_dir.exists() {
        return Ok(packs);
    }

    let mut entries: Vec<_> = std::fs::read_dir(packs_dir)
        .map_err(|e| NamError::Io(format!("reading packs dir: {}", e)))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map_or(false, |ext| ext == "json")
        })
        .collect();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let contents = std::fs::read_to_string(entry.path())
            .map_err(|e| NamError::Io(format!("reading {}: {}", entry.path().display(), e)))?;
        let pack: PackDefinition = serde_json::from_str(&contents)
            .map_err(|e| NamError::Parse(format!("parsing {}: {}", entry.path().display(), e)))?;
        packs.push(pack);
    }

    Ok(packs)
}

/// Build a TagSet from a pack definition for a specific file.
pub fn tags_for_file(pack: &PackDefinition, filename: &str) -> TagSet {
    let mut tags = TagSet::default();

    // Vendor
    tags.insert(
        StructuredTag::new(TagCategory::Vendor, pack.vendor.to_lowercase())
            .with_source(TagSource::Imported),
    );

    // Category → Block tag
    tags.insert(
        StructuredTag::new(TagCategory::Block, pack.category.as_str())
            .with_source(TagSource::Imported),
    );

    // Gear model
    if let Some(ref model) = pack.gear_model {
        tags.insert(
            StructuredTag::new(TagCategory::Module, model.to_lowercase())
                .with_source(TagSource::Imported),
        );
    }

    // Default tone (overridable per-file)
    let file_override = pack.files.get(filename);
    let tone = file_override
        .and_then(|f| f.tone.as_deref())
        .or(pack.default_tone.as_deref());
    if let Some(tone) = tone {
        tags.insert(
            StructuredTag::new(TagCategory::Tone, tone.to_lowercase())
                .with_source(TagSource::Imported),
        );
    }

    // Pack-level characters
    for ch in &pack.characters {
        tags.insert(
            StructuredTag::new(TagCategory::Character, ch.to_lowercase())
                .with_source(TagSource::Imported),
        );
    }

    // Per-file overrides
    if let Some(overrides) = file_override {
        // Boost pedal
        if let Some(ref pedal) = overrides.boost_pedal {
            tags.insert(
                StructuredTag::new(TagCategory::Module, format!("boost:{}", pedal.to_lowercase()))
                    .with_source(TagSource::Imported),
            );
        }

        // File-specific characters
        for ch in &overrides.characters {
            tags.insert(
                StructuredTag::new(TagCategory::Character, ch.to_lowercase())
                    .with_source(TagSource::Imported),
            );
        }

        // Custom tags (pre-formatted as "category:value")
        for raw in &overrides.tags {
            tags.insert(StructuredTag::parse(raw).with_source(TagSource::Imported));
        }
    }

    // Modeled by
    if let Some(ref modeler) = pack.modeled_by {
        tags.insert(
            StructuredTag::new(TagCategory::Vendor, format!("modeler:{}", modeler.to_lowercase()))
                .with_source(TagSource::Imported),
        );
    }

    tags
}

/// Given a filename, find the pack that owns it.
/// Matches against both the pack's `files` map and the pack's directory.
pub fn find_pack_for_file<'a>(
    packs: &'a [PackDefinition],
    relative_path: &str,
) -> Option<&'a PackDefinition> {
    // First: check if filename is explicitly listed in any pack's files map
    let filename = std::path::Path::new(relative_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    for pack in packs {
        if pack.files.contains_key(filename.as_ref()) {
            return Some(pack);
        }
    }

    // Second: match by directory path
    for pack in packs {
        let pack_dir = pack
            .directory
            .clone()
            .unwrap_or_else(|| format!("{}/{}", pack.category.directory(), pack.id));

        if relative_path.starts_with(&pack_dir) {
            return Some(pack);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pack() -> PackDefinition {
        PackDefinition {
            id: "engl-fireball".into(),
            label: "ENGL Fireball 100".into(),
            vendor: "ENGL".into(),
            category: PackCategory::Amp,
            gear_model: Some("Fireball 100".into()),
            modeled_by: None,
            default_tone: Some("high-gain".into()),
            characters: vec!["tight".into(), "aggressive".into()],
            files: {
                let mut m = HashMap::new();
                m.insert("ENGL Fireball+Ts9.nam".into(), FileOverride {
                    boost_pedal: Some("TS9".into()),
                    ..Default::default()
                });
                m.insert("ENGL Fireball Clean.nam".into(), FileOverride {
                    tone: Some("clean".into()),
                    characters: vec!["warm".into()],
                    ..Default::default()
                });
                m
            },
            directory: None,
        }
    }

    #[test]
    fn tags_default_file() {
        let pack = sample_pack();
        let tags = tags_for_file(&pack, "ENGL Fireball.nam");

        // Should have vendor, block, module, tone, characters
        assert!(!tags.by_category(TagCategory::Vendor).is_empty());
        assert!(!tags.by_category(TagCategory::Block).is_empty());
        assert!(!tags.by_category(TagCategory::Module).is_empty());
        assert!(!tags.by_category(TagCategory::Tone).is_empty());
        assert_eq!(tags.by_category(TagCategory::Character).len(), 2); // tight, aggressive
    }

    #[test]
    fn tags_with_boost_override() {
        let pack = sample_pack();
        let tags = tags_for_file(&pack, "ENGL Fireball+Ts9.nam");

        // Should have boost pedal as module tag
        let module_tags: Vec<_> = tags.by_category(TagCategory::Module)
            .iter()
            .map(|t| t.value.clone())
            .collect();
        assert!(module_tags.iter().any(|v| v.contains("boost:ts9")));
    }

    #[test]
    fn tags_with_tone_override() {
        let pack = sample_pack();
        let tags = tags_for_file(&pack, "ENGL Fireball Clean.nam");

        // Tone should be overridden to "clean" instead of pack default "high-gain"
        let tone_tags: Vec<_> = tags.by_category(TagCategory::Tone)
            .iter()
            .map(|t| t.value.clone())
            .collect();
        assert!(tone_tags.contains(&"clean".to_string()));
        assert!(!tone_tags.contains(&"high-gain".to_string()));
    }

    #[test]
    fn find_pack_by_filename() {
        let packs = vec![sample_pack()];
        let found = find_pack_for_file(&packs, "amps/engl-fireball/ENGL Fireball+Ts9.nam");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "engl-fireball");
    }

    #[test]
    fn find_pack_by_directory() {
        let packs = vec![sample_pack()];
        let found = find_pack_for_file(&packs, "amps/engl-fireball/some_unknown_file.nam");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "engl-fireball");
    }

    #[test]
    fn pack_round_trip_json() {
        let pack = sample_pack();
        let json = serde_json::to_string_pretty(&pack).unwrap();
        let loaded: PackDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.id, "engl-fireball");
        assert_eq!(loaded.vendor, "ENGL");
        assert_eq!(loaded.files.len(), 2);
    }

    #[test]
    fn load_packs_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let packs = load_packs(dir.path()).unwrap();
        assert!(packs.is_empty());
    }

    #[test]
    fn load_packs_with_file() {
        let dir = tempfile::tempdir().unwrap();
        let pack = sample_pack();
        let json = serde_json::to_string_pretty(&pack).unwrap();
        std::fs::write(dir.path().join("engl-fireball.json"), &json).unwrap();

        let packs = load_packs(dir.path()).unwrap();
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].id, "engl-fireball");
    }
}
