//! Node graph view -- main canvas component with pan/zoom and full interactions.
//!
//! Ported from legacy signal-ui. Key changes:
//! - Graph is passed as a prop (not a global signal)
//! - Uses `onmounted` + `get_client_rect()` instead of eval-polling for viewport
//! - Uses `transform: scale()` instead of CSS `zoom` for cross-browser consistency
//! - No tokio dependency
//!
//! Features:
//! - Module containers with child node blocks (HTML layer)
//! - SVG bezier wire connections (SVG layer)
//! - Pan by dragging the background
//! - Zoom with scroll wheel (cursor-anchored)
//! - Module/node dragging with snap-to-grid
//! - Interactive wire creation with validation
//! - Selection: click to select, Delete to remove
//! - Keyboard shortcuts: Delete, Escape, F (fit), G (snap), B (bypass), C (collapse)

use dioxus::prelude::dioxus_elements::geometry::WheelDelta;
use dioxus::prelude::*;
use uuid::Uuid;

use super::drag_handler::{
    calculate_canvas_bounds, calculate_fit, snap_to_grid, CanvasViewMode, DragMode, Selection,
};
use super::models::{NodeGraph, NodePosition};
use super::module_container::ModuleContainer;
use super::node_block::{NodeBlock, PortDragStart, PortHoverEvent};
use super::wire::{resolve_all_wires, ResolvedWire};
use super::wire_layer::{WireDraft, WireLayer};

// ── Props ────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct NodeGraphViewProps {
    /// The graph to render. Pass a signal for reactive updates.
    pub graph: NodeGraph,
    /// Callback when the graph is mutated (drag, delete, wire, bypass, collapse).
    #[props(default)]
    pub on_graph_change: Option<Callback<NodeGraph>>,
    /// Start in compact/read-only mode (no editing, auto-fit).
    #[props(default)]
    pub compact: bool,
}

// ── Component ────────────────────────────────────────────────────────

#[component]
pub fn NodeGraphView(props: NodeGraphViewProps) -> Element {
    // ── Viewport state ───────────────────────────────────────────
    let mut pan_x = use_signal(|| 0.0f64);
    let mut pan_y = use_signal(|| 0.0f64);
    let mut zoom = use_signal(|| 1.0f64);
    let mut has_fitted = use_signal(|| false);
    let mut viewport_w = use_signal(|| 1200.0f64);
    let mut viewport_h = use_signal(|| 700.0f64);
    let mut viewport_left = use_signal(|| 0.0f64);
    let mut viewport_top = use_signal(|| 0.0f64);

    // ── Interaction state ────────────────────────────────────────
    let mut drag_mode = use_signal(|| DragMode::None);
    let mut wire_draft = use_signal(|| Option::<WireDraft>::None);
    let mut hovered_port = use_signal(|| Option::<(Uuid, String, bool)>::None);
    let mut selection = use_signal(|| Selection::None);
    let mut snap_enabled = use_signal(|| true);
    let mut canvas_mode = use_signal(|| {
        if props.compact {
            CanvasViewMode::Performance
        } else {
            CanvasViewMode::Node
        }
    });

    // Local mutable copy of the graph for editing
    let mut graph = use_signal(|| props.graph.clone());

    // Sync incoming prop changes
    let prop_graph = props.graph.clone();
    use_effect(move || {
        graph.set(prop_graph.clone());
    });

    let compact = props.compact;
    let performance_mode = canvas_mode() == CanvasViewMode::Performance;

    // Helper to emit graph changes
    let on_change = props.on_graph_change.clone();
    let emit_change = move |g: &NodeGraph| {
        if let Some(ref cb) = on_change {
            cb.call(g.clone());
        }
    };

    // ── Read graph ───────────────────────────────────────────────
    let current_graph = graph.read().clone();
    let wires: Vec<ResolvedWire> = resolve_all_wires(&current_graph, performance_mode || compact);
    let (canvas_w, canvas_h) = calculate_canvas_bounds(&current_graph);

    // ── Viewport measurement via onmounted ───────────────────────
    let onmounted = move |evt: MountedEvent| {
        spawn(async move {
            if let Ok(rect) = evt.get_client_rect().await {
                let origin = rect.origin;
                let size = rect.size;
                viewport_left.set(origin.x);
                viewport_top.set(origin.y);
                viewport_w.set(size.width.max(1.0));
                viewport_h.set(size.height.max(1.0));
            }
        });
    };

    // ── Auto-fit on first render ─────────────────────────────────
    if !has_fitted() && viewport_w() > 1.0 {
        has_fitted.set(true);
        let (fit_zoom, fit_pan_x, fit_pan_y) =
            calculate_fit(canvas_w, canvas_h, viewport_w(), viewport_h());
        zoom.set(fit_zoom);
        pan_x.set(fit_pan_x);
        pan_y.set(fit_pan_y);
    }

    let cw = canvas_w;
    let ch = canvas_h;

    let is_dragging_anything = !matches!(drag_mode(), DragMode::None);
    let has_wire_draft = wire_draft().is_some();
    let cursor = if performance_mode {
        "default"
    } else if has_wire_draft {
        "crosshair"
    } else if is_dragging_anything {
        "grabbing"
    } else {
        "grab"
    };

    let selected_wire_id = match selection() {
        Selection::Wire(id) => Some(id),
        _ => None,
    };

    rsx! {
        div {
            class: if performance_mode {
                "relative w-full h-full overflow-y-auto overflow-x-hidden select-none"
            } else {
                "relative w-full h-full overflow-hidden select-none"
            },
            style: "background-color: #0a0a0f; \
                    background-image: radial-gradient(circle, #1a1a2e 1px, transparent 1px); \
                    background-size: 20px 20px; \
                    cursor: {cursor};",
            tabindex: "0",
            onmounted: onmounted,

            // ── Keyboard ─────────────────────────────────────
            onkeydown: move |evt| {
                let key = evt.key();
                match key {
                    Key::Delete | Key::Backspace => {
                        match selection() {
                            Selection::Wire(wire_id) => {
                                graph.write().disconnect(wire_id);
                                selection.set(Selection::None);
                                emit_change(&graph.read());
                            }
                            Selection::Module(module_id) => {
                                graph.write().remove_module(module_id);
                                selection.set(Selection::None);
                                emit_change(&graph.read());
                            }
                            Selection::Node(node_id) => {
                                graph.write().remove_node(node_id);
                                selection.set(Selection::None);
                                emit_change(&graph.read());
                            }
                            Selection::None => {}
                        }
                    }
                    Key::Escape => {
                        selection.set(Selection::None);
                        wire_draft.set(None);
                        hovered_port.set(None);
                    }
                    Key::Character(ref c) if c == "f" || c == "F" => {
                        if performance_mode { return; }
                        let g = graph.read();
                        let (cw, ch) = calculate_canvas_bounds(&g);
                        let (fit_zoom, fit_pan_x, fit_pan_y) =
                            calculate_fit(cw, ch, viewport_w(), viewport_h());
                        zoom.set(fit_zoom);
                        pan_x.set(fit_pan_x);
                        pan_y.set(fit_pan_y);
                    }
                    Key::Character(ref c) if c == "g" || c == "G" => {
                        snap_enabled.set(!snap_enabled());
                    }
                    Key::Character(ref c) if c == "b" || c == "B" => {
                        match selection() {
                            Selection::Module(module_id) => {
                                if let Some(m) = graph.write().find_module_mut(module_id) {
                                    m.bypassed = !m.bypassed;
                                }
                                emit_change(&graph.read());
                            }
                            Selection::Node(node_id) => {
                                if let Some(n) = graph.write().find_node_mut(node_id) {
                                    n.bypassed = !n.bypassed;
                                }
                                emit_change(&graph.read());
                            }
                            _ => {}
                        }
                    }
                    Key::Character(ref c) if c == "c" || c == "C" => {
                        if let Selection::Module(module_id) = selection() {
                            if let Some(m) = graph.write().find_module_mut(module_id) {
                                m.collapsed = !m.collapsed;
                            }
                            emit_change(&graph.read());
                        }
                    }
                    _ => {}
                }
            },

            // ── Mouse: pan start ─────────────────────────────
            onmousedown: move |evt| {
                if performance_mode { return; }
                evt.prevent_default();
                selection.set(Selection::None);
                drag_mode.set(DragMode::Pan {
                    start_mouse_x: evt.client_coordinates().x,
                    start_mouse_y: evt.client_coordinates().y,
                    start_pan_x: pan_x(),
                    start_pan_y: pan_y(),
                });
            },

            oncontextmenu: move |evt| {
                evt.prevent_default();
                evt.stop_propagation();
            },

            // ── Mouse: move ──────────────────────────────────
            onmousemove: move |evt| {
                let mx = evt.client_coordinates().x;
                let my = evt.client_coordinates().y;
                let current_zoom = zoom();

                match drag_mode() {
                    DragMode::Pan { start_mouse_x, start_mouse_y, start_pan_x, start_pan_y } => {
                        let dx = mx - start_mouse_x;
                        let dy = my - start_mouse_y;
                        pan_x.set(start_pan_x + dx);
                        pan_y.set(start_pan_y + dy);
                    }
                    DragMode::Module { module_id, start_mouse_x, start_mouse_y, start_module_x, start_module_y } => {
                        let dx = (mx - start_mouse_x) / current_zoom;
                        let dy = (my - start_mouse_y) / current_zoom;
                        let mut new_x = start_module_x + dx;
                        let mut new_y = start_module_y + dy;
                        if snap_enabled() {
                            new_x = snap_to_grid(new_x);
                            new_y = snap_to_grid(new_y);
                        }
                        new_x = new_x.max(0.0);
                        new_y = new_y.max(0.0);
                        if let Some(m) = graph.write().find_module_mut(module_id) {
                            m.position.x = new_x;
                            m.position.y = new_y;
                        }
                    }
                    DragMode::Node { node_id, start_mouse_x, start_mouse_y, start_node_x, start_node_y } => {
                        let dx = (mx - start_mouse_x) / current_zoom;
                        let dy = (my - start_mouse_y) / current_zoom;
                        let mut new_x = start_node_x + dx;
                        let mut new_y = start_node_y + dy;
                        if snap_enabled() {
                            new_x = snap_to_grid(new_x);
                            new_y = snap_to_grid(new_y);
                        }
                        new_x = new_x.max(0.0);
                        new_y = new_y.max(0.0);
                        if let Some(n) = graph.write().find_node_mut(node_id) {
                            n.position.x = new_x;
                            n.position.y = new_y;
                        }
                    }
                    DragMode::None => {}
                }

                // Update wire draft endpoint
                if let Some(mut draft) = wire_draft() {
                    let canvas_x = (mx - viewport_left() - pan_x()) / current_zoom;
                    let canvas_y = (my - viewport_top() - pan_y()) / current_zoom;
                    draft.mouse_pos = NodePosition::new(canvas_x, canvas_y);
                    wire_draft.set(Some(draft));
                }
            },

            // ── Mouse: up ────────────────────────────────────
            onmouseup: move |_| {
                let was_dragging = !matches!(drag_mode(), DragMode::None | DragMode::Pan { .. });
                drag_mode.set(DragMode::None);

                if let Some(draft) = wire_draft() {
                    if let Some((target_entity, target_port, target_is_input)) = hovered_port() {
                        if draft.is_from_output && target_is_input {
                            graph.write().try_connect(
                                draft.from_entity,
                                &draft.from_port,
                                target_entity,
                                &target_port,
                            );
                            emit_change(&graph.read());
                        } else if !draft.is_from_output && !target_is_input {
                            graph.write().try_connect(
                                target_entity,
                                &target_port,
                                draft.from_entity,
                                &draft.from_port,
                            );
                            emit_change(&graph.read());
                        }
                    }
                    wire_draft.set(None);
                    hovered_port.set(None);
                } else if was_dragging {
                    emit_change(&graph.read());
                }
            },

            onmouseleave: move |_| {
                drag_mode.set(DragMode::None);
                wire_draft.set(None);
                hovered_port.set(None);
            },

            // ── Zoom (cursor-anchored) ───────────────────────
            onwheel: move |evt| {
                if performance_mode { return; }
                evt.prevent_default();
                let delta = evt.delta();
                let dy = match delta {
                    WheelDelta::Pixels(p) => p.y,
                    WheelDelta::Lines(l) => l.y * 40.0,
                    WheelDelta::Pages(p) => p.y * 400.0,
                };
                let old_zoom = zoom();
                let zoom_factor = if dy < 0.0 { 1.08 } else { 1.0 / 1.08 };
                let new_zoom = (old_zoom * zoom_factor).clamp(0.1, 3.0);

                let local_x = evt.client_coordinates().x - viewport_left();
                let local_y = evt.client_coordinates().y - viewport_top();
                let canvas_x = (local_x - pan_x()) / old_zoom;
                let canvas_y = (local_y - pan_y()) / old_zoom;
                pan_x.set(local_x - canvas_x * new_zoom);
                pan_y.set(local_y - canvas_y * new_zoom);
                zoom.set(new_zoom);
            },

            // ── View mode toggles ────────────────────────────
            if !compact {
                div {
                    class: "absolute top-3 right-3 z-30 flex gap-2 select-none",
                    onmousedown: move |evt| evt.stop_propagation(),

                    if canvas_mode() == CanvasViewMode::Node {
                        button {
                            class: "px-3 py-1.5 rounded-lg text-xs font-medium text-zinc-300 hover:text-white transition-colors",
                            style: "background-color: rgba(0,0,0,0.6); backdrop-filter: blur(8px);",
                            title: "Collapse all modules",
                            onclick: move |_| {
                                let mut g = graph.write();
                                for m in g.modules.iter_mut() {
                                    m.collapsed = true;
                                }
                                drop(g);
                                emit_change(&graph.read());
                            },
                            "Collapse All"
                        }

                        button {
                            class: "px-3 py-1.5 rounded-lg text-xs font-medium text-zinc-300 hover:text-white transition-colors",
                            style: "background-color: rgba(0,0,0,0.6); backdrop-filter: blur(8px);",
                            title: "Expand all modules",
                            onclick: move |_| {
                                let mut g = graph.write();
                                for m in g.modules.iter_mut() {
                                    m.collapsed = false;
                                }
                                drop(g);
                                emit_change(&graph.read());
                            },
                            "Expand All"
                        }

                        div {
                            class: "w-px h-6 self-center",
                            style: "background-color: rgba(255,255,255,0.15);",
                        }
                    }

                    button {
                        class: if canvas_mode() == CanvasViewMode::Node {
                            "px-3 py-1.5 rounded-lg text-xs font-medium bg-blue-600 text-white"
                        } else {
                            "px-3 py-1.5 rounded-lg text-xs font-medium text-zinc-300 hover:text-white"
                        },
                        style: if canvas_mode() == CanvasViewMode::Node {
                            "backdrop-filter: blur(8px);"
                        } else {
                            "background-color: rgba(0,0,0,0.6); backdrop-filter: blur(8px);"
                        },
                        onclick: move |_| canvas_mode.set(CanvasViewMode::Node),
                        "Node View"
                    }

                    button {
                        class: if canvas_mode() == CanvasViewMode::Performance {
                            "px-3 py-1.5 rounded-lg text-xs font-medium bg-blue-600 text-white"
                        } else {
                            "px-3 py-1.5 rounded-lg text-xs font-medium text-zinc-300 hover:text-white"
                        },
                        style: if canvas_mode() == CanvasViewMode::Performance {
                            "backdrop-filter: blur(8px);"
                        } else {
                            "background-color: rgba(0,0,0,0.6); backdrop-filter: blur(8px);"
                        },
                        onclick: move |_| canvas_mode.set(CanvasViewMode::Performance),
                        "Performance View"
                    }
                }
            }

            // ── Canvas Layer ─────────────────────────────────
            div {
                style: if performance_mode {
                    "position: relative; left: 0px; top: 0px; width: 100%;".to_string()
                } else {
                    format!(
                        "position: absolute; left: {pan_x}px; top: {pan_y}px; \
                         transform: scale({zoom}); transform-origin: 0 0;",
                        pan_x = pan_x(),
                        pan_y = pan_y(),
                        zoom = zoom(),
                    )
                },

                div {
                    style: if performance_mode {
                        format!("position: relative; width: 100%; height: {canvas_h}px;")
                    } else {
                        format!("position: relative; width: {canvas_w}px; height: {canvas_h}px;")
                    },

                    for module in &current_graph.modules {
                        ModuleContainer {
                            key: "{module.id}",
                            module: module.clone(),
                            performance_mode: performance_mode,
                            is_selected: matches!(selection(), Selection::Module(id) if id == module.id),
                            on_select: {
                                let module_id = module.id;
                                move |_: Uuid| {
                                    selection.set(Selection::Module(module_id));
                                }
                            },
                            on_title_drag_start: {
                                let module_id = module.id;
                                let module_x = module.position.x;
                                let module_y = module.position.y;
                                move |evt: MouseEvent| {
                                    if compact || performance_mode { return; }
                                    evt.stop_propagation();
                                    selection.set(Selection::Module(module_id));
                                    drag_mode.set(DragMode::Module {
                                        module_id,
                                        start_mouse_x: evt.client_coordinates().x,
                                        start_mouse_y: evt.client_coordinates().y,
                                        start_module_x: module_x,
                                        start_module_y: module_y,
                                    });
                                }
                            },
                            on_toggle_collapse: {
                                move |id: Uuid| {
                                    if let Some(m) = graph.write().find_module_mut(id) {
                                        m.collapsed = !m.collapsed;
                                    }
                                    emit_change(&graph.read());
                                }
                            },
                            on_port_drag_start: {
                                let module_clone = module.clone();
                                move |evt: PortDragStart| {
                                    if compact || performance_mode { return; }
                                    let is_input = !evt.is_output;
                                    if let Some(pos) = module_clone.port_position(&evt.port_name, is_input) {
                                        wire_draft.set(Some(WireDraft {
                                            from_entity: module_clone.id,
                                            from_port: evt.port_name,
                                            from_pos: pos,
                                            is_from_output: evt.is_output,
                                            mouse_pos: pos,
                                        }));
                                    }
                                }
                            },
                            on_port_hover: {
                                move |evt: PortHoverEvent| {
                                    if compact || performance_mode { return; }
                                    if wire_draft().is_some() {
                                        hovered_port.set(Some((evt.node_id, evt.port_name, evt.is_hovering)));
                                    }
                                }
                            },
                            on_port_hover_end: {
                                move |_: ()| {
                                    hovered_port.set(None);
                                }
                            },
                            wire_draft_active: has_wire_draft && !compact,
                            hovered_port: hovered_port(),
                        }
                    }

                    for node in &current_graph.nodes {
                        NodeBlock {
                            key: "{node.id}",
                            node: node.clone(),
                            is_selected: matches!(selection(), Selection::Node(id) if id == node.id),
                            on_select: {
                                let node_id = node.id;
                                move |_: Uuid| {
                                    selection.set(Selection::Node(node_id));
                                }
                            },
                            on_header_drag_start: {
                                let node_id = node.id;
                                let node_x = node.position.x;
                                let node_y = node.position.y;
                                move |evt: MouseEvent| {
                                    if performance_mode { return; }
                                    evt.stop_propagation();
                                    selection.set(Selection::Node(node_id));
                                    drag_mode.set(DragMode::Node {
                                        node_id,
                                        start_mouse_x: evt.client_coordinates().x,
                                        start_mouse_y: evt.client_coordinates().y,
                                        start_node_x: node_x,
                                        start_node_y: node_y,
                                    });
                                }
                            },
                            on_port_drag_start: {
                                let node_clone = node.clone();
                                move |evt: PortDragStart| {
                                    if performance_mode { return; }
                                    let is_input = !evt.is_output;
                                    if let Some(pos) = node_clone.port_position(&evt.port_name, is_input) {
                                        wire_draft.set(Some(WireDraft {
                                            from_entity: node_clone.id,
                                            from_port: evt.port_name,
                                            from_pos: pos,
                                            is_from_output: evt.is_output,
                                            mouse_pos: pos,
                                        }));
                                    }
                                }
                            },
                            on_port_hover: {
                                move |evt: PortHoverEvent| {
                                    if wire_draft().is_some() {
                                        hovered_port.set(Some((evt.node_id, evt.port_name, evt.is_hovering)));
                                    }
                                }
                            },
                            on_port_hover_end: {
                                move |_: ()| {
                                    hovered_port.set(None);
                                }
                            },
                            wire_draft_active: has_wire_draft,
                            hovered_port: hovered_port(),
                        }
                    }
                }

                // SVG wire layer
                if !performance_mode {
                    WireLayer {
                        canvas_w: canvas_w,
                        canvas_h: canvas_h,
                        wires: wires.clone(),
                        wire_draft: wire_draft(),
                        hovered_port: hovered_port(),
                        selected_wire_id: selected_wire_id,
                        on_wire_click: move |wire_id: Uuid| {
                            selection.set(Selection::Wire(wire_id));
                        },
                    }
                }
            }

            // ── Controls overlay (bottom-right) ──────────────
            if !compact && !performance_mode {
                div {
                    class: "absolute bottom-4 right-4 flex items-center gap-2 select-none",
                    onmousedown: move |evt| evt.stop_propagation(),

                    button {
                        class: "px-3 py-1.5 rounded-lg text-xs font-medium transition-colors",
                        style: if snap_enabled() {
                            "background-color: rgba(34,211,238,0.2); color: #22d3ee; backdrop-filter: blur(8px);"
                        } else {
                            "background-color: rgba(0,0,0,0.6); color: #a1a1aa; backdrop-filter: blur(8px);"
                        },
                        title: "Toggle snap to grid (G)",
                        onclick: move |_| snap_enabled.set(!snap_enabled()),
                        "Grid"
                    }

                    button {
                        class: "px-3 py-1.5 rounded-lg text-xs font-medium \
                                text-zinc-300 hover:text-white transition-colors",
                        style: "background-color: rgba(0,0,0,0.6); backdrop-filter: blur(8px);",
                        title: "Fit all modules in view (F)",
                        onclick: move |_| {
                            let (fit_zoom, fit_pan_x, fit_pan_y) =
                                calculate_fit(cw, ch, viewport_w(), viewport_h());
                            zoom.set(fit_zoom);
                            pan_x.set(fit_pan_x);
                            pan_y.set(fit_pan_y);
                        },
                        "Fit"
                    }

                    button {
                        class: "px-2 py-1.5 rounded-lg text-xs font-medium \
                                text-zinc-300 hover:text-white transition-colors",
                        style: "background-color: rgba(0,0,0,0.6); backdrop-filter: blur(8px);",
                        onclick: move |_| {
                            zoom.set((zoom() / 1.2).clamp(0.1, 3.0));
                        },
                        "-"
                    }

                    div {
                        class: "px-3 py-1.5 rounded-lg text-xs font-mono text-zinc-400",
                        style: "background-color: rgba(0,0,0,0.6); backdrop-filter: blur(8px); \
                                min-width: 48px; text-align: center;",
                        "{(zoom() * 100.0) as i32}%"
                    }

                    button {
                        class: "px-2 py-1.5 rounded-lg text-xs font-medium \
                                text-zinc-300 hover:text-white transition-colors",
                        style: "background-color: rgba(0,0,0,0.6); backdrop-filter: blur(8px);",
                        onclick: move |_| {
                            zoom.set((zoom() * 1.2).clamp(0.1, 3.0));
                        },
                        "+"
                    }
                }
            }

            // ── Selection info bar ───────────────────────────
            {
                let info_text = match selection() {
                    Selection::Module(id) => {
                        current_graph.find_module(id)
                            .map(|m| {
                                let bypass_status = if m.bypassed { " [BYPASSED]" } else { "" };
                                let collapse_status = if m.collapsed { " [COLLAPSED]" } else { "" };
                                format!("{}{}{} -- Del: remove, B: bypass, C: collapse, Esc: deselect", m.name, bypass_status, collapse_status)
                            })
                            .unwrap_or_default()
                    }
                    Selection::Node(id) => {
                        current_graph.find_node(id)
                            .map(|n| {
                                let bypass_status = if n.bypassed { " [BYPASSED]" } else { "" };
                                format!("{}{} -- Del: remove, B: bypass, Esc: deselect", n.name, bypass_status)
                            })
                            .unwrap_or_default()
                    }
                    Selection::Wire(_) => "Wire -- Del: remove, Esc: deselect".to_string(),
                    Selection::None => String::new(),
                };
                if !info_text.is_empty() {
                    rsx! {
                        div {
                            class: "absolute top-4 left-1/2 -translate-x-1/2 px-4 py-2 rounded-lg \
                                    text-xs text-zinc-300 select-none pointer-events-none",
                            style: "background-color: rgba(0,0,0,0.7); backdrop-filter: blur(8px);",
                            "{info_text}"
                        }
                    }
                } else {
                    rsx! {}
                }
            }
        }
    }
}
