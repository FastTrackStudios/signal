//! Status indicators for the bar: a dot and a short label that say how a
//! subsystem is doing at a glance, with a menu behind a right-click or a
//! double-click — the way a DAW's audio and MIDI readouts work, instead of a
//! row of buttons that each open one thing.
//!
//! Blitz notes (see the `blitz-design` skill): the menu is an absolute box
//! under its indicator (there is no `position: fixed` for a click-outside
//! backdrop), so it closes when the pointer leaves the indicator-plus-menu;
//! rows are `div`s, because a `<button>` centres its content.

use dioxus::prelude::*;

/// One menu row: a label and what it does. `None` action = a disabled note.
#[derive(Clone, PartialEq)]
pub struct IndicatorItem {
    pub label: String,
    pub action: Option<Callback<()>>,
}

impl IndicatorItem {
    pub fn new(label: impl Into<String>, action: Callback<()>) -> Self {
        Self {
            label: label.into(),
            action: Some(action),
        }
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
    let dot_now = if on { dot.clone() } else { "#3f3f46".to_string() };
    let label_color = if flash && on { dot.clone() } else { "#a1a1aa".to_string() };
    rsx! {
        div {
            style: "position: relative; display: flex; align-items: center; height: 28px;",
            onmouseleave: move |_| {
                open.set(false);
                if let Some(cb) = on_close {
                    cb.call(());
                }
            },
            div {
                title: "{title} — right-click or double-click (tap, by touch) for options",
                style: "display: flex; align-items: center; gap: 6px; height: 24px; padding: 0 8px; \
                        border-radius: 6px; cursor: default; user-select: none; \
                        font-size: 11px; font-weight: 600; color: {label_color};",
                class: "hover:bg-accent/30",
                oncontextmenu: move |e: MouseEvent| {
                    e.prevent_default();
                    open.set(true);
                },
                ondoubleclick: move |_| open.set(true),
                // Touch has neither: a tap opens it.
                onclick: move |_| {
                    if signal_widgets::is_touch() {
                        open.toggle();
                    }
                },
                span {
                    style: "width: 7px; height: 7px; border-radius: 999px; flex-shrink: 0; \
                            background: {dot_now}; box-shadow: 0 0 6px {dot_now};",
                }
                "{label}"
            }
            if showing {
                div {
                    style: "position: absolute; top: 100%; right: 0; z-index: 300; \
                            min-width: 200px; padding: 4px; margin-top: 2px; \
                            display: flex; flex-direction: column; gap: 1px; \
                            border: 1px solid #2b2b31; border-radius: 10px; \
                            background: #0d0d10; box-shadow: 0 12px 32px #000c;",
                    for (i, item) in items.iter().enumerate() {
                        {
                            let action = item.action;
                            let enabled = action.is_some();
                            rsx! {
                                div {
                                    key: "{i}",
                                    style: format!(
                                        "padding: 6px 9px; border-radius: 6px; font-size: 11px; \
                                         text-align: left; white-space: nowrap; cursor: {}; color: {};",
                                        if enabled { "pointer" } else { "default" },
                                        if enabled { "#d4d4d8" } else { "#71717a" },
                                    ),
                                    class: if enabled { "hover:bg-accent/40" } else { "" },
                                    onclick: move |_| {
                                        if let Some(cb) = action {
                                            open.set(false);
                                            cb.call(());
                                        }
                                    },
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
