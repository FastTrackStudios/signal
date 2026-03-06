//! Song section grid -- domain-aware grid wrapping SceneTileGrid.
//!
//! Fetches a song from the controller and maps its [`Section`] entries
//! to [`TileData`] for the dumb [`SceneTileGrid`] component.

use dioxus::prelude::*;
use signal::song::Song;

use crate::components::{SceneTileGrid, TileData};

/// A domain-aware section grid for a Song.
///
/// Loads the song, maps its sections to colored tiles, and emits
/// `(song_id, section_id)` when a tile is clicked.
#[component]
pub fn SongSectionGrid(
    /// Song collection ID to display sections for.
    song_id: String,
    /// Currently active section ID, if any.
    #[props(default)]
    active_section_id: Option<String>,
    /// Callback when a section tile is selected. Receives `(song_id, section_id)`.
    on_section_select: EventHandler<(String, String)>,
) -> Element {
    let signal = crate::use_signal_service();
    let mut song = use_signal(|| None::<Song>);

    // Fetch song when song_id changes.
    {
        let signal = signal.clone();
        let song_id = song_id.clone();
        use_effect(move || {
            let signal = signal.clone();
            let song_id = song_id.clone();
            spawn(async move {
                song.set(
                    signal
                        .songs()
                        .load(song_id.as_str())
                        .await
                        .ok()
                        .flatten(),
                );
            });
        });
    }

    let current_song = song();

    match current_song {
        None => rsx! {
            div { class: "flex items-center justify-center h-full text-sm text-zinc-500",
                "Loading song..."
            }
        },
        Some(s) => {
            let sid = s.id.to_string();
            let section_ids: Vec<String> =
                s.sections.iter().map(|sec| sec.id.to_string()).collect();

            let tiles: Vec<TileData> = s
                .sections
                .iter()
                .map(|sec| TileData {
                    name: sec.name.clone(),
                    subtitle: None,
                    active: active_section_id
                        .as_ref()
                        .map_or(false, |aid| aid == &sec.id.to_string()),
                    empty: false,
                    preloaded: true,
                    loading: false,
                    disabled: false,
                })
                .collect();

            let slot_count = tiles.len().max(8);

            rsx! {
                SceneTileGrid {
                    tiles,
                    slot_count,
                    on_tile_click: move |idx: usize| {
                        if let Some(section_id) = section_ids.get(idx) {
                            on_section_select.call((sid.clone(), section_id.clone()));
                        }
                    },
                }
            }
        }
    }
}
