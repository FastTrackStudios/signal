//! Hooks for dock workspace manipulation.
//!
//! Provides `DockActions` — callbacks that UI components use to mutate
//! workspace/layout state, switch presets, and manage persistence.

use crate::context::DockProvider;
use crate::prelude::*;
use crate::signals::*;
use dock_proto::*;
use tokio::time::{sleep, Duration};

#[cfg(feature = "native")]
use dioxus::desktop::{
    tao::{
        dpi::{LogicalPosition, LogicalSize},
        event::{Event as TaoEvent, WindowEvent},
        window::WindowBuilder,
    },
    Config,
};

/// Collection of dock action callbacks for UI components.
#[derive(Clone)]
pub struct DockActions {
    pub split_tile: Callback<(NodeId, SplitDirection, PanelId)>,
    pub close_tile: Callback<NodeId>,
    pub update_split_ratio: Callback<(NodeId, f64)>,
    pub switch_tab: Callback<(NodeId, usize)>,
    pub add_tab: Callback<(NodeId, PanelId)>,
    pub remove_tab: Callback<(NodeId, PanelId)>,
    pub load_preset: Callback<usize>,
    pub save_current_preset: Callback<()>,
    pub save_as_new_preset: Callback<String>,
    pub toggle_maximize: Callback<PanelId>,
    pub start_drag: Callback<(PanelId, NodeId)>,
    pub drop_panel: Callback<(NodeId, DropZone)>,
    pub cancel_drag: Callback<()>,
    pub persist_presets: Callback<()>,
    pub toggle_auto_hide: Callback<NodeId>,
    pub float_panel: Callback<PanelId>,
    pub move_panel_to_zone: Callback<(PanelId, DropZone)>,
}

fn main_layout_clone() -> Option<DockLayout> {
    let ws = DOCK_WORKSPACE.read();
    ws.windows.get(&ws.main_window).map(|w| w.layout.clone())
}

#[allow(deprecated)]
fn sync_main_layout_signal(workspace: &DockWorkspace) {
    if let Some(main) = workspace.windows.get(&workspace.main_window) {
        *DOCK_LAYOUT.write() = main.layout.clone();
    }
}

fn window_for_node(workspace: &DockWorkspace, node_id: NodeId) -> Option<WindowId> {
    workspace
        .windows
        .iter()
        .find_map(|(window_id, window)| window.layout.get_node(node_id).map(|_| *window_id))
}

fn current_window_hint(workspace: &DockWorkspace) -> WindowId {
    if let Some(window_id) = try_use_context::<WindowId>() {
        if workspace.windows.contains_key(&window_id) {
            return window_id;
        }
    }
    workspace.main_window
}

#[cfg(feature = "native")]
#[component]
fn FloatingWindowRoot(window_id: WindowId) -> Element {
    dioxus::desktop::use_wry_event_handler(move |event, _| {
        if let TaoEvent::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            let mut workspace = DOCK_WORKSPACE.write();
            if workspace.close_window(window_id) {
                sync_main_layout_signal(&workspace);
            }
            DOCK_OPENED_FLOATING_WINDOWS.write().remove(&window_id);
        }
    });

    use_effect(move || {
        let exists = DOCK_WORKSPACE.read().windows.contains_key(&window_id);
        if !exists {
            dioxus::desktop::window().close();
        }
    });

    let Some(renderer) = DOCK_PANEL_RENDERER.read().clone() else {
        return rsx! {
            div { class: "h-full w-full flex items-center justify-center text-muted-foreground",
                "Dock panel renderer unavailable"
            }
        };
    };

    rsx! {
        DockProvider { render_panel: renderer,
            div { class: "h-full w-full",
                crate::components::DockRoot { window_id: Some(window_id) }
            }
        }
    }
}

#[cfg(feature = "native")]
fn open_floating_window(window: DockWindow) {
    if window.is_main {
        return;
    }

    if !DOCK_OPENED_FLOATING_WINDOWS.write().insert(window.id) {
        return;
    }

    spawn(async move {
        let dom = VirtualDom::new_with_props(
            FloatingWindowRoot,
            FloatingWindowRootProps {
                window_id: window.id,
            },
        );

        let builder = WindowBuilder::new()
            .with_title(window.title)
            .with_inner_size(LogicalSize::new(window.bounds.width, window.bounds.height))
            .with_position(LogicalPosition::new(window.bounds.x, window.bounds.y));

        let config = Config::new()
            .with_window(builder)
            .with_exits_when_last_window_closes(false);

        let _ = dioxus::desktop::window().new_window(dom, config).await;
    });
}

#[cfg(not(feature = "native"))]
fn open_floating_window(_window: DockWindow) {}

fn ensure_floating_windows_open() {
    let floating_windows = {
        let workspace = DOCK_WORKSPACE.read();
        workspace
            .windows
            .values()
            .filter(|w| !w.is_main)
            .cloned()
            .collect::<Vec<_>>()
    };

    for window in floating_windows {
        open_floating_window(window);
    }

    let alive = {
        let workspace = DOCK_WORKSPACE.read();
        workspace
            .windows
            .keys()
            .copied()
            .collect::<std::collections::HashSet<_>>()
    };
    DOCK_OPENED_FLOATING_WINDOWS
        .write()
        .retain(|window_id| alive.contains(window_id));
}

/// Hook providing dock layout action callbacks.
pub fn use_dock_actions() -> DockActions {
    DockActions {
        split_tile: Callback::new(
            move |(node_id, direction, panel): (NodeId, SplitDirection, PanelId)| {
                let mut workspace = DOCK_WORKSPACE.write();
                let window_id = window_for_node(&workspace, node_id)
                    .unwrap_or_else(|| current_window_hint(&workspace));
                if let Some(window) = workspace.windows.get_mut(&window_id) {
                    window.layout.split_tile(node_id, direction, panel, 50.0);
                }
                sync_main_layout_signal(&workspace);
            },
        ),
        close_tile: Callback::new(move |node_id: NodeId| {
            let mut workspace = DOCK_WORKSPACE.write();
            let window_id = window_for_node(&workspace, node_id)
                .unwrap_or_else(|| current_window_hint(&workspace));

            if let Some(window) = workspace.windows.get_mut(&window_id) {
                window.layout.close_tile(node_id);
            }

            if window_id != workspace.main_window
                && workspace
                    .windows
                    .get(&window_id)
                    .is_some_and(|w| w.layout.is_empty())
            {
                workspace.close_window(window_id);
                DOCK_OPENED_FLOATING_WINDOWS.write().remove(&window_id);
            }

            sync_main_layout_signal(&workspace);
        }),
        update_split_ratio: Callback::new(move |(node_id, ratio): (NodeId, f64)| {
            let mut workspace = DOCK_WORKSPACE.write();
            let window_id = window_for_node(&workspace, node_id)
                .unwrap_or_else(|| current_window_hint(&workspace));
            if let Some(window) = workspace.windows.get_mut(&window_id) {
                window.layout.update_split_ratio(node_id, ratio);
            }
            sync_main_layout_signal(&workspace);
        }),
        switch_tab: Callback::new(move |(node_id, index): (NodeId, usize)| {
            let mut workspace = DOCK_WORKSPACE.write();
            let window_id = window_for_node(&workspace, node_id)
                .unwrap_or_else(|| current_window_hint(&workspace));
            if let Some(window) = workspace.windows.get_mut(&window_id) {
                if let Some(FlatNode::Tile { tabs, .. }) = window.layout.get_node_mut(node_id) {
                    tabs.set_active(index);
                }
            }
            sync_main_layout_signal(&workspace);
        }),
        add_tab: Callback::new(move |(node_id, panel): (NodeId, PanelId)| {
            let mut workspace = DOCK_WORKSPACE.write();
            let window_id = window_for_node(&workspace, node_id)
                .unwrap_or_else(|| current_window_hint(&workspace));
            if let Some(window) = workspace.windows.get_mut(&window_id) {
                if let Some(FlatNode::Tile { tabs, .. }) = window.layout.get_node_mut(node_id) {
                    tabs.add_panel(panel);
                }
            }
            sync_main_layout_signal(&workspace);
        }),
        remove_tab: Callback::new(move |(node_id, panel): (NodeId, PanelId)| {
            let mut workspace = DOCK_WORKSPACE.write();
            let window_id = window_for_node(&workspace, node_id)
                .unwrap_or_else(|| current_window_hint(&workspace));
            if let Some(window) = workspace.windows.get_mut(&window_id) {
                if let Some(FlatNode::Tile { tabs, .. }) = window.layout.get_node_mut(node_id) {
                    tabs.remove_panel(panel);
                }
            }

            if window_id != workspace.main_window
                && workspace
                    .windows
                    .get(&window_id)
                    .is_some_and(|w| w.layout.is_empty())
            {
                workspace.close_window(window_id);
                DOCK_OPENED_FLOATING_WINDOWS.write().remove(&window_id);
            }

            sync_main_layout_signal(&workspace);
        }),
        load_preset: Callback::new(move |index: usize| {
            let Some(current_layout) = main_layout_clone() else {
                return;
            };
            let current_index = *DOCK_ACTIVE_PRESET_INDEX.read();
            {
                let mut presets = DOCK_PRESETS.write();
                if let Some(departing) = presets.presets.get_mut(current_index) {
                    departing.layout = current_layout.clone();
                }
            }

            let target_layout = {
                let presets = DOCK_PRESETS.read();
                presets
                    .presets
                    .get(index)
                    .map(|preset| preset.layout.clone())
            };

            if let Some(target_layout) = target_layout {
                let duration_ms = *DOCK_TRANSITION_DURATION_MS.read();
                let transition = crate::transition::compute_transition(
                    &current_layout,
                    &target_layout,
                    duration_ms,
                );

                if transition.is_empty() {
                    let main_window = DOCK_WORKSPACE.read().main_window;
                    let mut workspace = DOCK_WORKSPACE.write();
                    if let Some(main) = workspace.windows.get_mut(&main_window) {
                        main.layout = target_layout;
                    }
                    sync_main_layout_signal(&workspace);
                    *DOCK_ACTIVE_PRESET_INDEX.write() = index;
                    *DOCK_LAYOUT_TRANSITION.write() = None;
                    return;
                }

                *DOCK_LAYOUT_TRANSITION.write() = Some(transition);
                spawn(async move {
                    sleep(Duration::from_millis(duration_ms)).await;
                    let main_window = DOCK_WORKSPACE.read().main_window;
                    let mut workspace = DOCK_WORKSPACE.write();
                    if let Some(main) = workspace.windows.get_mut(&main_window) {
                        main.layout = target_layout;
                    }
                    sync_main_layout_signal(&workspace);
                    *DOCK_ACTIVE_PRESET_INDEX.write() = index;
                    *DOCK_LAYOUT_TRANSITION.write() = None;
                });
            }
        }),
        save_current_preset: Callback::new(move |_: ()| {
            let Some(layout) = main_layout_clone() else {
                return;
            };
            let mut presets = DOCK_PRESETS.write();
            presets.save_active(layout);
        }),
        save_as_new_preset: Callback::new(move |name: String| {
            let Some(layout) = main_layout_clone() else {
                return;
            };
            let preset = DockPreset::new(name, layout);
            let mut presets = DOCK_PRESETS.write();
            presets.add_preset(preset);
            let new_index = presets.presets.len() - 1;
            presets.set_active(new_index);
            *DOCK_ACTIVE_PRESET_INDEX.write() = new_index;
        }),
        toggle_maximize: Callback::new(move |panel: PanelId| {
            let current = *DOCK_MAXIMIZED_PANEL.read();
            if current == Some(panel) {
                *DOCK_MAXIMIZED_PANEL.write() = None;
            } else {
                *DOCK_MAXIMIZED_PANEL.write() = Some(panel);
            }
        }),
        start_drag: Callback::new(move |(panel, source): (PanelId, NodeId)| {
            *DOCK_DRAG_DROPPED.write() = false;
            DOCK_DRAG_STATE.write().start(panel, source);
        }),
        drop_panel: Callback::new(move |(target_node, zone): (NodeId, DropZone)| {
            let drag = DOCK_DRAG_STATE.read().clone();
            if let Some(panel) = drag.dragging_panel {
                let mut workspace = DOCK_WORKSPACE.write();

                let source_window = drag
                    .source_node
                    .and_then(|node| window_for_node(&workspace, node))
                    .or_else(|| workspace.find_panel(panel).map(|(window, _)| window))
                    .unwrap_or(workspace.main_window);
                let target_window = window_for_node(&workspace, target_node)
                    .unwrap_or_else(|| current_window_hint(&workspace));

                let _ = workspace.dock_back(panel, source_window, target_window, target_node, zone);

                if source_window != workspace.main_window
                    && !workspace.windows.contains_key(&source_window)
                {
                    DOCK_OPENED_FLOATING_WINDOWS.write().remove(&source_window);
                }

                sync_main_layout_signal(&workspace);
                *DOCK_DRAG_DROPPED.write() = true;
                crate::drag_ghost::animate_drop_to(&format!("dock-tile-{target_node}"));
            }
            DOCK_DRAG_STATE.write().end();
        }),
        cancel_drag: Callback::new(move |_: ()| {
            if *DOCK_DRAG_DROPPED.read() {
                *DOCK_DRAG_DROPPED.write() = false;
            } else {
                crate::drag_ghost::animate_cancel_back();
            }
            DOCK_DRAG_STATE.write().end();
        }),
        persist_presets: Callback::new(move |_: ()| {
            let presets = DOCK_PRESETS.read();
            if let Some(path) = dock_proto::persistence::default_presets_path() {
                if let Err(e) = dock_proto::save_presets_to_file(&presets, &path) {
                    tracing::warn!("Failed to save dock presets: {e}");
                }
            }
        }),
        toggle_auto_hide: Callback::new(move |node_id: NodeId| {
            let mut workspace = DOCK_WORKSPACE.write();
            let window_id = window_for_node(&workspace, node_id)
                .unwrap_or_else(|| current_window_hint(&workspace));
            if let Some(window) = workspace.windows.get_mut(&window_id) {
                window.layout.toggle_auto_hide(node_id);
            }
            sync_main_layout_signal(&workspace);
        }),
        float_panel: Callback::new(move |panel: PanelId| {
            let mut workspace = DOCK_WORKSPACE.write();

            let Some((source_window, _)) = workspace.find_panel(panel) else {
                return;
            };

            let Some(source_bounds) = workspace
                .windows
                .get(&source_window)
                .map(|w| w.bounds.clone())
            else {
                return;
            };

            let bounds = WindowBounds::new(
                source_bounds.x + 40.0,
                source_bounds.y + 40.0,
                source_bounds.width.clamp(480.0, 1200.0),
                source_bounds.height.clamp(320.0, 900.0),
                source_bounds.monitor_name,
            );

            let new_window_id = workspace.tear_off(panel, source_window, bounds);

            let mut newly_created = workspace.windows.get(&new_window_id).cloned();

            if source_window != workspace.main_window
                && workspace
                    .windows
                    .get(&source_window)
                    .is_some_and(|w| w.layout.is_empty())
            {
                workspace.windows.remove(&source_window);
                DOCK_OPENED_FLOATING_WINDOWS.write().remove(&source_window);
                if new_window_id == source_window {
                    newly_created = None;
                }
            }

            sync_main_layout_signal(&workspace);
            drop(workspace);

            if let Some(window) = newly_created {
                open_floating_window(window);
            }
        }),
        move_panel_to_zone: Callback::new(move |(panel, zone): (PanelId, DropZone)| {
            let mut workspace = DOCK_WORKSPACE.write();
            if let Some((window_id, _)) = workspace.find_panel(panel) {
                if let Some(window) = workspace.windows.get_mut(&window_id) {
                    if let Some(root) = window.layout.root() {
                        window.layout.move_panel(panel, root, zone);
                    }
                }
            }
            sync_main_layout_signal(&workspace);
        }),
    }
}

// ─── Rig Dock Functions ─────────────────────────────────────────────────

/// Initialize the rig dock with its own window and presets.
///
/// Adds a rig window to `DOCK_WORKSPACE` and sets up rig-specific presets.
/// Call once at startup after `init_dock_presets()`.
pub fn init_rig_dock(presets: PresetCollection) {
    let first_layout = presets
        .presets
        .first()
        .map(|p| p.layout.clone())
        .unwrap_or_else(|| DockLayout::single(PanelId::RigGrid));

    let rig_window = DockWindow::new(
        "Rig",
        first_layout,
        WindowBounds::new(0.0, 0.0, 1400.0, 900.0, "rig"),
        false,
    );
    let rig_window_id = rig_window.id;

    DOCK_WORKSPACE
        .write()
        .windows
        .insert(rig_window_id, rig_window);
    *RIG_DOCK_WINDOW_ID.write() = Some(rig_window_id);
    *RIG_DOCK_PRESETS.write() = presets;
    *RIG_DOCK_ACTIVE_PRESET_INDEX.write() = 0;
}

/// Switch the rig dock to a different preset (auto-saves the departing layout).
pub fn load_rig_preset(index: usize) {
    let Some(rig_window_id) = *RIG_DOCK_WINDOW_ID.read() else {
        return;
    };

    // Save current rig layout to departing preset
    let current_layout = {
        let ws = DOCK_WORKSPACE.read();
        ws.windows.get(&rig_window_id).map(|w| w.layout.clone())
    };
    if let Some(layout) = current_layout {
        let current_index = *RIG_DOCK_ACTIVE_PRESET_INDEX.read();
        let mut presets = RIG_DOCK_PRESETS.write();
        if let Some(departing) = presets.presets.get_mut(current_index) {
            departing.layout = layout;
        }
    }

    // Load target preset
    let target_layout = {
        let presets = RIG_DOCK_PRESETS.read();
        presets.presets.get(index).map(|p| p.layout.clone())
    };
    if let Some(target) = target_layout {
        let mut workspace = DOCK_WORKSPACE.write();
        if let Some(window) = workspace.windows.get_mut(&rig_window_id) {
            window.layout = target;
        }
        *RIG_DOCK_ACTIVE_PRESET_INDEX.write() = index;
    }
}

/// Initialize the dock system with default or persisted presets.
///
/// Call this once at app startup. It tries to load saved presets
/// from disk, falling back to built-in defaults.
#[allow(deprecated)]
pub fn init_dock_presets() {
    // Temporary behavior: always use built-in presets at startup.
    // Ignore on-disk presets until layout behavior is finalized.
    let presets = dock_proto::default_presets();

    let mut registry = PanelRegistry::new();
    PanelId::register_all(&mut registry);

    let first_layout = presets
        .presets
        .first()
        .map(|p| p.layout.clone())
        .unwrap_or_else(|| DockLayout::single(PanelId::Performance));

    let workspace = DockWorkspace::with_main_window(
        first_layout.clone(),
        "Main",
        WindowBounds::new(120.0, 120.0, 1400.0, 900.0, "main"),
        registry,
    );

    *DOCK_LAYOUT.write() = first_layout;
    *DOCK_WORKSPACE.write() = workspace;
    *DOCK_PRESETS.write() = presets;
    *DOCK_ACTIVE_PRESET_INDEX.write() = 0;
    *DOCK_LAYOUT_TRANSITION.write() = None;
    DOCK_OPENED_FLOATING_WINDOWS.write().clear();

    ensure_floating_windows_open();
}
