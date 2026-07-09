//! Drag-and-drop state machine and canvas geometry helpers.

use uuid::Uuid;

use super::models::NodeGraph;

// ── Selection ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    None,
    Module(Uuid),
    Node(Uuid),
    Wire(Uuid),
}

// ── Drag Mode ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum DragMode {
    None,
    Pan {
        start_mouse_x: f64,
        start_mouse_y: f64,
        start_pan_x: f64,
        start_pan_y: f64,
    },
    Module {
        module_id: Uuid,
        start_mouse_x: f64,
        start_mouse_y: f64,
        start_module_x: f64,
        start_module_y: f64,
    },
    Node {
        node_id: Uuid,
        start_mouse_x: f64,
        start_mouse_y: f64,
        start_node_x: f64,
        start_node_y: f64,
    },
}

// ── Canvas View Mode ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasViewMode {
    Node,
    Performance,
}

// ── Snap to Grid ─────────────────────────────────────────────────────

pub const GRID_SNAP: f64 = 20.0;

pub fn snap_to_grid(val: f64) -> f64 {
    (val / GRID_SNAP).round() * GRID_SNAP
}

// ── Canvas Bounds ────────────────────────────────────────────────────

pub fn calculate_canvas_bounds(graph: &NodeGraph) -> (f64, f64) {
    let mut max_x = 0.0f64;
    let mut max_y = 0.0f64;

    for module in &graph.modules {
        max_x = max_x.max(module.position.x + module.size.width);
        max_y = max_y.max(module.position.y + module.size.height);
    }

    for node in &graph.nodes {
        max_x = max_x.max(node.position.x + node.size.width);
        max_y = max_y.max(node.position.y + node.size.height);
    }

    (max_x + 100.0, max_y + 100.0)
}

// ── Fit Calculation ──────────────────────────────────────────────────

pub fn calculate_fit(
    canvas_w: f64,
    canvas_h: f64,
    viewport_w: f64,
    viewport_h: f64,
) -> (f64, f64, f64) {
    if canvas_w <= 0.0 || canvas_h <= 0.0 {
        return (1.0, 0.0, 0.0);
    }

    let padding = 20.0;
    let available_w = viewport_w - padding * 2.0;
    let available_h = viewport_h - padding * 2.0;

    let zoom_x = available_w / canvas_w;
    let zoom_y = available_h / canvas_h;
    let fit_zoom = zoom_x.min(zoom_y).clamp(0.1, 2.0);

    let scaled_w = canvas_w * fit_zoom;
    let scaled_h = canvas_h * fit_zoom;
    let pan_x = (viewport_w - scaled_w) / 2.0;
    let pan_y = (viewport_h - scaled_h) / 2.0;

    (fit_zoom, pan_x, pan_y)
}
