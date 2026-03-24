//! Place switch MIDI items at the edit cursor based on the selected track.
//!
//! Three actions share a common pattern:
//!
//! 1. Get the selected track
//! 2. Walk up parents to find the relevant scene/section folder and its index
//! 3. Walk up one more level to find the controller track (song/rig/profile folder)
//! 4. Place a one-bar MIDI item at the edit cursor on the controller track
//!
//! # Actions
//!
//! - **Place Section Switch**: selected track is a section (direct child of
//!   a song folder with `fts_signal/scene_count`) → places item on the song folder
//! - **Place Song Switch**: selected track is inside a song folder that's
//!   inside a rig folder → places item on the rig folder
//! - **Place Scene Switch**: same as section switch

use daw::Daw;
use eyre::{Result, WrapErr};
use tracing::info;

use crate::demo_profile::scene_color;

/// Base MIDI note for switching (C1 = 36).
const SWITCH_BASE_NOTE: u8 = 36;

/// Place a section-switch MIDI item at the edit cursor.
///
/// The selected track should be a section track (direct child of a song
/// folder with `fts_signal/scene_count`). The MIDI item is placed on
/// the song folder.
pub async fn place_section_switch(daw: &Daw) -> Result<()> {
    place_switch(daw, SwitchLevel::Section).await
}

/// Place a song-switch MIDI item at the edit cursor.
///
/// The selected track should be inside a song folder that's a child
/// of a rig folder with `fts_signal/scene_count`. The MIDI item is
/// placed on the rig folder.
pub async fn place_song_switch(daw: &Daw) -> Result<()> {
    place_switch(daw, SwitchLevel::Song).await
}

/// Place a scene-switch MIDI item at the edit cursor.
/// Same as section switch.
pub async fn place_scene_switch(daw: &Daw) -> Result<()> {
    place_switch(daw, SwitchLevel::Scene).await
}

enum SwitchLevel {
    /// Selected track is a section — its parent (song/profile folder) gets the MIDI item
    Section,
    /// Walk up to song folder → its parent (rig) gets the MIDI item
    Song,
    /// Same as Section
    Scene,
}

async fn place_switch(daw: &Daw, level: SwitchLevel) -> Result<()> {
    let project = daw.current_project().await.wrap_err("no current project")?;
    let tracks = project.tracks();

    // Get the selected track
    let selected = tracks.selected().await?;
    let selected_track = selected
        .first()
        .ok_or_else(|| eyre::eyre!("No track selected"))?;

    // Get all tracks for parent lookups
    let all_tracks = tracks.all().await?;

    // Build a guid → Track lookup
    let track_by_guid: std::collections::HashMap<&str, &daw::service::Track> = all_tracks
        .iter()
        .map(|t| (t.guid.as_str(), t))
        .collect();

    // Find the selected track info
    let selected_info = track_by_guid
        .get(selected_track.guid())
        .ok_or_else(|| eyre::eyre!("Selected track not found in track list"))?;

    // Get edit cursor position
    let transport = project.transport();
    let state = transport.get_state().await?;
    let cursor_time = state
        .edit_position
        .time
        .map(|t| t.as_seconds())
        .unwrap_or(0.0);

    // Calculate bar duration
    let beats_per_bar = state.time_signature.numerator as f64;
    let beat_duration = 60.0 / state.tempo.bpm;
    let bar_duration = beat_duration * beats_per_bar;

    match level {
        SwitchLevel::Section | SwitchLevel::Scene => {
            // Walk up from selected track to find a parent with scene_count.
            // The selected track (or an ancestor) is the section; its parent
            // with scene_count is the controller (song/profile folder).
            let (section_index, section_name, controller_guid) =
                find_section_and_controller(selected_info, &track_by_guid, &tracks, &all_tracks)
                    .await?;
            let controller_track = tracks
                .by_guid(&controller_guid)
                .await?
                .ok_or_else(|| eyre::eyre!("Controller track not found"))?;

            let note = SWITCH_BASE_NOTE + section_index as u8;
            let label = match level {
                SwitchLevel::Section => "section",
                SwitchLevel::Scene => "scene",
                _ => unreachable!(),
            };

            place_midi_item(
                &controller_track,
                cursor_time,
                bar_duration,
                beats_per_bar,
                note,
                &section_name,
            )
            .await?;

            info!(
                "[place-switch] Placed {label} switch '{}' (note {note}) at {cursor_time:.2}s",
                section_name,
            );
        }
        SwitchLevel::Song => {
            // Walk up from selected track to find the song folder, then its parent (rig)
            let (song_index, song_name, rig_guid) =
                find_song_and_rig(selected_info, &track_by_guid, &tracks, &all_tracks).await?;

            let rig_track = tracks
                .by_guid(&rig_guid)
                .await?
                .ok_or_else(|| eyre::eyre!("Rig track not found"))?;

            let note = SWITCH_BASE_NOTE + song_index as u8;

            // Use the song folder's color
            let song_color = find_track_color(&all_tracks, &song_name);

            place_midi_item_with_color(
                &rig_track,
                cursor_time,
                bar_duration,
                beats_per_bar,
                note,
                &song_name,
                song_color,
            )
            .await?;

            info!(
                "[place-switch] Placed song switch '{}' (note {note}) at {cursor_time:.2}s",
                song_name,
            );
        }
    }

    Ok(())
}

/// Walk up the parent chain from the selected track to find:
/// 1. A track whose parent has `fts_signal/scene_count` — that track is the section
/// 2. The parent with scene_count — that's the controller (song/profile folder)
///
/// Returns (section_index, section_name, controller_guid).
async fn find_section_and_controller(
    selected: &daw::service::Track,
    track_map: &std::collections::HashMap<&str, &daw::service::Track>,
    tracks: &daw::Tracks,
    all_tracks: &[daw::service::Track],
) -> Result<(usize, String, String)> {
    let mut current = selected;

    loop {
        // Check if current track's parent has fts_signal/scene_count
        if let Some(parent_guid) = &current.parent_guid {
            let parent_handle = tracks.by_guid(parent_guid).await?;
            if let Some(ref parent) = parent_handle {
                if let Some(count_str) =
                    parent.get_ext_state("fts_signal", "scene_count").await?
                {
                    if count_str.parse::<u32>().is_ok() {
                        // Current track is the section, parent is the controller
                        let index = find_section_index(
                            &current.guid,
                            parent_guid,
                            all_tracks,
                        )?;
                        return Ok((index, current.name.clone(), parent_guid.clone()));
                    }
                }
            }
        }

        // Walk up to parent
        let parent_guid = current
            .parent_guid
            .as_deref()
            .ok_or_else(|| {
                eyre::eyre!(
                    "Could not find a section in parent chain of '{}'",
                    selected.name
                )
            })?;

        current = track_map
            .get(parent_guid)
            .ok_or_else(|| eyre::eyre!("Parent track {} not found", parent_guid))?;
    }
}

/// Walk up the parent chain from the selected track to find:
/// 1. The song folder (a folder track that is a child of a rig folder with scene_count)
/// 2. The rig folder (the song folder's parent)
async fn find_song_and_rig(
    selected: &daw::service::Track,
    track_map: &std::collections::HashMap<&str, &daw::service::Track>,
    tracks: &daw::Tracks,
    all_tracks: &[daw::service::Track],
) -> Result<(usize, String, String)> {
    // Walk up the parent chain. We're looking for a folder track whose parent
    // has fts_signal/scene_count ext_state — that parent is the rig, and
    // our folder is the song.
    let mut current = selected;

    loop {
        if current.is_folder {
            // Check if this track's parent has fts_signal/scene_count
            if let Some(parent_guid) = &current.parent_guid {
                let parent_handle = tracks.by_guid(parent_guid).await?;
                if let Some(ref parent) = parent_handle {
                    if let Some(count_str) =
                        parent.get_ext_state("fts_signal", "scene_count").await?
                    {
                        if count_str.parse::<u32>().is_ok() {
                            // This folder's parent is the rig. Find our index
                            // among folder siblings (songs).
                            let index = find_folder_child_index(
                                &current.guid,
                                parent_guid,
                                all_tracks,
                            )?;
                            return Ok((index, current.name.clone(), parent_guid.clone()));
                        }
                    }
                }
            }
        }

        // Walk up to parent
        let parent_guid = current
            .parent_guid
            .as_deref()
            .ok_or_else(|| {
                eyre::eyre!(
                    "Could not find a song folder in parent chain of '{}'",
                    selected.name
                )
            })?;

        current = track_map
            .get(parent_guid)
            .ok_or_else(|| eyre::eyre!("Parent track {} not found", parent_guid))?;
    }
}

/// Find the 0-based index of a non-folder track among all non-folder children
/// of a parent. The scene timer mutes/unmutes non-folder children by index,
/// so this matches the timer's child track ordering.
fn find_section_index(
    target_guid: &str,
    parent_guid: &str,
    all_tracks: &[daw::service::Track],
) -> Result<usize> {
    let mut section_index = 0usize;
    for track in all_tracks {
        if track.parent_guid.as_deref() == Some(parent_guid) && !track.is_folder {
            if track.guid == target_guid {
                return Ok(section_index);
            }
            section_index += 1;
        }
    }
    Err(eyre::eyre!("Track not found among sections of parent {parent_guid}"))
}

/// Find the 0-based index of a track among folder-only siblings of a parent.
/// Used for song switching where only folder tracks (songs) count.
fn find_folder_child_index(
    target_guid: &str,
    parent_guid: &str,
    all_tracks: &[daw::service::Track],
) -> Result<usize> {
    let mut folder_index = 0usize;
    for track in all_tracks {
        if track.parent_guid.as_deref() == Some(parent_guid) && track.is_folder {
            if track.guid == target_guid {
                return Ok(folder_index);
            }
            folder_index += 1;
        }
    }
    Err(eyre::eyre!("Track not found among folder siblings of parent {parent_guid}"))
}

/// Find a track's color by name.
fn find_track_color(all_tracks: &[daw::service::Track], name: &str) -> u32 {
    all_tracks
        .iter()
        .find(|t| t.name == name)
        .and_then(|t| t.color)
        .unwrap_or(0x6B7280)
}

/// Place a one-bar MIDI item at the given position on the controller track.
async fn place_midi_item(
    controller: &daw::TrackHandle,
    start_time: f64,
    bar_duration: f64,
    beats_per_bar: f64,
    note: u8,
    name: &str,
) -> Result<()> {
    place_midi_item_with_color(
        controller,
        start_time,
        bar_duration,
        beats_per_bar,
        note,
        name,
        scene_color(name),
    )
    .await
}

async fn place_midi_item_with_color(
    controller: &daw::TrackHandle,
    start_time: f64,
    bar_duration: f64,
    beats_per_bar: f64,
    note: u8,
    name: &str,
    color: u32,
) -> Result<()> {
    let end_time = start_time + bar_duration;

    let midi_notes = vec![daw::service::MidiNoteCreate::new(
        note,
        100,
        0.0,
        beats_per_bar,
    )];

    let item = controller
        .items()
        .create_midi_item_with_notes(start_time, end_time, midi_notes)
        .await
        .wrap_err_with(|| format!("create MIDI item for '{name}'"))?;

    if let Some(item) = item {
        item.set_color(Some(color)).await?;
        item.active_take().set_name(name).await?;
    }

    Ok(())
}
