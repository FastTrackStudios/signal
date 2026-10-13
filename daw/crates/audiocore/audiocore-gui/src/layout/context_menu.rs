//! Right-click context menu overlay.
//!
//! Wraps children and shows a popup menu on right-click. Uses a fixed overlay
//! for click-outside detection, same pattern as the dropdown.

use crate::theme::use_theme;
use nice_plug_dioxus::prelude::*;

use dioxus_elements::input_data::MouseButton;

/// A single item in a context menu.
#[derive(Clone, Copy, PartialEq)]
pub struct MenuItem {
    pub label: &'static str,
    pub on_click: Callback<()>,
    pub disabled: bool,
    pub separator_after: bool,
}

impl MenuItem {
    /// Create a new enabled menu item.
    pub fn new(label: &'static str, on_click: Callback<()>) -> Self {
        Self {
            label,
            on_click,
            disabled: false,
            separator_after: false,
        }
    }

    /// Create a disabled menu item.
    pub fn disabled(label: &'static str) -> Self {
        Self {
            label,
            on_click: Callback::default(),
            disabled: true,
            separator_after: false,
        }
    }

    /// Add a separator after this item.
    pub fn with_separator(mut self) -> Self {
        self.separator_after = true;
        self
    }
}

/// Right-click context menu.
///
/// Wraps children and shows a popup menu on right-click. The menu is
/// dismissed by clicking outside or selecting an item.
#[component]
pub fn ContextMenu(items: Vec<MenuItem>, children: Element) -> Element {
    let t = use_theme();
    let t = *t.read();
    let mut menu_pos = use_signal(|| None::<(f64, f64)>);
    let mut hovered_idx = use_signal(|| None::<usize>);

    let pos = *menu_pos.read();

    rsx! {
        div {
            style: "display:contents;",
            onmousedown: move |evt: MouseEvent| {
                if evt.trigger_button() == Some(MouseButton::Secondary) {
                    let coords = evt.client_coordinates();
                    menu_pos.set(Some((coords.x, coords.y)));
                }
            },

            {children}
        }

        if let Some((x, y)) = pos {
            // Click-outside overlay
            div {
                style: "position:fixed; inset:0; z-index:9999;",
                onclick: move |_| menu_pos.set(None),
            }

            // Menu
            div {
                style: format!(
                    "position:fixed; left:{x}px; top:{y}px; z-index:10000; \
                     min-width:140px; \
                     background:{BG}; border:1px solid {BORDER}; \
                     border-radius:{RADIUS}; padding:4px 0; \
                     box-shadow:{SHADOW};",
                    BG = t.card_bg,
                    BORDER = t.border,
                    RADIUS = t.radius_button,
                    SHADOW = t.shadow_raised,
                ),

                for (idx, item) in items.iter().enumerate() {
                    {
                        let is_hovered = *hovered_idx.read() == Some(idx) && !item.disabled;
                        let item_bg = if is_hovered { t.surface_hover } else { "transparent" };
                        let item_color = if item.disabled { t.text_dim } else { t.text };
                        let item_opacity = if item.disabled { "0.4" } else { "1.0" };
                        let cursor = if item.disabled { "default" } else { "pointer" };
                        let separator = item.separator_after;
                        let disabled = item.disabled;
                        let on_click = item.on_click;

                        rsx! {
                            div {
                                key: "{idx}",
                                style: format!(
                                    "padding:4px 12px; cursor:{cursor}; \
                                     font-size:{FSIZE}; color:{item_color}; \
                                     background:{item_bg}; opacity:{item_opacity};",
                                    FSIZE = t.font_size_value,
                                ),
                                onmouseenter: move |_| {
                                    if !disabled {
                                        hovered_idx.set(Some(idx));
                                    }
                                },
                                onmouseleave: move |_| hovered_idx.set(None),
                                onclick: move |_| {
                                    if !disabled {
                                        on_click.call(());
                                        menu_pos.set(None);
                                    }
                                },
                                "{item.label}"
                            }

                            if separator {
                                div {
                                    style: format!(
                                        "height:1px; background:{BORDER}; margin:4px 0;",
                                        BORDER = t.border,
                                    ),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
