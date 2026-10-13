//! Panel header — compact header bar with drag handle and action controls.
//!
//! The header is draggable (starts a panel drag for rearrangement) and
//! includes buttons for splitting, maximizing, and closing tiles.
//! The split buttons show a panel picker dropdown.

use crate::hooks::use_dock_actions;
use crate::prelude::*;
use crate::signals::{ContextMenuState, DOCK_CONTEXT_MENU};
use dock_proto::{NodeId, PanelId, SplitDirection};

/// Panel header component with drag handle and controls.
#[component]
pub fn PanelHeader(node_id: NodeId, panel_id: PanelId) -> Element {
    let actions = use_dock_actions();
    let mut show_panel_menu = use_signal(|| false);
    let mut menu_direction = use_signal(|| SplitDirection::Horizontal);
    let drag_source_id = format!("dock-drag-source-header-{node_id}-{}", panel_id.as_str());

    rsx! {
        div {
            id: "{drag_source_id}",
            class: "flex items-center justify-between px-2 py-1 bg-zinc-950 border-b border-zinc-800 text-xs select-none cursor-grab active:cursor-grabbing",

            // Make the header draggable
            draggable: "true",
            ondragstart: {
                let actions = actions.clone();
                let drag_source_id = drag_source_id.clone();
                move |_: DragEvent| {
                    actions.start_drag.call((panel_id, node_id));
                    crate::drag_ghost::start_drag_ghost(&drag_source_id, panel_id.display_name());
                }
            },
            ondragend: {
                let actions = actions.clone();
                move |_: DragEvent| {
                    actions.cancel_drag.call(());
                }
            },
            oncontextmenu: move |e: MouseEvent| {
                e.prevent_default();
                let coords = e.client_coordinates();
                *DOCK_CONTEXT_MENU.write() = Some(ContextMenuState {
                    node_id,
                    panel_id,
                    x: coords.x,
                    y: coords.y,
                });
            },

            // Panel name (left side)
            div {
                class: "flex items-center gap-1.5 text-zinc-400 font-medium truncate",
                // Drag grip indicator
                span {
                    class: "text-zinc-600 text-[10px]",
                    "\u{2630}" // hamburger menu icon as grip
                }
                span { "{panel_id.display_name()}" }
            }

            // Action buttons (right side)
            div {
                class: "flex items-center gap-0.5 text-zinc-600 relative",

                // Split horizontal
                button {
                    class: "hover:text-zinc-300 hover:bg-zinc-800 px-1.5 py-0.5 rounded transition-colors",
                    title: "Split Horizontal",
                    onclick: {
                        move |e: MouseEvent| {
                            e.stop_propagation();
                            menu_direction.set(SplitDirection::Horizontal);
                            show_panel_menu.set(!show_panel_menu());
                        }
                    },
                    "\u{2502}" // vertical bar
                }

                // Split vertical
                button {
                    class: "hover:text-zinc-300 hover:bg-zinc-800 px-1.5 py-0.5 rounded transition-colors",
                    title: "Split Vertical",
                    onclick: {
                        move |e: MouseEvent| {
                            e.stop_propagation();
                            menu_direction.set(SplitDirection::Vertical);
                            show_panel_menu.set(!show_panel_menu());
                        }
                    },
                    "\u{2500}" // horizontal bar
                }

                // Maximize
                button {
                    class: "hover:text-zinc-300 hover:bg-zinc-800 px-1.5 py-0.5 rounded transition-colors",
                    title: "Maximize",
                    onclick: {
                        let actions = actions.clone();
                        move |e: MouseEvent| {
                            e.stop_propagation();
                            actions.toggle_maximize.call(panel_id);
                        }
                    },
                    "\u{25A1}" // white square
                }

                // Close
                button {
                    class: "hover:text-red-400 hover:bg-zinc-800 px-1.5 py-0.5 rounded transition-colors",
                    title: "Close",
                    onclick: {
                        let actions = actions.clone();
                        move |e: MouseEvent| {
                            e.stop_propagation();
                            actions.close_tile.call(node_id);
                        }
                    },
                    "\u{2715}" // multiplication sign (x)
                }

                // Panel selection dropdown
                if show_panel_menu() {
                    PanelPickerMenu {
                        node_id,
                        direction: menu_direction(),
                        on_close: move || show_panel_menu.set(false),
                    }
                }
            }
        }
    }
}

/// Dropdown menu for selecting which panel to add when splitting.
#[component]
fn PanelPickerMenu(
    node_id: NodeId,
    direction: SplitDirection,
    on_close: EventHandler<()>,
) -> Element {
    let actions = use_dock_actions();

    rsx! {
        // Backdrop to close menu on click outside
        div {
            class: "fixed inset-0 z-40",
            onclick: move |_| on_close.call(()),
        }
        // Menu dropdown
        div {
            class: "absolute right-0 top-full mt-1 z-50 bg-zinc-900 border border-zinc-700 rounded-md shadow-lg py-1 min-w-[160px]",

            div {
                class: "px-2 py-1 text-zinc-500 text-[10px] uppercase tracking-wider",
                if direction == SplitDirection::Horizontal {
                    "Split Right"
                } else {
                    "Split Below"
                }
            }

            for panel in PanelId::all() {
                {
                    let p = *panel;
                    rsx! {
                        button {
                            class: "w-full text-left px-3 py-1.5 text-xs text-zinc-300 hover:bg-zinc-800 hover:text-zinc-100 transition-colors",
                            onclick: {
                                let actions = actions.clone();
                                move |_| {
                                    actions.split_tile.call((node_id, direction, p));
                                    on_close.call(());
                                }
                            },
                            "{p.display_name()}"
                        }
                    }
                }
            }
        }
    }
}
