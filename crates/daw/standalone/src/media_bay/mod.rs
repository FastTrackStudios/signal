//! REAPER-style project Media/FX Bay (Ctrl+B).
//!
//! Per-project housekeeping registry over three views:
//!
//! - **SourceMedia** — unique source-file paths referenced by any
//!   take in the project (deduplicated).
//! - **MediaItems** — every individual `Item` in the project.
//! - **Effects** — every FX entry across every chain.
//!
//! Pull-based: every query walks `ProjectState` to derive the live
//! list. The bay's *persistent* state is small — retained paths +
//! folder layout — and lives next to `ProjectState` (see [`BayState`]).
//!
//! ## File intake (WASM)
//!
//! The bay is also the **single point of indirection** for resolving
//! source-file paths to bytes. Native callers register a
//! `BayFileResolver` that reads from disk; WASM callers register one
//! backed by `postMessage`d bytes / IndexedDB / fetch. The renderer
//! and `materialize_audio` both go through the bay so the same
//! project state plays back identically on both targets.
//!
//! ```ignore
//! daw.media_bay().set_file_resolver(Box::new(FsResolver));         // native
//! daw.media_bay().set_file_resolver(Box::new(JsBlobResolver{...})); // wasm
//! ```

mod resolver;
mod state;
mod types;

#[cfg(feature = "http-resolver")]
pub use resolver::HttpBaseUrlResolver;
pub use resolver::{BayFileResolver, FsFileResolver, InMemoryResolver, ProjectRelativeResolver};
pub use state::BayState;
pub(crate) use state::BayStateExt;
pub use types::{BayFolder, BayView, MediaBayEntry, ReplaceScope, SourceUsage};

use std::collections::HashMap;

use daw_proto::{DawError, DawResult, ProjectContext};

use crate::sync::Standalone;

/// Handle on the project Media/FX Bay. Cheap to clone — internally
/// just clones the `Standalone` (an `Arc`). Construct via
/// [`Standalone::media_bay`]. The resolver lives on `Standalone` so
/// every bay handle on the same DAW shares the install.
#[derive(Clone)]
pub struct MediaBay {
    pub(crate) daw: Standalone,
}

impl MediaBay {
    pub(crate) fn new(daw: Standalone) -> Self {
        Self { daw }
    }

    /// Install a file resolver. WASM apps install a JS-backed
    /// resolver; native apps install [`FsFileResolver`] (or roll
    /// their own to add caching / fallback URLs / etc.).
    pub fn set_file_resolver(&self, resolver: Box<dyn BayFileResolver>) {
        *self.daw.bay_resolver.lock().expect("resolver poisoned") = Some(resolver);
    }

    /// Every distinct `Take.source_file_path` referenced by the
    /// project. Callers use this to discover what they need to fetch
    /// (typically: `for path in paths_to_resolve()` → HTTP fetch →
    /// `set_file_resolver(InMemoryResolver with all fetched bytes)`,
    /// or push the bytes one-at-a-time into a custom resolver).
    ///
    /// Order is stable (sorted lexicographically) so client-side
    /// caches keyed by index stay valid across calls.
    pub fn paths_to_resolve(&self, project: ProjectContext) -> Vec<String> {
        let Some(guid) = self.resolve_project(&project) else {
            return Vec::new();
        };
        self.daw
            .read_project(&guid, |p| {
                let mut paths: std::collections::HashSet<String> = std::collections::HashSet::new();
                for tl in p.takes.values() {
                    for take in &tl.takes {
                        if let Some(path) = &take.source_file_path
                            && !path.is_empty()
                        {
                            paths.insert(path.clone());
                        }
                    }
                }
                let mut v: Vec<String> = paths.into_iter().collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }

    /// Resolve `path` to bytes via the installed resolver. Returns
    /// `Err` if no resolver is set or the resolver itself fails.
    pub fn resolve_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let guard = self.daw.bay_resolver.lock().expect("resolver poisoned");
        let r = guard
            .as_ref()
            .ok_or_else(|| "no BayFileResolver installed".to_string())?;
        r.resolve(path)
    }

    /// Resolve a source path to an on-disk file when the installed
    /// resolver can (native filesystems) — the streaming-PCM fast path.
    pub fn resolve_file_path(&self, path: &str) -> Option<std::path::PathBuf> {
        let guard = self.daw.bay_resolver.lock().expect("resolver poisoned");
        guard.as_ref().and_then(|r| r.resolve_path(path))
    }

    // ── Listing ─────────────────────────────────────────────────

    /// List entries for a view. `filter` is a case-insensitive
    /// substring matched against `name` and `path` (any field).
    pub fn list(&self, project: ProjectContext, view: BayView, filter: &str) -> Vec<MediaBayEntry> {
        let Some(guid) = self.resolve_project(&project) else {
            return Vec::new();
        };
        let filter_l = filter.to_ascii_lowercase();
        self.daw
            .read_project(&guid, |p| match view {
                BayView::SourceMedia => Self::derive_source_entries(p, &filter_l),
                BayView::MediaItems => Self::derive_item_entries(p, &filter_l),
                BayView::Effects => Self::derive_fx_entries(p, &filter_l),
            })
            .unwrap_or_default()
    }

    /// Look up a single entry by id.
    pub fn get(&self, project: ProjectContext, view: BayView, id: &str) -> Option<MediaBayEntry> {
        let guid = self.resolve_project(&project)?;
        self.daw.read_project(&guid, |p| match view {
            BayView::SourceMedia => Self::derive_source_entries(p, "")
                .into_iter()
                .find(|e| e.id == id),
            BayView::MediaItems => Self::derive_item_entries(p, "")
                .into_iter()
                .find(|e| e.id == id),
            BayView::Effects => Self::derive_fx_entries(p, "")
                .into_iter()
                .find(|e| e.id == id),
        })?
    }

    // ── Usage tracking ──────────────────────────────────────────

    pub fn usage_count(&self, project: ProjectContext, source_path: &str) -> u32 {
        let Some(guid) = self.resolve_project(&project) else {
            return 0;
        };
        self.daw
            .read_project(&guid, |p| {
                let mut n = 0u32;
                for tl in p.takes.values() {
                    let active = tl.takes.get(tl.active_idx as usize);
                    if active
                        .and_then(|t| t.source_file_path.as_deref())
                        .map(|p| p == source_path)
                        .unwrap_or(false)
                    {
                        n += 1;
                    }
                }
                n
            })
            .unwrap_or(0)
    }

    pub fn usages(&self, project: ProjectContext, source_path: &str) -> Vec<SourceUsage> {
        let Some(guid) = self.resolve_project(&project) else {
            return Vec::new();
        };
        self.daw
            .read_project(&guid, |p| {
                let mut out = Vec::new();
                let mut ordinal = 0u32;
                // Walk items in stable order so ordinal numbering is
                // deterministic. items_by_track keeps insertion order
                // per track; iterating tracks gives a stable shape.
                for t in &p.tracks {
                    if let Some(item_guids) = p.items_by_track.get(&t.guid) {
                        for ig in item_guids {
                            let Some(tl) = p.takes.get(ig) else { continue };
                            let Some(active) = tl.takes.get(tl.active_idx as usize) else {
                                continue;
                            };
                            if active.source_file_path.as_deref() == Some(source_path) {
                                out.push(SourceUsage {
                                    item_guid: ig.clone(),
                                    take_guid: active.guid.clone(),
                                    ordinal,
                                });
                                ordinal += 1;
                            }
                        }
                    }
                }
                out
            })
            .unwrap_or_default()
    }

    // ── Source-level mutations ──────────────────────────────────

    pub fn set_muted_all_uses(
        &self,
        project: ProjectContext,
        source_path: &str,
        muted: bool,
    ) -> DawResult<()> {
        let guid = self.resolve_project(&project).ok_or_else(not_found_proj)?;
        self.daw
            .write_project(&guid, |p| {
                let touched: Vec<String> = p
                    .takes
                    .iter()
                    .filter_map(|(item_guid, tl)| {
                        let active = tl.takes.get(tl.active_idx as usize)?;
                        if active.source_file_path.as_deref() == Some(source_path) {
                            Some(item_guid.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                for item_guid in touched {
                    if let Some(entry) = p.items.get_mut(&item_guid) {
                        entry.item.muted = muted;
                    }
                }
            })
            .ok_or_else(not_found_proj)?;
        Ok(())
    }

    pub fn rename_source(
        &self,
        project: ProjectContext,
        old_path: &str,
        new_path: &str,
    ) -> DawResult<()> {
        let guid = self.resolve_project(&project).ok_or_else(not_found_proj)?;
        self.daw
            .write_project(&guid, |p| {
                for tl in p.takes.values_mut() {
                    for take in tl.takes.iter_mut() {
                        if take.source_file_path.as_deref() == Some(old_path) {
                            take.source_file_path = Some(new_path.to_string());
                        }
                    }
                }
                // Carry retention across the rename so the entry
                // stays "available" under its new name.
                let bay = p.bay_state.get_or_create();
                if bay.retained.remove(old_path) {
                    bay.retained.insert(new_path.to_string());
                }
                // Update folder assignments too.
                for folder in bay.folders.values_mut() {
                    if folder.view == BayView::SourceMedia {
                        for entry in folder.entries.iter_mut() {
                            if entry == old_path {
                                *entry = new_path.to_string();
                            }
                        }
                    }
                }
            })
            .ok_or_else(not_found_proj)?;
        Ok(())
    }

    pub fn replace_in_project(
        &self,
        project: ProjectContext,
        old_path: &str,
        new_path: &str,
        scope: ReplaceScope,
    ) -> DawResult<()> {
        let guid = self.resolve_project(&project).ok_or_else(not_found_proj)?;
        // Compute usage ordering up front (before mutating) so
        // SingleInstance picks the right take regardless of HashMap
        // iteration order during the rewrite.
        let usages = self.usages(ProjectContext::Project(guid.clone()), old_path);
        let target_take_guids: Vec<String> = match scope {
            ReplaceScope::AllInstances => usages.into_iter().map(|u| u.take_guid).collect(),
            ReplaceScope::SingleInstance { ordinal } => usages
                .into_iter()
                .find(|u| u.ordinal == ordinal)
                .map(|u| vec![u.take_guid])
                .unwrap_or_default(),
        };
        if target_take_guids.is_empty() {
            return Err(DawError::not_found("source", old_path));
        }
        self.daw
            .write_project(&guid, |p| {
                for tl in p.takes.values_mut() {
                    for take in tl.takes.iter_mut() {
                        if target_take_guids.contains(&take.guid) {
                            take.source_file_path = Some(new_path.to_string());
                        }
                    }
                }
            })
            .ok_or_else(not_found_proj)?;
        Ok(())
    }

    // ── Item-level mutations ────────────────────────────────────

    pub fn set_item_muted(
        &self,
        project: ProjectContext,
        item_guid: &str,
        muted: bool,
    ) -> DawResult<()> {
        let guid = self.resolve_project(&project).ok_or_else(not_found_proj)?;
        let did = self
            .daw
            .write_project(&guid, |p| {
                if let Some(entry) = p.items.get_mut(item_guid) {
                    entry.item.muted = muted;
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);
        if did {
            Ok(())
        } else {
            Err(DawError::not_found("Item", item_guid))
        }
    }

    pub fn rename_item(
        &self,
        project: ProjectContext,
        item_guid: &str,
        new_name: &str,
    ) -> DawResult<()> {
        let guid = self.resolve_project(&project).ok_or_else(not_found_proj)?;
        let did = self
            .daw
            .write_project(&guid, |p| {
                let Some(tl) = p.takes.get_mut(item_guid) else {
                    return false;
                };
                let Some(t) = tl.takes.get_mut(tl.active_idx as usize) else {
                    return false;
                };
                t.name = new_name.to_string();
                true
            })
            .unwrap_or(false);
        if did {
            Ok(())
        } else {
            Err(DawError::not_found("Item", item_guid))
        }
    }

    // ── Retention ───────────────────────────────────────────────

    pub fn set_retained(
        &self,
        project: ProjectContext,
        source_path: &str,
        keep: bool,
    ) -> DawResult<()> {
        let guid = self.resolve_project(&project).ok_or_else(not_found_proj)?;
        self.daw
            .write_project(&guid, |p| {
                let bay = p.bay_state.get_or_create();
                if keep {
                    bay.retained.insert(source_path.to_string());
                } else {
                    bay.retained.remove(source_path);
                }
            })
            .ok_or_else(not_found_proj)?;
        Ok(())
    }

    pub fn retained(&self, project: ProjectContext) -> Vec<String> {
        let Some(guid) = self.resolve_project(&project) else {
            return Vec::new();
        };
        self.daw
            .read_project(&guid, |p| {
                p.bay_state
                    .as_ref()
                    .map(|b| {
                        let mut v: Vec<String> = b.retained.iter().cloned().collect();
                        v.sort();
                        v
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    // ── Bay folders ─────────────────────────────────────────────

    pub fn create_bay_folder(
        &self,
        project: ProjectContext,
        view: BayView,
        name: &str,
    ) -> DawResult<()> {
        let guid = self.resolve_project(&project).ok_or_else(not_found_proj)?;
        self.daw
            .write_project(&guid, |p| {
                p.bay_state.get_or_create().folders.insert(
                    (view, name.to_string()),
                    BayFolder {
                        name: name.to_string(),
                        view,
                        entries: Vec::new(),
                    },
                );
            })
            .ok_or_else(not_found_proj)?;
        Ok(())
    }

    pub fn delete_bay_folder(
        &self,
        project: ProjectContext,
        view: BayView,
        name: &str,
    ) -> DawResult<()> {
        let guid = self.resolve_project(&project).ok_or_else(not_found_proj)?;
        self.daw
            .write_project(&guid, |p| {
                if let Some(bay) = p.bay_state.as_mut_inner() {
                    bay.folders.remove(&(view, name.to_string()));
                }
            })
            .ok_or_else(not_found_proj)?;
        Ok(())
    }

    pub fn move_to_bay_folder(
        &self,
        project: ProjectContext,
        view: BayView,
        entry_id: &str,
        folder_name: Option<&str>,
    ) -> DawResult<()> {
        let guid = self.resolve_project(&project).ok_or_else(not_found_proj)?;
        self.daw
            .write_project(&guid, |p| {
                let bay = p.bay_state.get_or_create();
                // Remove from any current folder first.
                for f in bay.folders.values_mut() {
                    if f.view == view {
                        f.entries.retain(|e| e != entry_id);
                    }
                }
                if let Some(fname) = folder_name {
                    let key = (view, fname.to_string());
                    let folder = bay.folders.entry(key.clone()).or_insert(BayFolder {
                        name: fname.to_string(),
                        view,
                        entries: Vec::new(),
                    });
                    if !folder.entries.iter().any(|e| e == entry_id) {
                        folder.entries.push(entry_id.to_string());
                    }
                }
            })
            .ok_or_else(not_found_proj)?;
        Ok(())
    }

    pub fn bay_folders(&self, project: ProjectContext, view: BayView) -> Vec<BayFolder> {
        let Some(guid) = self.resolve_project(&project) else {
            return Vec::new();
        };
        self.daw
            .read_project(&guid, |p| {
                p.bay_state
                    .as_ref()
                    .map(|b| {
                        b.folders
                            .values()
                            .filter(|f| f.view == view)
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    // ── Snapshot / restore ──────────────────────────────────────

    /// Postcard-encoded snapshot of the bay's persistent state
    /// (retained paths + folder layout). Live source/item/fx lists
    /// rebuild from `ProjectState` on read; they're not in the
    /// snapshot.
    pub fn save_bay(&self, project: ProjectContext) -> Vec<u8> {
        let Some(guid) = self.resolve_project(&project) else {
            return Vec::new();
        };
        self.daw
            .read_project(&guid, |p| {
                p.bay_state
                    .as_ref()
                    .map(|b| b.serialize())
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    pub fn load_bay(&self, project: ProjectContext, bytes: Vec<u8>) -> DawResult<()> {
        let guid = self.resolve_project(&project).ok_or_else(not_found_proj)?;
        let parsed = BayState::deserialize(&bytes)
            .map_err(|e| DawError::operation_failed(format!("load_bay parse failed: {e}")))?;
        self.daw
            .write_project(&guid, |p| {
                p.bay_state.get_or_create().merge_from(parsed);
            })
            .ok_or_else(not_found_proj)?;
        Ok(())
    }

    // ── Internals ───────────────────────────────────────────────

    fn resolve_project(&self, ctx: &ProjectContext) -> Option<String> {
        match ctx {
            ProjectContext::Project(g) => Some(g.clone()),
            ProjectContext::Current => self.daw.state.lock().ok()?.current_project_guid.clone(),
        }
    }

    fn derive_source_entries(p: &crate::sync::ProjectState, filter_l: &str) -> Vec<MediaBayEntry> {
        // Tally usages per source path.
        let mut counts: HashMap<String, u32> = HashMap::new();
        let mut all_muted: HashMap<String, bool> = HashMap::new();
        for (item_guid, tl) in &p.takes {
            let Some(active) = tl.takes.get(tl.active_idx as usize) else {
                continue;
            };
            let Some(path) = &active.source_file_path else {
                continue;
            };
            *counts.entry(path.clone()).or_insert(0) += 1;
            let item_muted = p
                .items
                .get(item_guid)
                .map(|e| e.item.muted)
                .unwrap_or(false);
            let entry = all_muted.entry(path.clone()).or_insert(true);
            *entry &= item_muted;
        }
        // Include retained paths even if zero usages.
        let retained = p
            .bay_state
            .as_ref()
            .map(|b| b.retained.clone())
            .unwrap_or_default();
        for r in &retained {
            counts.entry(r.clone()).or_insert(0);
        }
        let folders = p.bay_state.as_ref().map(|b| &b.folders);

        let mut out: Vec<MediaBayEntry> = counts
            .into_iter()
            .filter_map(|(path, n)| {
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                if !filter_l.is_empty()
                    && !name.to_ascii_lowercase().contains(filter_l)
                    && !path.to_ascii_lowercase().contains(filter_l)
                {
                    return None;
                }
                let bay_folder = folders.and_then(|fm| {
                    fm.iter().find_map(|((view, fname), f)| {
                        if *view == BayView::SourceMedia && f.entries.iter().any(|e| e == &path) {
                            Some(fname.clone())
                        } else {
                            None
                        }
                    })
                });
                Some(MediaBayEntry {
                    id: path.clone(),
                    name,
                    path: Some(path.clone()),
                    usage_count: n,
                    retained: retained.contains(&path),
                    all_muted: if n == 0 {
                        None
                    } else {
                        all_muted.get(&path).copied()
                    },
                    bay_folder,
                })
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    fn derive_item_entries(p: &crate::sync::ProjectState, filter_l: &str) -> Vec<MediaBayEntry> {
        let folders = p.bay_state.as_ref().map(|b| &b.folders);
        let mut out = Vec::new();
        for t in &p.tracks {
            let Some(item_guids) = p.items_by_track.get(&t.guid) else {
                continue;
            };
            for ig in item_guids {
                let Some(entry) = p.items.get(ig) else {
                    continue;
                };
                let take_name = p
                    .takes
                    .get(ig)
                    .and_then(|tl| tl.takes.get(tl.active_idx as usize))
                    .map(|t| t.name.clone())
                    .unwrap_or_default();
                if !filter_l.is_empty() && !take_name.to_ascii_lowercase().contains(filter_l) {
                    continue;
                }
                let bay_folder = folders.and_then(|fm| {
                    fm.iter().find_map(|((view, fname), f)| {
                        if *view == BayView::MediaItems && f.entries.iter().any(|e| e == ig) {
                            Some(fname.clone())
                        } else {
                            None
                        }
                    })
                });
                out.push(MediaBayEntry {
                    id: ig.clone(),
                    name: take_name,
                    path: p
                        .takes
                        .get(ig)
                        .and_then(|tl| tl.takes.get(tl.active_idx as usize))
                        .and_then(|t| t.source_file_path.clone()),
                    usage_count: 1,
                    retained: false,
                    all_muted: Some(entry.item.muted),
                    bay_folder,
                });
            }
        }
        out
    }

    fn derive_fx_entries(p: &crate::sync::ProjectState, filter_l: &str) -> Vec<MediaBayEntry> {
        let folders = p.bay_state.as_ref().map(|b| &b.folders);
        let mut out = Vec::new();
        for chain in p.fx_chains.values() {
            for entry in chain {
                let name = entry.fx.plugin_name.clone();
                if !filter_l.is_empty() && !name.to_ascii_lowercase().contains(filter_l) {
                    continue;
                }
                let bay_folder = folders.and_then(|fm| {
                    fm.iter().find_map(|((view, fname), f)| {
                        if *view == BayView::Effects
                            && f.entries.iter().any(|e| e == &entry.fx.guid)
                        {
                            Some(fname.clone())
                        } else {
                            None
                        }
                    })
                });
                out.push(MediaBayEntry {
                    id: entry.fx.guid.clone(),
                    name,
                    path: None,
                    usage_count: 1,
                    retained: false,
                    all_muted: Some(!entry.fx.enabled),
                    bay_folder,
                });
            }
        }
        out
    }
}

fn not_found_proj() -> DawError {
    DawError::not_found("Project", "context")
}
