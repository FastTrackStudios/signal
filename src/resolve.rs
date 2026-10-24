use crate::{NamCatalog, NamError};
use std::path::{Path, PathBuf};

/// Resolve a content hash to an absolute filesystem path.
///
/// Looks up the entry in the catalog, combines its `relative_path` with the
/// provided `nam_root` base directory to produce an absolute path.
///
/// # Errors
///
/// Returns an error if the hash is not found in the catalog or if the
/// resolved file does not exist.
pub fn resolve_path(
    catalog: &NamCatalog,
    hash: &str,
    nam_root: &Path,
) -> Result<PathBuf, NamError> {
    let entry = catalog
        .get_entry(hash)
        .ok_or_else(|| NamError::ContentError(format!("no entry with hash {hash}")))?;

    let absolute = nam_root.join(&entry.relative_path);

    if !absolute.exists() {
        return Err(NamError::ContentError(format!(
            "file not found at resolved path: {}",
            absolute.display()
        )));
    }

    Ok(absolute)
}

/// Resolve a content hash to a path without checking existence.
/// Useful when constructing paths for VST chunk rewriting on a different machine.
///
/// # Errors
///
/// Returns an error if the hash is not found in the catalog.
pub fn resolve_path_unchecked(
    catalog: &NamCatalog,
    hash: &str,
    nam_root: &Path,
) -> Result<PathBuf, NamError> {
    let entry = catalog
        .get_entry(hash)
        .ok_or_else(|| NamError::ContentError(format!("no entry with hash {hash}")))?;

    Ok(nam_root.join(&entry.relative_path))
}

/// Get the NAM root directory from environment or a default.
///
/// Checks `NAM_LIBRARY_PATH` env var first, then falls back to the provided default.
pub fn nam_root_from_env(default: &Path) -> PathBuf {
    std::env::var("NAM_LIBRARY_PATH")
        .ok()
        .map_or_else(|| default.to_path_buf(), PathBuf::from)
}
