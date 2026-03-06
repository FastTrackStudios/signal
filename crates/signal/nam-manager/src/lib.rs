//! NAM Manager — manages Neural Amp Modeler captures and IR files as first-class entities.
//!
//! Provides content-addressable identity (SHA-256), a JSON catalog with tags and gain stage
//! groups, NAM VST3 state chunk rewriting, and path resolution across machines.

use std::collections::HashMap;

pub mod catalog;
pub mod gain_group;
pub mod ir;
pub mod nam_file;
pub mod pack;
pub mod resolve;
pub mod scanner;
pub mod vst_chunk;

// Re-export primary types at crate root for convenience.
pub use catalog::{CatalogStats, IrPairing, NamCatalog};
pub use gain_group::{GainStage, GainStageGroup};
pub use ir::IrMetadata;
pub use nam_file::{NamFileEntry, NamFileKind, NamMetadata};
pub use pack::{FileOverride, PackCategory, PackDefinition};
pub use resolve::{nam_root_from_env, resolve_path, resolve_path_unchecked};
pub use scanner::{apply_packs, merge_into_catalog, scan_directory, sha256_hex};
pub use vst_chunk::{create_default_chunk, decode_chunk, encode_chunk, rewrite_paths, NamVstChunk};

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

/// Generate skeleton pack definition JSON files from a source directory structure.
///
/// Creates one pack per **model** (leaf-level grouping), not per top-level brand.
/// For example, `ML Sound Labs/Fender Deluxe Reverb/` becomes its own pack rather than
/// being lumped into a single "ML Sound Labs" pack.
///
/// The algorithm: walk each category dir, find directories that directly contain
/// `.nam`/`.wav` files (or whose children like "Amp Only"/"Full Rig" do), and create
/// one pack per model-level directory.
pub fn generate_pack_skeletons(
    source_dir: &std::path::Path,
    packs_output_dir: &std::path::Path,
) -> Result<Vec<String>, NamError> {
    use std::collections::HashMap;

    std::fs::create_dir_all(packs_output_dir)
        .map_err(|e| NamError::Io(format!("creating packs dir: {}", e)))?;

    let category_map: HashMap<&str, PackCategory> = [
        ("Amps", PackCategory::Amp),
        ("Drive Pedals", PackCategory::Drive),
        ("Archetypes", PackCategory::Archetype),
        ("IR", PackCategory::Ir),
    ]
    .into_iter()
    .collect();

    let mut generated = Vec::new();

    for (dir_name, category) in &category_map {
        let category_dir = source_dir.join(dir_name);
        if !category_dir.exists() {
            continue;
        }

        let mut model_dirs = discover_model_dirs(&category_dir)?;
        model_dirs.sort();

        for model_dir in model_dirs {
            let files = collect_pack_files(&model_dir);
            if files.is_empty() {
                continue;
            }

            // Build the label from the path components below the category dir.
            // e.g. "ML Sound Labs/Fender Deluxe Reverb" or just "ENGL Fireball Pack"
            let rel = model_dir
                .strip_prefix(&category_dir)
                .unwrap_or(&model_dir);
            let label = rel.to_string_lossy().replace('/', " — ");
            let pack_id = slugify(&label);

            // Infer vendor from the label (uses the full path, so "ML Sound Labs — Fender..."
            // will match "ML Sound Labs" and the model-level name for gear_model)
            let vendor = infer_vendor_from_name(&label);

            // Try to extract a gear_model from the deepest directory name
            let gear_model = model_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string());

            let pack = PackDefinition {
                id: pack_id.clone(),
                label: label.clone(),
                vendor,
                category: *category,
                gear_model,
                modeled_by: None,
                default_tone: None,
                characters: vec![],
                files: files
                    .into_iter()
                    .map(|f| (f, pack::FileOverride::default()))
                    .collect(),
                directory: None,
            };

            let json = serde_json::to_string_pretty(&pack)
                .map_err(|e| NamError::Parse(format!("serializing pack: {}", e)))?;
            let out_path = packs_output_dir.join(format!("{}.json", pack_id));
            std::fs::write(&out_path, &json)
                .map_err(|e| NamError::Io(format!("writing {}: {}", out_path.display(), e)))?;

            generated.push(pack_id);
        }
    }

    Ok(generated)
}

/// Discover model-level directories — the directories that should each become a pack.
///
/// A model dir is one where `.nam`/`.wav` files live (directly or in variant subdirs
/// like "Amp Only"/"Full Rig" that don't represent separate models).
///
/// If a directory directly contains audio files, it's a model dir.
/// If it only contains subdirectories, we check if those subdirs are "variant" dirs
/// (like "Amp Only", "Full Rig") or actual model dirs. Variant dirs mean the parent
/// is the model; otherwise we recurse.
fn discover_model_dirs(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, NamError> {
    let mut results = Vec::new();

    let has_audio_files = dir_has_audio_files(dir);
    if has_audio_files {
        results.push(dir.to_path_buf());
        return Ok(results);
    }

    // Check children
    let children: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| NamError::Io(format!("reading {}: {}", dir.display(), e)))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();

    if children.is_empty() {
        return Ok(results);
    }

    // Check if ALL child dirs are "variant" dirs (contain audio files directly)
    // If so, this directory is the model level.
    let all_children_have_files = children
        .iter()
        .all(|c| dir_has_audio_files(&c.path()));
    let any_child_is_variant = children.iter().any(|c| {
        let name = c.file_name().to_string_lossy().to_lowercase();
        matches!(
            name.as_str(),
            "amp only" | "full rig" | "full" | "amp" | "direct" | "mic'd" | "micd"
        )
    });

    if all_children_have_files && any_child_is_variant {
        // This dir is the model — its children are just variants
        results.push(dir.to_path_buf());
    } else {
        // Recurse into children — they might be brand dirs (like "ML Sound Labs")
        for child in &children {
            results.extend(discover_model_dirs(&child.path())?);
        }
    }

    Ok(results)
}

/// Check if a directory directly contains any `.nam` or `.wav` files (non-recursive).
fn dir_has_audio_files(dir: &std::path::Path) -> bool {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| {
            e.path().is_file()
                && matches!(
                    e.path().extension().and_then(|ext| ext.to_str()),
                    Some("nam") | Some("wav")
                )
        })
}

/// Collect .nam and .wav filenames from a directory (recursively).
fn collect_pack_files(dir: &std::path::Path) -> Vec<String> {
    walkdir::WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().is_file()
                && matches!(
                    e.path().extension().and_then(|ext| ext.to_str()),
                    Some("nam") | Some("wav")
                )
        })
        .map(|e| {
            e.path()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        })
        .collect()
}

/// Infer vendor from a directory label. For multi-segment labels like
/// "ML Sound Labs — Fender Deluxe Reverb", checks the first segment for known
/// studio/capture brands before checking individual amp brands.
fn infer_vendor_from_name(name: &str) -> String {
    // For "Brand — Model" labels, check the first segment separately
    let first_segment = name.split('—').next().unwrap_or(name).trim();

    // Multi-word studio/capture brands (check these first — they are the vendor,
    // even when the label also contains an amp brand like "Fender" or "Marshall")
    let studio_brands = [
        ("ML Sound Labs", "ML Sound Labs"),
        ("ML ", "ML Sound Labs"),
        ("UAD", "Universal Audio"),
        ("Extreme Metal Pack", "Extreme Metal Pack"),
    ];
    for (pattern, vendor) in &studio_brands {
        if first_segment.contains(pattern) {
            return vendor.to_string();
        }
    }

    // Single-word amp/pedal brands
    let known_brands = [
        "ENGL", "EVH", "Revv", "Matchless", "Vox", "Marshall", "Mesa", "Fender", "Friedman",
        "Orange", "Peavey", "Browne", "JHS", "Boss", "MXR", "Ibanez", "Bogner", "Diezel",
        "Hughes", "Soldano", "Randall", "Blackstar", "PRS", "Suhr", "Roland",
    ];

    // Check first segment for amp brands
    let upper = first_segment.to_uppercase();
    for brand in &known_brands {
        if upper.contains(&brand.to_uppercase()) {
            return brand.to_string();
        }
    }

    // Special product-name cases
    if name.contains("6505") || name.contains("5150") {
        return "Peavey".to_string();
    }

    // Fallback: first word
    first_segment
        .split_whitespace()
        .next()
        .unwrap_or(name)
        .to_string()
}

/// A FULL-rig model file resolved to its pack and filesystem path.
#[derive(Debug, Clone)]
pub struct FullRigModel {
    /// Original filename from the pack (e.g. "ML PEAV Block Clean FULL.nam")
    pub filename: String,
    /// Absolute path to the .nam file on this machine
    pub absolute_path: String,
    /// Tone tag inferred from pack metadata or filename (e.g. "clean", "drive", "lead")
    pub tone: Option<String>,
}

/// Return FULL-rig `.nam` models grouped by their pack definition.
///
/// Loads all pack definitions from `packs_dir`, filters to the given `vendor`
/// and `PackCategory::Amp`, then resolves FULL-rig filenames (those containing
/// "FULL" in the filename) to absolute paths by searching `search_roots` recursively.
///
/// Multiple search roots are supported — the first match wins. This allows finding
/// files both in the signal-library and in a source captures directory.
///
/// Packs with no resolvable FULL files are omitted.
pub fn full_rig_models_by_pack(
    packs_dir: &std::path::Path,
    search_roots: &[&std::path::Path],
    vendor: &str,
) -> Result<Vec<(PackDefinition, Vec<FullRigModel>)>, NamError> {
    // Build a filename → absolute path index from all search roots
    let mut filename_index: HashMap<String, std::path::PathBuf> = HashMap::new();
    for root in search_roots {
        if !root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(root)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |e| e == "nam") {
                if let Some(name) = path.file_name() {
                    let name_str = name.to_string_lossy().to_string();
                    // First match wins — don't overwrite
                    filename_index.entry(name_str).or_insert_with(|| path.to_path_buf());
                }
            }
        }
    }

    let packs = pack::load_packs(packs_dir)?;
    let mut results = Vec::new();

    for pack in packs {
        if pack.category != PackCategory::Amp {
            continue;
        }
        if !pack.vendor.eq_ignore_ascii_case(vendor) {
            continue;
        }

        let mut models = Vec::new();
        for (filename, file_override) in &pack.files {
            if !filename.contains("FULL") {
                continue;
            }

            let abs_path = match filename_index.get(filename) {
                Some(p) => p,
                None => continue,
            };

            // Tone: prefer per-file override, then pack default, then infer from filename
            let tone = file_override
                .tone
                .clone()
                .or_else(|| pack.default_tone.clone())
                .or_else(|| infer_tone_from_filename(filename));

            models.push(FullRigModel {
                filename: filename.clone(),
                absolute_path: abs_path.to_string_lossy().to_string(),
                tone,
            });
        }

        // Sort by tone for deterministic ordering: clean → drive → lead/overdrive
        models.sort_by(|a, b| tone_sort_key(&a.tone).cmp(&tone_sort_key(&b.tone)));

        if !models.is_empty() {
            results.push((pack, models));
        }
    }

    // Sort packs by label for deterministic output
    results.sort_by(|a, b| a.0.label.cmp(&b.0.label));

    Ok(results)
}

/// Infer a tone label from a FULL-rig filename.
fn infer_tone_from_filename(filename: &str) -> Option<String> {
    let lower = filename.to_lowercase();
    if lower.contains("clean") {
        Some("clean".to_string())
    } else if lower.contains("lead") {
        Some("lead".to_string())
    } else if lower.contains("overdrive") {
        Some("overdrive".to_string())
    } else if lower.contains("drive") {
        Some("drive".to_string())
    } else if lower.contains("plexi") {
        Some("crunch".to_string())
    } else {
        None
    }
}

/// Sort key for tone ordering: clean first, then drive, then lead/overdrive.
fn tone_sort_key(tone: &Option<String>) -> u8 {
    match tone.as_deref() {
        Some("clean") => 0,
        Some("crunch") => 1,
        Some("drive") => 2,
        Some("lead") => 3,
        Some("overdrive") => 4,
        _ => 5,
    }
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

    /// Generate pack skeletons from the real NAM Captures directory.
    /// Run with: `cargo test -p nam-manager -- --ignored generate_real_packs --nocapture`
    #[test]
    #[ignore]
    fn generate_real_packs() {
        let source = std::path::Path::new("/Users/codywright/Music/Audiohaven/NAM Captures");
        if !source.exists() {
            eprintln!("Source directory not found, skipping");
            return;
        }

        let output = std::path::Path::new(
            "/Users/codywright/Documents/Development/FastTrackStudio/signal-library/nam/packs",
        );
        let generated = generate_pack_skeletons(source, output).unwrap();
        eprintln!("Generated {} pack definitions:", generated.len());
        for id in &generated {
            eprintln!("  - {}.json", id);
        }
    }
}
