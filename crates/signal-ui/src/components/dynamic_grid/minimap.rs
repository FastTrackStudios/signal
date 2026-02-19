//! Minimap overlay — shows a bird's-eye view of the grid with viewport indicator.

use dioxus::prelude::*;

/// Minimap data for a single slot (simplified view).
#[derive(Clone, PartialEq)]
pub struct MinimapSlot {
    pub col: usize,
    pub row: usize,
    pub color: String,
}

/// A minimap overlay showing the grid layout and current viewport.
#[derive(Props, Clone, PartialEq)]
pub struct MinimapProps {
    /// Simplified slot data for rendering.
    slots: Vec<MinimapSlot>,

    /// Total grid columns.
    cols: usize,

    /// Total grid rows.
    rows: usize,

    /// Current viewport as (x, y, w, h) in grid-relative coordinates (0.0–1.0).
    #[props(default = (0.0, 0.0, 1.0, 1.0))]
    viewport: (f64, f64, f64, f64),

    /// Minimap display size.
    #[props(default = 120)]
    size: u32,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn Minimap(props: MinimapProps) -> Element {
    let s = props.size;
    let sf = s as f64;
    let cols = props.cols.max(1) as f64;
    let rows = props.rows.max(1) as f64;

    let cell_w = sf / cols;
    let cell_h = sf / rows;

    let (vx, vy, vw, vh) = props.viewport;
    let vx_px = vx * sf;
    let vy_px = vy * sf;
    let vw_px = (vw * sf).max(4.0);
    let vh_px = (vh * sf).max(4.0);

    rsx! {
        div {
            class: format!(
                "absolute bottom-3 left-3 rounded-lg overflow-hidden border border-border/30 {}",
                props.class
            ),
            style: "width: {s}px; height: {s}px; background-color: rgba(0,0,0,0.7); backdrop-filter: blur(8px); z-index: 40;",

            // Grid cells
            for slot in props.slots.iter() {
                {
                    let x = slot.col as f64 * cell_w;
                    let y = slot.row as f64 * cell_h;
                    let w = cell_w.max(2.0);
                    let h = cell_h.max(2.0);
                    rsx! {
                        div {
                            class: "absolute rounded-[1px]",
                            style: "left: {x:.1}px; top: {y:.1}px; width: {w:.1}px; height: {h:.1}px; background-color: {slot.color}; opacity: 0.7;",
                        }
                    }
                }
            }

            // Viewport indicator
            div {
                class: "absolute border border-white/50 rounded-sm",
                style: "left: {vx_px:.1}px; top: {vy_px:.1}px; width: {vw_px:.1}px; height: {vh_px:.1}px;",
            }
        }
    }
}
