//! Status indicators for the bar: a dot and a short label that say how a
//! subsystem is doing at a glance, with its menu a click away — the way a
//! DAW's audio and MIDI readouts work, instead of a row of buttons that each
//! open one thing. A subsystem that needs you (audio stopped) turns into an
//! alarm pill whose click is the fix.
//!
//! Blitz notes (see the `blitz-design` skill): the menu is an absolute box
//! under its indicator (there is no `position: fixed` for a click-outside
//! backdrop), so it closes when the pointer leaves the indicator-plus-menu;
//! rows are `div`s, because a `<button>` centres its content.

use dioxus::prelude::*;

use crate::theme::{DIM, FAINT, LINE_STRONG, LIVE, MENU, MUTED, R_MD, R_SM, T_BODY, T_SMALL, TEXT};

/// The alarm red: an indicator that needs you. White on it is 4.8:1.
const ALARM: &str = "#dc2626";

/// One menu row: a label and what it does. `None` action = a disabled note.
#[derive(Clone, PartialEq)]
pub struct IndicatorItem {
    pub label: String,
    pub action: Option<Callback<()>>,
    /// The current choice of a few (the buffer size): a drawn check.
    pub checked: bool,
}

impl IndicatorItem {
    pub fn new(label: impl Into<String>, action: Callback<()>) -> Self {
        Self {
            label: label.into(),
            action: Some(action),
            checked: false,
        }
    }

    /// A heading over the rows after it — no action.
    pub fn head(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            action: None,
            checked: false,
        }
    }

    #[must_use]
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }
}

/// A dot + label in the bar; right-click or double-click opens `items`.
/// `extra` renders inside the dropdown under the items (a live readout).
#[component]
pub fn Indicator(
    label: String,
    /// The dot's colour — the state at a glance.
    dot: String,
    title: String,
    items: Vec<IndicatorItem>,
    #[props(default)] extra: Option<Element>,
    /// Keep the dropdown open (an inline readout is showing).
    #[props(default)]
    pinned: bool,
    #[props(default)] on_close: Option<Callback<()>>,
    /// Blink the dot (and the label in its colour): something here needs
    /// attention — audio off — without a banner pushing the page down.
    #[props(default)]
    flash: bool,
    /// An alarm: the indicator becomes a red pill reading this ("Audio
    /// stopped · Start"), and a click runs `on_alarm` — the fix, not a menu.
    /// The menu stays a right-click away.
    #[props(default)]
    alarm: Option<String>,
    #[props(default)] on_alarm: Option<Callback<()>>,
) -> Element {
    let mut open = use_signal(|| false);
    let showing = open() || pinned;
    // The blink's phase, local so only the indicator redraws.
    // It only ticks while flashing: a steady indicator re-renders nothing.
    let mut lit = use_signal(|| true);
    let mut flashing = use_signal(|| flash);
    if *flashing.peek() != flash {
        flashing.set(flash);
    }
    use_future(move || async move {
        loop {
            architect::platform::sleep(std::time::Duration::from_millis(550)).await;
            if *flashing.peek() {
                lit.toggle();
            } else if !*lit.peek() {
                lit.set(true);
            }
        }
    });
    let on = !flash || lit();
    let dot_now = if on { dot.clone() } else { DIM.to_string() };
    let label_color = if flash && on { dot.clone() } else { MUTED.to_string() };
    let pill_dot = if on { "#ffffff" } else { ALARM };
    rsx! {
        div {
            style: "position: relative; display: flex; align-items: center; height: 28px;",
            onmouseleave: move |_| {
                open.set(false);
                if let Some(cb) = on_close {
                    cb.call(());
                }
            },
            if let Some(alarm) = alarm.clone() {
                // The alarm: loud, and its click is the fix.
                div {
                    title: "{title} — click to fix; right-click for options",
                    style: "display: flex; align-items: center; gap: 7px; height: 24px; padding: 0 10px; \
                            border-radius: {R_SM}; cursor: pointer; user-select: none; white-space: nowrap; \
                            background: {ALARM}; color: #ffffff; font-size: {T_BODY}; font-weight: 700;",
                    oncontextmenu: move |e: MouseEvent| {
                        e.prevent_default();
                        open.set(true);
                    },
                    onclick: move |_| {
                        if let Some(cb) = on_alarm {
                            cb.call(());
                        } else {
                            open.toggle();
                        }
                    },
                    span {
                        style: "width: 7px; height: 7px; border-radius: 999px; flex-shrink: 0; \
                                background: {pill_dot};",
                    }
                    "{alarm}"
                }
            } else {
                div {
                    title: "{title} — click for options",
                    style: "display: flex; align-items: center; gap: 6px; height: 24px; padding: 0 8px; \
                            border-radius: {R_SM}; cursor: pointer; user-select: none; \
                            font-size: {T_SMALL}; font-weight: 600; color: {label_color};",
                    class: "sg-hover",
                    oncontextmenu: move |e: MouseEvent| {
                        e.prevent_default();
                        open.set(true);
                    },
                    onclick: move |_| open.toggle(),
                    span {
                        style: "width: 7px; height: 7px; border-radius: 999px; flex-shrink: 0; \
                                background: {dot_now}; box-shadow: 0 0 6px {dot_now};",
                    }
                    "{label}"
                }
            }
            if showing {
                div {
                    style: "position: absolute; top: 100%; right: 0; z-index: 300; \
                            min-width: 200px; padding: 4px; margin-top: 2px; \
                            display: flex; flex-direction: column; gap: 1px; \
                            border: 1px solid {LINE_STRONG}; border-radius: {R_MD}; \
                            background: {MENU}; box-shadow: 0 12px 32px #000c;",
                    for (i, item) in items.iter().enumerate() {
                        {
                            let action = item.action;
                            let enabled = action.is_some();
                            rsx! {
                                div {
                                    key: "{i}",
                                    style: format!(
                                        "display: flex; align-items: center; gap: 6px; padding: 6px 9px; border-radius: {R_SM}; \
                                         font-size: {}; text-align: left; white-space: nowrap; cursor: {}; color: {}; {}",
                                        if enabled { T_BODY } else { "10px" },
                                        if enabled { "pointer" } else { "default" },
                                        if enabled { TEXT } else { FAINT },
                                        if enabled { "" } else { "padding-top: 8px; font-weight: 700; letter-spacing: 0.12em; text-transform: uppercase;" },
                                    ),
                                    class: if enabled { "sg-hover" } else { "" },
                                    onclick: move |_| {
                                        if let Some(cb) = action {
                                            open.set(false);
                                            cb.call(());
                                        }
                                    },
                                    if enabled {
                                        span { style: "width: 12px; flex-shrink: 0; display: flex; color: {LIVE};",
                                            if item.checked {
                                                fts_chrome::Glyph { icon: fts_chrome::Icon::Check, size: 12 }
                                            }
                                        }
                                    }
                                    "{item.label}"
                                }
                            }
                        }
                    }
                    if let Some(extra) = extra {
                        {extra}
                    }
                }
            }
        }
    }
}
