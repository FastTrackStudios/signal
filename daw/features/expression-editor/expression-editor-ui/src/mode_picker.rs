//! Which surface you are working on.
//!
//! This was a full-width row across the top — seven buttons pressed a
//! handful of times a session, sitting above the music for all of it.
//! Thirty pixels of height is worth far more here than thirty of width:
//! the roll is a *pitch* axis, and height is what it never has enough of.
//!
//! It is now one line in the panel that opens when you ask it to. As an
//! always-open list it was seven rows and two captions — two hundred
//! pixels of the panel spent, before anything about the note you have
//! selected, on a choice you make once and leave alone.
//!
//! ## A popup, not a `<select>`
//!
//! Blitz treats `select` as a focusable form control but implements no
//! dropdown behind it, so a native one renders as a box that does
//! nothing. This is a button and an absolutely-positioned list.

use dioxus::prelude::*;
use expression_editor_core::{Editor, Mode, ModeFamily};

use crate::theme;

/// The current mode, and a list to change it from.
#[component]
pub fn ModePicker(editor: Signal<Editor>) -> Element {
    let mut editor = editor;
    let mut open = use_signal(|| false);
    let current = editor.read().mode;

    rsx! {
        div {
            "data-testid": "mode-picker",
            style: "position: relative; display: flex; align-items: center; \
                    gap: 8px; padding: 6px 10px; \
                    border-bottom: 1px solid {theme::PANEL_BORDER};",

            span {
                style: "font-size: 9px; letter-spacing: 0.08em; \
                        text-transform: uppercase; color: {theme::TEXT_DIM}; \
                        flex: 0 0 auto;",
                "Mode"
            }
            button {
                "data-testid": "mode-current",
                title: "{current.label()}",
                style: "flex: 1 1 auto; min-width: 0; height: 20px; padding: 0 6px; \
                        display: flex; align-items: center; justify-content: space-between; \
                        gap: 6px; cursor: pointer; font-size: 11px; \
                        font-family: system-ui, sans-serif; border-radius: 4px; \
                        border: 1px solid {theme::PANEL_BORDER}; \
                        background: {theme::CONTROL}; color: {theme::TEXT};",
                onclick: move |_| {
                    let now = open();
                    open.set(!now);
                },
                span {
                    style: "display: flex; align-items: center; gap: 6px; overflow: hidden;",
                    svg {
                        view_box: "0 0 16 16",
                        style: "width: 12px; height: 12px; flex: 0 0 auto;",
                        path {
                            d: theme::mode_icon(current),
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.3",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                        }
                    }
                    "{current.label()}"
                }
                span { style: "color: {theme::TEXT_DIM};", "▾" }
            }

            if open() {
                div {
                    "data-testid": "mode-list",
                    style: "position: absolute; right: 10px; left: 10px; top: 30px; \
                            z-index: 50; padding: 4px 0; border-radius: 6px; \
                            border: 1px solid {theme::PANEL_BORDER}; \
                            background: {theme::PANEL}; \
                            box-shadow: 0 8px 28px rgba(0,0,0,0.5);",
                    for family in ModeFamily::ALL {
                        // The family is a caption rather than a rule. A
                        // divider says "these are different" without ever
                        // saying how, and the difference is the one that
                        // decides what an edit writes back: note events
                        // on one side, stretch markers and envelope
                        // points on the other.
                        div {
                            style: "font-size: 9px; letter-spacing: 0.08em; \
                                    text-transform: uppercase; color: {theme::TEXT_DIM}; \
                                    padding: 5px 10px 2px;",
                            "{family.label()}"
                        }
                        for m in family.modes().iter().copied() {
                            ModeRow {
                                key: "{m:?}",
                                mode: m,
                                active: current == m,
                                onpick: move |_| {
                                    editor.write().set_mode(m);
                                    open.set(false);
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One mode. A row, not a chip: it is a choice from a list.
#[component]
fn ModeRow(mode: Mode, active: bool, onpick: EventHandler<()>) -> Element {
    let (fg, bg) = if active {
        (theme::SELECTED, theme::CONTROL_ACTIVE)
    } else {
        (theme::TEXT, "transparent")
    };
    rsx! {
        button {
            "data-testid": "mode-{mode:?}",
            title: "{mode.label()}",
            style: "display: flex; align-items: center; gap: 8px; \
                    width: 100%; border: none; text-align: left; \
                    padding: 4px 10px; cursor: pointer; \
                    background: {bg}; color: {fg}; \
                    font-size: 11px; font-family: system-ui, sans-serif;",
            onclick: move |_| onpick.call(()),
            svg {
                view_box: "0 0 16 16",
                style: "width: 12px; height: 12px; flex: 0 0 auto;",
                path {
                    d: theme::mode_icon(mode),
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "1.3",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                }
            }
            span { "{mode.label()}" }
        }
    }
}
