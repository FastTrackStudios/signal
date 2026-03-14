use crate::ir::parse_wav_header;
use crate::nam_file::{
    infer_tags_from_metadata, kind_from_path, parse_nam_metadata, NamFileEntry, NamFileKind,
};
use crate::{NamCatalog, NamError};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Scan the `signal-library/nam/` directory tree and discover all NAM/IR files.
///
/// Returns a map of SHA-256 hash → `NamFileEntry`.
/// For `.nam` files, metadata is extracted from the JSON.
/// For `.wav` files, WAV header metadata is extracted.
pub fn scan_directory(nam_root: &Path) -> Result<HashMap<String, NamFileEntry>, NamError> {
    let mut entries = HashMap::new();

    for entry in WalkDir::new(nam_root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let kind = match kind_from_path(path) {
            Some(k) => k,
            None => continue,
        };

        let file_entry = scan_file(path, nam_root, kind)?;
        entries.insert(file_entry.hash.clone(), file_entry);
    }

    Ok(entries)
}

/// Scan a single file: hash it, extract metadata, build a `NamFileEntry`.
fn scan_file(
    path: &Path,
    nam_root: &Path,
    kind: NamFileKind,
) -> Result<NamFileEntry, NamError> {
    let contents = std::fs::read(path)?;

    let hash = sha256_hex(&contents);

    let relative_path = path
        .strip_prefix(nam_root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut entry = NamFileEntry {
        hash,
        kind,
        relative_path,
        filename: filename.clone(),
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
    };

    match kind {
        NamFileKind::AmpModel => {
            let text = String::from_utf8_lossy(&contents);
            if let Ok(meta) = parse_nam_metadata(&text) {
                entry.tags = infer_tags_from_metadata(&meta, &filename);
                entry.nam_version = meta.version;
                entry.architecture = meta.architecture;
                entry.sample_rate = meta.sample_rate;
                entry.gain = meta.gain;
                entry.loudness = meta.loudness;
                entry.gear_type = meta.gear_type;
                entry.gear_make = meta.gear_make;
                entry.gear_model = meta.gear_model;
                entry.tone_type = meta.tone_type;
                entry.modeled_by = meta.modeled_by;
            }
        }
        NamFileKind::ImpulseResponse => {
            if let Ok(ir_meta) = parse_wav_header(path) {
                entry.ir_channels = Some(ir_meta.channels);
                entry.ir_sample_rate = Some(ir_meta.sample_rate);
                entry.ir_duration_ms = Some(ir_meta.duration_ms);
            }
        }
    }

    Ok(entry)
}

/// Merge newly scanned entries into an existing catalog, preserving user-assigned tags.
pub fn merge_into_catalog(
    catalog: &mut NamCatalog,
    scanned: HashMap<String, NamFileEntry>,
) {
    for (hash, mut new_entry) in scanned {
        if let Some(existing) = catalog.entries.get(&hash) {
            // Preserve user-assigned tags from existing entry
            new_entry.tags.merge(&existing.tags);
        }
        catalog.entries.insert(hash, new_entry);
    }
}

/// Compute SHA-256 hex digest of data.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Apply pack definitions to scanned entries, overriding/merging tags from pack metadata.
///
/// This is the primary tagging mechanism — pack JSON files are the curated source of truth
/// for vendor, model, category, and tone tags.
pub fn apply_packs(
    entries: &mut HashMap<String, NamFileEntry>,
    packs: &[crate::PackDefinition],
) {
    for entry in entries.values_mut() {
        if let Some(pack) = crate::pack::find_pack_for_file(packs, &entry.relative_path) {
            let pack_tags = crate::pack::tags_for_file(pack, &entry.filename);
            entry.tags.merge(&pack_tags);
        }
    }
}

/// Collect all files from a source directory and return their paths grouped by subdirectory.
/// Used during import to map source structure to signal-library layout.
pub fn collect_source_files(source_dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(source_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().is_file()
                && matches!(
                    e.path().extension().and_then(|x| x.to_str()),
                    Some("nam") | Some("wav")
                )
        })
        .map(|e| e.into_path())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_deterministic() {
        let hash1 = sha256_hex(b"hello world");
        let hash2 = sha256_hex(b"hello world");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // 256 bits = 64 hex chars
    }

    #[test]
    fn scan_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let entries = scan_directory(dir.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn scan_with_nam_file() {
        let dir = tempfile::tempdir().unwrap();
        let nam_content = r#"{
            "version": "0.5.1",
            "architecture": "LSTM",
            "sample_rate": 48000,
            "metadata": {"gain": 5.0, "gear_make": "Revv"},
            "weights": []
        }"#;
        std::fs::write(dir.path().join("test.nam"), nam_content).unwrap();

        let entries = scan_directory(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let entry = entries.values().next().unwrap();
        assert_eq!(entry.kind, NamFileKind::AmpModel);
        assert_eq!(entry.architecture.as_deref(), Some("LSTM"));
        assert_eq!(entry.gain, Some(5.0));
    }
}
