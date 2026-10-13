//! The kit as one track group.
//!
//! Every recorded drum track — each mic and each trigger under the
//! top-level Drums folder — in a group named `Drums` for record arm and
//! media edit, and nothing else. Armed
//! together, so a kit is never recorded part by part while grouping is
//! on; edited together, so a slip, a cut or a comp on one is on all.
//! Their faders, mutes and solos stay their own.

use daw::service::track::{GroupFamily, GroupFlagChange, GroupRole};
use daw::service::{ProjectContext, TrackRef, Tracks};

/// The kit's recorded tracks.
///
/// The top-level Drums folder's tracks holding items — not under its
/// Process or a Verb folder, and not a Sample track (an instrument played
/// from the kit, not a recording of it).
#[must_use]
pub fn kit_tracks(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
) -> Vec<daw_proto::Track> {
    use daw::service::Items as _;
    let tracks = Tracks::all(daw, project.clone());
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    tracks
        .iter()
        .filter(|t| {
            let names = crate::gain_stage::path_of(t, &by_guid);
            !t.is_folder
                && names
                    .first()
                    .is_some_and(|top| top.trim().eq_ignore_ascii_case("drums"))
                && !names.iter().any(|n| {
                    let n = n.trim();
                    n.eq_ignore_ascii_case("process") || n.eq_ignore_ascii_case("verb")
                })
                && !t.name.trim().to_ascii_lowercase().ends_with("sample")
                && !daw
                    .get_items(project.clone(), TrackRef::Guid(t.guid.clone()))
                    .is_empty()
        })
        .cloned()
        .collect()
}

/// Put the kit in its group; the tracks grouped. Idempotent: a kit
/// already grouped keeps its slot.
///
/// # Errors
///
/// A backend call failed, or every group slot is taken.
pub fn group(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    dry: bool,
) -> Result<Vec<String>, String> {
    let kit = kit_tracks(daw, project);
    if kit.is_empty() {
        return Err("no recorded tracks under a top-level Drums folder".to_owned());
    }
    let named: Vec<String> = kit.iter().map(|t| t.name.clone()).collect();
    if dry {
        return Ok(named);
    }
    // The slot the kit already shares, else a free one.
    let shared = kit
        .iter()
        .map(|t| t.grouping.recarm_lead & t.grouping.media_edit_lead)
        .reduce(|a, b| a & b)
        .unwrap_or(0);
    let slot = if shared == 0 {
        daw.first_free_group_slot(project.clone(), 1, 128)
            .ok_or("every group slot is taken")?
    } else {
        shared.trailing_zeros().saturating_add(1)
    };
    for t in &kit {
        let track = TrackRef::Guid(t.guid.clone());
        Tracks::set_group_membership(daw, project.clone(), track.clone(), slot, true)
            .map_err(|e| e.to_string())?;
        // A member in every family: out of all but record arm and media
        // edit.
        for family in [
            GroupFamily::Volume,
            GroupFamily::Vca,
            GroupFamily::Pan,
            GroupFamily::Width,
            GroupFamily::Mute,
            GroupFamily::Solo,
            GroupFamily::Polarity,
            GroupFamily::AutoMode,
        ] {
            Tracks::set_group_flags(
                daw,
                project.clone(),
                track.clone(),
                GroupFlagChange {
                    slot,
                    family,
                    role: GroupRole::None,
                },
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Tracks::set_group_name(daw, project.clone(), slot, "Drums").map_err(|e| e.to_string())?;
    Ok(named)
}
