//! Kit discovery, capture, analysis, and workspace assembly.
use crate::audio_read::edit_item;
use crate::{WorkspaceError, attach_timeline, blend, drum_host, read_take_mono, track_timeline};
use daw::service::{ItemRef, Items, ProjectContext, Tracks};
use expression_editor_core::{Editor, Mode, Viewport};
/// A folded drum workspace: the view, the write half, and a label.
pub struct DrumWorkspace<D: expression_editor_audio::daw_bound::DrumDaw> {
    pub editor: Editor,
    pub host: drum_host::DrumHost<D>,
    pub label: String,
}

/// Fold an already-open project's kit into a drum workspace.
///
/// Split out of the `.rpp` loader because opening a file was the only
/// part of that ever specific to the standalone backend. Everything
/// here — finding the kit folder, scoring the candidates against each
/// other, reading each mic's timeline, folding the role lanes — asks
/// the daw the same questions whichever daw it is. That is what lets
/// the REAPER panel build this same workspace from the project REAPER
/// already has open, rather than re-reading it from disk.
/// All facade reads happen on the caller's thread. Only owned sample
/// What a track's audio arrived as: decoded from the project, or read
/// back from a previous open.
///
/// A cached track carries no samples — the decode is exactly what the
/// cache exists to skip — so its detection signals are empty until they
/// are filled in behind the window.
enum CapturedAudio {
    Decoded(Vec<f64>),
    Cached(crate::analysis_cache::Analysis),
}

/// buffers enter the analysis workers, so thread-affine backends such as
/// REAPER use exactly the same loader as standalone hosts.
// r[impl drums.host.daw-agnostic]
pub fn drum_workspace<D: expression_editor_audio::daw_bound::DrumDaw>(
    daw: &D,
    ctx: ProjectContext,
    name: &str,
    kit_folder: Option<&str>,
    viewport: Viewport,
    // `cache_in`: where to keep detected hits between opens, if
    // anywhere. `None` analyses every time, which is what a caller with
    // no project file on disk has to do.
    cache_in: Option<&std::path::Path>,
) -> Result<DrumWorkspace<D>, WorkspaceError> {
    let name = name.to_string();

    let tracks = Tracks::all(daw, ctx.clone());

    // The kit folder. Candidates come from the name — an explicit
    // `--drums <name>` (exact, then substring) or, with no argument,
    // any folder named like a kit — and the BEST-SCORING candidate
    // wins, not the first.
    //
    // A session routinely has more than one folder called `Drums`: the
    // tracked kit and a folder of reference stems or a printed mix.
    // Name cannot separate them, so `--drums Drums` could not either.
    // `score_kit` reads their shape instead — a kit covers kick, snare
    // and toms and groups its mics in sub-folders; a stem folder is
    // four flat tracks. Taking the first match opened the stems on a
    // real project and presented as "the tom lanes are broken", since
    // that folder has no toms.
    let candidates: Vec<&daw::service::Track> = match kit_folder {
        Some(want) => {
            let w = want.to_ascii_lowercase();
            let exact: Vec<_> = tracks
                .iter()
                .filter(|t| t.is_folder && t.name.to_ascii_lowercase() == w)
                .collect();
            if exact.is_empty() {
                tracks
                    .iter()
                    .filter(|t| t.is_folder && t.name.to_ascii_lowercase().contains(&w))
                    .collect()
            } else {
                exact
            }
        }
        None => tracks
            .iter()
            .filter(|t| t.is_folder && expression_editor_core::kit::is_kit_folder(&t.name))
            .collect(),
    };
    let kit = pick_best_kit(&tracks, &candidates).ok_or_else(|| WorkspaceError::NoKitFolder {
        project: name.clone(),
        wanted: kit_folder.map(str::to_string),
    })?;

    let by_guid: std::collections::HashMap<&str, &daw::service::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();

    struct Member {
        guid: String,
        name: String,
        folder: Option<String>,
        role: expression_editor_core::kit::LaneRole,
        doc: expression_editor_core::ExpressionDoc,
        item: ItemRef,
    }

    // The kit's member tracks, with their folder chains — cheap
    // metadata, gathered sequentially.
    struct Job {
        guid: String,
        name: String,
        folder: Option<String>,
        role: expression_editor_core::kit::LaneRole,
        /// The whole track — its audio is composed from every playing
        /// item, not read from one of them.
        track: daw::service::Track,
        /// The longest playing item, kept as the anchor edits are
        /// written back through. Reading and writing want different
        /// things: reading wants the whole performance, writing wants
        /// a concrete item to address.
        item_guid: String,
    }
    let mut jobs: Vec<Job> = Vec::new();
    let mut items_seen = 0usize;
    for track in &tracks {
        if track.is_folder {
            continue;
        }
        // Folder chain, nearest first, walked over `parent_guid` —
        // depth-capped so a cyclic project cannot hang the load.
        let mut chain: Vec<&str> = Vec::new();
        let mut under_kit = false;
        let mut cur = track.parent_guid.as_deref();
        for _ in 0..64 {
            let Some(parent) = cur.and_then(|g| by_guid.get(g)) else {
                break;
            };
            chain.push(parent.name.as_str());
            if parent.guid == kit.guid {
                under_kit = true;
            }
            cur = parent.parent_guid.as_deref();
        }
        if !under_kit {
            continue;
        }
        let role = expression_editor_core::kit::kit_role(&track.name, &chain);
        // `edit_item` is still the audio-track test — a track with no
        // playing audio item is not a lane — but its pick is no longer
        // what gets read.
        let (seen, pick) = edit_item(daw, &ctx, track);
        items_seen += seen;
        let Some((item_guid, _length_secs, _volume)) = pick else {
            continue;
        };
        jobs.push(Job {
            guid: track.guid.clone(),
            name: track.name.clone(),
            folder: chain.first().map(|s| s.to_string()),
            role,
            track: track.clone(),
            item_guid,
        });
    }

    // The span every lane is composed over: the furthest item end in
    // the kit. Lanes must share one timeline or they cannot be
    // compared, and a per-track length would make each mic its own
    // clock.
    let mut timeline_secs = 0.0f64;
    for job in &jobs {
        for item in Items::get_items(
            daw,
            ctx.clone(),
            daw::service::TrackRef::Guid(job.track.guid.clone()),
        ) {
            if !crate::audio_read::is_playing(&job.track, &item) {
                continue;
            }
            let end = item.position.as_seconds() + item.length.as_seconds();
            if end > timeline_secs {
                timeline_secs = end;
            }
        }
    }
    // One rate for the composed buffers. Probed from a real take
    // rather than assumed: `track_timeline` resamples anything that
    // disagrees, but it needs a target to resample to.
    let rate = jobs
        .first()
        .and_then(|job| {
            let (_, pick) = edit_item(daw, &ctx, &job.track);
            pick
        })
        .and_then(|(guid, len, vol)| read_take_mono(daw, &ctx, &guid, len, vol))
        .map_or(48_000.0, |(_, r)| r);

    // Keep the facade on its owner thread. Bounded workers process only
    // owned audio; no backend value, accessor, or Dioxus signal crosses.
    // Opening a kit is the slowest thing this crate does, and the two
    // halves have very different fixes — decoding is serial because the
    // facade is not `Send`, detection is already spread across workers —
    // so the span carries them apart rather than as one total.
    let decode_ms = std::cell::Cell::new(0.0f64);
    let cached_tracks = std::cell::Cell::new(0usize);
    let started = std::time::Instant::now();
    let mut rows = crate::analysis::capture_and_analyze(
        jobs,
        |job| {
            // The cache answers with what detection produced, which is
            // all the lanes need to be drawn. The samples behind it are
            // wanted the first time anything detects — fills, the
            // quantize panel — and the host reads them then; see
            // `DrumHost::signals_pending`.
            if let Some(project) = cache_in
                && let Some(hit) = crate::analysis_cache::load(project, &job.guid)
            {
                cached_tracks.set(cached_tracks.get() + 1);
                return Some((job, CapturedAudio::Cached(hit)));
            }
            let at = std::time::Instant::now();
            let samples = track_timeline(daw, &ctx, &job.track, timeline_secs, rate);
            decode_ms.set(decode_ms.get() + at.elapsed().as_secs_f64() * 1000.0);
            (!samples.is_empty()).then_some((job, CapturedAudio::Decoded(samples)))
        },
        |(job, captured)| match captured {
            CapturedAudio::Cached(hit) => {
                let doc = crate::analysis::percussion_doc_cached(&hit);
                (job, Vec::new(), rate, doc)
            }
            CapturedAudio::Decoded(samples) => {
                let (doc, cacheable) = crate::analysis::percussion_analysis(&samples, rate);
                // Kept for the next open: detection is cheap next to the
                // decode that fed it, but the peaks are the lanes'
                // backdrop and the two together are what a cached open
                // can show before touching any audio.
                if let Some(project) = cache_in {
                    crate::analysis_cache::store(project, &job.guid, &cacheable);
                }
                (job, samples, rate, doc)
            }
        },
    );
    let total_ms = started.elapsed().as_secs_f64() * 1000.0;
    tracing::info!(
        drums.tracks = rows.len(),
        drums.from_cache = cached_tracks.get(),
        drums.decode_ms = decode_ms.get(),
        drums.detect_ms = total_ms - decode_ms.get(),
        drums.timeline_secs = timeline_secs,
        "analysed the kit"
    );
    for (_, _, _, doc) in &mut rows {
        attach_timeline(daw, &ctx, doc);
    }

    let sample_rate = rows.first().map_or(0.0f64, |(_, _, rate, _)| *rate);

    let mut signals: std::collections::HashMap<
        expression_editor_core::kit::LaneRole,
        Vec<Vec<f64>>,
    > = std::collections::HashMap::new();
    for role in expression_editor_core::kit::LaneRole::ALL {
        let idx: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, (job, _, _, _))| job.role == role)
            .map(|(i, _)| i)
            .collect();
        if idx.is_empty() {
            continue;
        }
        let names: Vec<&str> = idx.iter().map(|&i| rows[i].0.name.as_str()).collect();
        let units = expression_editor_core::kit::detection_units(role, &names);
        let built: Vec<Vec<f64>> = units
            .iter()
            .map(|unit| blend(unit.iter().map(|&(u, w)| (rows[idx[u]].1.as_slice(), w))))
            .filter(|sig: &Vec<f64>| !sig.is_empty())
            .collect();
        if !built.is_empty() {
            signals.insert(role, built);
        }
    }

    let members: Vec<Member> = rows
        .into_iter()
        .map(|(job, _, _, doc)| Member {
            guid: job.guid,
            name: job.name,
            folder: job.folder,
            role: job.role,
            doc,
            item: ItemRef::Guid(job.item_guid),
        })
        .collect();

    if members.is_empty() {
        return Err(WorkspaceError::NoEditableItem {
            project: name,
            items: items_seen,
        });
    }

    let mics = members.len();
    let take_secs = timeline_secs;
    // The kit group's items, by role — the host edits all of them
    // for every gesture. r[impl drums.group.kit]
    let host_lanes: Vec<drum_host::HostLane> = expression_editor_core::kit::LaneRole::ALL
        .into_iter()
        .filter_map(|role| {
            let items: Vec<ItemRef> = members
                .iter()
                .filter(|m| m.role == role)
                .map(|m| m.item.clone())
                .collect();
            if items.is_empty() {
                return None;
            }
            Some(drum_host::HostLane {
                role,
                items,
                signals: signals.remove(&role).unwrap_or_default(),
            })
        })
        .collect();

    let mut it = members.into_iter();
    let first = it.next().expect("checked non-empty");
    let mut editor = Editor::new(first.doc, viewport);
    editor.set_mode(Mode::UnpitchedAudio);
    let mut roles = vec![(first.guid.clone(), first.role)];
    if let Some(t) = editor.tracks.track_mut(0) {
        t.guid = first.guid;
        t.name = first.name;
        t.folder = first.folder;
    }
    for m in it {
        let i = editor.add_track_with_guid(m.guid.clone(), m.name, m.doc);
        if let Some(t) = editor.tracks.track_mut(i) {
            t.set_mode(Mode::UnpitchedAudio);
            t.folder = m.folder;
        }
        roles.push((m.guid, m.role));
    }
    editor.tracks.fold_roles(&roles);
    // The stack *is* the drum workspace view; the roll is one click
    // away per lane.
    editor.stacked = true;
    // Grid targets come from the project's tempo, not a default —
    // a hit quantized against 120 in an 84 bpm session lands
    // nowhere musical.
    // r[impl drums.group.tempo]
    let bpm = daw::service::tempo_map::TempoMap::get_tempo_at(daw, ctx.clone(), 0.0);
    if bpm > 0.0 {
        editor.bpm = bpm;
    }

    let host = drum_host::DrumHost::new(
        daw.clone(),
        ctx,
        host_lanes,
        sample_rate,
        take_secs,
        60.0 / editor.bpm.max(1.0),
    );
    Ok(DrumWorkspace {
        label: format!("{name} — drums: {} ({mics} mics)", kit.name),
        editor,
        host,
    })
}

/// Every track beneath `folder`, as `(name, is_folder)` for scoring.
///
/// Walks `parent_guid` upward from each track rather than downward, so a
/// project whose folder chain is malformed cannot loop; the depth cap is the
/// same one the role walk uses.
fn descendants_of<'a>(
    tracks: &'a [daw::service::Track],
    folder_guid: &str,
) -> Vec<(&'a str, bool)> {
    let by_guid: std::collections::HashMap<&str, &daw::service::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    tracks
        .iter()
        .filter(|t| {
            let mut cur = t.parent_guid.as_deref();
            for _ in 0..64 {
                let Some(parent) = cur.and_then(|g| by_guid.get(g)) else {
                    return false;
                };
                if parent.guid == folder_guid {
                    return true;
                }
                cur = parent.parent_guid.as_deref();
            }
            false
        })
        .map(|t| (t.name.as_str(), t.is_folder))
        .collect()
}

/// The candidate that most looks like a tracked kit. Ties keep the earlier
/// one, so a single-candidate project behaves exactly as before.
fn pick_best_kit<'a>(
    tracks: &'a [daw::service::Track],
    candidates: &[&'a daw::service::Track],
) -> Option<&'a daw::service::Track> {
    candidates
        .iter()
        .copied()
        .max_by_key(|c| expression_editor_core::kit::score_kit(&descendants_of(tracks, &c.guid)))
}
