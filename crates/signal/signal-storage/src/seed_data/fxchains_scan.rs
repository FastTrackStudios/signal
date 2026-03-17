//! FXChains directory scanner for REAPER-native signal presets.
//!
//! Scans `FXChains/FTS-Signal/01-Blocks/` and `02-Modules/` for `.RfxChain`
//! files, optionally enriched by `.signal.styx` sidecars.
//!
//! # Directory layout
//!
//! ```text
//! FXChains/FTS-Signal/
//! ├── 01-Blocks/
//! │   ├── Amps/
//! │   │   ├── Neural DSP/
//! │   │   │   ├── Clean Funk.RfxChain
//! │   │   │   └── Clean Funk.signal.styx   (optional)
//! │   │   └── Line 6/
//! │   ├── Drives/
//! │   └── ...
//! └── 02-Modules/
//!     ├── Worship Clean.RfxChain
//!     └── Blues Crunch.RfxChain
//! ```

use std::path::{Path, PathBuf};

use signal_proto::catalog::slugify;
use signal_proto::metadata::Metadata;
use signal_proto::module_type::ModuleType;
use signal_proto::{
    seed_id, Block, BlockType, Module, ModulePreset, ModuleSnapshot, Preset, SignalChain, Snapshot,
};

use crate::sidecar::{self, PresetKind};

/// Root directory name under FXChains/.
const FTS_SIGNAL_DIR: &str = "FTS-Signal";
/// Block presets subfolder.
const BLOCKS_DIR: &str = "01-Blocks";
/// Module presets subfolder.
const MODULES_DIR: &str = "02-Modules";

/// Resolve the FTS-Signal FXChains root directory.
///
/// Looks under `<fts_home>/Reaper/FXChains/FTS-Signal/`.
pub fn fxchains_root() -> PathBuf {
    utils::paths::reaper_fxchains().join(FTS_SIGNAL_DIR)
}

/// Scan `FXChains/FTS-Signal/01-Blocks/` for `.RfxChain` block presets.
///
/// Each subfolder under `01-Blocks/` represents a category (e.g. `Amps/`, `Drives/`).
/// Within each category, subfolders represent plugin vendors/groups, and `.RfxChain`
/// files are individual block presets.
///
/// Returns `Preset` values with `BlockType::Custom` (refined by sidecar if present).
pub fn scan_blocks(root: &Path) -> Vec<Preset> {
    let blocks_dir = root.join(BLOCKS_DIR);
    if !blocks_dir.is_dir() {
        return Vec::new();
    }

    let mut presets = Vec::new();
    collect_rfxchain_presets_recursive(&blocks_dir, &blocks_dir, "fxc-block", &mut presets);
    presets
}

/// Scan `FXChains/FTS-Signal/02-Modules/` for `.RfxChain` module presets.
///
/// Module presets are FX chains (ordered sequences of blocks). Each `.RfxChain`
/// file is a complete module preset.
pub fn scan_modules(root: &Path) -> Vec<ModulePreset> {
    let modules_dir = root.join(MODULES_DIR);
    if !modules_dir.is_dir() {
        return Vec::new();
    }

    let mut module_presets = Vec::new();

    let entries = read_dir_sorted(&modules_dir);
    for path in entries {
        if !is_rfxchain(&path) {
            continue;
        }

        let name = file_stem(&path);
        let slug = slugify(&name);
        let seed_key = format!("fxc-module-{slug}");
        let preset_id = signal_proto::ModulePresetId::from_uuid(seed_id(&seed_key));
        let snapshot_id =
            signal_proto::ModuleSnapshotId::from_uuid(seed_id(&format!("{seed_key}-default")));

        let sidecar = sidecar::read_sidecar(&path);

        let mut metadata = Metadata::new().with_tag("FXChains");
        if let Some(ref sc) = sidecar {
            for tag in &sc.tags {
                metadata = metadata.with_tag(tag);
            }
            if let Some(ref desc) = sc.description {
                metadata = metadata.with_description(desc);
            }
        }

        // Create a minimal module (empty chain — the rfxchain IS the state).
        let module = Module::from_chain(SignalChain::serial(vec![]));
        let snapshot =
            ModuleSnapshot::new(snapshot_id, &name, module).with_metadata(metadata.clone());

        let module_preset =
            ModulePreset::new(preset_id, &name, ModuleType::default(), snapshot, vec![])
                .with_metadata(metadata);

        module_presets.push(module_preset);
    }

    module_presets
}

// ─── Internal helpers ────────────────────────────────────────

/// Recursively collect `.RfxChain` files from a directory tree, creating
/// one `Preset` per file (each preset gets exactly one snapshot).
fn collect_rfxchain_presets_recursive(
    dir: &Path,
    base: &Path,
    prefix: &str,
    out: &mut Vec<Preset>,
) {
    let entries = read_dir_sorted(dir);

    for path in entries {
        if path.is_dir() {
            collect_rfxchain_presets_recursive(&path, base, prefix, out);
            continue;
        }

        if !is_rfxchain(&path) {
            continue;
        }

        let name = file_stem(&path);
        let relative = path.strip_prefix(base).unwrap_or(&path);

        // Build a slug from the full relative path for uniqueness
        let path_slug = slugify(&relative.to_string_lossy().replace(['/', '\\'], "-"));
        let seed_key = format!("{prefix}-{path_slug}");
        let preset_id = seed_id(&seed_key);
        let snapshot_id = seed_id(&format!("{seed_key}-default"));

        // Try reading sidecar
        let sidecar = sidecar::read_sidecar(&path);

        // Determine block type from sidecar or folder name
        let block_type = sidecar
            .as_ref()
            .and_then(|sc| match &sc.kind {
                PresetKind::Block { block_type } => BlockType::from_str_lenient(block_type),
                _ => None,
            })
            .or_else(|| block_type_from_folder(dir, base))
            .unwrap_or(BlockType::Custom);

        // Build metadata from sidecar + folder context
        let folder = relative
            .parent()
            .filter(|p| p != &Path::new(""))
            .map(|p| p.to_string_lossy().replace('\\', "/"));

        let mut metadata = Metadata::new().with_tag("FXChains");
        if let Some(ref folder) = folder {
            metadata = metadata.with_folder(folder);
        }
        if let Some(ref sc) = sidecar {
            for tag in &sc.tags {
                metadata = metadata.with_tag(tag);
            }
            if let Some(ref desc) = sc.description {
                metadata = metadata.with_description(desc);
            }
        }

        // Read the rfxchain file content as state data
        let state_data = std::fs::read(&path).ok();

        let snapshot = {
            let s = Snapshot::new(snapshot_id, &name, Block::from_parameters(vec![]))
                .with_metadata(metadata.clone());
            if let Some(data) = state_data {
                s.with_state_data(data)
            } else {
                s
            }
        };

        let preset =
            Preset::new(preset_id, &name, block_type, snapshot, vec![]).with_metadata(metadata);

        out.push(preset);
    }
}

/// Try to infer `BlockType` from the folder name under `01-Blocks/`.
///
/// e.g. `01-Blocks/Amps/Neural DSP/` → `BlockType::Amp`
/// Uses the first directory component after the base (the category folder).
fn block_type_from_folder(dir: &Path, base: &Path) -> Option<BlockType> {
    let relative = dir.strip_prefix(base).ok()?;
    let first_component = relative.components().next()?;
    let folder_name = first_component.as_os_str().to_str()?;

    // Map common folder names to block types (case-insensitive)
    match folder_name.to_lowercase().as_str() {
        "amps" | "amp" => Some(BlockType::Amp),
        "drives" | "drive" => Some(BlockType::Drive),
        "reverbs" | "reverb" => Some(BlockType::Reverb),
        "delays" | "delay" => Some(BlockType::Delay),
        "eq" | "eqs" => Some(BlockType::Eq),
        "compression" | "compressor" | "compressors" => Some(BlockType::Compressor),
        "modulation" => Some(BlockType::Modulation),
        "chorus" => Some(BlockType::Chorus),
        "flanger" | "flangers" => Some(BlockType::Flanger),
        "phaser" | "phasers" => Some(BlockType::Phaser),
        "cabinets" | "cabinet" | "cabs" => Some(BlockType::Cabinet),
        "gates" | "gate" => Some(BlockType::Gate),
        "special" => Some(BlockType::Special),
        "wah" => Some(BlockType::Wah),
        "filter" | "filters" => Some(BlockType::Filter),
        "pitch" => Some(BlockType::Pitch),
        _ => None,
    }
}

fn is_rfxchain(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map_or(false, |ext| ext.eq_ignore_ascii_case("rfxchain"))
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

fn read_dir_sorted(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();
    paths
}
