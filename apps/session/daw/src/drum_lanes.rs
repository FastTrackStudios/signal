//! The kit's lanes: the comp it was played from and the edit made of it.
//!
//! Every drum track alike, because the kit is one recording: its comping
//! lane — the comp that plays — named `COMP`, and a lane `EDIT` holding a
//! copy of it, where the edits land. Either can be played, the whole kit
//! at once, to weigh the edit against the comp it was made from. Which
//! lane plays is the kit's own and only changes when asked (the panel's
//! lane steps, `DRUMS_PLAY_LANE`): a phase never switches it, so what has
//! been edited plays edited in every mode.

use daw::service::{ItemRef, Items as _, ProjectContext, Takes as _, TrackRef, Tracks};

/// The comp's lane name.
pub const COMP: &str = "COMP";
/// The edit's lane name.
pub const EDIT: &str = "EDIT";

/// The kit's tracks: every track under the top-level Drums folder, but
/// none in its Process (the mix, not the recording).
#[must_use]
pub fn kit_tracks(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
) -> Vec<daw_proto::Track> {
    let tracks = Tracks::all(daw, project.clone());
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    tracks
        .iter()
        .filter(|t| !t.is_folder)
        .filter(|t| {
            let path = crate::gain_stage::path_of(t, &by_guid);
            path.first()
                .is_some_and(|p| p.trim().eq_ignore_ascii_case("drums"))
                && !path
                    .iter()
                    .any(|p| p.trim().eq_ignore_ascii_case("process"))
        })
        .cloned()
        .collect()
}

/// A lane by name on `track`.
#[must_use]
pub fn lane_named(track: &daw_proto::Track, name: &str) -> Option<u32> {
    track
        .lane_names
        .iter()
        .position(|n| n.trim().eq_ignore_ascii_case(name.trim()))
        .and_then(|i| u32::try_from(i).ok())
        .filter(|&i| i < track.lane_count)
}

/// Set the kit up for editing; what it did — or, `dry`, would — track by
/// track.
///
/// Each track's comping lane (the comping lane, else the lowest playing,
/// else the first) named `COMP`, a new lane `EDIT` after the last with a
/// copy of it, and `EDIT` playing. Appended, so the takes keep the lane
/// numbers REAPER's comp areas point at; a track already set up is left.
///
/// # Errors
///
/// The engine refusing one of the writes, named by the track's path.
pub fn set_up(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    dry: bool,
) -> Result<Vec<String>, String> {
    let all = Tracks::all(daw, project.clone());
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        all.iter().map(|t| (t.guid.as_str(), t)).collect();
    let mut said = Vec::new();
    for track in kit_tracks(daw, project) {
        let path = crate::gain_stage::path_of(&track, &by_guid).join("/");
        let at = TrackRef::Guid(track.guid.clone());
        if lane_named(&track, EDIT).is_some() {
            said.push(format!("{path}: already set up"));
            continue;
        }
        let items = daw.get_items(project.clone(), at.clone());
        if items.is_empty() {
            continue;
        }
        let comp = daw
            .comping(project.clone(), at.clone())
            .ok()
            .and_then(|c| c.comp_lane)
            .or_else(|| (track.lane_play_mask != 0).then(|| track.lane_play_mask.trailing_zeros()))
            .unwrap_or(0);
        let (count, edit) = if track.lane_count == 0 {
            (2, 1)
        } else {
            (track.lane_count.saturating_add(1), track.lane_count)
        };
        let on_comp: Vec<&daw_proto::Item> = items
            .iter()
            .filter(|i| i.fixed_lane.unwrap_or(0) == comp)
            .collect();
        said.push(format!(
            "{path}: lane {comp} → {COMP}, lane {edit} → {EDIT} ({} items copied), {EDIT} plays",
            on_comp.len()
        ));
        if dry {
            continue;
        }
        let fail = |e: daw_proto::DawError| format!("{path}: {e}");
        daw.set_lane_count(project.clone(), at.clone(), count)
            .map_err(fail)?;
        daw.set_lane_name(project.clone(), at.clone(), comp, COMP)
            .map_err(fail)?;
        daw.set_lane_name(project.clone(), at.clone(), edit, EDIT)
            .map_err(fail)?;
        for item in on_comp {
            let copy = daw
                .duplicate_item(project.clone(), ItemRef::Guid(item.guid.clone()))
                .ok_or_else(|| format!("{path}: an item would not copy"))?;
            daw.set_fixed_lane(project.clone(), ItemRef::Guid(copy), edit)
                .map_err(fail)?;
        }
        daw.set_lane_play_mask(project.clone(), at, 1_u64.checked_shl(edit).unwrap_or(0))
            .map_err(fail)?;
    }
    if !dry && !said.is_empty() {
        crate::studio::request_resync();
        crate::actions::list().changed("drums.lanes", "the kit's lanes were set up");
    }
    Ok(said)
}

/// Start the edit over, from the comp.
///
/// On every drum track with an `EDIT` lane, its slices thrown away and
/// the `COMP` lane copied onto it again. One undo step; the old edit is
/// in the song's history (and any checkpoint made before it).
///
/// # Errors
///
/// The engine refusing one of the writes, named by the track's path.
pub fn reset_edit(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    dry: bool,
) -> Result<Vec<String>, String> {
    let all = Tracks::all(daw, project.clone());
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        all.iter().map(|t| (t.guid.as_str(), t)).collect();
    let mut said = Vec::new();
    if !dry {
        daw::service::Projects::begin_undo_block(daw, project.clone(), "Start the drum edit over");
    }
    for track in kit_tracks(daw, project) {
        let path = crate::gain_stage::path_of(&track, &by_guid).join("/");
        let (Some(comp), Some(edit)) = (lane_named(&track, COMP), lane_named(&track, EDIT)) else {
            continue;
        };
        let at = TrackRef::Guid(track.guid.clone());
        let items = daw.get_items(project.clone(), at);
        let on = |lane: u32| {
            items
                .iter()
                .filter(move |i| i.fixed_lane.unwrap_or(0) == lane)
                .collect::<Vec<_>>()
        };
        let (old, fresh) = (on(edit), on(comp));
        said.push(format!(
            "{path}: {} slices on {EDIT} dropped, {} items of {COMP} copied onto it",
            old.len(),
            fresh.len()
        ));
        if dry {
            continue;
        }
        let fail = |e: daw_proto::DawError| format!("{path}: {e}");
        for item in old {
            daw.delete_item(project.clone(), ItemRef::Guid(item.guid.clone()))
                .map_err(fail)?;
        }
        // A comp cut where nothing changes — the same recording running
        // on, split by a stray edit — copies as one: a seam the edit did
        // not make is one it cannot close.
        let mut fresh = fresh;
        fresh.sort_by(|a, b| a.position.as_seconds().total_cmp(&b.position.as_seconds()));
        let spans: Vec<(f64, f64, f64)> = fresh
            .iter()
            .map(|i| {
                (
                    i.position.as_seconds(),
                    i.length.as_seconds(),
                    i.start_offset.as_seconds(),
                )
            })
            .collect();
        for (first, length) in glued(&spans) {
            let Some(item) = fresh.get(first) else {
                continue;
            };
            let copy = daw
                .duplicate_item(project.clone(), ItemRef::Guid(item.guid.clone()))
                .ok_or_else(|| format!("{path}: an item would not copy"))?;
            if (length - item.length.as_seconds()).abs() > 1e-9 {
                daw.set_length(
                    project.clone(),
                    ItemRef::Guid(copy.clone()),
                    daw_proto::primitives::Duration::from_seconds(length),
                )
                .map_err(fail)?;
                // The run's volume curves as one: each item's on from where
                // it starts in the run (a leveled kick stays leveled).
                let start = item.position.as_seconds();
                let run: Vec<&daw_proto::Item> = fresh
                    .iter()
                    .skip(first)
                    .take_while(|i| i.position.as_seconds() < start + length - 1e-9)
                    .copied()
                    .collect();
                join_curves(daw, project, &run, &copy);
            }
            daw.set_fixed_lane(project.clone(), ItemRef::Guid(copy), edit)
                .map_err(fail)?;
        }
    }
    if !dry {
        // Nothing is placed by hand any more: every hit plays as played.
        crate::hits::forget_placed(daw, project);
        daw::service::Projects::end_undo_block(
            daw,
            project.clone(),
            "Start the drum edit over",
            None,
        );
        if !said.is_empty() {
            crate::studio::request_resync();
            crate::actions::list().changed("drums.lanes", "the kit's edit was started over");
        }
    }
    Ok(said)
}

/// The take volume curves of `run` (comp items meeting end to end, by
/// position) written as one onto `onto`, each from where its item starts
/// in the run — what joining them into one copy keeps.
fn join_curves(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    run: &[&daw_proto::Item],
    onto: &str,
) {
    use daw_proto::automation::{AddPointParams, Automation as _};
    let volume = |item: &str| {
        let take = daw.get_active_take(project.clone(), ItemRef::Guid(item.to_owned()))?;
        Some(daw_proto::EnvelopeLocation {
            track: TrackRef::Index(0),
            envelope: daw_proto::EnvelopeRef::Take {
                item_guid: item.to_owned(),
                take_guid: take.guid,
                kind: daw_proto::TakeEnvelopeKind::Volume,
            },
        })
    };
    let Some(start) = run.first().map(|i| i.position.as_seconds()) else {
        return;
    };
    let points: Vec<(f64, f64)> = run
        .iter()
        .filter_map(|i| {
            let at = i.position.as_seconds() - start;
            Some(
                daw.points(project.clone(), volume(&i.guid)?)
                    .into_iter()
                    .map(move |p| (p.time.as_seconds() + at, p.value)),
            )
        })
        .flatten()
        .collect();
    let Some(target) = volume(onto) else {
        return;
    };
    if points.is_empty() {
        return;
    }
    let had = daw.points(project.clone(), target.clone()).len();
    for i in (0..had).rev() {
        daw.delete_point(
            project.clone(),
            target.clone(),
            u32::try_from(i).unwrap_or(u32::MAX),
        );
    }
    for (at, value) in points {
        daw.add_point(
            project.clone(),
            target.clone(),
            AddPointParams::new(
                daw_proto::PositionInSeconds::from_seconds(at),
                value,
                daw_proto::automation::EnvelopeShape::Linear,
            ),
        );
    }
}

/// How near two comp items must meet, on the timeline and in their
/// source, to be one recording running on, seconds.
const RUNS_ON: f64 = 0.001;

/// Comp items (`(position, length, source offset)`, by position) as the
/// runs to copy: each run's first item and the length it covers.
///
/// Items that meet where the one before ends, on the timeline and in the
/// source, are one.
#[must_use]
pub fn glued(items: &[(f64, f64, f64)]) -> Vec<(usize, f64)> {
    let mut runs: Vec<(usize, f64)> = Vec::new();
    let mut last: Option<(f64, f64)> = None;
    for (k, &(at, length, from)) in items.iter().enumerate() {
        let joins = last.is_some_and(|(end, source_end)| {
            (at - end).abs() <= RUNS_ON && (from - source_end).abs() <= RUNS_ON
        });
        match runs.last_mut() {
            Some((first, run)) if joins => {
                let start = items.get(*first).map_or(at, |i| i.0);
                *run = at + length - start;
            }
            _ => runs.push((k, length)),
        }
        last = Some((at + length, from + length));
    }
    runs
}

/// Play the lane named `name` on every drum track that has one; how many
/// do.
///
/// # Errors
///
/// No drum track having a lane of that name, or the engine refusing.
pub fn play(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    name: &str,
) -> Result<u32, String> {
    let mut played = 0_u32;
    for track in kit_tracks(daw, project) {
        let Some(lane) = lane_named(&track, name) else {
            continue;
        };
        let mask = 1_u64.checked_shl(lane).unwrap_or(0);
        if track.lane_play_mask != mask {
            daw.set_lane_play_mask(project.clone(), TrackRef::Guid(track.guid.clone()), mask)
                .map_err(|e| e.to_string())?;
        }
        played = played.saturating_add(1);
    }
    if played == 0 {
        return Err(format!(
            "no drum track has a lane named {name} — DRUMS_EDIT_LANES makes {COMP} and {EDIT}"
        ));
    }
    crate::studio::request_resync();
    crate::actions::list().changed("drums.lanes", "the kit plays another lane");
    Ok(played)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lane_is_found_by_name_and_only_while_it_exists() {
        let track = daw_proto::Track {
            lane_count: 3,
            lane_names: vec!["comp".into(), "1".into(), "EDIT".into(), "ghost".into()],
            ..daw_proto::Track::default()
        };
        assert_eq!(lane_named(&track, "COMP"), Some(0));
        assert_eq!(lane_named(&track, " edit "), Some(2));
        assert_eq!(lane_named(&track, "ghost"), None, "past the lane count");
    }
}

#[cfg(test)]
mod glue_tests {
    use super::glued;

    /// A take split at 2 s and 19.5 s where nothing changed copies as one
    /// item; a real comp seam — another take's audio — stays two.
    #[test]
    fn a_stray_split_copies_as_one() {
        let take = [(0.0, 2.0, 1.5), (2.0, 17.5, 3.5), (19.5, 250.0, 21.0)];
        assert_eq!(glued(&take), [(0, 269.5)]);
        let comped = [(0.0, 2.0, 1.5), (2.0, 17.5, 40.0)];
        assert_eq!(glued(&comped), [(0, 2.0), (1, 17.5)]);
    }
}
