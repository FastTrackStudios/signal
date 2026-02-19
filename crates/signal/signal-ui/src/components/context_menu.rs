//! Context menu — right-click menu with positioned overlay.

use dioxus::prelude::*;

/// A single menu item.
#[derive(Clone, PartialEq)]
pub struct ContextMenuItem {
    pub id: String,
    pub label: String,
    pub shortcut: Option<String>,
    pub danger: bool,
    pub disabled: bool,
}

/// Context menu positioned at a click point.
#[derive(Props, Clone, PartialEq)]
pub struct ContextMenuProps {
    /// Menu items.
    items: Vec<ContextMenuItem>,

    /// Position (x, y) in pixels.
    position: (f64, f64),

    /// Whether the menu is visible.
    open: bool,

    /// Callback when an item is selected.
    #[props(default)]
    on_select: Option<Callback<String>>,

    /// Callback to close the menu.
    #[props(default)]
    on_close: Option<Callback<()>>,
}

#[component]
pub fn ContextMenu(props: ContextMenuProps) -> Element {
    if !props.open {
        return rsx! {};
    }

    let (x, y) = props.position;

    rsx! {
        // Backdrop (click to close)
        div {
            class: "fixed inset-0 z-50",
            onclick: move |_| {
                if let Some(cb) = &props.on_close {
                    cb.call(());
                }
            },

            // Menu panel
            div {
                class: "absolute min-w-[160px] py-1 rounded-lg border border-border bg-popover shadow-lg",
                style: "left: {x:.0}px; top: {y:.0}px;",
                onclick: move |evt| evt.stop_propagation(),

                for item in props.items.iter() {
                    {
                        let item_id = item.id.clone();
                        let is_disabled = item.disabled;
                        let text_class = if item.danger {
                            "text-destructive"
                        } else if item.disabled {
                            "text-muted-foreground opacity-50"
                        } else {
                            "text-popover-foreground"
                        };
                        rsx! {
                            button {
                                class: format!(
                                    "w-full flex items-center justify-between px-3 py-1.5 text-xs transition-colors {} {}",
                                    text_class,
                                    if is_disabled { "cursor-not-allowed" } else { "hover:bg-accent cursor-pointer" }
                                ),
                                disabled: is_disabled,
                                onclick: move |_| {
                                    if !is_disabled {
                                        if let Some(cb) = &props.on_select {
                                            cb.call(item_id.clone());
                                        }
                                        if let Some(cb) = &props.on_close {
                                            cb.call(());
                                        }
                                    }
                                },
                                span { "{item.label}" }
                                if let Some(shortcut) = &item.shortcut {
                                    span {
                                        class: "text-[10px] text-muted-foreground ml-4",
                                        "{shortcut}"
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

/// Separator between context menu items.
#[component]
pub fn ContextMenuSeparator() -> Element {
    rsx! {
        div { class: "my-1 border-t border-border" }
    }
}
