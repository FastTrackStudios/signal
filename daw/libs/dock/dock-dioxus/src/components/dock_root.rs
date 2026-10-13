//! Root dock layout component.
//!
//! Reads the global `DOCK_WORKSPACE` signal and renders one window's
//! split/tile tree. Handles maximized panels and global drag state.

use crate::components::split_pane::SplitPane;
use crate::components::tile_pane::TilePane;
use crate::components::PanelContextMenuOverlay;
use crate::context::use_dock_context;
use crate::prelude::*;
use crate::signals::*;
use crate::transition::TransitionKind;
use dock_proto::{DockLayout, FlatNode, NodeId, WindowId};

/// Root dock layout component.
#[component]
pub fn DockRoot(window_id: Option<WindowId>) -> Element {
    let (window_id, layout) = {
        let workspace = DOCK_WORKSPACE.read();
        let resolved_window = window_id.unwrap_or(workspace.main_window);
        let layout = workspace
            .windows
            .get(&resolved_window)
            .map(|w| w.layout.clone())
            .or_else(|| {
                workspace
                    .windows
                    .get(&workspace.main_window)
                    .map(|w| w.layout.clone())
            })
            .unwrap_or_else(DockLayout::empty);
        (resolved_window, layout)
    };
    let maximized = *DOCK_MAXIMIZED_PANEL.read();
    let is_dragging = DOCK_DRAG_STATE.read().is_dragging();
    let is_resizing = DOCK_RESIZING.read().is_some();
    let transition = DOCK_LAYOUT_TRANSITION.read().clone();
    let transition_for_effect = transition.clone();
    let mut transition_started = use_signal(|| false);
    use_context_provider(move || window_id);

    use_effect(move || {
        if transition_for_effect.is_some() {
            transition_started.set(false);
            spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(16)).await;
                transition_started.set(true);
            });
        } else {
            transition_started.set(false);
        }
    });

    // Global classes during drag/resize operations
    let drag_class = if is_dragging || is_resizing {
        "select-none"
    } else {
        ""
    };

    // If a panel is maximized, render it full-screen
    if let Some(panel_id) = maximized {
        let ctx = use_dock_context();
        return rsx! {
            div {
                class: "h-full w-full relative",
                {ctx.render_panel.render(panel_id)}
                // Restore button (top-right corner)
                button {
                    class: "absolute top-2 right-2 z-50 px-2.5 py-1 bg-muted/90 text-muted-foreground rounded-md text-xs hover:bg-accent hover:text-foreground transition-colors shadow-lg backdrop-blur-sm",
                    onclick: move |_| {
                        *DOCK_MAXIMIZED_PANEL.write() = None;
                    },
                    "\u{25A3} Restore" // dotted square + text
                }
            }
        };
    }

    // Normal mode: render the tree
    rsx! {
        div {
            class: "h-full w-full {drag_class} relative",
            tabindex: "0",
            onkeydown: move |e: KeyboardEvent| {
                if matches!(e.key(), Key::Escape) && DOCK_DRAG_STATE.read().is_dragging() {
                    *DOCK_DRAG_DROPPED.write() = false;
                    crate::drag_ghost::animate_cancel_back();
                    DOCK_DRAG_STATE.write().end();
                    e.prevent_default();
                }
            },
            if let Some(root_id) = layout.root() {
                DockNodeRenderer { node_id: root_id, window_id }
            } else {
                div { class: "h-full w-full flex items-center justify-center text-muted-foreground",
                    "Empty layout — add a panel to get started"
                }
            }

            if let Some(t) = transition {
                div {
                    class: "absolute inset-0 pointer-events-none z-40",
                    for panel_anim in t.panels {
                        {
                            let (x, y, w, h, opacity, scale) = match panel_anim.kind {
                                TransitionKind::Persisted => {
                                    if transition_started() {
                                        (
                                            panel_anim.to_rect.x_pct,
                                            panel_anim.to_rect.y_pct,
                                            panel_anim.to_rect.w_pct,
                                            panel_anim.to_rect.h_pct,
                                            1.0,
                                            1.0,
                                        )
                                    } else {
                                        (
                                            panel_anim.from_rect.x_pct,
                                            panel_anim.from_rect.y_pct,
                                            panel_anim.from_rect.w_pct,
                                            panel_anim.from_rect.h_pct,
                                            1.0,
                                            1.0,
                                        )
                                    }
                                }
                                TransitionKind::Added => {
                                    if transition_started() {
                                        (
                                            panel_anim.to_rect.x_pct,
                                            panel_anim.to_rect.y_pct,
                                            panel_anim.to_rect.w_pct,
                                            panel_anim.to_rect.h_pct,
                                            1.0,
                                            1.0,
                                        )
                                    } else {
                                        (
                                            panel_anim.to_rect.x_pct,
                                            panel_anim.to_rect.y_pct,
                                            panel_anim.to_rect.w_pct,
                                            panel_anim.to_rect.h_pct,
                                            0.0,
                                            0.96,
                                        )
                                    }
                                }
                                TransitionKind::Removed => {
                                    if transition_started() {
                                        (
                                            panel_anim.from_rect.x_pct,
                                            panel_anim.from_rect.y_pct,
                                            panel_anim.from_rect.w_pct,
                                            panel_anim.from_rect.h_pct,
                                            0.0,
                                            0.96,
                                        )
                                    } else {
                                        (
                                            panel_anim.from_rect.x_pct,
                                            panel_anim.from_rect.y_pct,
                                            panel_anim.from_rect.w_pct,
                                            panel_anim.from_rect.h_pct,
                                            1.0,
                                            1.0,
                                        )
                                    }
                                }
                            };

                            let style = format!(
                                "left:{x}%; top:{y}%; width:{w}%; height:{h}%; opacity:{opacity}; transform: scale({scale}); transition: all {}ms ease;",
                                panel_anim.duration_ms
                            );

                            rsx! {
                                div {
                                    key: "{panel_anim.panel.as_str()}",
                                    class: "absolute rounded-md border border-blue-500/60 bg-blue-500/15 shadow-lg backdrop-blur-[1px]",
                                    style: "{style}",
                                }
                            }
                        }
                    }
                }
            }

            PanelContextMenuOverlay {}
        }
    }
}

/// Recursive node renderer — dispatches to SplitPane or TilePane.
#[component]
fn DockNodeRenderer(node_id: NodeId, window_id: WindowId) -> Element {
    let layout = {
        let workspace = DOCK_WORKSPACE.read();
        workspace
            .windows
            .get(&window_id)
            .map(|w| w.layout.clone())
            .or_else(|| {
                workspace
                    .windows
                    .get(&workspace.main_window)
                    .map(|w| w.layout.clone())
            })
            .unwrap_or_else(DockLayout::empty)
    };

    match layout.get_node(node_id) {
        Some(FlatNode::Split {
            direction,
            ratio,
            first,
            second,
        }) => {
            let direction = *direction;
            let ratio = *ratio;
            let first = *first;
            let second = *second;
            rsx! {
                SplitPane {
                    node_id,
                    direction,
                    ratio,
                    first: rsx! { DockNodeRenderer { node_id: first, window_id } },
                    second: rsx! { DockNodeRenderer { node_id: second, window_id } },
                }
            }
        }
        Some(FlatNode::Tile { tabs, .. }) => {
            let tabs = tabs.clone();
            rsx! { TilePane { node_id, tabs } }
        }
        None => rsx! {
            div { class: "h-full w-full bg-background flex items-center justify-center text-muted-foreground",
                "Missing node"
            }
        },
    }
}
