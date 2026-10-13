//! The setlist navigator: the wide layout's left column
//! ([`crate::shell::NavigatorColumn`]) and the small-screen layout's
//! Control view ([`crate::compact`]).
//!
//! Flat, as the bars are, edge to edge: each song a bar filled as far as
//! the set has got through it, and the one playing opened into its
//! sections — each a bar of its own, set in a little, filled as it plays —
//! with the song's progress running down the window's very edge beside
//! them. A vertical progress bar, and the place
//! another song is picked: a press on a song picks it, a press on a section
//! of the song playing goes there.

use dioxus::prelude::*;
use session_ui::components::sidebar_items::{SectionItem, SongItemData};

use crate::engine::{Move, transport};
use crate::setlist::{Setlist, Song};
use crate::shell::{DIM, RULE};

/// The navigator, over the setlist in context. `on_pick` picks a song, as
/// the wide layout's tabs do.
#[component]
pub fn Navigator(on_pick: EventHandler<usize>) -> Element {
    let setlist: Signal<Setlist> = use_context();
    let reading = crate::progress::use_reading();
    let r = reading();
    let list = setlist.read();
    let rows: Vec<(usize, SongItemData, Option<usize>, Vec<f64>)> = list
        .songs
        .iter()
        .enumerate()
        .map(|(index, song)| {
            let playing = index == list.at;
            let at = if playing { r.at } else { song.left_at };
            let (data, current, starts) = song_item(song, at, list.progress_of(index, r.at));
            (index, data, playing.then_some(current).flatten(), starts)
        })
        .collect();
    let at_song = list.at;
    let pending = list.pending.clone();
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100%; height:100%; box-sizing:border-box; \
                    overflow-y:auto; display:flex; flex-direction:column;",
            for (index, data, current, starts) in rows {
                NavSong {
                    key: "{index}",
                    data,
                    open: index == at_song,
                    current,
                    on_song: move |()| on_pick.call(index),
                    on_section: move |section: usize| {
                        if let Some(from) = starts.get(section) {
                            transport(Move::Seek, *from);
                        }
                    },
                }
            }
            // Songs still opening: their place in the set, greyed.
            for title in pending {
                div {
                    key: "pending-{title}",
                    style: "height:48px; flex:none; display:flex; align-items:center; padding:0 14px; \
                            border-bottom:1px solid {RULE}; color:{DIM}; font-size:14px; opacity:0.6;",
                    "{title}"
                }
            }
        }
    }
}

/// A colour a little or a lot of the way in: `#rrggbb` with an alpha.
fn tint(color: &str, alpha: u8) -> String {
    if color.len() == 7 && color.starts_with('#') {
        format!("{color}{alpha:02x}")
    } else {
        color.to_owned()
    }
}

/// One song in the navigator, flat and edge to edge: its bar filled as far
/// as the set has got through it; open (the one playing), its progress
/// down the window's very edge beside it and its sections under it, each
/// a bar of its own, set in a little.
#[component]
fn NavSong(
    data: SongItemData,
    open: bool,
    current: Option<usize>,
    on_song: EventHandler<()>,
    on_section: EventHandler<usize>,
) -> Element {
    let color = data.bright_color.clone();
    rsx! {
        div {
            style: "flex:none; display:flex; align-items:stretch; border-bottom:1px solid {RULE};",
            if open {
                // How far through the song, down its whole length.
                div {
                    style: "position:relative; flex:none; width:6px; background:{tint(&color, 0x33)};",
                    div {
                        style: "position:absolute; left:0; top:0; width:6px; height:{data.progress}%; \
                                background:{color};",
                    }
                }
            }
            div {
                style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:1px;",
                Bar {
                    label: data.label.clone(),
                    progress: data.progress,
                    color: color.clone(),
                    tall: 50.0,
                    indent: 0.0,
                    lit: open,
                    on_press: move |()| on_song.call(()),
                }
                if open {
                    for (i, section) in data.sections.iter().cloned().enumerate() {
                        Bar {
                            key: "{i}",
                            label: section.label,
                            progress: section.progress,
                            color: section.bright_color,
                            tall: 40.0,
                            indent: 12.0,
                            lit: current == Some(i),
                            on_press: move |()| on_section.call(i),
                        }
                    }
                }
            }
        }
    }
}

/// One bar: its colour washed across, filled to `progress` (0–100), its
/// name on it; `lit`, the one being played, brighter and bold.
#[component]
fn Bar(
    label: String,
    progress: f64,
    color: String,
    tall: f64,
    indent: f64,
    lit: bool,
    on_press: EventHandler<()>,
) -> Element {
    let (ground, fill, ink, weight) = if lit {
        (tint(&color, 0x40), tint(&color, 0x99), "#ffffff", 700)
    } else {
        (tint(&color, 0x1f), tint(&color, 0x55), "#c9ccd2", 600)
    };
    rsx! {
        div {
            style: "position:relative; flex:none; height:{tall}px; margin-left:{indent}px; \
                    background:{ground}; overflow:hidden; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            div {
                style: "position:absolute; left:0; top:0; bottom:0; width:{progress}%; background:{fill};",
            }
            div {
                style: "position:absolute; left:0; top:0; right:0; bottom:0; display:flex; align-items:center; \
                        padding:0 14px; color:{ink}; font-size:14px; font-weight:{weight}; \
                        white-space:nowrap; overflow:hidden; text-overflow:ellipsis;",
                "{label}"
            }
        }
    }
}

/// One song as a navigator row: its sections, each filled as far as `at`
/// (seconds, in the song's project), and the song filled to `progress`
/// (0–1). Also which section `at` is in, and where each starts.
fn song_item(song: &Song, at: f64, progress: f64) -> (SongItemData, Option<usize>, Vec<f64>) {
    let parts = crate::progress::Song::of(&song.session);
    let (sections, current, starts) = parts.map_or_else(
        || (Vec::new(), None, Vec::new()),
        |parts| {
            let sections = parts
                .sections
                .iter()
                .zip(&parts.bar)
                .map(|((from, to), bar)| SectionItem {
                    label: bar.name.clone(),
                    progress: ((at - from) / (to - from).max(f64::EPSILON)).clamp(0.0, 1.0) * 100.0,
                    bright_color: bar.color.clone(),
                    muted_color: bar.color.clone(),
                    comment: None,
                })
                .collect();
            let starts = parts.sections.iter().map(|(from, _)| *from).collect();
            (sections, parts.current(at), starts)
        },
    );
    let data = SongItemData {
        label: song.title(),
        progress: progress * 100.0,
        bright_color: song.color.clone(),
        muted_color: song.color.clone(),
        sections,
        key_label: None,
    };
    (data, current, starts)
}
