//! Demo setlist builder.
//!
//! Creates a full guitar rig setlist in REAPER:
//!
//! ```text
//! Guitar Rig/                              (rig folder — Signal Controller for song switching)
//!   Guitar Input                           (rig input — sends to all song inputs)
//!   Belief/                                (song folder — Signal Controller for section switching)
//!     Belief Input                         (song input — sends to section inputs)
//!     Scene 1: Clean/
//!       Belief Input: Clean
//!       [L] Clean
//!     Scene 2: Ambient/
//!       Belief Input: Ambient
//!       [L] Ambient
//!     Scene 3: Rhythm/
//!       Belief Input: Rhythm
//!       [L] Rhythm
//!   Vienna/
//!     Vienna Input
//!     Scene 1: Clean/  ...
//!   ...8 songs total
//! ```
//!
//! The rig-level Signal Controller receives MIDI notes to switch between songs
//! (muting/unmuting sends from Guitar Input to each song's input).
//! Each song's Signal Controller receives MIDI notes to switch between its
//! own sections (muting/unmuting sends from song input to section inputs).
//!
//! MIDI items are placed sequentially: one bar per section within each song,
//! songs laid out back-to-back. Song-level items go on the Guitar Rig folder,
//! section-level items go on each song's folder.

use daw::{Daw, TrackHandle};
use eyre::{Result, WrapErr};
use tracing::info;

use crate::demo_profile::{add_layer_fx, scene_color};

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
        .add("CLAP: FTS Signal Controller (FastTrack Studio)")
        .await
        .wrap_err("add Signal Controller to Guitar Rig")?;

    // Store song count so the controller knows how many songs exist
    rig_folder
        .set_ext_state("fts_signal", "scene_count", &SETLIST.len().to_string())
        .await?;

    // ── Guitar Input ──────────────────────────────────────────────────
    // Rig-level input. Parent send disabled — routes via sends to each
    // song's input track. Song switching mutes/unmutes these sends.
    let rig_input = tracks.add("Guitar Input", None).await?;
    rig_input.set_color(0x6B7280).await?;
    rig_input.set_parent_send(false).await?;

    // Collect song input tracks so we can create sends after all songs
    let mut song_inputs: Vec<TrackHandle> = Vec::new();
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
        markers
            .add(song_start_time, song.title)
            .await
            .wrap_err_with(|| format!("add marker for '{}'", song.title))?;

        // ── Song folder ───────────────────────────────────────────────
        let song_folder = tracks.add(song.title, None).await?;
        song_folder.set_folder_depth(1).await?;
        song_folder.set_color(song.color).await?;

        // Signal Controller on song folder — receives MIDI to switch sections
        song_folder
            .fx_chain()
            .add("CLAP: FTS Signal Controller (FastTrack Studio)")
            .await
            .wrap_err_with(|| format!("add controller to '{}'", song.title))?;

        // Store section count for section switching
        song_folder
            .set_ext_state("fts_signal", "scene_count", &section_count.to_string())
            .await?;

        // ── Song input track ──────────────────────────────────────────
        let song_input = tracks
            .add(&format!("{} Input", song.title), None)
            .await?;
        song_input.set_color(0x6B7280).await?;
        song_input.set_parent_send(false).await?;

        // ── Build sections ────────────────────────────────────────────
        let mut section_inputs: Vec<TrackHandle> = Vec::new();

        for (sec_idx, &(sec_name, module_types)) in song.sections.iter().enumerate() {
            let is_last_section = sec_idx == section_count - 1;

            // Section folder
            let sec_folder = tracks
                .add(&format!("Scene {}: {sec_name}", sec_idx + 1), None)
                .await?;
            sec_folder.set_folder_depth(1).await?;
            sec_folder.set_color(scene_color(sec_name)).await?;

            // Section input
            let sec_input = tracks
                .add(&format!("{} Input: {sec_name}", song.title), None)
                .await?;
            sec_input.set_color(0x6B7280).await?;
            sec_input.set_parent_send(false).await?;

            // Layer track
            let layer = tracks
                .add(&format!("[L] {sec_name}"), None)
                .await?;
            layer.set_color(scene_color(sec_name)).await?;

            // Close folders:
            //   last section of last song: close section + song + rig  (-3)
            //   last section of other song: close section + song       (-2)
            //   other sections:             close section only         (-1)
            let depth = if is_last_section && is_last_song {
                -3 // close section + song + rig
            } else if is_last_section {
                -2 // close section + song
            } else {
                -1 // close section only
            };
            layer.set_folder_depth(depth).await?;

            // Send: section input → layer
            sec_input.sends().add_to(layer.guid()).await?;

            // Add FX chain to layer
            add_layer_fx(&layer, module_types).await?;

            section_inputs.push(sec_input);
        }

        // Send: song input → each section input
        for sec_input in &section_inputs {
            song_input.sends().add_to(sec_input.guid()).await?;
        }

        // ── Section MIDI items (on the song folder) ───────────────────
        // Each section gets a one-bar MIDI item with a note that the
        // song's Signal Controller uses to switch sections.
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

            let item = song_items
                .create_midi_item_with_notes(start, end, midi_notes)
                .await
                .wrap_err_with(|| {
                    format!("create section MIDI item for '{sec_name}' in '{}'", song.title)
                })?;

            if let Some(item) = item {
                item.set_color(Some(scene_color(sec_name))).await?;
                item.active_take().set_name(sec_name).await?;
            }

            info!(
                "[demo-setlist]   Section bar {}: '{}' (note {})",
                item_bar + 1,
                sec_name,
                note,
            );
        }

        song_inputs.push(song_input);
        current_bar += section_count;
    }

    // ── Rig input sends to all song inputs ────────────────────────────
    for song_input in &song_inputs {
        rig_input.sends().add_to(song_input.guid()).await?;
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

        let item = rig_items
            .create_midi_item_with_notes(start, end, midi_notes)
            .await
            .wrap_err_with(|| format!("create song MIDI item for '{}'", song.title))?;

        if let Some(item) = item {
            item.set_color(Some(song.color)).await?;
            item.active_take().set_name(song.title).await?;
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

    let total_sections: usize = SETLIST.iter().map(|s| s.sections.len()).sum();
    info!(
        "[demo-setlist] Created rig with {} songs, {} total sections, {} bars",
        SETLIST.len(),
        total_sections,
        current_bar,
    );

    Ok(())
}
