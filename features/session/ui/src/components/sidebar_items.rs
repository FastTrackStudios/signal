//! Sidebar Item Components
//!
//! Components for displaying songs and sections in the sidebar navigator.
//! Copied from `FastTrackStudio` for exact styling match.

use crate::prelude::*;
use architect_ui::prelude::*;

/// Section item data for sidebar
#[derive(Clone, Debug, PartialEq)]
pub struct SectionItem {
    pub label: String,
    pub progress: f64,
    pub bright_color: String,
    pub muted_color: String,
    /// Optional comment/descriptor shown in italic after the label
    pub comment: Option<String>,
}

/// Song item data for sidebar
#[derive(Clone, Debug, PartialEq)]
pub struct SongItemData {
    pub label: String,
    pub progress: f64,
    pub bright_color: String,
    pub muted_color: String,
    pub sections: Vec<SectionItem>,
    /// The song's rendered key (setlist override or its own key), shown as a
    /// corner badge. `None` hides it.
    pub key_label: Option<String>,
}

/// Song item component for sidebar
///
/// A generic component that displays a song with its sections.
/// All data is passed as props, keeping it domain-agnostic.
#[component]
pub fn SongItem(
    song_data: SongItemData,
    index: usize,
    is_expanded: bool,
    is_playing: bool,
    current_section_index: Option<usize>,
    /// When true, the SONG bar uses a themed selection (subtle muted-color
    /// tint + primary ring) instead of the dark "stage" fill. Lets a
    /// light-themed host (Task setlist navigator) opt out of the white box.
    #[props(default)]
    plain_selection: bool,
    on_song_click: Callback<()>,
    on_section_click: Callback<usize>,
) -> Element {
    // Show progress if the song is expanded OR if it's actively playing
    let show_progress = is_expanded || is_playing;

    rsx! {
        div {
            key: "{index}",
            "data-testid": "sidebar-song-{index}",
            class: "relative",
            // Key badge (setlist key / capo shape key) in the top-right corner.
            if let Some(key) = song_data.key_label.clone() {
                span {
                    class: "pointer-events-none absolute right-1.5 top-1.5 z-10 rounded bg-background/75 px-1.5 py-0.5 text-[10px] font-bold leading-none text-foreground shadow-sm",
                    "{key}"
                }
            }
            div {
                class: "flex",
                // Vertical progress bar (only show if expanded)
                if is_expanded {
                    ProgressBar {
                        orientation: ProgressBarOrientation::Vertical,
                        progress: song_data.progress,
                        height: format!(
                            "calc(3rem + {} * 3rem + {} * 0.25rem);",
                            song_data.sections.len(),
                            song_data.sections.len()
                        ),
                        bright_color: song_data.bright_color.clone(),
                        muted_color: song_data.muted_color.clone(),
                    }
                }
                div {
                    class: "flex-1 space-y-1",
                    // Song progress bar. `data-testid` on a wrapper div, not
                    // `ProgressBar` itself (an architect-ui component with
                    // fixed markup) — the click still reaches its `onclick`
                    // via normal DOM bubbling.
                    div {
                        "data-testid": "sidebar-song-click-{index}",
                        ProgressBar {
                            label: song_data.label.clone(),
                            progress: song_data.progress,
                            bright_color: song_data.bright_color.clone(),
                            muted_color: song_data.muted_color.clone(),
                            is_selected: is_expanded,
                            is_inactive: !show_progress,
                            always_black_bg: !plain_selection,
                            on_click: Some(Callback::new(move |_| {
                                on_song_click.call(());
                            })),
                        }
                    }
                    // Sections (only if expanded)
                    if is_expanded {
                        for (section_idx, section) in song_data.sections.iter().enumerate() {
                            div {
                                key: "{index}-{section_idx}",
                                class: "pl-4",
                                "data-testid": "sidebar-section-{index}-{section_idx}",
                                ProgressBar {
                                    label: section.label.clone(),
                                    progress: section.progress,
                                    bright_color: section.bright_color.clone(),
                                    muted_color: section.muted_color.clone(),
                                    is_selected: current_section_index == Some(section_idx),
                                    is_inactive: !show_progress,
                                    on_click: {
                                        let section_idx = section_idx;
                                        Some(Callback::new(move |_| {
                                            on_section_click.call(section_idx);
                                        }))
                                    },
                                    comment: section.comment.clone(),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
