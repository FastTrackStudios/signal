//! Demo setlist builder.
//!
//! Creates a full guitar rig setlist in REAPER:
//!
//! ```text
//! Guitar Rig/                              (rig folder — Signal Controller for song switching)
//!   Guitar Input                           (rig input — audio source)
//!   Belief/                                (song folder — Signal Controller + MIDI items)
//!     Clean                                (section track — receives from parent)
//!     Ambient
//!     Rhythm
//!   Vienna/
//!     Clean
//!     Crunch  ...
//!   ...8 songs total
//! ```
//!
//! Each section is a single track (no sub-folder, no per-song input track).
//! The scene timer mutes/unmutes child tracks directly based on MIDI items
//! on the controller folder.
//!
//! MIDI items are placed sequentially: one bar per section within each song,
//! songs laid out back-to-back. Song-level items go on the Guitar Rig folder,
//! section-level items go on each song's folder.

use daw::Daw;
use eyre::{Result, WrapErr};
use tracing::info;

use crate::demo_profile::scene_color;

/// A song in the demo setlist.
struct SongDef {
    title: &'static str,
    sections: &'static [(&'static str, &'static [&'static str])],
    color: u32,
}

const SETLIST: &[SongDef] = &[
    SongDef {
        title: "Belief",
        color: 0x22C55E, // green
        sections: &[
            ("Clean", &["input", "amp", "master"]),
            ("Ambient", &["input", "amp", "modulation", "time", "master"]),
            ("Rhythm", &["input", "drive", "amp", "dynamics", "master"]),
        ],
    },
    SongDef {
        title: "Vienna",
        color: 0x3B82F6, // blue
        sections: &[
            ("Clean", &["input", "amp", "master"]),
            ("Crunch", &["input", "drive", "amp", "master"]),
            ("Drive", &["input", "drive", "amp", "dynamics", "master"]),
            (
                "Lead",
                &["input", "drive", "amp", "modulation", "time", "master"],
            ),
        ],
    },
    SongDef {
        title: "Anomalie",
        color: 0x8B5CF6, // violet
        sections: &[("Default", &["input", "amp", "dynamics", "master"])],
    },
    SongDef {
        title: "New Whip",
        color: 0xEAB308, // yellow
        sections: &[
            ("Clean", &["input", "amp", "master"]),
            ("Rhythm", &["input", "drive", "amp", "dynamics", "master"]),
            (
                "Lead",
                &["input", "drive", "amp", "modulation", "time", "master"],
            ),
        ],
    },
    SongDef {
        title: "Leave No Stone",
        color: 0xEF4444, // red
        sections: &[
            (
                "Lead",
                &["input", "drive", "amp", "modulation", "time", "master"],
            ),
            ("Edge", &["input", "drive", "amp", "dynamics", "master"]),
            ("Djent", &["input", "drive", "amp", "dynamics", "master"]),
            (
                "Harmony Lead",
                &["input", "drive", "amp", "modulation", "time", "master"],
            ),
        ],
    },
    SongDef {
        title: "Won't Stand Down",
        color: 0xF97316, // orange
        sections: &[
            ("Chug", &["input", "drive", "amp", "dynamics", "master"]),
            (
                "Filtered",
                &["input", "drive", "amp", "modulation", "master"],
            ),
            ("Rhythm", &["input", "drive", "amp", "dynamics", "master"]),
            (
                "Lead",
                &["input", "drive", "amp", "modulation", "time", "master"],
            ),
        ],
    },
    SongDef {
        title: "What About Me",
        color: 0xEC4899, // pink
        sections: &[
            (
                "Lead",
                &["input", "drive", "amp", "modulation", "time", "master"],
            ),
            ("Dry Lead", &["input", "drive", "amp", "master"]),
        ],
    },
    SongDef {
        title: "Harder to Breathe",
        color: 0x06B6D4, // cyan
        sections: &[
            ("Crunch", &["input", "drive", "amp", "master"]),
            ("Dry Drive", &["input", "drive", "amp", "master"]),
            ("Drive", &["input", "drive", "amp", "dynamics", "master"]),
            (
                "Lead",
                &["input", "drive", "amp", "modulation", "time", "master"],
            ),
        ],
    },
];

/// Base MIDI note for switching. C1 (36) = Song/Section 1, C#1 (37) = 2, etc.
const SWITCH_BASE_NOTE: u8 = 36;

/// Create the demo setlist in the current REAPER project.
pub async fn load_demo_setlist(daw: &Daw) -> Result<()> {
    let project = daw.current_project().await.wrap_err("no current project")?;
    let tracks = project.tracks();
    let markers = project.markers();

    info!(
        "[demo-setlist] Creating demo setlist with {} songs",
        SETLIST.len()
    );

    // Get tempo for bar duration calculation
    let transport = project.transport();
    let state = transport.get_state().await?;
    let beats_per_bar = state.time_signature.numerator as f64;
    let beat_duration = 60.0 / state.tempo.bpm;
    let bar_duration = beat_duration * beats_per_bar;

    // ── Guitar Rig folder ─────────────────────────────────────────────
    // Top-level rig folder with Signal Controller for song switching.
    let rig_folder = tracks.add("Guitar Rig", None).await?;
    rig_folder.set_folder_depth(1).await?;
    rig_folder.set_color(0x9CA3AF).await?; // gray

    // Signal Controller on rig folder — receives MIDI to switch songs
    rig_folder
        .fx_chain()
        .add("CLAP: FTS Signal Controller (FastTrackStudio)")
        .await
        .wrap_err("add Signal Controller to Guitar Rig")?;

    // Store song count so the controller knows how many songs exist
    rig_folder
        .set_ext_state("fts_signal", "scene_count", &SETLIST.len().to_string())
        .await?;

    // ── Guitar Input ──────────────────────────────────────────────────
    // Rig-level audio source. Sends are created to each section track.
    // Scene switching mutes/unmutes the sends, not the tracks.
    let rig_input = tracks.add("Guitar Input", None).await?;
    rig_input.set_color(0x6B7280).await?;

    // Collect all section tracks so we can create sends after building the hierarchy
    let mut all_section_tracks: Vec<daw::TrackHandle> = Vec::new();

    let mut current_bar: usize = 0;

    for (song_idx, song) in SETLIST.iter().enumerate() {
        let section_count = song.sections.len();
        let song_start_bar = current_bar;
        let song_start_time = song_start_bar as f64 * bar_duration;
        let is_last_song = song_idx == SETLIST.len() - 1;

        info!(
            "[demo-setlist] Song {}/{}: '{}' ({} sections, bar {})",
            song_idx + 1,
            SETLIST.len(),
            song.title,
            section_count,
            song_start_bar + 1,
        );

        // ── Song marker ───────────────────────────────────────────────
        if let Err(e) = markers.add(song_start_time, song.title).await {
            tracing::warn!("[demo-setlist] Failed to add marker for '{}': {e:#}", song.title);
        }

        // ── Song folder ───────────────────────────────────────────────
        let song_folder = tracks.add(song.title, None).await?;
        song_folder.set_folder_depth(1).await?;
        song_folder.set_color(song.color).await?;

        // Signal Controller on song folder — receives MIDI to switch sections
        if let Err(e) = song_folder
            .fx_chain()
            .add("CLAP: FTS Signal Controller (FastTrackStudio)")
            .await
        {
            tracing::warn!("[demo-setlist] Failed to add controller to '{}': {e:#}", song.title);
        }

        // Store section count for section switching
        song_folder
            .set_ext_state("fts_signal", "scene_count", &section_count.to_string())
            .await?;

        // ── Build sections ────────────────────────────────────────────
        // Each section is a single track, direct child of song folder.
        // Guitar Input sends are created to each section for routing.
        // Scene switching mutes/unmutes sends, not tracks.

        for (sec_idx, &(sec_name, _module_types)) in song.sections.iter().enumerate() {
            let is_last_section = sec_idx == section_count - 1;

            let section = tracks.add(sec_name, None).await?;
            section.set_color(scene_color(sec_name)).await?;
            all_section_tracks.push(section.clone());

            // Close folders:
            //   last section of last song: close song + rig  (-2)
            //   last section of other song: close song       (-1)
            //   other sections:             normal child      (0)
            let depth = if is_last_section && is_last_song {
                -2 // close song + rig
            } else if is_last_section {
                -1 // close song
            } else {
                0 // normal child
            };
            section.set_folder_depth(depth).await?;
        }

        // ── Section MIDI items (on the song folder) ───────────────────
        let song_items = song_folder.items();
        for (sec_idx, &(sec_name, _)) in song.sections.iter().enumerate() {
            let item_bar = song_start_bar + sec_idx;
            let start = item_bar as f64 * bar_duration;
            let end = start + bar_duration;
            let note = SWITCH_BASE_NOTE + sec_idx as u8;

            let midi_notes = vec![daw::service::MidiNoteCreate::new(
                note,
                100,
                0.0,
                beats_per_bar,
            )];

            match song_items
                .create_midi_item_with_notes(start, end, midi_notes)
                .await
            {
                Ok(Some(item)) => {
                    if let Err(e) = item.set_color(Some(scene_color(sec_name))).await {
                        tracing::warn!("[demo-setlist] Failed to set color for '{sec_name}': {e:#}");
                    }
                    if let Err(e) = item.active_take().set_name(sec_name).await {
                        tracing::warn!("[demo-setlist] Failed to set name for '{sec_name}': {e:#}");
                    }
                }
                Ok(None) => {
                    tracing::warn!("[demo-setlist] No MIDI item returned for '{sec_name}'");
                }
                Err(e) => {
                    tracing::warn!("[demo-setlist] Failed MIDI item for '{sec_name}': {e:#}");
                }
            }

            info!(
                "[demo-setlist]   Section bar {}: '{}' (note {})",
                item_bar + 1,
                sec_name,
                note,
            );
        }

        current_bar += section_count;
    }

    // ── Song-switching MIDI items (on the rig folder) ─────────────────
    // One item per song, spanning the song's bars, with a note that the
    // rig's Signal Controller uses to switch songs.
    let rig_items = rig_folder.items();
    let mut bar_offset: usize = 0;
    for (song_idx, song) in SETLIST.iter().enumerate() {
        let song_bars = song.sections.len();
        let start = bar_offset as f64 * bar_duration;
        let end = (bar_offset + song_bars) as f64 * bar_duration;
        let note = SWITCH_BASE_NOTE + song_idx as u8;

        let midi_notes = vec![daw::service::MidiNoteCreate::new(
            note,
            100,
            0.0,
            song_bars as f64 * beats_per_bar,
        )];

        match rig_items
            .create_midi_item_with_notes(start, end, midi_notes)
            .await
        {
            Ok(Some(item)) => {
                let _ = item.set_color(Some(song.color)).await;
                let _ = item.active_take().set_name(song.title).await;
            }
            Ok(None) => {
                tracing::warn!("[demo-setlist] No MIDI item returned for song '{}'", song.title);
            }
            Err(e) => {
                tracing::warn!("[demo-setlist] Failed song MIDI item for '{}': {e:#}", song.title);
            }
        }

        info!(
            "[demo-setlist] Song item bars {}-{}: '{}' (note {})",
            bar_offset + 1,
            bar_offset + song_bars,
            song.title,
            note,
        );

        bar_offset += song_bars;
    }

    // ── Create sends from Guitar Input to each section track ─────────
    // Scene switching mutes/unmutes these sends (not the tracks).
    let input_sends = rig_input.sends();
    for section_track in &all_section_tracks {
        match input_sends.add_to(section_track.guid()).await {
            Ok(send) => {
                // Mute all sends by default — scene timer unmutes the active one
                let _ = send.mute().await;
            }
            Err(e) => {
                tracing::warn!("[demo-setlist] Failed to create send: {e:#}");
            }
        }
    }

    // Store the Guitar Input track's GUID so the scene timer can find it
    rig_folder
        .set_ext_state("fts_signal", "input_track_guid", &rig_input.guid().to_string())
        .await?;

    let total_sections: usize = SETLIST.iter().map(|s| s.sections.len()).sum();
    info!(
        "[demo-setlist] Created rig with {} songs, {} total sections, {} bars, {} sends from Guitar Input",
        SETLIST.len(),
        total_sections,
        current_bar,
        all_section_tracks.len(),
    );

    Ok(())
}
