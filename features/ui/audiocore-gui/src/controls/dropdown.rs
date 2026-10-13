//! Themed dropdown select menu — click-to-open, click-outside-to-close.
//!
//! Two variants:
//! - `Dropdown`: standalone with `Vec<String>` items and `on_change` callback
//! - `ParamDropdown`: bound to a nice_plug enum parameter

use crate::theme::use_theme;
use nice_plug::prelude::ParamPtr;
use nice_plug_dioxus::prelude::*;

/// Themed dropdown select menu.
///
/// Click to open, click-outside to close. Uses a fixed overlay for click-outside detection.
#[component]
pub fn Dropdown(
    items: Vec<String>,
    selected: usize,
    on_change: EventHandler<usize>,
    #[props(default)] label: Option<&'static str>,
    #[props(default = "150px")] width: &'static str,
) -> Element {
    let t = use_theme();
    let t = *t.read();
    let mut is_open = use_signal(|| false);
    let mut hovered_idx = use_signal(|| None::<usize>);

    let open = *is_open.read();
    let current_label = items.get(selected).cloned().unwrap_or_default();

    // SVG chevron rotation
    let chevron_rotation = if open { "180" } else { "0" };

    rsx! {
        div {
            style: format!(
                "display:flex; flex-direction:column; gap:{TIGHT};",
                TIGHT = t.spacing_tight,
            ),

            if let Some(lbl) = label {
                div {
                    style: format!("{LABEL}", LABEL = t.style_label()),
                    "{lbl}"
                }
            }

            div {
                style: format!("position:relative; width:{width};"),

                // Button
                div {
                    style: format!(
                        "display:flex; align-items:center; justify-content:space-between; \
                         padding:4px 8px; cursor:pointer; \
                         background:{BG}; border:1px solid {BORDER}; \
                         border-radius:{RADIUS}; color:{TEXT}; \
                         font-size:{FSIZE}; user-select:none;",
                        BG = t.surface_raised,
                        BORDER = if open { t.accent } else { t.border },
                        RADIUS = t.radius_button,
                        TEXT = t.text,
                        FSIZE = t.font_size_value,
                    ),
                    onclick: move |_| is_open.set(!open),

                    span { "{current_label}" }

                    // Chevron SVG
                    svg {
                        width: "10",
                        height: "10",
                        view_box: "0 0 10 10",
                        style: format!(
                            "transform:rotate({chevron_rotation}deg); \
                             transition:transform 0.15s;",
                        ),
                        path {
                            d: "M2 3.5 L5 6.5 L8 3.5",
                            stroke: "{t.text_dim}",
                            stroke_width: "1.5",
                            fill: "none",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                        }
                    }
                }

                // Overlay + Menu (when open)
                if open {
                    // Click-outside overlay
                    div {
                        style: "position:fixed; inset:0; z-index:9999;",
                        onclick: move |_| is_open.set(false),
                    }

                    // Menu
                    div {
                        style: format!(
                            "position:absolute; top:calc(100% + 2px); left:0; right:0; \
                             z-index:10000; max-height:200px; overflow-y:auto; \
                             background:{BG}; border:1px solid {BORDER}; \
                             border-radius:{RADIUS}; \
                             box-shadow:{SHADOW};",
                            BG = t.card_bg,
                            BORDER = t.border,
                            RADIUS = t.radius_button,
                            SHADOW = t.shadow_raised,
                        ),

                        for (idx, item) in items.iter().enumerate() {
                            {
                                let is_selected = idx == selected;
                                let is_hovered = *hovered_idx.read() == Some(idx);
                                let item_bg = if is_selected {
                                    t.accent_dim
                                } else if is_hovered {
                                    t.surface_hover
                                } else {
                                    "transparent"
                                };
                                let item_color = if is_selected { t.text_bright } else { t.text };

                                rsx! {
                                    div {
                                        key: "{idx}",
                                        style: format!(
                                            "padding:4px 8px; cursor:pointer; \
                                             font-size:{FSIZE}; color:{item_color}; \
                                             background:{item_bg};",
                                            FSIZE = t.font_size_value,
                                        ),
                                        onmouseenter: move |_| hovered_idx.set(Some(idx)),
                                        onmouseleave: move |_| hovered_idx.set(None),
                                        onclick: move |evt: MouseEvent| {
                                            evt.stop_propagation();
                                            on_change.call(idx);
                                            is_open.set(false);
                                        },
                                        "{item}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Dropdown bound to a nice_plug enum parameter.
///
/// Reads enum variant names from the parameter and auto-generates the item list.
#[component]
pub fn ParamDropdown(
    param_ptr: ParamPtr,
    #[props(default)] label: Option<&'static str>,
    #[props(default = "150px")] width: &'static str,
) -> Element {
    let t = use_theme();
    let t = *t.read();
    let ctx = use_param_context();
    let mut revision = use_signal(|| 0u32);
    let _ = *revision.read();

    let mut is_open = use_signal(|| false);
    let mut hovered_idx = use_signal(|| None::<usize>);

    let open = *is_open.read();
    let step_count = unsafe { param_ptr.step_count() }.unwrap_or(0) as usize + 1;
    let normalized = unsafe { param_ptr.modulated_normalized_value() };
    let selected = (normalized * (step_count - 1) as f32).round() as usize;

    // Build item list from parameter
    let items: Vec<String> = (0..step_count)
        .map(|i| {
            let norm = i as f32 / (step_count - 1).max(1) as f32;
            unsafe { param_ptr.normalized_value_to_string(norm, false) }
        })
        .collect();

    let current_label = items.get(selected).cloned().unwrap_or_default();
    let chevron_rotation = if open { "180" } else { "0" };

    rsx! {
        div {
            style: format!(
                "display:flex; flex-direction:column; gap:{TIGHT};",
                TIGHT = t.spacing_tight,
            ),

            if let Some(lbl) = label {
                div {
                    style: format!("{LABEL}", LABEL = t.style_label()),
                    "{lbl}"
                }
            }

            div {
                style: format!("position:relative; width:{width};"),

                div {
                    style: format!(
                        "display:flex; align-items:center; justify-content:space-between; \
                         padding:4px 8px; cursor:pointer; \
                         background:{BG}; border:1px solid {BORDER}; \
                         border-radius:{RADIUS}; color:{TEXT}; \
                         font-size:{FSIZE}; user-select:none;",
                        BG = t.surface_raised,
                        BORDER = if open { t.accent } else { t.border },
                        RADIUS = t.radius_button,
                        TEXT = t.text,
                        FSIZE = t.font_size_value,
                    ),
                    onclick: move |_| is_open.set(!open),

                    span { "{current_label}" }

                    svg {
                        width: "10",
                        height: "10",
                        view_box: "0 0 10 10",
                        style: format!(
                            "transform:rotate({chevron_rotation}deg); \
                             transition:transform 0.15s;",
                        ),
                        path {
                            d: "M2 3.5 L5 6.5 L8 3.5",
                            stroke: "{t.text_dim}",
                            stroke_width: "1.5",
                            fill: "none",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                        }
                    }
                }

                if open {
                    div {
                        style: "position:fixed; inset:0; z-index:9999;",
                        onclick: move |_| is_open.set(false),
                    }

                    div {
                        style: format!(
                            "position:absolute; top:calc(100% + 2px); left:0; right:0; \
                             z-index:10000; max-height:200px; overflow-y:auto; \
                             background:{BG}; border:1px solid {BORDER}; \
                             border-radius:{RADIUS}; \
                             box-shadow:{SHADOW};",
                            BG = t.card_bg,
                            BORDER = t.border,
                            RADIUS = t.radius_button,
                            SHADOW = t.shadow_raised,
                        ),

                        for idx in 0..step_count {
                            {
                                let is_selected = idx == selected;
                                let is_hovered = *hovered_idx.read() == Some(idx);
                                let item_bg = if is_selected {
                                    t.accent_dim
                                } else if is_hovered {
                                    t.surface_hover
                                } else {
                                    "transparent"
                                };
                                let item_color = if is_selected { t.text_bright } else { t.text };
                                let item_label = items[idx].clone();

                                rsx! {
                                    div {
                                        key: "{idx}",
                                        style: format!(
                                            "padding:4px 8px; cursor:pointer; \
                                             font-size:{FSIZE}; color:{item_color}; \
                                             background:{item_bg};",
                                            FSIZE = t.font_size_value,
                                        ),
                                        onmouseenter: move |_| hovered_idx.set(Some(idx)),
                                        onmouseleave: move |_| hovered_idx.set(None),
                                        onclick: {
                                            let ctx = ctx.clone();
                                            move |evt: MouseEvent| {
                                                evt.stop_propagation();
                                                let norm = idx as f32 / (step_count - 1).max(1) as f32;
                                                ctx.begin_set_raw(param_ptr);
                                                ctx.set_normalized_raw(param_ptr, norm);
                                                ctx.end_set_raw(param_ptr);
                                                revision += 1;
                                                is_open.set(false);
                                            }
                                        },
                                        "{item_label}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
