use crate::{NamCatalog, NamError};
use std::path::{Path, PathBuf};

/// Resolve a content hash to an absolute filesystem path.
///
/// Looks up the entry in the catalog, combines its `relative_path` with the
/// provided `nam_root` base directory to produce an absolute path.
pub fn resolve_path(
    catalog: &NamCatalog,
    hash: &str,
    nam_root: &Path,
) -> Result<PathBuf, NamError> {
    let entry = catalog
        .get_entry(hash)
        .ok_or_else(|| NamError::NotFound(format!("no entry with hash {}", hash)))?;

    let absolute = nam_root.join(&entry.relative_path);

    if !absolute.exists() {
        return Err(NamError::NotFound(format!(
            "file not found at resolved path: {}",
            absolute.display()
        )));
    }

    Ok(absolute)
}

/// Resolve a content hash to a path without checking existence.
/// Useful when constructing paths for VST chunk rewriting on a different machine.
pub fn resolve_path_unchecked(
    catalog: &NamCatalog,
    hash: &str,
    nam_root: &Path,
) -> Result<PathBuf, NamError> {
    let entry = catalog
        .get_entry(hash)
        .ok_or_else(|| NamError::NotFound(format!("no entry with hash {}", hash)))?;

    Ok(nam_root.join(&entry.relative_path))
}

/// Get the NAM root directory from environment or a default.
///
/// Checks `NAM_LIBRARY_PATH` env var first, then falls back to the provided default.
pub fn nam_root_from_env(default: &Path) -> PathBuf {
    std::env::var("NAM_LIBRARY_PATH")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| default.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nam_file::{NamFileEntry, NamFileKind};
    use signal_proto::tagging::TagSet;

    #[test]
    fn resolve_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let nam_root = dir.path();

        // Create a file
        let sub = nam_root.join("amps");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("test.nam"), "{}").unwrap();

        let mut catalog = NamCatalog::new();
        catalog.entries.insert(
            "abc".into(),
            NamFileEntry {
                hash: "abc".into(),
                kind: NamFileKind::AmpModel,
                relative_path: "amps/test.nam".into(),
                filename: "test.nam".into(),
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

        let path = resolve_path(&catalog, "abc", nam_root).unwrap();
        assert!(path.exists());
        assert!(path.ends_with("amps/test.nam"));
    }

    #[test]
    fn resolve_missing_hash() {
        let catalog = NamCatalog::new();
        let result = resolve_path(&catalog, "nonexistent", Path::new("/tmp"));
        assert!(result.is_err());
    }

    #[test]
    fn resolve_unchecked_doesnt_require_file() {
        let mut catalog = NamCatalog::new();
        catalog.entries.insert(
            "xyz".into(),
            NamFileEntry {
                hash: "xyz".into(),
                kind: NamFileKind::AmpModel,
                relative_path: "amps/ghost.nam".into(),
                filename: "ghost.nam".into(),
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

        let path = resolve_path_unchecked(&catalog, "xyz", Path::new("/library/nam")).unwrap();
        assert_eq!(path, PathBuf::from("/library/nam/amps/ghost.nam"));
    }
}
