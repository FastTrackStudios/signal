use serde::{Deserialize, Serialize};
use signal_proto::tagging::{StructuredTag, TagCategory, TagSet, TagSource};
use std::path::Path;

/// Distinguishes between NAM amp models and cabinet impulse responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamFileKind {
    /// `.nam` — neural amp model
    AmpModel,
    /// `.wav` — cabinet impulse response
    ImpulseResponse,
}

/// One entry per file in the library. Content-addressable by SHA-256 hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamFileEntry {
    /// SHA-256 hex digest of file contents
    pub hash: String,
    pub kind: NamFileKind,
    /// Path relative to `signal-library/nam/` (portable across machines)
    pub relative_path: String,
    /// Original filename without directory
    pub filename: String,

    // -- Extracted from .nam JSON metadata --
    pub nam_version: Option<String>,
    pub architecture: Option<String>,
    pub sample_rate: Option<u32>,
    pub gain: Option<f64>,
    pub loudness: Option<f64>,
    pub gear_type: Option<String>,
    pub gear_make: Option<String>,
    pub gear_model: Option<String>,
    pub tone_type: Option<String>,
    pub modeled_by: Option<String>,

    // -- IR-specific metadata (WAV header) --
    pub ir_channels: Option<u16>,
    pub ir_sample_rate: Option<u32>,
    pub ir_duration_ms: Option<f64>,

    // -- User-assigned tags --
    pub tags: TagSet,
}

/// Metadata extracted from a `.nam` file's JSON (everything except weights).
#[derive(Debug, Clone, Default)]
pub struct NamMetadata {
    pub version: Option<String>,
    pub architecture: Option<String>,
    pub sample_rate: Option<u32>,
    pub gain: Option<f64>,
    pub loudness: Option<f64>,
    pub gear_type: Option<String>,
    pub gear_make: Option<String>,
    pub gear_model: Option<String>,
    pub tone_type: Option<String>,
    pub modeled_by: Option<String>,
}

/// Parse a `.nam` file and extract metadata, skipping the weights array.
///
/// NAM files are JSON with a large `weights` array that we don't need.
/// We deserialize into `serde_json::Value` and pluck out only the fields we want.
pub fn parse_nam_metadata(contents: &str) -> Result<NamMetadata, serde_json::Error> {
    let val: serde_json::Value = serde_json::from_str(contents)?;
    let obj = val.as_object();

    let mut meta = NamMetadata::default();

    if let Some(obj) = obj {
        meta.version = obj.get("version").and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        });
        meta.architecture = obj
            .get("architecture")
            .and_then(|v| v.as_str())
            .map(String::from);
        meta.sample_rate = obj
            .get("sample_rate")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);

        if let Some(md) = obj.get("metadata").and_then(|v| v.as_object()) {
            meta.gain = md.get("gain").and_then(|v| v.as_f64());
            meta.loudness = md.get("loudness").and_then(|v| v.as_f64());
            meta.gear_type = md
                .get("gear_type")
                .and_then(|v| v.as_str())
                .map(String::from);
            meta.gear_make = md
                .get("gear_make")
                .and_then(|v| v.as_str())
                .map(String::from);
            meta.gear_model = md
                .get("gear_model")
                .and_then(|v| v.as_str())
                .map(String::from);
            meta.tone_type = md
                .get("tone_type")
                .and_then(|v| v.as_str())
                .map(String::from);
            meta.modeled_by = md
                .get("modeled_by")
                .and_then(|v| v.as_str())
                .map(String::from);
        }
    }

    Ok(meta)
}

/// Infer tags from NAM metadata fields.
pub fn infer_tags_from_metadata(meta: &NamMetadata, filename: &str) -> TagSet {
    let mut tags = TagSet::default();

    // Vendor from gear_make or modeled_by
    if let Some(ref make) = meta.gear_make {
        tags.insert(
            StructuredTag::new(TagCategory::Vendor, make.to_lowercase())
                .with_source(TagSource::InferredStructure),
        );
    }
    if let Some(ref modeler) = meta.modeled_by {
        tags.insert(
            StructuredTag::new(TagCategory::Vendor, modeler.to_lowercase())
                .with_source(TagSource::InferredStructure),
        );
    }

    // Tone from tone_type
    if let Some(ref tone) = meta.tone_type {
        tags.insert(
            StructuredTag::new(TagCategory::Tone, tone.to_lowercase())
                .with_source(TagSource::InferredStructure),
        );
    }

    // Block type from gear_type
    if let Some(ref gear) = meta.gear_type {
        let block_value = match gear.to_lowercase().as_str() {
            "amp" | "amplifier" => "amp",
            "cabinet" | "cab" => "cabinet",
            "drive" | "pedal" | "overdrive" | "distortion" => "drive",
            _ => gear.as_str(),
        };
        tags.insert(
            StructuredTag::new(TagCategory::Block, block_value.to_lowercase())
                .with_source(TagSource::InferredStructure),
        );
    }

    // Character keywords from filename
    let lower = filename.to_lowercase();
    let character_keywords = [
        "aggressive",
        "warm",
        "bright",
        "dark",
        "crunchy",
        "smooth",
        "scooped",
        "mid-forward",
        "clean",
        "heavy",
        "vintage",
        "modern",
    ];
    for keyword in &character_keywords {
        if lower.contains(keyword) {
            tags.insert(
                StructuredTag::new(TagCategory::Character, (*keyword).to_string())
                    .with_source(TagSource::InferredName),
            );
        }
    }

    tags
}

/// Determine file kind from extension.
pub fn kind_from_path(path: &Path) -> Option<NamFileKind> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("nam") => Some(NamFileKind::AmpModel),
        Some("wav") => Some(NamFileKind::ImpulseResponse),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_nam() {
        let json = r#"{
            "version": "0.5.1",
            "architecture": "WaveNet",
            "sample_rate": 48000,
            "metadata": {
                "gain": 7.5,
                "loudness": -12.3,
                "gear_type": "amp",
                "gear_make": "ENGL",
                "gear_model": "Fireball 100",
                "tone_type": "high-gain",
                "modeled_by": "ToneJunkie"
            },
            "config": {},
            "weights": [1.0, 2.0, 3.0]
        }"#;

        let meta = parse_nam_metadata(json).unwrap();
        assert_eq!(meta.version.as_deref(), Some("0.5.1"));
        assert_eq!(meta.architecture.as_deref(), Some("WaveNet"));
        assert_eq!(meta.sample_rate, Some(48000));
        assert_eq!(meta.gain, Some(7.5));
        assert_eq!(meta.loudness, Some(-12.3));
        assert_eq!(meta.gear_type.as_deref(), Some("amp"));
        assert_eq!(meta.gear_make.as_deref(), Some("ENGL"));
        assert_eq!(meta.gear_model.as_deref(), Some("Fireball 100"));
        assert_eq!(meta.tone_type.as_deref(), Some("high-gain"));
        assert_eq!(meta.modeled_by.as_deref(), Some("ToneJunkie"));
    }

    #[test]
    fn infer_tags_from_metadata_works() {
        let meta = NamMetadata {
            gear_make: Some("ENGL".into()),
            tone_type: Some("high-gain".into()),
            gear_type: Some("amp".into()),
            ..Default::default()
        };
        let tags = infer_tags_from_metadata(&meta, "ENGL-Fireball-aggressive-ch1.nam");
        assert!(!tags.by_category(TagCategory::Vendor).is_empty());
        assert!(!tags.by_category(TagCategory::Tone).is_empty());
        assert!(!tags.by_category(TagCategory::Block).is_empty());
        assert!(!tags.by_category(TagCategory::Character).is_empty());
    }

    #[test]
    fn kind_from_extension() {
        assert_eq!(
            kind_from_path(Path::new("foo.nam")),
            Some(NamFileKind::AmpModel)
        );
        assert_eq!(
            kind_from_path(Path::new("bar.wav")),
            Some(NamFileKind::ImpulseResponse)
        );
        assert_eq!(kind_from_path(Path::new("baz.txt")), None);
    }
}
