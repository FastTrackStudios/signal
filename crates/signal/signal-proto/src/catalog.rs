//! Preset catalogue — types and parsers for on-disk preset libraries.
//!
//! Supports the Neural DSP binary JUCE preset format shared by all their
//! plugins (Archetype series, Fortin, Mantra, Parallax, Soldano, etc.).
//!
//! ## Usage
//!
//! ```rust,no_run
//! use signal_proto::catalog::{scan_preset_library, NdspPlugin, NDSP_PLUGINS};
//!
//! // Scan all installed NDSP plugins
//! for plugin in NDSP_PLUGINS {
//!     let presets = scan_preset_library(&plugin.disk_library_path());
//!     println!("{}: {} presets", plugin.name, presets.len());
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ─── Plugin Registry ────────────────────────────────────────────

/// A Neural DSP plugin descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NdspPlugin {
    /// Human-readable name (e.g., "Archetype John Mayer X").
    pub name: &'static str,
    /// Short slug for file paths (e.g., "archetype-john-mayer-x").
    pub slug: &'static str,
    /// Binary header ID in preset files (e.g., "mayer").
    pub binary_id: &'static str,
    /// Folder name under `/Library/Audio/Presets/Neural DSP/`.
    pub preset_folder: &'static str,
}

impl NdspPlugin {
    /// Full path to the preset library on macOS.
    pub fn disk_library_path(&self) -> PathBuf {
        PathBuf::from("/Library/Audio/Presets/Neural DSP").join(self.preset_folder)
    }
}

/// All known Neural DSP plugins.
pub const NDSP_PLUGINS: &[NdspPlugin] = &[
    NdspPlugin {
        name: "Archetype John Mayer X",
        slug: "archetype-john-mayer-x",
        binary_id: "mayer",
        preset_folder: "Archetype John Mayer X",
    },
    NdspPlugin {
        name: "Archetype Tim Henson X",
        slug: "archetype-tim-henson-x",
        binary_id: "henson-x",
        preset_folder: "Archetype Tim Henson X",
    },
    NdspPlugin {
        name: "Archetype Petrucci X",
        slug: "archetype-petrucci-x",
        binary_id: "petrucci-x",
        preset_folder: "Archetype Petrucci X",
    },
    NdspPlugin {
        name: "Archetype Rabea X",
        slug: "archetype-rabea-x",
        binary_id: "rabea-x",
        preset_folder: "Archetype Rabea X",
    },
    NdspPlugin {
        name: "Archetype Cory Wong X",
        slug: "archetype-cory-wong-x",
        binary_id: "cory-x",
        preset_folder: "Archetype Cory Wong X",
    },
    NdspPlugin {
        name: "Archetype Nolly X",
        slug: "archetype-nolly-x",
        binary_id: "nolly-x",
        preset_folder: "Archetype Nolly X",
    },
    NdspPlugin {
        name: "Fortin Nameless Suite X",
        slug: "fortin-nameless-suite-x",
        binary_id: "nameless-X",
        preset_folder: "Fortin Nameless Suite X",
    },
    NdspPlugin {
        name: "Mantra",
        slug: "mantra",
        binary_id: "mantra",
        preset_folder: "Mantra",
    },
    NdspPlugin {
        name: "Parallax X",
        slug: "parallax-x",
        binary_id: "parallax-X",
        preset_folder: "Parallax X",
    },
    NdspPlugin {
        name: "Soldano SLO-100 X",
        slug: "soldano-slo-100-x",
        binary_id: "soldano-X",
        preset_folder: "Soldano SLO-100 X",
    },
];

// ─── Catalogue Types ────────────────────────────────────────────

/// Top-level catalogue index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub version: u32,
    pub generated: String,
    pub plugins: Vec<CatalogPlugin>,
}

/// Per-plugin block entry in the top-level catalogue index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogPlugin {
    pub name: String,
    pub manufacturer: String,
    pub slug: String,
    pub binary_id: String,
    pub disk_library_path: String,
    pub total_snapshots: usize,
    pub folders: Vec<String>,
}

/// A preset discovered on disk in a plugin's preset library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskPreset {
    /// Human-readable name parsed from the binary file.
    pub name: String,
    /// Folder hierarchy relative to the plugin's preset dir (e.g., "John Mayer" or "Artists/Cory Wong").
    pub category: String,
    /// Genre/style tags embedded in the preset file.
    pub tags: Vec<String>,
    /// Key parameter values used as a fingerprint for matching loaded state to disk files.
    pub fingerprint: PresetFingerprint,
    /// Original file path on disk.
    pub source_path: PathBuf,
}

/// Generic parameter fingerprint — all float parameters from the appModel.
///
/// Uses a `BTreeMap` so the fingerprint is plugin-agnostic: it works for
/// all Neural DSP plugins without hardcoding parameter names. The keys are
/// parameter names (e.g., "threeInOneGain", "selectedAmp") and values are
/// the parameter values as strings (f64 for numeric, raw string for enum-like).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PresetFingerprint {
    pub params: BTreeMap<String, String>,
}

impl PresetFingerprint {
    /// Compute a distance score between two fingerprints.
    ///
    /// Only compares parameters present in *both* fingerprints. Parameters
    /// that parse as f64 are compared numerically; string params use exact
    /// equality with a penalty of 10.0 for mismatches.
    ///
    /// Returns 0.0 for identical fingerprints. Returns `f64::MAX` if no
    /// parameters overlap.
    pub fn distance(&self, other: &PresetFingerprint) -> f64 {
        let mut total = 0.0;
        let mut count = 0;

        for (key, val_a) in &self.params {
            let Some(val_b) = other.params.get(key) else {
                continue;
            };

            count += 1;
            match (val_a.parse::<f64>(), val_b.parse::<f64>()) {
                (Ok(a), Ok(b)) => {
                    total += (a - b).abs();
                }
                _ => {
                    // String comparison (e.g., selectedAmp)
                    if val_a != val_b {
                        total += 10.0;
                    }
                }
            }
        }

        if count > 0 {
            total / count as f64
        } else {
            f64::MAX
        }
    }
}

/// Per-snapshot metadata written to the catalogue as JSON.
///
/// Each Neural DSP "preset" maps to a **Snapshot** in our domain model —
/// a saved parameter state of a plugin block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub name: String,
    /// URL/filesystem-safe slug derived from the name (e.g., "gravity-clean").
    pub id: String,
    /// Slug of the parent plugin block (e.g., "archetype-john-mayer-x").
    pub block: String,
    /// Folder path within the block's snapshot tree (e.g., "John Mayer", "Artists/Cory Wong").
    pub folder: String,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset_uid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_cycle_index: Option<u32>,
    pub state_file: String,
    /// REAPER VST chunk file (captured by harvest). Preferred over `state_file`
    /// because it contains the exact binary blob that `set_vst_chunk` expects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reaper_chunk_file: Option<String>,
    pub fingerprint: PresetFingerprint,
}

/// Plugin block metadata written to the catalogue as `block.json`.
///
/// Each Neural DSP plugin maps to a single **PluginBlock** — the block
/// definition with all its parameter mappings. The snapshots (factory
/// "presets") live in the `snapshots/` subdirectory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockMetadata {
    pub name: String,
    pub manufacturer: String,
    pub slug: String,
    pub binary_id: String,
    pub format: String,
    pub disk_library_path: String,
    pub total_snapshots: usize,
    pub folders: Vec<String>,
}

// ─── NDSP Binary Format Parser ──────────────────────────────────

/// Extract a string value for `key` from the Neural DSP binary preset format.
///
/// Format: `<key>\0 <type_byte> <length_byte> \x05 <string_bytes> \0`
///
/// The binary format uses null-terminated key names followed by a type byte,
/// a length byte (includes the marker byte + null terminator), a `\x05` marker,
/// then the UTF-8 string bytes.
pub fn ndsp_binary_string(data: &[u8], key: &[u8]) -> Option<String> {
    let needle = [key, b"\x00"].concat();
    let idx = data.windows(needle.len()).position(|w| w == needle)?;
    let offset = idx + needle.len();
    if offset + 3 > data.len() {
        return None;
    }
    let length_byte = data[offset + 1] as usize;
    if data[offset + 2] != 0x05 || length_byte < 2 {
        return None;
    }
    let str_len = length_byte - 2; // subtract marker byte + null
    let start = offset + 3;
    if start + str_len > data.len() {
        return None;
    }
    let bytes = &data[start..start + str_len];
    Some(
        String::from_utf8_lossy(bytes)
            .trim_end_matches('\0')
            .to_string(),
    )
}

/// Extract a string value at a specific byte offset in the binary data.
///
/// Used when you've already found the position (e.g., iterating through
/// the tags section) and can't use `ndsp_binary_string` which always
/// finds the first occurrence of a key.
fn ndsp_binary_string_at(data: &[u8], offset: usize) -> Option<String> {
    if offset + 3 > data.len() {
        return None;
    }
    let length_byte = data[offset + 1] as usize;
    if data[offset + 2] != 0x05 || length_byte < 2 {
        return None;
    }
    let str_len = length_byte - 2;
    let start = offset + 3;
    if start + str_len > data.len() {
        return None;
    }
    let bytes = &data[start..start + str_len];
    Some(
        String::from_utf8_lossy(bytes)
            .trim_end_matches('\0')
            .to_string(),
    )
}

/// Extract tags from a Neural DSP binary preset file.
///
/// Tags are stored between the `tags\0` and `appModel\0` sections.
/// Each tag is a `value\0` entry with a string payload.
pub fn ndsp_binary_tags(data: &[u8]) -> Vec<String> {
    let mut tags = Vec::new();
    let tag_section = match data.windows(5).position(|w| w == b"tags\x00") {
        Some(idx) => idx,
        None => return tags,
    };
    // Tags end where appModel begins
    let end = data
        .windows(9)
        .position(|w| w == b"appModel\x00")
        .unwrap_or(data.len());

    let mut search_start = tag_section;
    while search_start < end {
        let val_needle = b"value\x00";
        let val_idx = match data[search_start..end]
            .windows(val_needle.len())
            .position(|w| w == val_needle)
        {
            Some(rel) => search_start + rel,
            None => break,
        };
        let offset = val_idx + val_needle.len();
        if let Some(tag) = ndsp_binary_string_at(data, offset) {
            if !tag.is_empty() {
                tags.push(tag);
            }
        }
        search_start = val_idx + val_needle.len();
    }
    tags
}

/// Build a generic fingerprint from a Neural DSP binary preset file on disk.
///
/// Extracts ALL key-value pairs from the `appModel` section of the binary
/// file. This makes the fingerprint plugin-agnostic — no hardcoded parameter
/// names needed.
pub fn fingerprint_from_disk(data: &[u8]) -> PresetFingerprint {
    let mut params = BTreeMap::new();

    // Find the appModel section
    let app_model_start = match data.windows(9).position(|w| w == b"appModel\x00") {
        Some(idx) => idx + 9, // skip past "appModel\0"
        None => return PresetFingerprint { params },
    };

    // Walk through the binary data extracting key-value pairs.
    // Each key is a null-terminated string, followed by type+length+value.
    let mut pos = app_model_start;
    while pos < data.len() {
        // Find the next null-terminated key
        let key_start = pos;
        let key_end = match data[pos..].iter().position(|&b| b == 0) {
            Some(rel) => pos + rel,
            None => break,
        };

        // Key must be at least 1 byte and ASCII-ish
        let key_bytes = &data[key_start..key_end];
        if key_bytes.is_empty() || !key_bytes.iter().all(|&b| b.is_ascii_graphic()) {
            pos = key_end + 1;
            continue;
        }

        let key = String::from_utf8_lossy(key_bytes).to_string();
        let value_offset = key_end + 1; // skip the null

        // Try to parse a string value at this offset
        if let Some(value) = ndsp_binary_string_at(data, value_offset) {
            params.insert(key, value);
            // Advance past the value
            if value_offset + 3 <= data.len() {
                let length_byte = data[value_offset + 1] as usize;
                pos = value_offset + 3 + length_byte.saturating_sub(2);
            } else {
                break;
            }
        } else {
            pos = key_end + 1;
        }
    }

    PresetFingerprint { params }
}

/// Extract an XML attribute value: `key="value"` → `value`.
pub fn xml_attr(xml: &str, key: &str) -> Option<String> {
    let needle = format!("{}=\"", key);
    let start = xml.find(&needle)? + needle.len();
    let end = xml[start..].find('"')? + start;
    Some(xml[start..end].to_string())
}

/// Build a fingerprint from the REAPER state chunk's embedded XML.
///
/// Extracts all attributes from the `<appModel ...>` element as fingerprint
/// parameters. This mirrors `fingerprint_from_disk` but operates on the XML
/// representation found in REAPER state chunks.
pub fn fingerprint_from_xml(xml: &str) -> PresetFingerprint {
    let mut params = BTreeMap::new();

    // Find the <appModel element
    let app_model = match xml.find("<appModel") {
        Some(idx) => &xml[idx..],
        None => return PresetFingerprint { params },
    };

    // Extract the attributes section (between first space and ">")
    let attrs_start = match app_model.find(' ') {
        Some(idx) => idx + 1,
        None => return PresetFingerprint { params },
    };
    let attrs_end = app_model
        .find('>')
        .unwrap_or(app_model.len())
        .min(app_model.find("/>").unwrap_or(app_model.len()));
    let attrs_str = &app_model[attrs_start..attrs_end];

    // Parse key="value" pairs
    let mut remaining = attrs_str;
    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        let eq_pos = match remaining.find('=') {
            Some(pos) => pos,
            None => break,
        };
        let key = remaining[..eq_pos].trim();
        let after_eq = &remaining[eq_pos + 1..];

        // Value must be quoted
        if !after_eq.starts_with('"') {
            break;
        }
        let value_start = 1; // skip opening quote
        let value_end = match after_eq[value_start..].find('"') {
            Some(pos) => value_start + pos,
            None => break,
        };
        let value = &after_eq[value_start..value_end];

        if !key.is_empty() {
            params.insert(key.to_string(), value.to_string());
        }

        remaining = &after_eq[value_end + 1..];
    }

    PresetFingerprint { params }
}

/// Extract the embedded XML from the REAPER state chunk's raw bytes.
///
/// The XML starts with `<?xml` and contains the `<appModel>` element
/// with all preset parameter values.
pub fn extract_xml_from_chunk(data: &[u8]) -> Option<String> {
    let xml_start = data.windows(5).position(|w| w == b"<?xml")?;
    // Find the end: last '>' before the next non-printable section
    let mut end = xml_start;
    for i in xml_start..data.len() {
        if data[i] >= 0x20 && data[i] < 0x7F {
            end = i + 1;
        } else {
            break;
        }
    }
    Some(String::from_utf8_lossy(&data[xml_start..end]).to_string())
}

// ─── Disk Scanner ───────────────────────────────────────────────

/// Scan a Neural DSP preset library directory and return all presets found.
///
/// Recursively walks the directory, reading `.xml` files (which are actually
/// binary JUCE format despite the extension). Extracts name, category (folder
/// hierarchy), tags, and parameter fingerprint from each file.
pub fn scan_preset_library(preset_dir: &Path) -> Vec<DiskPreset> {
    let mut presets = Vec::new();

    fn walk(dir: &Path, base: &Path, presets: &mut Vec<DiskPreset>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, presets);
            } else if path.extension().map(|e| e == "xml").unwrap_or(false) {
                let Ok(data) = std::fs::read(&path) else {
                    continue;
                };
                let name = ndsp_binary_string(&data, b"name").unwrap_or_default();
                let category = path
                    .parent()
                    .and_then(|p| p.strip_prefix(base).ok())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                let tags = ndsp_binary_tags(&data);
                let fingerprint = fingerprint_from_disk(&data);
                presets.push(DiskPreset {
                    name,
                    category,
                    tags,
                    fingerprint,
                    source_path: path,
                });
            }
        }
    }

    walk(preset_dir, preset_dir, &mut presets);
    presets
}

/// Find the best matching disk preset for a given fingerprint.
///
/// Returns the closest match and its distance score. A distance of 0.0
/// means an exact match. Returns `None` if the library is empty.
pub fn match_preset<'a>(
    library: &'a [DiskPreset],
    fp: &PresetFingerprint,
) -> Option<(&'a DiskPreset, f64)> {
    library
        .iter()
        .map(|p| (p, p.fingerprint.distance(fp)))
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

// ─── Slugify ────────────────────────────────────────────────────

/// Convert a human-readable name to a URL/filesystem-safe slug.
///
/// ```
/// # use signal_proto::catalog::slugify;
/// assert_eq!(slugify("Gravity Clean"), "gravity-clean");
/// assert_eq!(slugify("John Mayer's Tone!"), "john-mayers-tone");
/// assert_eq!(slugify("  Multiple   Spaces  "), "multiple-spaces");
/// ```
pub fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else if c.is_whitespace() {
                '-'
            } else {
                // Drop non-alphanumeric, non-dash chars
                '\0'
            }
        })
        .filter(|&c| c != '\0')
        .collect::<String>()
        // Collapse multiple dashes
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Gravity Clean"), "gravity-clean");
        assert_eq!(slugify("John Mayer's Tone!"), "john-mayers-tone");
        assert_eq!(slugify("  Multiple   Spaces  "), "multiple-spaces");
        assert_eq!(slugify("Already-Slugged"), "already-slugged");
        assert_eq!(slugify("100% Gain"), "100-gain");
    }

    #[test]
    fn test_fingerprint_distance_identical() {
        let mut params = BTreeMap::new();
        params.insert("gain".to_string(), "0.5".to_string());
        params.insert("tone".to_string(), "0.7".to_string());

        let fp = PresetFingerprint {
            params: params.clone(),
        };
        assert_eq!(fp.distance(&fp), 0.0);
    }

    #[test]
    fn test_fingerprint_distance_numeric() {
        let mut a = BTreeMap::new();
        a.insert("gain".to_string(), "0.5".to_string());
        a.insert("tone".to_string(), "0.7".to_string());

        let mut b = BTreeMap::new();
        b.insert("gain".to_string(), "0.6".to_string());
        b.insert("tone".to_string(), "0.7".to_string());

        let fp_a = PresetFingerprint { params: a };
        let fp_b = PresetFingerprint { params: b };
        let dist = fp_a.distance(&fp_b);
        assert!((dist - 0.05).abs() < 1e-9, "expected ~0.05, got {dist}");
    }

    #[test]
    fn test_fingerprint_distance_string_mismatch() {
        let mut a = BTreeMap::new();
        a.insert("selectedAmp".to_string(), "1".to_string());
        a.insert("gain".to_string(), "0.5".to_string());

        let mut b = BTreeMap::new();
        b.insert("selectedAmp".to_string(), "2".to_string());
        b.insert("gain".to_string(), "0.5".to_string());

        let fp_a = PresetFingerprint { params: a };
        let fp_b = PresetFingerprint { params: b };
        // "1" and "2" both parse as f64 → numeric diff = 1.0
        // gain diff = 0.0
        // average = (1.0 + 0.0) / 2 = 0.5
        let dist = fp_a.distance(&fp_b);
        assert!((dist - 0.5).abs() < 1e-9, "expected 0.5, got {dist}");
    }

    #[test]
    fn test_fingerprint_no_overlap() {
        let mut a = BTreeMap::new();
        a.insert("gain".to_string(), "0.5".to_string());

        let mut b = BTreeMap::new();
        b.insert("tone".to_string(), "0.7".to_string());

        let fp_a = PresetFingerprint { params: a };
        let fp_b = PresetFingerprint { params: b };
        assert_eq!(fp_a.distance(&fp_b), f64::MAX);
    }

    #[test]
    fn test_xml_attr() {
        let xml = r#"<appModel selectedAmp="3" gain="0.5" name="Test">"#;
        assert_eq!(xml_attr(xml, "selectedAmp"), Some("3".to_string()));
        assert_eq!(xml_attr(xml, "gain"), Some("0.5".to_string()));
        assert_eq!(xml_attr(xml, "missing"), None);
    }

    #[test]
    fn test_fingerprint_from_xml() {
        let xml = r#"<?xml version="1.0"?><root><appModel selectedAmp="3" threeInOneGain="0.273937" outputGain="-4.5"/></root>"#;
        let fp = fingerprint_from_xml(xml);
        assert_eq!(fp.params.get("selectedAmp"), Some(&"3".to_string()));
        assert_eq!(
            fp.params.get("threeInOneGain"),
            Some(&"0.273937".to_string())
        );
        assert_eq!(fp.params.get("outputGain"), Some(&"-4.5".to_string()));
    }

    #[test]
    fn test_ndsp_plugins_registry() {
        assert_eq!(NDSP_PLUGINS.len(), 10);
        assert_eq!(NDSP_PLUGINS[0].binary_id, "mayer");
        assert_eq!(NDSP_PLUGINS[9].binary_id, "soldano-X");
        // All slugs are unique
        let slugs: std::collections::HashSet<_> = NDSP_PLUGINS.iter().map(|p| p.slug).collect();
        assert_eq!(slugs.len(), 10);
    }

    #[test]
    fn test_snapshot_metadata_serde() {
        let meta = SnapshotMetadata {
            name: "Gravity Clean".to_string(),
            id: "gravity-clean".to_string(),
            block: "archetype-john-mayer-x".to_string(),
            folder: "John Mayer".to_string(),
            tags: vec!["Clean".to_string(), "Blues".to_string()],
            preset_uid: Some("4361648983680894524".to_string()),
            midi_cycle_index: Some(2),
            state_file: "Gravity Clean.bin".to_string(),
            reaper_chunk_file: Some("Gravity Clean.chunk".to_string()),
            fingerprint: PresetFingerprint::default(),
        };
        let json = serde_json::to_string_pretty(&meta).unwrap();
        let roundtrip: SnapshotMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.name, "Gravity Clean");
        assert_eq!(roundtrip.id, "gravity-clean");
        assert_eq!(roundtrip.block, "archetype-john-mayer-x");
        assert_eq!(roundtrip.folder, "John Mayer");
        assert_eq!(roundtrip.state_file, "Gravity Clean.bin");
        assert_eq!(
            roundtrip.reaper_chunk_file,
            Some("Gravity Clean.chunk".to_string())
        );
        assert_eq!(
            roundtrip.preset_uid,
            Some("4361648983680894524".to_string())
        );

        // Test backwards compat: no reaper_chunk_file in old JSON
        let old_json = r#"{"name":"Test","id":"test","block":"b","folder":"","tags":[],"state_file":"Test.bin","fingerprint":{"params":{}}}"#;
        let old: SnapshotMetadata = serde_json::from_str(old_json).unwrap();
        assert_eq!(old.reaper_chunk_file, None);
    }
}
