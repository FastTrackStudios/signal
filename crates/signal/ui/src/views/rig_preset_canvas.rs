//! Unified rig preset canvas — renders all engines/layers/modules on a single
//! pannable/zoomable grid with visual grouping.
//!
//! Takes the resolved `Vec<EngineFlowData>` (already computed by
//! `collection_browser::resolve_rig_scene_engines()`) and lays out ALL blocks
//! on one surface, wrapped in a [`PanZoomCanvas`].

use dioxus::prelude::*;

use super::collection_browser::EngineFlowData;
use super::signal_chain_layout::layout_signal_chain;
use crate::components::{FlowBlock, PanZoomCanvas, SignalChainGrid};

// region: --- Constants

/// Grid cell size — must match `signal_chain_grid.rs` CELL_SIZE.
const CELL_SIZE: u32 = 64;
/// Grid gap — must match `signal_chain_grid.rs` CELL_GAP.
const CELL_GAP: u32 = 4;
/// Columns per logical block — must match `signal_chain_grid.rs` COLS_PER_BLOCK.
const COLS_PER_BLOCK: u32 = 3;
/// Wire span between blocks — must match `signal_chain_grid.rs` WIRE_SPAN.
const WIRE_SPAN: u32 = 1;

/// Horizontal gap (in logical block columns) between modules within a layer.
const MODULE_GAP_COLS: usize = 1;
/// Vertical gap (in lanes) between layers within an engine.
const LAYER_GAP_LANES: usize = 1;
/// Vertical gap (in lanes) between engines (extra space for visual separation).
const ENGINE_GAP_LANES: usize = 1;
/// Lanes reserved for engine header label above each engine section.
const ENGINE_HEADER_LANES: usize = 1;

// endregion: --- Constants

// region: --- Layout types

/// A module's bounding box on the unified grid for background rendering.
#[derive(Clone)]
struct ModuleGroupRect {
    name: String,
    color_bg: String,
    color_border: String,
    /// Pixel position and size on the canvas.
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// An engine section label position.
#[derive(Clone)]
struct EngineSectionLabel {
    name: String,
    /// Pixel Y position of the label.
    y: f64,
}

/// Result of the unified rig layout algorithm.
struct RigLayout {
    blocks: Vec<FlowBlock>,
    total_cols: usize,
    total_lanes: usize,
    module_groups: Vec<ModuleGroupRect>,
    engine_labels: Vec<EngineSectionLabel>,
}

// endregion: --- Layout types

// region: --- Layout algorithm

/// Convert a logical grid column to a pixel X position.
fn col_to_px(col: usize) -> f64 {
    CELL_GAP as f64 + col as f64 * COLS_PER_BLOCK as f64 * (CELL_SIZE + CELL_GAP) as f64
}

/// Convert a logical grid column count to a pixel width.
fn cols_to_width(cols: usize) -> f64 {
    if cols == 0 {
        return 0.0;
    }
    let total_grid_cols = cols as u32 * COLS_PER_BLOCK - WIRE_SPAN;
    total_grid_cols as f64 * (CELL_SIZE + CELL_GAP) as f64
}

/// Convert a lane index to a pixel Y position.
fn lane_to_px(lane: usize) -> f64 {
    CELL_GAP as f64 + lane as f64 * (CELL_SIZE + CELL_GAP) as f64
}

/// Convert a lane count to a pixel height.
fn lanes_to_height(lanes: usize) -> f64 {
    if lanes == 0 {
        return 0.0;
    }
    lanes as f64 * (CELL_SIZE + CELL_GAP) as f64
}

/// Lay out all modules from a rig's engines onto a single unified grid.
///
/// Walks: Engines (vertical sections) → Layers (rows) → Modules (horizontal groups).
/// Each module's signal chain is laid out independently, then offset to its global position.
fn layout_rig_preset(engines: &[EngineFlowData]) -> RigLayout {
    let mut all_blocks = Vec::new();
    let mut module_groups = Vec::new();
    let mut engine_labels = Vec::new();
    let mut max_col: usize = 0;
    let mut y_lane: usize = 0;

    for engine in engines {
        // Engine header label
        engine_labels.push(EngineSectionLabel {
            name: engine.name.clone(),
            y: lane_to_px(y_lane),
        });
        y_lane += ENGINE_HEADER_LANES;

        for layer in &engine.layers {
            let mut x_col: usize = 0;
            let mut max_layer_height: usize = 0;

            for mc in &layer.module_chains {
                let (mut blocks, cols, lanes) = layout_signal_chain(&mc.chain);

                // Offset all blocks to global position
                for b in &mut blocks {
                    b.column += x_col;
                    b.lane += y_lane;
                }

                // Record module group background
                if cols > 0 && lanes > 0 {
                    module_groups.push(ModuleGroupRect {
                        name: mc.name.clone(),
                        color_bg: mc.color_bg.clone(),
                        color_border: mc.color_border.clone(),
                        x: col_to_px(x_col) - CELL_GAP as f64 / 2.0,
                        y: lane_to_px(y_lane) - CELL_GAP as f64 / 2.0,
                        w: cols_to_width(cols) + CELL_GAP as f64,
                        h: lanes_to_height(lanes) + CELL_GAP as f64,
                    });
                }

                all_blocks.extend(blocks);
                x_col += cols + MODULE_GAP_COLS;
                if lanes > max_layer_height {
                    max_layer_height = lanes;
                }
            }

            if x_col > max_col {
                max_col = x_col;
            }
            y_lane += max_layer_height.max(1) + LAYER_GAP_LANES;
        }

        y_lane += ENGINE_GAP_LANES;
    }

    // Remove trailing gap
    if y_lane > ENGINE_GAP_LANES {
        y_lane -= ENGINE_GAP_LANES;
    }

    let total_cols = if max_col > MODULE_GAP_COLS {
        max_col - MODULE_GAP_COLS
    } else {
        max_col.max(1)
    };
    let total_lanes = y_lane.max(1);

    RigLayout {
        blocks: all_blocks,
        total_cols,
        total_lanes,
        module_groups,
        engine_labels,
    }
}

// endregion: --- Layout algorithm

// region: --- RigPresetCanvas

/// Unified rig preset canvas — renders all engines/layers/modules on a single
/// pannable/zoomable surface with visual grouping.
#[component]
pub fn RigPresetCanvas(
    /// Resolved engine hierarchy from the preset.
    engines: Vec<EngineFlowData>,
) -> Element {
    if engines.is_empty() {
        return rsx! {
            div { class: "text-xs text-zinc-600 italic py-4 text-center",
                "No engines in this preset"
            }
        };
    }

    let layout = layout_rig_preset(&engines);

    // Compute pixel dimensions for the canvas
    let total_grid_cols = if layout.total_cols > 0 {
        layout.total_cols as u32 * COLS_PER_BLOCK - WIRE_SPAN
    } else {
        1
    };
    let canvas_w = (total_grid_cols * (CELL_SIZE + CELL_GAP) + CELL_GAP) as f64;
    let canvas_h = (layout.total_lanes as u32 * (CELL_SIZE + CELL_GAP) + CELL_GAP) as f64;

    rsx! {
        PanZoomCanvas {
            content_width: canvas_w,
            content_height: canvas_h,

            // Layer 0: Engine section labels
            for label in layout.engine_labels.iter() {
                {
                    let name = label.name.clone();
                    let y = label.y;
                    rsx! {
                        div {
                            key: "eng-{name}",
                            class: "absolute flex items-center gap-2",
                            style: "left: {CELL_GAP}px; top: {y}px; \
                                    height: {CELL_SIZE}px;",
                            span {
                                class: "text-xs font-semibold text-zinc-400 uppercase tracking-wider",
                                "{name}"
                            }
                        }
                    }
                }
            }

            // Layer 1: Module group backgrounds
            for (i, group) in layout.module_groups.iter().enumerate() {
                {
                    let name = group.name.clone();
                    let bg = group.color_bg.clone();
                    let border = group.color_border.clone();
                    let x = group.x;
                    let y = group.y;
                    let w = group.w;
                    let h = group.h;
                    rsx! {
                        div {
                            key: "mg-{i}",
                            class: "absolute rounded-lg",
                            style: "left: {x}px; top: {y}px; \
                                    width: {w}px; height: {h}px; \
                                    background-color: {bg}0A; \
                                    border: 1px solid {border}30;",
                            // Module name label (top-left inside the group)
                            div {
                                class: "absolute flex items-center gap-1.5 px-2 py-0.5",
                                style: "top: -14px; left: 4px;",
                                span {
                                    class: "w-2 h-2 rounded-sm flex-shrink-0",
                                    style: "background-color: {bg};",
                                }
                                span {
                                    class: "text-[10px] font-medium text-zinc-500",
                                    "{name}"
                                }
                            }
                        }
                    }
                }
            }

            // Layer 2: Signal chain grid (blocks + SVG wires)
            SignalChainGrid {
                blocks: layout.blocks,
                total_columns: layout.total_cols,
                total_lanes: layout.total_lanes,
            }
        }
    }
}

// endregion: --- RigPresetCanvas
