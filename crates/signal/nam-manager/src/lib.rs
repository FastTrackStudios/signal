//! NAM Manager — manages Neural Amp Modeler captures and IR files as first-class entities.
//!
//! Provides content-addressable identity (SHA-256), a JSON catalog with tags and gain stage
//! groups, NAM VST3 state chunk rewriting, and path resolution across machines.

pub mod catalog;
pub mod gain_group;
pub mod ir;
pub mod nam_file;
pub mod resolve;
pub mod scanner;
pub mod vst_chunk;

// Re-export primary types at crate root for convenience.
pub use catalog::{CatalogStats, IrPairing, NamCatalog};
pub use gain_group::{GainStage, GainStageGroup};
pub use ir::IrMetadata;
pub use nam_file::{NamFileEntry, NamFileKind, NamMetadata};
pub use resolve::{nam_root_from_env, resolve_path, resolve_path_unchecked};
pub use scanner::{merge_into_catalog, scan_directory, sha256_hex};
pub use vst_chunk::{decode_chunk, encode_chunk, rewrite_paths, NamVstChunk};

/// Errors that can occur in nam-manager operations.
#[derive(Debug, thiserror::Error)]
pub enum NamError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

/// Slugify a string for use as a directory name.
/// Converts to lowercase, replaces non-alphanumeric chars with hyphens, collapses runs.
pub fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Import files from a source directory into the signal-library NAM directory.
///
/// Copies files preserving subdirectory structure (slugified), then scans
/// the target directory and merges results into the catalog.
pub fn import_directory(
    source_dir: &std::path::Path,
    nam_root: &std::path::Path,
    catalog: &mut NamCatalog,
) -> Result<usize, NamError> {
    let files = scanner::collect_source_files(source_dir);
    let mut count = 0;

    for source_path in &files {
        // Compute relative path from source_dir
        let rel = source_path.strip_prefix(source_dir).unwrap_or(source_path);

        // Slugify directory components but keep the filename as-is
        let mut dest_parts = Vec::new();
        if let Some(parent) = rel.parent() {
            for component in parent.components() {
                if let std::path::Component::Normal(s) = component {
                    dest_parts.push(slugify(&s.to_string_lossy()));
                }
            }
        }
        let filename = rel
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let mut dest = nam_root.to_path_buf();
        for part in &dest_parts {
            dest.push(part);
        }

        std::fs::create_dir_all(&dest)
            .map_err(|e| NamError::Io(format!("creating dir {}: {}", dest.display(), e)))?;

        dest.push(&filename);

        // Only copy if not already present
        if !dest.exists() {
            std::fs::copy(source_path, &dest).map_err(|e| {
                NamError::Io(format!(
                    "copying {} → {}: {}",
                    source_path.display(),
                    dest.display(),
                    e
                ))
            })?;
            count += 1;
        }
    }

    // Scan and merge
    let scanned = scan_directory(nam_root)?;
    merge_into_catalog(catalog, scanned);

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_various() {
        assert_eq!(slugify("ENGL Fireball 100"), "engl-fireball-100");
        assert_eq!(slugify("6505+"), "6505");
        assert_eq!(slugify("ML Sound Labs"), "ml-sound-labs");
        assert_eq!(slugify("Browne/Dual Protein"), "browne-dual-protein");
    }

    #[test]
    fn import_directory_works() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();

        // Create source files
        let amps_dir = source.path().join("Amps").join("ENGL");
        std::fs::create_dir_all(&amps_dir).unwrap();
        std::fs::write(
            amps_dir.join("fireball.nam"),
            r#"{"version":"0.5","architecture":"LSTM","sample_rate":48000,"metadata":{},"weights":[]}"#,
        )
        .unwrap();

        let ir_dir = source.path().join("IR");
        std::fs::create_dir_all(&ir_dir).unwrap();
        // Minimal WAV: just enough header
        let mut wav = vec![0u8; 44];
        wav[0..4].copy_from_slice(b"RIFF");
        wav[4..8].copy_from_slice(&36u32.to_le_bytes());
        wav[8..12].copy_from_slice(b"WAVE");
        wav[12..16].copy_from_slice(b"fmt ");
        wav[16..20].copy_from_slice(&16u32.to_le_bytes());
        wav[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
        wav[22..24].copy_from_slice(&1u16.to_le_bytes()); // mono
        wav[24..28].copy_from_slice(&48000u32.to_le_bytes());
        wav[28..32].copy_from_slice(&(48000u32 * 2).to_le_bytes());
        wav[32..34].copy_from_slice(&2u16.to_le_bytes());
        wav[34..36].copy_from_slice(&16u16.to_le_bytes());
        wav[36..40].copy_from_slice(b"data");
        wav[40..44].copy_from_slice(&0u32.to_le_bytes());
        std::fs::write(ir_dir.join("cab.wav"), &wav).unwrap();

        let mut catalog = NamCatalog::new();
        let copied = import_directory(source.path(), dest.path(), &mut catalog).unwrap();
        assert_eq!(copied, 2);
        assert_eq!(catalog.entries.len(), 2);
        assert_eq!(catalog.amp_models().len(), 1);
        assert_eq!(catalog.impulse_responses().len(), 1);
    }
}
