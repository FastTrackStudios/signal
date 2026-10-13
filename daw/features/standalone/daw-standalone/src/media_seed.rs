//! Seed a project with real on-disk media: create a folder/track layout,
//! drop one audio item per stem (a take whose `source_file_path` points at
//! the WAV), then memory-map the sources so the render graph plays them.
//!
//! This is what turns the setlist demo from "markers only" into an actually
//! *playable* multitrack song. Stems are mmap'd (not decoded into RAM), so a
//! 23×78 MB session materializes in milliseconds with flat memory — the OS
//! page cache streams samples during playback.

use daw_proto::midi::Midi;
use daw_proto::{ItemRef, ProjectContext, Takes, TrackRef, Tracks};

use crate::audio_engine::materialize::{MaterializeReport, materialize_audio_streaming};
use crate::sync::Standalone;

/// One stem to seed: a display name, the WAV path, and an optional group
/// (folder) it belongs to. Stems sharing a `group` are nested under one
/// folder track, in first-seen order.
#[derive(Debug, Clone)]
pub struct StemSpec {
    pub name: String,
    pub path: String,
    pub group: Option<String>,
}

impl StemSpec {
    pub fn new(name: impl Into<String>, path: impl Into<String>, group: Option<&str>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            group: group.map(str::to_string),
        }
    }
}

/// One track created by seeding, with the identity a caller needs to decorate
/// it afterwards (colour, default mute, …) without re-reading the project.
#[derive(Debug, Clone)]
pub struct SeededTrack {
    pub guid: String,
    pub name: String,
    pub group: Option<String>,
    pub is_folder: bool,
}

/// Result of seeding: how many stem tracks + folder tracks were created, the
/// full list of created tracks (folders + stems, in creation order), and the
/// materialize report (how many sources loaded / failed).
#[derive(Debug, Default)]
pub struct SeedReport {
    pub tracks_created: usize,
    pub folders_created: usize,
    pub tracks: Vec<SeededTrack>,
    pub materialize: MaterializeReport,
}

/// Create a grouped track layout for `stems` in `project_guid`, place each
/// stem as an audio item starting at `position_seconds` (source offset 0)
/// with `length_seconds`, then materialize (mmap) every source from disk.
///
/// Grouping uses REAPER's folder-depth model: each group gets a folder track
/// (depth +1) and its last child closes the folder (depth −1). Ungrouped
/// stems become top-level tracks. The folder parent doubles as the group bus
/// (mute/volume gang) — e.g. the "Vocals" folder is the vocal VCA.
pub fn seed_media_tracks(
    daw: &Standalone,
    project_guid: &str,
    stems: &[StemSpec],
    position_seconds: f64,
    length_seconds: f64,
) -> SeedReport {
    let ctx = ProjectContext::Project(project_guid.to_string());
    let mut report = SeedReport::default();

    // Preserve first-seen group order; collect each group's stem indices.
    let mut group_order: Vec<Option<String>> = Vec::new();
    for s in stems {
        if !group_order.iter().any(|g| g == &s.group) {
            group_order.push(s.group.clone());
        }
    }

    for group in &group_order {
        let members: Vec<&StemSpec> = stems.iter().filter(|s| &s.group == group).collect();
        if members.is_empty() {
            continue;
        }

        // A named group gets a folder parent; `None` stems stay top-level.
        let folder_track = match group {
            Some(name) => {
                let g = match Tracks::add(daw, ctx.clone(), name, None) {
                    Ok(g) => g,
                    Err(_) => continue,
                };
                // Open a folder at this track.
                let _ = Tracks::set_folder_depth(daw, ctx.clone(), TrackRef::Guid(g.clone()), 1);
                report.folders_created += 1;
                report.tracks.push(SeededTrack {
                    guid: g.clone(),
                    name: name.clone(),
                    group: group.clone(),
                    is_folder: true,
                });
                Some(g)
            }
            None => None,
        };

        let last = members.len() - 1;
        for (i, stem) in members.iter().enumerate() {
            let Ok(track) = Tracks::add(daw, ctx.clone(), &stem.name, None) else {
                continue;
            };
            report.tracks_created += 1;
            report.tracks.push(SeededTrack {
                guid: track.clone(),
                name: stem.name.clone(),
                group: group.clone(),
                is_folder: false,
            });

            // The last child of a folder closes it (depth −1).
            if folder_track.is_some() && i == last {
                let _ =
                    Tracks::set_folder_depth(daw, ctx.clone(), TrackRef::Guid(track.clone()), -1);
            }

            seed_audio_item(
                daw,
                project_guid,
                &ctx,
                &track,
                stem,
                position_seconds,
                length_seconds,
            );
        }
    }

    // Memory-map every stem straight from disk (uncompressed WAV fast path);
    // anything non-PCM falls back to a full decode via the byte resolver.
    report.materialize = materialize_audio_streaming(
        daw,
        project_guid,
        |path| std::fs::read(path).map_err(|e| e.to_string()),
        |path| Some(std::path::PathBuf::from(path)),
    );
    report
}

/// Create one audio item+take on `track` whose source is `stem.path`.
/// Follows the proven pattern (create a MIDI item, flip the active take to
/// audio, point it at the file); `materialize_*` decodes/mmaps it later.
fn seed_audio_item(
    daw: &Standalone,
    project_guid: &str,
    ctx: &ProjectContext,
    track_guid: &str,
    stem: &StemSpec,
    position_seconds: f64,
    length_seconds: f64,
) {
    let Some(loc) = Midi::create_midi_item(
        daw,
        ctx.clone(),
        TrackRef::Guid(track_guid.to_string()),
        position_seconds,
        position_seconds + length_seconds,
    ) else {
        return;
    };
    let ItemRef::Guid(item_guid) = &loc.item else {
        return;
    };
    let Some(active) = Takes::get_active_take(daw, ctx.clone(), ItemRef::Guid(item_guid.clone()))
    else {
        return;
    };
    daw.write_project(project_guid, |p| {
        for tl in p.takes.values_mut() {
            for t in tl.takes.iter_mut() {
                if t.guid == active.guid {
                    t.is_midi = false;
                    t.source_type = daw_proto::item::SourceType::Audio;
                    t.source_file_path = Some(stem.path.clone());
                }
            }
        }
    });
}
