//! The context menu overlay.
//!
//! What the menu *offers* is decided in
//! [`expression_editor_core::menu`] so it can be tested without a
//! browser; this only draws it and reports what was chosen.
//!
//! Drawn as an absolutely-positioned div rather than SVG, and with
//! explicit pixel geometry rather than a layout that measures its own
//! text — Blitz gives no text metrics, so a menu that sized itself to
//! its labels would be a different width in every host.

use dioxus::prelude::*;
use expression_editor_core::menu::{self, Command};
use expression_editor_core::{Editor, NoteId};

use crate::theme;

/// Row height, and the width the panel is drawn at.
const ROW_H: f64 = 24.0;
const WIDTH: f64 = 210.0;
const PAD_Y: f64 = 6.0;

/// Where the menu is, and what it was opened over.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContextMenu {
    pub open: bool,
    /// Canvas coordinates of the click.
    pub at: (f64, f64),
    pub under: Option<NoteId>,
    /// Document time of the click — what "split here" acts on.
    pub t: f64,
    /// The row clicked, which with `t` says whether a razor was under it.
    pub row: i32,
}

impl ContextMenu {
    pub fn show(&mut self, x: f64, y: f64, under: Option<NoteId>, t: f64, row: i32) {
        self.open = true;
        self.at = (x, y);
        self.under = under;
        self.t = t;
        self.row = row;
    }

    pub fn close(&mut self) {
        self.open = false;
    }
}

/// A command the core could not complete on its own, handed back so the
/// surface can open whatever it needs.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    Lyric(NoteId),
    Articulation(NoteId),
    Properties,
}

#[component]
pub fn ContextMenuOverlay(
    editor: Signal<Editor>,
    menu_state: Signal<ContextMenu>,
    pending: Signal<Option<Pending>>,
) -> Element {
    let mut editor = editor;
    let mut menu_state = menu_state;
    let mut pending = pending;

    let state = menu_state.read().clone();
    if !state.open {
        return rsx! {};
    }

    let ed = editor.read();
    let vp = ed.viewport;
    // `menu_at`, not `note_menu`: a razor under the click gets the
    // razor's verbs. The map has bound right-click on an area to this
    // menu since razors existed, and until now the menu answered with
    // Cut/Copy/Properties for notes.
    let items = menu::menu_at(&ed, state.under, state.t, state.row);
    drop(ed);

    // Height has to account for the rules between groups, or the last
    // item hangs out of the panel.
    let breaks = items.iter().filter(|i| i.group_break).count() as f64;
    let h = items.len() as f64 * ROW_H + breaks * 5.0 + PAD_Y * 2.0;

    // Flip rather than clamp when the menu would run off an edge: a
    // menu pinned to the bottom of the canvas covers the thing that was
    // right-clicked, which is the one thing it must not hide.
    let x = if state.at.0 + WIDTH > vp.w {
        (state.at.0 - WIDTH).max(0.0)
    } else {
        state.at.0
    };
    let y = if state.at.1 + h > vp.h {
        (state.at.1 - h).max(0.0)
    } else {
        state.at.1
    };

    let under = state.under;
    let mut y_cursor = PAD_Y;

    rsx! {
        // A full-canvas catcher, so clicking anywhere else dismisses.
        // Below the panel in z-order and above everything else.
        div {
            style: "position: absolute; left: 0; top: 0; width: {vp.w:.0}px; \
                    height: {vp.h:.0}px; z-index: 40;",
            onpointerdown: move |_| menu_state.write().close(),
        }
        div {
            style: "position: absolute; left: {x + crate::canvas::GUTTER_W:.1}px; \
                    top: {y + crate::canvas::RULER_H:.1}px; width: {WIDTH:.0}px; \
                    height: {h:.0}px; z-index: 41; background: {theme::PANEL}; \
                    border: 1px solid {theme::PANEL_BORDER}; border-radius: 4px; \
                    padding: {PAD_Y:.0}px 0; font-size: 12px; \
                    font-family: system-ui, sans-serif; user-select: none;",
            for item in items.iter().cloned() {
                {
                    let top = y_cursor;
                    if item.group_break {
                        y_cursor += 5.0;
                    }
                    let row_top = if item.group_break { top + 5.0 } else { top };
                    y_cursor += ROW_H;
                    let cmd = item.command.clone();
                    let enabled = item.enabled;
                    let fg = if enabled { theme::TEXT } else { theme::TEXT_FAINT };
                    let cursor = if enabled { "pointer" } else { "default" };
                    // rsx's format segments take a bare name, not an
                    // expression, so every derived number lands here.
                    let rule_top = top + 2.0;
                    let rule_w = WIDTH - 12.0;
                    rsx! {
                        if item.group_break {
                            div {
                                style: "position: absolute; left: 6px; top: {rule_top:.1}px; \
                                        width: {rule_w:.0}px; height: 1px; \
                                        background: {theme::PANEL_BORDER};",
                            }
                        }
                        div {
                            style: "position: absolute; left: 0; top: {row_top:.1}px; \
                                    width: {WIDTH:.0}px; height: {ROW_H:.0}px; \
                                    display: flex; align-items: center; \
                                    padding: 0 10px; color: {fg}; cursor: {cursor};",
                            onpointerdown: move |e: PointerEvent| {
                                e.stop_propagation();
                                if !enabled {
                                    return;
                                }
                                // Core runs what it can; what it cannot
                                // is handed up rather than silently
                                // dropped, so a menu item never looks
                                // like it did nothing.
                                let done = editor.write().run_command(&cmd, under);
                                if !done {
                                    let p = match &cmd {
                                        Command::EditLyric(id) => Some(Pending::Lyric(*id)),
                                        Command::SetArticulation(id) => {
                                            Some(Pending::Articulation(*id))
                                        }
                                        Command::Properties => Some(Pending::Properties),
                                        _ => None,
                                    };
                                    if p.is_some() {
                                        pending.set(p);
                                    }
                                }
                                menu_state.write().close();
                            },
                            span {
                                style: "flex: 1 1 auto; white-space: nowrap; overflow: hidden;",
                                "{item.label}"
                            }
                            if let Some(k) = item.shortcut {
                                span { style: "color: {theme::TEXT_FAINT}; font-size: 11px;", "{k}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
