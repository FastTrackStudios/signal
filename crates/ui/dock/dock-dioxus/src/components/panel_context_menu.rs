//! Panel context menu overlay.

use crate::hooks::use_dock_actions;
use crate::prelude::*;
use crate::signals::{DOCK_CONTEXT_MENU, DOCK_MAXIMIZED_PANEL, DOCK_WORKSPACE};
use dock_proto::{DropZone, PanelId, SplitDirection};

#[component]
pub fn PanelContextMenuOverlay() -> Element {
    let Some(state) = *DOCK_CONTEXT_MENU.read() else {
        return rsx! {};
    };
    let actions = use_dock_actions();
    let mut show_move_submenu = use_signal(|| false);

    let is_maximized = *DOCK_MAXIMIZED_PANEL.read() == Some(state.panel_id);
    let auto_hide = {
        let workspace = DOCK_WORKSPACE.read();
        workspace.windows.values().any(|window| {
            window.layout.get_node(state.node_id).is_some()
                && matches!(
                    window.layout.zone_mode(state.node_id),
                    dock_proto::DockZoneMode::AutoHide
                )
        })
    };

    let close_menu = move || {
        *DOCK_CONTEXT_MENU.write() = None;
    };

    rsx! {
        div {
            class: "fixed inset-0 z-[200]",
            tabindex: "0",
            autofocus: true,
            onkeydown: move |e: KeyboardEvent| {
                if matches!(e.key(), Key::Escape) {
                    close_menu();
                }
            },
            onclick: move |_| close_menu(),

            div {
                class: "absolute min-w-[220px] rounded-md border border-zinc-700 bg-zinc-900 py-1 text-xs text-zinc-200 shadow-xl",
                style: "left:{state.x}px; top:{state.y}px;",
                onclick: move |e| e.stop_propagation(),

                MenuItem {
                    label: "Split Right".to_string(),
                    on_click: move || {
                        actions
                            .split_tile
                            .call((state.node_id, SplitDirection::Horizontal, default_split_panel(state.panel_id)));
                        close_menu();
                    }
                }
                MenuItem {
                    label: "Split Down".to_string(),
                    on_click: move || {
                        actions
                            .split_tile
                            .call((state.node_id, SplitDirection::Vertical, default_split_panel(state.panel_id)));
                        close_menu();
                    }
                }

                MenuItem {
                    label: if is_maximized { "Restore".to_string() } else { "Maximize".to_string() },
                    on_click: move || {
                        actions.toggle_maximize.call(state.panel_id);
                        close_menu();
                    }
                }
                MenuItem {
                    label: "Close".to_string(),
                    on_click: move || {
                        actions.close_tile.call(state.node_id);
                        close_menu();
                    }
                }

                Separator {}
                MenuItem {
                    label: if auto_hide { "Pin".to_string() } else { "Auto-hide".to_string() },
                    on_click: move || {
                        actions.toggle_auto_hide.call(state.node_id);
                        close_menu();
                    }
                }
                MenuItem {
                    label: "Float in New Window".to_string(),
                    on_click: move || {
                        actions.float_panel.call(state.panel_id);
                        close_menu();
                    }
                }

                Separator {}
                div {
                    class: "relative",
                    button {
                        class: "w-full px-3 py-1.5 text-left hover:bg-zinc-800 transition-colors",
                        onmouseenter: move |_| show_move_submenu.set(true),
                        onmouseleave: move |_| show_move_submenu.set(false),
                        onclick: move |_| show_move_submenu.set(!show_move_submenu()),
                        "Move to..."
                    }
                    if show_move_submenu() {
                        div {
                            class: "absolute left-full top-0 ml-1 min-w-[140px] rounded-md border border-zinc-700 bg-zinc-900 py-1 shadow-lg",
                            MenuItem {
                                label: "Left".to_string(),
                                on_click: move || {
                                    actions.move_panel_to_zone.call((state.panel_id, DropZone::Left));
                                    close_menu();
                                }
                            }
                            MenuItem {
                                label: "Right".to_string(),
                                on_click: move || {
                                    actions.move_panel_to_zone.call((state.panel_id, DropZone::Right));
                                    close_menu();
                                }
                            }
                            MenuItem {
                                label: "Top".to_string(),
                                on_click: move || {
                                    actions.move_panel_to_zone.call((state.panel_id, DropZone::Top));
                                    close_menu();
                                }
                            }
                            MenuItem {
                                label: "Bottom".to_string(),
                                on_click: move || {
                                    actions.move_panel_to_zone.call((state.panel_id, DropZone::Bottom));
                                    close_menu();
                                }
                            }
                            MenuItem {
                                label: "Center".to_string(),
                                on_click: move || {
                                    actions.move_panel_to_zone.call((state.panel_id, DropZone::Center));
                                    close_menu();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Separator() -> Element {
    rsx! {
        div { class: "my-1 h-px bg-zinc-800" }
    }
}

#[derive(Props, Clone, PartialEq)]
struct MenuItemProps {
    label: String,
    on_click: EventHandler<()>,
}

#[component]
fn MenuItem(props: MenuItemProps) -> Element {
    rsx! {
        button {
            class: "w-full px-3 py-1.5 text-left hover:bg-zinc-800 transition-colors",
            onclick: move |_| props.on_click.call(()),
            "{props.label}"
        }
    }
}

fn default_split_panel(current: PanelId) -> PanelId {
    PanelId::all()
        .iter()
        .copied()
        .find(|p| *p != current)
        .unwrap_or(PanelId::Navigator)
}
