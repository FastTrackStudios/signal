//! Dropdown selector for a stepped parameter.
//!
//! [`Segmented`](crate::controls::Segmented) is the right control for three or
//! four choices — every option visible, one click to any of them. Past that it
//! turns into a wall of buttons that eats the width a graph wants, and the
//! options stop being scannable. This is the same parameter contract in a
//! dropdown: the current choice, and the rest one click away.
//!
//! Written as a plain absolutely-positioned list rather than a portal-based
//! popover: this renders under Blitz inside plugin editors, where the simplest
//! thing that lays out predictably is worth more than polish. The list opens
//! *upward* by default because these controls live in a bar along the bottom
//! of the surface.

use dioxus::prelude::*;

use crate::param::ParamHandle;

/// A stepped parameter as a dropdown.
///
/// The parameter's normalized range is divided evenly across `options`, the
/// same mapping [`Segmented`](crate::controls::Segmented) uses, so the two are
/// interchangeable for any stepped param.
#[component]
pub fn Dropdown(
    handle: ParamHandle,
    options: Vec<String>,
    /// Accent for the current selection.
    #[props(default = "#8aa4ff".to_string())]
    color: String,
    /// Trigger width in px. The list matches it.
    #[props(default = 96.0)]
    width: f64,
    /// Open the list downward instead of up.
    #[props(default = false)]
    down: bool,
    #[props(default)] testid: Option<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let count = options.len().max(1);
    let selected = if count > 1 {
        ((handle.normalized() as f64) * (count - 1) as f64).round() as usize
    } else {
        0
    };
    let is_open = *open.read();
    let testid = testid.unwrap_or_else(|| "dropdown".to_string());
    let current = options
        .get(selected)
        .cloned()
        .unwrap_or_else(|| "—".to_string());

    rsx! {
        div {
            "data-testid": "{testid}",
            "data-index": "{selected}",
            "data-open": "{is_open}",
            style: format!("position:relative; width:{width}px;"),

            // Trigger.
            div {
                "data-testid": "{testid}-trigger",
                style: format!(
                    "display:flex; align-items:center; justify-content:space-between; \
                     gap:6px; padding:5px 8px; border-radius:6px; cursor:pointer; \
                     font-size:11px; font-weight:600; \
                     color:var(--foreground); \
                     background:var(--card, rgba(255,255,255,0.05)); \
                     border:1px solid {};",
                    if is_open {
                        color.as_str()
                    } else {
                        "var(--border, rgba(148,163,184,0.30))"
                    },
                ),
                onclick: move |_| open.toggle(),
                span { "{current}" }
                // A caret drawn as text: no icon dependency, and it renders
                // identically in every host.
                span {
                    style: "font-size:8px; color:var(--muted-foreground, #8a8a92);",
                    if is_open ^ down { "▲" } else { "▼" }
                }
            }

            if is_open {
                div {
                    "data-testid": "{testid}-list",
                    style: format!(
                        "position:absolute; {} left:0; width:{width}px; z-index:40; \
                         display:flex; flex-direction:column; padding:3px; gap:1px; \
                         border-radius:7px; \
                         background:var(--popover, var(--card, #14161b)); \
                         border:1px solid var(--border, rgba(148,163,184,0.34)); \
                         box-shadow:0 8px 26px rgba(0,0,0,0.55);",
                        if down { "top:30px;" } else { "bottom:30px;" },
                    ),
                    for (index , option) in options.iter().enumerate() {
                        {
                            let active = index == selected;
                            let handle = handle.clone();
                            let step = if count > 1 {
                                index as f32 / (count - 1) as f32
                            } else {
                                0.0
                            };
                            rsx! {
                                div {
                                    "data-testid": "{testid}-option-{index}",
                                    style: format!(
                                        "padding:5px 8px; border-radius:5px; cursor:pointer; \
                                         font-size:11px; font-weight:{}; color:{}; background:{};",
                                        if active { 700 } else { 500 },
                                        if active { "#0b0b0d" } else { "var(--foreground)" },
                                        if active { color.as_str() } else { "transparent" },
                                    ),
                                    onclick: move |_| {
                                        handle.begin_edit();
                                        handle.set_normalized(step);
                                        handle.end_edit();
                                        open.set(false);
                                    },
                                    "{option}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
