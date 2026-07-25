//! Filesystem-direct pack discovery — no DB.
//!
//! Walks a library root, opens each `.signalpack` header (no audio decode),
//! and produces [`PackEntry`] values containing everything the browser needs
//! to render a tag-filterable list. Convertible to [`ColumnItem`] so the
//! same `signal-browser` consumers (Dioxus collection_browser, ratatui TUI)
//! can render packs without knowing they came from disk.
//!
//! Cheap: opening a pack header is a small `read_exact` of the index
//! footer. Scanning ~5,000 packs takes well under a second.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use signal_proto::tagging::{StructuredTag, TagSet};
use signal_sampler::{LibrarySpec, read_pack_header};

use crate::types::{ColumnItem, DetailData};

/// What kind of loadable file this entry represents. Drives icon + dispatch
/// in the TUI / collection browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryKind {
    /// `.signalpack` — binary audio container (requires multi-mic Engine wrapper to load).
    Pack,
    /// `.signalblock` — saved configured block.
    Block,
    /// `.signalmodule` — reusable processing-graph node template.
    Module,
    /// `.signalengine` — one playable Engine instance.
    Engine,
    /// `.signalpreset` — full kit/rig with multiple engines + routing.
    Preset,
}

impl EntryKind {
    pub fn icon(self) -> &'static str {
        match self {
            Self::Preset => "🎚️",
            Self::Engine => "⚙️",
            Self::Module => "📦",
            Self::Block => "🔲",
            Self::Pack => "🎵",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Preset => "preset",
            Self::Engine => "engine",
            Self::Module => "module",
            Self::Block => "block",
            Self::Pack => "pack",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "signalpack" => Some(Self::Pack),
            "signalblock" => Some(Self::Block),
            "signalmodule" => Some(Self::Module),
            "signalengine" => Some(Self::Engine),
            "signalpreset" => Some(Self::Preset),
            _ => None,
        }
    }
}

/// One non-fatal problem encountered while scanning a library root.
#[derive(Clone, Debug)]
pub struct ScanError {
    pub path: Option<PathBuf>,
    pub message: String,
}

/// Detailed result for a filesystem scan. The TUI uses this to show whether
/// an empty or sparse browser is caused by no files, unreadable directories,
/// or loadable files that failed to parse.
#[derive(Clone, Debug, Default)]
pub struct ScanReport {
    pub root: PathBuf,
    pub entries: Vec<PackEntry>,
    pub candidate_count: usize,
    pub skipped_walk_entries: usize,
    pub failed_loadables: usize,
    pub errors: Vec<ScanError>,
}

impl ScanReport {
    pub fn loaded_count(&self) -> usize {
        self.entries.len()
    }

    pub fn has_failures(&self) -> bool {
        self.skipped_walk_entries > 0 || self.failed_loadables > 0
    }
}

/// Lightweight summary of a single loadable file on disk (.signalpack,
/// .signalengine, .signalpreset, .signalmodule, .signalblock).
#[derive(Clone, Debug)]
pub struct PackEntry {
    /// Discriminator — which load path this entry uses.
    pub kind: EntryKind,
    /// Absolute path to the file.
    pub path: PathBuf,
    /// `LibrarySpec.name`, falling back to the file stem.
    pub name: String,
    /// `LibrarySpec.instrument`. Empty when not classified.
    pub instrument: String,
    /// `LibrarySpec.category`. Empty when not classified.
    pub category: String,
    /// `LibrarySpec.style`.
    pub style: Vec<String>,
    /// Structured tags materialised from `LibrarySpec.tags`.
    pub tags: TagSet,
    /// Vendor (`LibrarySpec.vendor`).
    pub vendor: String,
    /// Path component groups for tree-style display: e.g.
    /// `["Drum Kits", "Stylus RMX", "RMX Grooves"]`. Last segment is the
    /// pack's immediate parent folder. Empty when the pack sits at `root`.
    pub folder: Vec<String>,
    /// Total samples in the pack body.
    pub sample_count: usize,
    /// Pack file size on disk in bytes.
    pub size_bytes: u64,
}

impl PackEntry {
    /// Convert to a `ColumnItem` for rendering in the multi-column browser.
    pub fn to_column_item(&self) -> ColumnItem {
        let subtitle = match (&self.instrument, &self.category) {
            (i, c) if !i.is_empty() && !c.is_empty() => Some(format!("{i} · {c}")),
            (i, _) if !i.is_empty() => Some(i.clone()),
            (_, c) if !c.is_empty() => Some(c.clone()),
            _ => None,
        };
        let badge = (!self.vendor.is_empty()).then(|| self.vendor.clone());
        ColumnItem {
            id: self.path.to_string_lossy().into_owned(),
            name: self.name.clone(),
            subtitle,
            badge,
            metadata: None,
            structured_tags: self.tags.clone(),
            detail: DetailData::default(),
            tag: None,
            folder: (!self.folder.is_empty()).then(|| self.folder.join(" / ")),
        }
    }
}

/// Scan `root` recursively for any Signal loadable file (.signalpack,
/// .signalengine, .signalpreset, .signalmodule, .signalblock). Returns
/// `PackEntry` records sorted by path. Errors during individual reads
/// are logged and skipped.
///
/// Renamed conceptually to "loadables" but keeps the name `scan_packs`
/// for back-compat. Use [`scan_loadables`] (alias) if you prefer.
pub fn scan_packs(root: &Path) -> Vec<PackEntry> {
    scan_packs_report(root).entries
}

/// Scan `root` and retain diagnostic information about skipped directories
/// and files that looked loadable but failed to parse.
pub fn scan_packs_report(root: &Path) -> ScanReport {
    const MAX_SCAN_ERRORS: usize = 32;
    let mut report = ScanReport {
        root: root.to_path_buf(),
        ..ScanReport::default()
    };
    let mut entries: Vec<(PathBuf, EntryKind)> = Vec::new();

    for item in walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(12)
        .into_iter()
    {
        let entry = match item {
            Ok(entry) => entry,
            Err(err) => {
                report.skipped_walk_entries += 1;
                if report.errors.len() < MAX_SCAN_ERRORS {
                    report.errors.push(ScanError {
                        path: err.path().map(Path::to_path_buf),
                        message: err.to_string(),
                    });
                }
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.into_path();
        let Some(kind) = path
            .extension()
            .and_then(|x| x.to_str())
            .and_then(EntryKind::from_extension)
        else {
            continue;
        };
        report.candidate_count += 1;
        entries.push((path, kind));
    }

    let built: Vec<Result<PackEntry, (PathBuf, signal_sampler::SamplerError)>> = entries
        .par_iter()
        .map(|(path, kind)| {
            build_entry_dispatch(path, root, *kind).map_err(|err| (path.clone(), err))
        })
        .collect();

    let mut out = Vec::with_capacity(built.len());
    for item in built {
        match item {
            Ok(entry) => out.push(entry),
            Err((path, err)) => {
                report.failed_loadables += 1;
                if report.errors.len() < MAX_SCAN_ERRORS {
                    report.errors.push(ScanError {
                        path: Some(path),
                        message: err.to_string(),
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    report.entries = out;
    report
}

/// Alias for `scan_packs` — mirrors the new "loadables" terminology.
pub fn scan_loadables(root: &Path) -> Vec<PackEntry> {
    scan_packs(root)
}

/// Alias for [`scan_packs_report`] using the newer loadables terminology.
pub fn scan_loadables_report(root: &Path) -> ScanReport {
    scan_packs_report(root)
}

fn build_entry_dispatch(
    path: &Path,
    root: &Path,
    kind: EntryKind,
) -> Result<PackEntry, signal_sampler::SamplerError> {
    match kind {
        EntryKind::Pack => build_pack_entry(path, root),
        EntryKind::Engine => build_engine_entry(path, root),
        EntryKind::Preset => build_preset_entry(path, root),
        EntryKind::Module => build_module_entry(path, root),
        EntryKind::Block => build_block_entry(path, root),
    }
}

fn folder_segments(path: &Path, root: &Path) -> Vec<String> {
    path.parent()
        .and_then(|p| p.strip_prefix(root).ok())
        .map(|rel| {
            rel.components()
                .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn build_engine_entry(path: &Path, root: &Path) -> Result<PackEntry, signal_sampler::SamplerError> {
    let spec = signal_sampler::EngineSpec::from_file(path)?;
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut tags = TagSet::new();
    if !spec.engine_type.is_empty() {
        tags.insert(StructuredTag::new(
            signal_proto::tagging::TagCategory::Instrument,
            spec.engine_type.clone(),
        ));
    }
    Ok(PackEntry {
        kind: EntryKind::Engine,
        path: path.to_path_buf(),
        name: if spec.name.is_empty() {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string()
        } else {
            spec.name
        },
        instrument: spec.engine_type,
        category: "engine".into(),
        style: Vec::new(),
        tags,
        vendor: String::new(),
        folder: folder_segments(path, root),
        sample_count: 0,
        size_bytes,
    })
}

fn build_preset_entry(path: &Path, root: &Path) -> Result<PackEntry, signal_sampler::SamplerError> {
    let spec = signal_sampler::PresetSpec::from_file(path)?;
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(PackEntry {
        kind: EntryKind::Preset,
        path: path.to_path_buf(),
        name: if spec.name.is_empty() {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string()
        } else {
            spec.name
        },
        instrument: String::new(),
        category: "preset".into(),
        style: Vec::new(),
        tags: TagSet::new(),
        vendor: String::new(),
        folder: folder_segments(path, root),
        sample_count: spec.engines.len(),
        size_bytes,
    })
}

fn build_module_entry(path: &Path, root: &Path) -> Result<PackEntry, signal_sampler::SamplerError> {
    let spec = signal_sampler::ModuleSpec::from_file(path)?;
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(PackEntry {
        kind: EntryKind::Module,
        path: path.to_path_buf(),
        name: if spec.name.is_empty() {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string()
        } else {
            spec.name
        },
        instrument: String::new(),
        category: "module".into(),
        style: Vec::new(),
        tags: TagSet::new(),
        vendor: String::new(),
        folder: folder_segments(path, root),
        sample_count: 0,
        size_bytes,
    })
}

fn build_block_entry(path: &Path, root: &Path) -> Result<PackEntry, signal_sampler::SamplerError> {
    let spec = signal_sampler::BlockSpec::from_file(path)?;
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(PackEntry {
        kind: EntryKind::Block,
        path: path.to_path_buf(),
        name: if spec.name.is_empty() {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string()
        } else {
            spec.name
        },
        instrument: spec.block_type,
        category: "block".into(),
        style: Vec::new(),
        tags: TagSet::new(),
        vendor: String::new(),
        folder: folder_segments(path, root),
        sample_count: 0,
        size_bytes,
    })
}

fn build_pack_entry(path: &Path, root: &Path) -> Result<PackEntry, signal_sampler::SamplerError> {
    let header = read_pack_header(path)?;
    let spec: LibrarySpec = header.spec;
    let name = if spec.name.is_empty() {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string()
    } else {
        spec.name.clone()
    };
    let mut tags = spec.tag_set();
    // Round out the tag set with anything implied but not explicit so the
    // filter chips work uniformly across older and newer packs.
    if !spec.instrument.is_empty() && tag_missing(&tags, "instrument", &spec.instrument) {
        tags.insert(StructuredTag::new(
            signal_proto::tagging::TagCategory::Instrument,
            spec.instrument.clone(),
        ));
    }
    if !spec.vendor.is_empty() && tag_missing(&tags, "vendor", &spec.vendor) {
        tags.insert(StructuredTag::new(
            signal_proto::tagging::TagCategory::Vendor,
            spec.vendor.clone(),
        ));
    }

    let folder = path
        .parent()
        .and_then(|p| p.strip_prefix(root).ok())
        .map(|rel| {
            rel.components()
                .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(PackEntry {
        kind: EntryKind::Pack,
        path: path.to_path_buf(),
        name,
        instrument: spec.instrument,
        category: spec.category,
        style: spec.style,
        tags,
        vendor: spec.vendor,
        folder,
        sample_count: header.sample_count,
        size_bytes: header.size_bytes,
    })
}

fn tag_missing(tags: &TagSet, category_str: &str, value: &str) -> bool {
    !tags
        .values()
        .any(|t| t.category.as_str() == category_str && t.value == value)
}

/// Filter packs by an instrument substring (case-insensitive).
/// Empty `q` returns all entries unchanged.
pub fn filter_instrument<'a>(entries: &'a [PackEntry], q: &str) -> Vec<&'a PackEntry> {
    if q.is_empty() {
        return entries.iter().collect();
    }
    let q = q.to_ascii_lowercase();
    entries
        .iter()
        .filter(|e| e.instrument.to_ascii_lowercase().contains(&q))
        .collect()
}

/// Filter packs by category (case-insensitive exact match).
pub fn filter_category<'a>(entries: &'a [PackEntry], category: &str) -> Vec<&'a PackEntry> {
    if category.is_empty() {
        return entries.iter().collect();
    }
    let q = category.to_ascii_lowercase();
    entries
        .iter()
        .filter(|e| e.category.to_ascii_lowercase() == q)
        .collect()
}

/// Free-text search across name + folder + tags.
pub fn search<'a>(entries: &'a [PackEntry], q: &str) -> Vec<&'a PackEntry> {
    if q.is_empty() {
        return entries.iter().collect();
    }
    let q = q.to_ascii_lowercase();
    entries
        .iter()
        .filter(|e| {
            e.name.to_ascii_lowercase().contains(&q)
                || e.folder.iter().any(|s| s.to_ascii_lowercase().contains(&q))
                || e.instrument.to_ascii_lowercase().contains(&q)
                || e.category.to_ascii_lowercase().contains(&q)
                || e.tags
                    .values()
                    .any(|t| t.value.to_ascii_lowercase().contains(&q))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_report_for_empty_root_has_counts() {
        let root =
            std::env::temp_dir().join(format!("signal-browser-empty-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let report = scan_packs_report(&root);

        assert_eq!(report.root, root);
        assert_eq!(report.loaded_count(), 0);
        assert_eq!(report.candidate_count, 0);
        assert_eq!(report.failed_loadables, 0);
        assert_eq!(report.skipped_walk_entries, 0);
        assert!(report.errors.is_empty());

        let _ = std::fs::remove_dir_all(&report.root);
    }
}
