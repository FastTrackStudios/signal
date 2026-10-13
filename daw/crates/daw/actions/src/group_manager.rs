//! Track-group manager — realizes the canonical group partition in a project.
//!
//! The DAW exposes a fixed set of track-group slots (REAPER: 128). The
//! **partition** that assigns those slots to fixed instrument-category bands
//! is canonical and lives in [`music_catalog::groups`] (the single source of
//! truth shared across FTS repos). This module owns only the operations that
//! *realize* that partition in a live project.
//!
//! Written against the `daw` domain API (`Tracks` group methods + `Projects`
//! undo) on `daw::reaper::Reaper` — no raw FFI. The REAPER-specific bitmask
//! plumbing lives in the `daw-reaper` backend.

use daw::service::{ProjectContext, Projects as _, TrackRef, Tracks as _};
use music_catalog::groups::{SLOT_BANDS, TOTAL_GROUP_SLOTS, band_for_category, slot_label};
use tracing::{info, warn};

/// Write the instrument-category partition into the project's track-group
/// names. Idempotent — re-running just rewrites the same labels.
///
/// Returns the number of slots successfully named.
pub fn apply_group_naming() -> u32 {
    let daw = daw::reaper::Reaper;
    let mut named = 0u32;
    for slot in 1..=TOTAL_GROUP_SLOTS {
        match daw.set_group_name(ProjectContext::Current, slot as u32, &slot_label(slot)) {
            Ok(()) => named += 1,
            Err(_) => warn!(slot, "[group] failed to set track-group name"),
        }
    }
    info!(
        named,
        bands = SLOT_BANDS.len(),
        "[group] Applied instrument-category naming to track groups"
    );
    named
}

/// Assign the currently-selected tracks to the next free slot in `category`'s
/// band, as a mutual group (all flag families, lead+follow). Returns the
/// number of tracks assigned.
pub fn assign_selected_to_category(category: &str) -> usize {
    let Some(band) = band_for_category(category) else {
        warn!(category, "[group] unknown category");
        return 0;
    };
    let daw = daw::reaper::Reaper;

    let selected: Vec<String> = daw
        .selected(ProjectContext::Current)
        .into_iter()
        .map(|t| t.guid)
        .collect();
    if selected.is_empty() {
        warn!("[group] no tracks selected to assign");
        return 0;
    }

    let Some(slot) = daw.first_free_group_slot(
        ProjectContext::Current,
        band.start as u32,
        band.end() as u32,
    ) else {
        warn!(band = band.label, "[group] no free slot in band");
        return 0;
    };

    let undo = format!(
        "Assign {} track(s) to group {}",
        selected.len(),
        slot_label(slot as u16)
    );
    daw.begin_undo_block(ProjectContext::Current, &undo);
    for guid in &selected {
        let _ = daw.set_group_membership(
            ProjectContext::Current,
            TrackRef::Guid(guid.clone()),
            slot,
            true,
        );
    }
    daw.end_undo_block(ProjectContext::Current, &undo, None);

    info!(
        category = band.label,
        slot,
        label = %slot_label(slot as u16),
        tracks = selected.len(),
        "[group] Assigned selection to group slot"
    );
    selected.len()
}
