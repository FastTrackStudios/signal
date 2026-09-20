//! A dropdown that renders, because the native one does not.
//!
//! # Why this exists
//!
//! `<select>` is the obvious way to offer a choice, and in signal UI it is the
//! wrong one. Blitz draws the element's box but not the popup a browser would
//! open for it: the control appears with the platform's default chrome — light
//! grey on a dark rig, its own font, its own metrics — and the list it is
//! supposed to drop never arrives, because the popup is the browser's, not the
//! document's. The keys rig reached this conclusion first and has no `<select>`
//! anywhere; this is that fix, made shareable.
//!
//! So a picker is built from the elements Blitz does render: a button that
//! shows the current choice, and an absolutely-positioned list of buttons that
//! appears when it is pressed. Nothing platform-drawn, so it looks the same
//! standalone, as a plugin, and embedded in REAPER — which is the whole point
//! of the rendering rules.
//!
//! # What Blitz does not give us
//!
//! Three of this widget's obvious implementations are unavailable, and the
//! substitutions are deliberate rather than sloppy (see `blitz.is/status/css`):
//!
//! | wanted | why not | instead |
//! |---|---|---|
//! | `position: fixed` backdrop | unsupported; an absolute box positions against its immediate parent, so nothing in here can cover the window | close on `focusout` |
//! | `overflow-y: auto` on a long list | unsupported | the list grows; keep option counts human |
//! | `text-overflow: ellipsis` | unsupported | `overflow: hidden` clips |
//!
//! # Closing it
//!
//! A real dropdown closes when you click away from it. Focus leaving the
//! picker is that gesture, and it costs nothing that a backdrop would have
//! given: Escape and tabbing away close it too, which a backdrop never did.

use dioxus::prelude::*;

/// How much room the picker takes.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum PickerSize {
    /// Chain-slot scale — the 8px type used on the drive board.
    Tiny,
    /// Panel scale.
    #[default]
    Normal,
}

impl PickerSize {
    const fn metrics(self) -> (&'static str, &'static str, &'static str) {
        // (font-size, height, horizontal padding)
        match self {
            Self::Tiny => ("8px", "14px", "3px"),
            Self::Normal => ("11px", "22px", "7px"),
        }
    }
}

/// A choice among `options`, drawn rather than delegated to the platform.
///
/// `selected` indexes `options`; an index past the end shows `placeholder`,
/// which is what an unset slot should look like rather than silently showing
/// the first option as though it were chosen.
#[component]
pub fn Picker(
    options: Vec<String>,
    selected: u32,
    on_select: EventHandler<u32>,
    #[props(default = PickerSize::Normal)] size: PickerSize,
    #[props(default = String::new())] placeholder: String,
    /// Width of the closed button. The list matches it unless the option text
    /// needs more, so a long capture name is readable when opened even in a
    /// narrow slot.
    #[props(default = String::new())] width: String,
    #[props(default = false)] disabled: bool,
) -> Element {
    let mut open = use_signal(|| false);
    let (font, height, pad) = size.metrics();

    let current = options
        .get(selected as usize)
        .cloned()
        .unwrap_or_else(|| placeholder.clone());
    // One option is not a choice — draw it as the label it is, so the board
    // does not sprout affordances that lead nowhere.
    let interactive = !disabled && options.len() > 1;

    // RSX format strings take an identifier or a simple expression, not an
    // `if`, so anything conditional is decided here and interpolated as a
    // value.
    let (label_colour, cursor) = if interactive {
        ("#d4d4d8", "pointer")
    } else {
        ("#71717a", "default")
    };

    let width_rule = if width.is_empty() {
        String::new()
    } else {
        format!("width: {width}; ")
    };

    rsx! {
        div {
            style: "position: relative; display: inline-flex; flex-shrink: 0; {width_rule}",
            // Click-away, without a backdrop.
            //
            // The obvious implementation is a fixed full-viewport layer behind
            // the list, and Blitz does not support `position: fixed` at all —
            // an absolute box positions against its immediate parent, so there
            // is no way to cover the window from in here. Focus leaving the
            // picker is the same gesture by another route, and it also closes
            // on Escape and on tabbing away, which a backdrop never did.
            tabindex: "-1",
            onfocusout: move |_| open.set(false),
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    open.set(false);
                }
            },
            button {
                style: "width: 100%; display: flex; align-items: center; gap: 3px; \
                        appearance: none; box-sizing: border-box; \
                        font-size: {font}; height: {height}; padding: 0 {pad}; \
                        border: 1px solid #2a2a30; border-radius: 3px; \
                        background-color: rgba(0, 0, 0, 0.35); \
                        color: {label_colour}; cursor: {cursor}; \
                        text-align: left; overflow: hidden; white-space: nowrap;",
                disabled: !interactive,
                // The board's chunks are themselves draggable faders; a press
                // on the picker must not also move the control behind it.
                onpointerdown: move |e: PointerEvent| e.stop_propagation(),
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    if interactive {
                        let was = *open.peek();
                        open.set(!was);
                    }
                },
                span {
                    // No `text-overflow` on Blitz — a long name clips.
                    style: "flex: 1 1 auto; overflow: hidden; white-space: nowrap;",
                    "{current}"
                }
                if interactive {
                    span { style: "flex-shrink: 0; font-size: 7px; color: #52525b;", "▾" }
                }
            }

            if open() {
                div {
                    style: "position: absolute; top: calc(100% + 2px); left: 0; z-index: 91; \
                            min-width: 100%; \
                            display: flex; flex-direction: column; \
                            border: 1px solid #3f3f46; border-radius: 4px; \
                            background-color: #131317; \
                            box-shadow: 0 6px 18px rgba(0, 0, 0, 0.6);",
                    onpointerdown: move |e: PointerEvent| e.stop_propagation(),
                    for (i, label) in options.iter().enumerate() {
                        button {
                            key: "{i}",
                            style: "appearance: none; border: none; \
                                    background-color: {row_bg(i as u32 == selected)}; \
                                    color: {row_fg(i as u32 == selected)}; \
                                    font-size: {font}; padding: 4px 9px; text-align: left; \
                                    white-space: nowrap; cursor: pointer;",
                            onpointerdown: move |e: PointerEvent| e.stop_propagation(),
                            onclick: {
                                let index = i as u32;
                                move |e: MouseEvent| {
                                    e.stop_propagation();
                                    open.set(false);
                                    on_select.call(index);
                                }
                            },
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}

/// The current row is marked by the rig's amber, the way an engaged block is.
const fn row_bg(current: bool) -> &'static str {
    if current {
        "rgba(245, 158, 11, 0.16)"
    } else {
        "transparent"
    }
}

const fn row_fg(current: bool) -> &'static str {
    if current { "#fbbf24" } else { "#d4d4d8" }
}
