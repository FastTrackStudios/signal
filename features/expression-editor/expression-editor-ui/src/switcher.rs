//! The track switcher.
//!
//! Vovious's TrackSwitcher is keyboard-first by design — `T` opens it,
//! and per-track shortcuts are shown on each row — so the visible bar
//! is a readout of state you can also reach without it, never the only
//! way in.
//!
//! Two behaviours carried over because they cost nothing and save a
//! click each time: the reference tick lives on the row itself, and a
//! track's position in this list *is* its shortcut number.

use dioxus::prelude::*;
use expression_editor_core::{Editor, RefColor};

use crate::theme;

/// Shortcut label for row `i`. Rows past the tenth have none — a
/// two-key shortcut for track eleven is slower than clicking it.
fn shortcut(i: usize) -> Option<String> {
    (i < 10).then(|| format!("{}", (i + 1) % 10))
}

/// Whether the switcher takes a row at all.
///
/// One track is the ordinary case and a switcher for it is pure chrome,
/// so it stays hidden until there is something to switch to.
///
/// Public because the roll's box is the window less the chrome above it,
/// so `sizing` has to ask the same question this component answers. Two
/// copies of the predicate would mean a second track silently stealing a
/// row from the roll.
pub fn is_shown(ed: &Editor) -> bool {
    if ed.tracks.len() < 2 {
        return false;
    }
    // A stacked workspace of role lanes picks its mic *per lane*, in
    // the lane's own gutter. Thirteen chips named In/Out/Trig across
    // the top say nothing a lane's selector doesn't say better, and
    // they cost a row of the roll's scarcest resource.
    if ed.stacked {
        let lanes = ed.tracks.layout().lanes();
        if !lanes.is_empty() && lanes.iter().all(|l| l.role.is_some()) {
            return false;
        }
    }
    true
}

#[component]
pub fn TrackSwitcher(editor: Signal<Editor>) -> Element {
    let mut editor = editor;

    let ed = editor.read();
    if !is_shown(&ed) {
        return rsx! {};
    }
    let active = ed.active_track();
    let rows: Vec<(usize, String, bool, RefColor)> = ed
        .tracks
        .tracks()
        .iter()
        .enumerate()
        .map(|(i, t)| (i, t.name.clone(), t.reference, t.ref_color))
        .collect();
    drop(ed);

    rsx! {
        div {
            "data-testid": "track-switcher",
            style: "display: flex; flex: 0 0 auto; align-items: stretch; gap: 4px; \
                    box-sizing: border-box; height: {crate::sizing::SWITCHER_H}px; \
                    padding: 0 6px; background: {theme::SURFACE_BAR}; \
                    border-bottom: 1px solid {theme::PANEL_BORDER}; \
                    font-size: 11px; overflow-x: auto;",
            for (i, name, is_ref, ref_color) in rows {
                {
                    let is_active = i == active;
                    let bg = if is_active { theme::CONTROL_SELECTED } else { theme::CONTROL };
                    let fg = if is_active { theme::TEXT_BRIGHT } else { theme::TEXT_DIM };
                    let key_label = shortcut(i);
                    // The tick reads as on/off without needing a
                    // checkbox — Blitz renders `<input>` as an empty box.
                    let tick = if is_ref { "◉" } else { "○" };
                    let tick_fg = if is_ref { theme::REFERENCE } else { theme::TEXT_FAINT };
                    let tick_title = match ref_color {
                        RefColor::Default => "reference",
                        RefColor::Host => "reference (host colour)",
                        RefColor::Shadow => "reference (outline)",
                    };
                    rsx! {
                        div {
                            key: "t{i}",
                            style: "display: flex; align-items: center; gap: 6px; \
                                    padding: 3px 8px; border-radius: 3px; \
                                    background: {bg}; color: {fg}; white-space: nowrap; \
                                    cursor: pointer; user-select: none;",
                            onclick: move |_| {
                                editor.write().switch_track(i);
                            },
                            if let Some(k) = key_label {
                                span { style: "color: {theme::TEXT_FAINT};", "{k}" }
                            }
                            span { "{name}" }
                            // Toggling a reference must not also switch
                            // to it, so this swallows the row's click.
                            span {
                                style: "color: {tick_fg}; cursor: pointer;",
                                title: "{tick_title}",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    // The active track is never its own
                                    // reference; the flag is allowed to
                                    // sit there so it comes back when
                                    // you switch away.
                                    let mut w = editor.write();
                                    if let Some(t) = w.tracks.track_mut(i) {
                                        t.reference = !t.reference;
                                    }
                                },
                                "{tick}"
                            }
                        }
                    }
                }
            }
        }
    }
}
