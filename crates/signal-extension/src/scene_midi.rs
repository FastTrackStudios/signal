//! Generate MIDI items for scene switching.
//!
//! Creates colored, labeled MIDI items on the profile folder track.
//! Each item contains a specific MIDI note that the FTS Signal Controller
//! recognizes to switch between scene variations (muting/unmuting sends).
//!
//! MIDI note mapping:
//!   C1 (note 36) = Scene 1
//!   C#1 (note 37) = Scene 2
//!   D1 (note 38) = Scene 3
//!   ...etc
//!
//! Items are placed sequentially, one bar each, starting at bar 1.

use daw::Daw;
use eyre::{Result, WrapErr};
use tracing::info;

/// Base MIDI note for scene switching. Scene N uses note (BASE + N - 1).
pub const SCENE_SWITCH_BASE_NOTE: u8 = 36; // C1

/// Generate MIDI items for all scenes found in the profile structure.
///
/// Scans tracks for the profile folder (has `fts_signal/scene_count` ext_state),
/// then creates one MIDI item per scene on the profile folder track.
pub async fn generate_scene_midi_items(daw: &Daw) -> Result<()> {
    let project = daw.current_project().await.wrap_err("no current project")?;
    let tracks = project.tracks();
    let all_tracks = tracks.all().await?;

    // Find the profile folder track (has fts_signal/scene_count)
    let mut profile_track = None;
    let mut scene_count = 0u32;
    let mut profile_name = String::new();

    for track_info in &all_tracks {
        let track = match tracks.by_guid(&track_info.guid).await? {
            Some(t) => t,
            None => continue,
        };

        if let Some(count_str) = track
            .get_ext_state("fts_signal", "scene_count")
            .await?
        {
            if let Ok(count) = count_str.parse::<u32>() {
                scene_count = count;
                profile_name = track_info.name.clone();
                profile_track = Some(track);
                break;
            }
        }
    }

    let profile_track = profile_track
        .ok_or_else(|| eyre::eyre!("No profile folder found (missing fts_signal/scene_count ext_state)"))?;

    info!(
        "[scene-midi] Found profile '{}' with {scene_count} scenes",
        profile_name
    );

    // Collect scene names and colors from child folder tracks
    let profile_guid = profile_track.guid().to_string();
    let mut scenes: Vec<(String, u32)> = Vec::new();

    for track_info in &all_tracks {
        if track_info.parent_guid.as_deref() != Some(&profile_guid) {
            continue;
        }
        // Scene folder tracks are named "Scene N: Name"
        if track_info.name.starts_with("Scene ") {
            let color = track_info.color.unwrap_or(0x808080);
            // Extract scene name: "Scene 1: Clean" → "Clean"
            let scene_name = track_info
                .name
                .split(": ")
                .nth(1)
                .unwrap_or(&track_info.name)
                .to_string();
            scenes.push((scene_name, color));
        }
    }

    if scenes.is_empty() {
        // Fall back to scene_count with generic names
        for i in 0..scene_count {
            scenes.push((format!("Scene {}", i + 1), 0x808080));
        }
    }

    info!("[scene-midi] Generating {} MIDI items", scenes.len());

    // Get tempo to calculate bar duration
    let transport = project.transport();
    let state = transport.get_state().await?;
    let beats_per_bar = state.time_signature.numerator as f64;
    let beat_duration = 60.0 / state.tempo.bpm; // seconds per beat
    let bar_duration = beat_duration * beats_per_bar;

    let items = profile_track.items();

    for (i, (scene_name, color)) in scenes.iter().enumerate() {
        let start = i as f64 * bar_duration;
        let end = start + bar_duration;
        let note = SCENE_SWITCH_BASE_NOTE + i as u8;

        // Create MIDI item with a single note spanning the full bar.
        // The note pitch identifies the scene. Velocity 100.
        let midi_notes = vec![daw::service::MidiNoteCreate::new(
            note,
            100,     // velocity
            0.0,     // start_ppq (relative to item start)
            beats_per_bar, // length in quarter notes = full bar
        )];

        let item = items
            .create_midi_item_with_notes(start, end, midi_notes)
            .await
            .wrap_err_with(|| format!("create MIDI item for scene '{scene_name}'"))?;

        if let Some(item) = item {
            // Color the item to match the scene
            item.set_color(Some(*color)).await?;

            // Name the take so the scene name is visible in the arrange view
            let take_list = item.takes();
            let takes = take_list.all().await?;
            if let Some(first_take) = takes.first() {
                let take_handle = take_list.by_index(first_take.index).await?;
                if let Some(take) = take_handle {
                    take.set_name(&scene_name).await?;
                }
            }

            info!(
                "[scene-midi] Created MIDI item: bar {} = '{}' (note {note}, color #{color:06X})",
                i + 1,
                scene_name
            );
        }
    }

    info!("[scene-midi] Generated {} scene MIDI items", scenes.len());
    Ok(())
}
