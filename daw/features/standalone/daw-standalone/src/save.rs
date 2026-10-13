//! Saving a standalone project back to disk — as a **new** `.rpp`.
//!
//! The write path is `dawfile-standalone`'s patched export: the original
//! project text is re-parsed and only what the edit actually changed is
//! rewritten, item blocks reconciled by `IGUID`, so every construct this
//! backend never modelled — FX chunks, ruler lanes, extension state —
//! goes out byte-for-byte as REAPER wrote it.
//!
//! Until that round trip is trusted at 100 %, saving **never touches the
//! original**: the file lands beside it as `<stem>.fts-edit.rpp`
//! (numbered up when taken), so the source session stays exactly as
//! REAPER left it and the experiment is always inspectable next to it.
//!
//! Spec: `features/expression-editor/spec/drum-mode.md`, `drums.save.*`.

use std::path::{Path, PathBuf};

use dawfile_standalone::document::{ItemNode, SourceRef, TakeNode};
use dawfile_standalone::id::EntityId;
use dawfile_standalone::project::DawProject;

use crate::sync::{ProjectState, Standalone};

/// Save `project_guid` as a new `.rpp` beside its original.
///
/// Returns the path written. Fails when the project was not opened from
/// a file (there is nothing to patch against — scaffolding a project
/// from nothing is `dawfile_reaper::scaffold`'s job, not this one's).
// r[impl drums.save.new-file]
// r[impl drums.save.patched]
pub fn save_project_as(daw: &Standalone, project_guid: &str) -> Result<PathBuf, String> {
    save_project_as_reported(daw, project_guid).map(|(p, _)| p)
}

/// [`save_project_as`], also returning what the patch rewrote — one
/// line per changed token. Empty means the copy is byte-identical.
pub fn save_project_as_reported(
    daw: &Standalone,
    project_guid: &str,
) -> Result<(PathBuf, Vec<String>), String> {
    let original: PathBuf = daw
        .read_project(project_guid, |p| PathBuf::from(p.info.path.clone()))
        .ok_or_else(|| format!("no project {project_guid}"))?;
    if original.as_os_str().is_empty() {
        return Err("project has no source .rpp to patch against".into());
    }

    let (mut project, _report) =
        DawProject::import_rpp_file(&original).map_err(|e| e.to_string())?;

    daw.read_project(project_guid, |p| {
        project.edit(|doc| {
            // Walk the document's tracks and replace each one's items
            // with the backend's current view. Matching is by guid —
            // both sides adopt the RPP's own `TRACKID`/`IGUID`, so a
            // track the backend synthesized a guid for (an old project
            // with none) simply keeps its file-side items untouched.
            for track_node in doc.tracks.iter_mut() {
                let track_guid = track_node.id.as_str().to_string();
                if let Some(track) = p.tracks.iter().find(|t| t.guid == track_guid) {
                    // The lane layout is the backend's; the exporter writes
                    // it by meaning, so an untouched track is not rewritten.
                    track_node.track.lane_count = track.lane_count;
                    track_node.track.lane_play_mask = track.lane_play_mask;
                    track_node.track.lane_names = track.lane_names.clone();
                    track_node.track.lane_display = track.lane_display;
                }
                if let Some(ext) = p.track_ext.get(&track_guid) {
                    track_node.comping = ext.comping.clone();
                }
                let Some(order) = p.items_by_track.get(&track_guid) else {
                    continue;
                };
                let old: Vec<ItemNode> = std::mem::take(&mut track_node.items);
                let mut items = Vec::with_capacity(order.len());
                for item_guid in order {
                    let Some(entry) = p.items.get(item_guid) else {
                        continue;
                    };
                    let previous = old.iter().find(|n| n.id.as_str() == item_guid);
                    items.push(item_node(p, entry.item.clone(), previous));
                }
                track_node.items = items;
            }
        });
    })
    .ok_or_else(|| format!("no project {project_guid}"))?;

    let (text, report) = project.to_rpp_patched().map_err(|e| e.to_string())?;
    let target = sibling_path(&original);
    if report.is_empty() {
        // Nothing changed: copy the original bytes verbatim rather than
        // the re-stringified tree, so an unedited save is byte-identical
        // (the stringifier normalizes token quoting, which is churn a
        // diff should never show for a no-op).
        std::fs::copy(&original, &target)
            .map_err(|e| format!("copy to {}: {e}", target.display()))?;
    } else {
        tracing::info!(changes = report.changes.len(), "patched export");
        std::fs::write(&target, text).map_err(|e| format!("write {}: {e}", target.display()))?;
    }
    Ok((target, report.changes))
}

/// One item, rebuilt from backend state.
///
/// Where the item existed before the edit, the previous node is the
/// base and only the fields this backend actually *edits* are written
/// over it — position, length, snap offset, mute, fades, loop, take
/// placement, stretch markers. Everything else (colours, take pitch,
/// channel modes, the exact source block, envelopes) keeps the file's
/// own values, because the backend's copies went through lossy
/// conversions (COLORREF→RGB, fade-shape stand-ins) that must not leak
/// into a saved file as phantom edits. A split piece (a new guid) is
/// built whole, inheriting its source from the old take it was cut
/// from.
fn item_node(
    p: &ProjectState,
    item: daw_proto::item::Item,
    previous: Option<&ItemNode>,
) -> ItemNode {
    let base_item = match previous {
        Some(prev) => {
            let mut merged = prev.item.clone();
            merged.position = item.position;
            merged.length = item.length;
            merged.snap_offset = item.snap_offset;
            merged.muted = item.muted;
            merged.locked = item.locked;
            merged.loop_source = item.loop_source;
            merged.volume = item.volume;
            merged.fixed_lane = item.fixed_lane;
            // Fades only when the backend moved them — its *shape*
            // codes are stand-ins for curves the facade cannot name,
            // and writing a stand-in over an untouched Bezier would be
            // an edit nobody made.
            if (merged.fade_in_length.as_seconds() - item.fade_in_length.as_seconds()).abs() > 1e-9
            {
                merged.fade_in_length = item.fade_in_length;
                merged.fade_in_shape = item.fade_in_shape;
            }
            if (merged.fade_out_length.as_seconds() - item.fade_out_length.as_seconds()).abs()
                > 1e-9
            {
                merged.fade_out_length = item.fade_out_length;
                merged.fade_out_shape = item.fade_out_shape;
            }
            merged
        }
        None => item,
    };
    let takes = p
        .takes
        .get(&base_item.guid)
        .map(|tl| {
            tl.takes
                .iter()
                .enumerate()
                .map(|(index, t)| {
                    let prev_take = previous
                        .into_iter()
                        .flat_map(|n| n.takes.iter())
                        .find(|tn| tn.id.as_str() == t.guid);
                    let mut take = match prev_take {
                        Some(prev) => {
                            let mut merged = prev.take.clone();
                            merged.start_offset = t.start_offset;
                            merged.play_rate = t.play_rate;
                            merged.is_active = t.is_active;
                            merged
                        }
                        None => t.clone(),
                    };
                    take.is_active = index == tl.active_idx as usize;
                    let source = prev_take
                        .map(|tn| tn.source.clone())
                        .or_else(|| source_for(t.source_file_path.as_deref()))
                        .unwrap_or(SourceRef::Empty);
                    TakeNode {
                        id: EntityId::adopt(t.guid.clone()),
                        take,
                        source,
                        stretch_markers: p
                            .stretch_markers
                            .get(&t.guid)
                            .cloned()
                            .unwrap_or_default(),
                        envelopes: prev_take.map(|tn| tn.envelopes.clone()).unwrap_or_default(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    ItemNode {
        id: EntityId::adopt(base_item.guid.clone()),
        item: base_item,
        takes,
        envelopes: previous.map(|n| n.envelopes.clone()).unwrap_or_default(),
    }
}

fn source_for(path: Option<&str>) -> Option<SourceRef> {
    let path = path?;
    let kind = Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_uppercase())
        .map(|e| match e.as_str() {
            "WAV" => "WAVE".to_string(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| "WAVE".into());
    Some(SourceRef::File {
        path: path.to_string(),
        kind,
    })
}

/// `song.rpp` → `song.fts-edit.rpp`, `song.fts-edit-2.rpp`, … — the
/// first name that does not exist. Never the original.
// r[impl drums.save.new-file]
fn sibling_path(original: &Path) -> PathBuf {
    let stem = original
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into());
    let dir = original.parent().unwrap_or_else(|| Path::new("."));
    let mut n = 1usize;
    loop {
        let name = if n == 1 {
            format!("{stem}.fts-edit.rpp")
        } else {
            format!("{stem}.fts-edit-{n}.rpp")
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}
