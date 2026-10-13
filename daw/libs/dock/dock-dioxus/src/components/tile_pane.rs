//! Tile pane — content wrapper with optional tab bar, panel header,
//! and drag-and-drop drop zone overlays.

use crate::components::tab_bar::DockTabBar;
use crate::context::use_dock_context;
use crate::hooks::use_dock_actions;
use crate::prelude::*;
use crate::signals::*;
use dock_proto::{DropZone, NodeId, TabGroup};

/// Tile pane component with drop zone support.
#[component]
pub fn TilePane(node_id: NodeId, tabs: TabGroup) -> Element {
    let ctx = use_dock_context();
    let actions = use_dock_actions();
    let drag_state = DOCK_DRAG_STATE.read();
    let is_drag_active = drag_state.is_dragging();
    let is_hover_target = drag_state.hover_target == Some(node_id);
    let active_zone = if is_hover_target {
        drag_state.hover_zone
    } else {
        None
    };

    let active_panel = tabs.active_panel();

    rsx! {
        div {
            id: "dock-tile-{node_id}",
            class: "flex flex-col h-full w-full bg-background border border-border rounded-sm overflow-hidden relative",

            ondragover: {
                move |e: DragEvent| {
                    e.prevent_default();
                    if !DOCK_DRAG_STATE.read().is_dragging() {
                        return;
                    }
                    let coords = e.element_coordinates();
                    // Approximate tile dimensions from element coords
                    // The element_coordinates give position within this div
                    let rel_x = if coords.x > 0.0 { (coords.x / 300.0).clamp(0.0, 1.0) } else { 0.5 };
                    let rel_y = if coords.y > 0.0 { (coords.y / 200.0).clamp(0.0, 1.0) } else { 0.5 };

                    if let Some(zone) = DropZone::from_relative_position(rel_x, rel_y) {
                        DOCK_DRAG_STATE.write().update_hover(node_id, zone);
                    }
                }
            },

            ondragleave: move |_| {
                let mut state = DOCK_DRAG_STATE.write();
                if state.hover_target == Some(node_id) {
                    state.clear_hover();
                }
            },

            ondrop: {
                let actions = actions.clone();
                move |e: DragEvent| {
                    e.prevent_default();
                    let state = DOCK_DRAG_STATE.read().clone();
                    if let (Some(_panel), Some(zone)) = (state.dragging_panel, state.hover_zone) {
                        actions.drop_panel.call((node_id, zone));
                    } else {
                        actions.cancel_drag.call(());
                    }
                }
            },

            // Tab bar (only shown when multiple panels in this tile)
            if tabs.has_tabs() {
                DockTabBar { node_id, tabs: tabs.clone() }
            }

            // Panel content
            div {
                class: "flex-1 overflow-auto",
                if let Some(panel_id) = active_panel {
                    {ctx.render_panel.render(panel_id)}
                } else {
                    div { class: "flex items-center justify-center h-full text-muted-foreground",
                        "Empty tile"
                    }
                }
            }

            // Drop zone overlays (shown during drag)
            if is_drag_active {
                DropZoneOverlays { active_zone }
            }
        }
    }
}

/// Visual overlays showing where a panel will be dropped.
#[component]
fn DropZoneOverlays(active_zone: Option<DropZone>) -> Element {
    let dragged_panel_name = DOCK_DRAG_STATE
        .read()
        .dragging_panel
        .map(|p| p.display_name().to_string())
        .unwrap_or_else(|| "Panel".to_string());
    let zones = [
        (DropZone::Top, "top-0 left-0 right-0 h-[30%]"),
        (DropZone::Bottom, "bottom-0 left-0 right-0 h-[30%]"),
        (DropZone::Left, "top-0 bottom-0 left-0 w-[30%]"),
        (DropZone::Right, "top-0 bottom-0 right-0 w-[30%]"),
        (
            DropZone::Center,
            "top-[30%] bottom-[30%] left-[30%] right-[30%]",
        ),
    ];

    rsx! {
        for (zone, position_class) in zones {
            {
                let is_active = active_zone == Some(zone);
                let bg = if is_active {
                    "bg-blue-500/30 border-2 border-dashed border-blue-500/80"
                } else {
                    "bg-blue-500/10 border border-dashed border-blue-500/30"
                };
                rsx! {
                    div {
                        class: "absolute {position_class} {bg} pointer-events-none rounded transition-all duration-150 z-10",
                    }
                }
            }
        }

        if active_zone == Some(DropZone::Center) {
            div {
                class: "absolute z-20 left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 rounded-md border border-blue-400/80 bg-blue-950/80 px-3 py-1.5 text-[11px] text-blue-100 shadow-lg pointer-events-none",
                "Merge tab: {dragged_panel_name}"
            }
        }

        if let Some(zone) = active_zone {
            if zone != DropZone::Center {
                div {
                    class: "absolute z-20 bg-blue-400/90 pointer-events-none animate-pulse",
                    style: match zone {
                        DropZone::Left => "left:30%; top:6%; bottom:6%; width:2px;",
                        DropZone::Right => "right:30%; top:6%; bottom:6%; width:2px;",
                        DropZone::Top => "top:30%; left:6%; right:6%; height:2px;",
                        DropZone::Bottom => "bottom:30%; left:6%; right:6%; height:2px;",
                        DropZone::Center => "",
                    },
                }
            }
        }
    }
}
