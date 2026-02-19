//! Morph slider -- A/B morphing with assignment dropdowns.
//!
//! A horizontal slider for smoothly morphing between two endpoints:
//!
//! ```text
//! +------------------------------------------------------+
//! |  [A: Verse  v]  A-----------[*]-----------B  [B: Chorus v]  |
//! |                    ! Structural differences               |
//! +------------------------------------------------------+
//! ```
//!
//! This is a domain-agnostic component. It knows nothing about snapshots,
//! MIDI, or rig state -- callers provide labels, dropdown items, and callbacks.

use dioxus::prelude::*;

/// A selectable item for the A/B assignment dropdowns.
#[derive(Clone, PartialEq)]
pub struct DropdownItem {
    /// Unique identifier for this item.
    pub id: String,
    /// Display label.
    pub label: String,
}

/// A horizontal morph slider with A/B assignment dropdowns.
///
/// The slider interpolates between endpoint A (left, position=0.0) and
/// endpoint B (right, position=1.0). Assignment dropdowns let the user
/// pick which items are loaded into each slot.
#[component]
pub fn MorphSlider(
    /// Current morph position [0.0, 1.0].
    position: f64,
    /// Callback when the user drags the slider.
    on_position_change: EventHandler<f64>,
    /// Display label for endpoint A.
    #[props(default = "\u{2014}".to_string())]
    label_a: String,
    /// Display label for endpoint B.
    #[props(default = "\u{2014}".to_string())]
    label_b: String,
    /// Available items for the A dropdown.
    #[props(default)]
    items_a: Vec<DropdownItem>,
    /// Available items for the B dropdown.
    #[props(default)]
    items_b: Vec<DropdownItem>,
    /// Currently selected item ID for A (used for highlight).
    #[props(default)]
    selected_a: Option<String>,
    /// Currently selected item ID for B (used for highlight).
    #[props(default)]
    selected_b: Option<String>,
    /// Callback when user selects an item for A.
    on_select_a: EventHandler<String>,
    /// Callback when user selects an item for B.
    on_select_b: EventHandler<String>,
    /// Whether structural differences prevent morphing.
    #[props(default)]
    has_warning: bool,
    /// Custom warning text. Defaults to a generic message.
    #[props(default)]
    warning_text: Option<String>,
) -> Element {
    let mut dropdown_a_open = use_signal(|| false);
    let mut dropdown_b_open = use_signal(|| false);

    let position_pct = (position * 100.0).round();

    rsx! {
        div { class: "flex flex-col gap-1",
            // Main row: [A button] -- slider -- [B button]
            div { class: "flex items-center gap-3",
                // Endpoint A assignment button
                div { class: "relative",
                    button {
                        class: "flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium \
                                bg-blue-900/40 text-blue-300 border border-blue-800/50 \
                                hover:bg-blue-800/50 transition-colors min-w-[80px] justify-between",
                        onclick: move |_| {
                            *dropdown_a_open.write() = !dropdown_a_open();
                            *dropdown_b_open.write() = false;
                        },
                        span { "A: {label_a}" }
                        span { class: "text-[10px] opacity-60", "\u{25BE}" }
                    }
                    if dropdown_a_open() {
                        {render_dropdown(
                            &items_a,
                            selected_a.as_deref(),
                            &on_select_a,
                            &mut dropdown_a_open,
                        )}
                    }
                }

                // Slider track
                div { class: "flex-1 flex items-center gap-2",
                    span { class: "text-[10px] font-bold text-blue-400 select-none", "A" }
                    div { class: "flex-1 relative h-6 flex items-center",
                        // Track background
                        div { class: "absolute inset-x-0 h-1 bg-zinc-700 rounded-full" }
                        // Filled portion (A -> thumb)
                        div {
                            class: "absolute left-0 h-1 bg-gradient-to-r from-blue-500 to-orange-500 rounded-full",
                            style: "width: {position_pct}%",
                        }
                        // Range input (invisible but captures interaction)
                        input {
                            r#type: "range",
                            class: "absolute inset-0 w-full h-full opacity-0 cursor-pointer z-10",
                            min: "0",
                            max: "1000",
                            value: "{(position * 1000.0).round() as i64}",
                            oninput: move |evt| {
                                if let Ok(val) = evt.value().parse::<f64>() {
                                    on_position_change.call(val / 1000.0);
                                }
                            },
                        }
                        // Thumb indicator
                        div {
                            class: "absolute w-4 h-4 rounded-full bg-white border-2 border-zinc-400 \
                                    shadow-md pointer-events-none transform -translate-x-1/2",
                            style: "left: {position_pct}%",
                        }
                    }
                    span { class: "text-[10px] font-bold text-orange-400 select-none", "B" }
                }

                // Endpoint B assignment button
                div { class: "relative",
                    button {
                        class: "flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium \
                                bg-orange-900/40 text-orange-300 border border-orange-800/50 \
                                hover:bg-orange-800/50 transition-colors min-w-[80px] justify-between",
                        onclick: move |_| {
                            *dropdown_b_open.write() = !dropdown_b_open();
                            *dropdown_a_open.write() = false;
                        },
                        span { "B: {label_b}" }
                        span { class: "text-[10px] opacity-60", "\u{25BE}" }
                    }
                    if dropdown_b_open() {
                        {render_dropdown(
                            &items_b,
                            selected_b.as_deref(),
                            &on_select_b,
                            &mut dropdown_b_open,
                        )}
                    }
                }
            }

            // Warning row (structural differences)
            if has_warning {
                div { class: "flex items-center gap-1.5 px-2 py-1 rounded bg-amber-900/30 \
                              border border-amber-800/40 text-amber-400 text-[11px]",
                    svg {
                        class: "w-3.5 h-3.5 flex-shrink-0",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 \
                                1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 \
                                16c-.77 1.333.192 3 1.732 3z",
                        }
                    }
                    span {
                        {warning_text.as_deref().unwrap_or(
                            "Structural differences \u{2014} endpoints use different configurations. Morphing disabled, use crossfade."
                        )}
                    }
                }
            }
        }
    }
}

/// Render a dropdown of available items for assignment.
fn render_dropdown(
    items: &[DropdownItem],
    current_id: Option<&str>,
    on_select: &EventHandler<String>,
    open_signal: &mut Signal<bool>,
) -> Element {
    let on_select = on_select.clone();
    let mut open_signal = *open_signal;

    rsx! {
        div { class: "absolute top-full left-0 mt-1 z-50 min-w-[140px] py-1 \
                      bg-zinc-800 border border-zinc-700 rounded-lg shadow-xl",
            if items.is_empty() {
                div { class: "px-3 py-2 text-xs text-zinc-500 italic", "No items available" }
            }
            for item in items.iter() {
                {
                    let item_id = item.id.clone();
                    let is_current = current_id == Some(item_id.as_str());
                    let label = item.label.clone();
                    rsx! {
                        button {
                            key: "{item_id}",
                            class: if is_current {
                                "w-full text-left px-3 py-1.5 text-xs bg-zinc-700 text-white"
                            } else {
                                "w-full text-left px-3 py-1.5 text-xs text-zinc-300 hover:bg-zinc-700 transition-colors"
                            },
                            onclick: {
                                let item_id = item_id.clone();
                                move |_| {
                                    on_select.call(item_id.clone());
                                    *open_signal.write() = false;
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
