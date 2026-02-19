//! Read-only signal chain flow grid.
//!
//! Ported from the legacy `SignalFlowGridView` — renders blocks on a CSS grid
//! with SVG routing wires. Visual style matches the Helix / Quad Cortex signal
//! flow grid: colored blocks with a header label bar, positioned on a 2D grid.
//!
//! Each block occupies a 2×1 (wide) cell so labels are readable.
//! Parallel splits render as stacked rows with vertical routing wires.
//!
//! This is a **dumb** read-only component — no callbacks, no bypass, no widgets.

use dioxus::prelude::*;

use super::block_colors::block_color;

// region: --- Constants

/// Cell size in pixels — matches the legacy SignalFlowGridView CELL_SIZE.
const CELL_SIZE: u32 = 64;
/// Gap between cells.
const CELL_GAP: u32 = 4;
/// Each block spans this many grid columns (2 = "wide" block for readable labels).
const BLOCK_SPAN: u32 = 2;
/// Width of the connector column between blocks (in grid units).
const WIRE_SPAN: u32 = 1;
/// Total grid columns per logical block = block + wire gap.
const COLS_PER_BLOCK: u32 = BLOCK_SPAN + WIRE_SPAN;
/// Header bar height in pixels (for multi-cell blocks).
const HEADER_H: u32 = 24;

// endregion: --- Constants

// region: --- Types

/// A block positioned in the signal flow grid.
#[derive(Clone, PartialEq)]
pub struct FlowBlock {
    /// Unique identifier.
    pub id: String,
    /// Display label for the block.
    pub label: String,
    /// Short label for compact (1×1) display — typically first 3 chars.
    pub short_label: String,
    /// Block type key for `block_color()` lookup (e.g. `"amp"`, `"delay"`).
    pub block_type_key: String,
    /// Parameter names and values (0.0–1.0) for optional inline display.
    pub params: Vec<(String, f32)>,
    /// Vertical lane index (0 = main path, 1+ = parallel lanes).
    pub lane: usize,
    /// Horizontal column index (0 = leftmost).
    pub column: usize,
}

// endregion: --- Types

// region: --- Wire geometry

struct Wire {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

/// Pixel X of the right edge of a block at logical column `col`.
fn block_right_x(col: usize) -> f32 {
    let grid_col_start = col as f32 * COLS_PER_BLOCK as f32;
    let px = CELL_GAP as f32 + (grid_col_start + BLOCK_SPAN as f32) * (CELL_SIZE + CELL_GAP) as f32
        - CELL_GAP as f32;
    px
}

/// Pixel X of the left edge of a block at logical column `col`.
fn block_left_x(col: usize) -> f32 {
    let grid_col_start = col as f32 * COLS_PER_BLOCK as f32;
    CELL_GAP as f32 + grid_col_start * (CELL_SIZE + CELL_GAP) as f32
}

/// Pixel Y center of a lane.
fn lane_center_y(lane: usize) -> f32 {
    CELL_GAP as f32 + lane as f32 * (CELL_SIZE + CELL_GAP) as f32 + CELL_SIZE as f32 / 2.0
}

/// Compute SVG routing wires between blocks.
fn compute_wires(blocks: &[FlowBlock], cols: usize, rows: usize) -> Vec<Wire> {
    let mut wires = Vec::new();

    let mut grid: Vec<Vec<Option<usize>>> = vec![vec![None; cols]; rows];
    for (i, b) in blocks.iter().enumerate() {
        if b.lane < rows && b.column < cols {
            grid[b.lane][b.column] = Some(i);
        }
    }

    for col in 0..cols.saturating_sub(1) {
        let sources: Vec<usize> = (0..rows).filter_map(|r| grid[r][col]).collect();
        let targets: Vec<usize> = (0..rows).filter_map(|r| grid[r][col + 1]).collect();

        if sources.is_empty() || targets.is_empty() {
            continue;
        }

        if sources.len() == 1 && targets.len() == 1 {
            // 1:1 connection
            let s = &blocks[sources[0]];
            let t = &blocks[targets[0]];
            let x1 = block_right_x(s.column);
            let y1 = lane_center_y(s.lane);
            let x2 = block_left_x(t.column);
            let y2 = lane_center_y(t.lane);
            if s.lane == t.lane {
                wires.push(Wire { x1, y1, x2, y2 });
            } else {
                let mid_x = (x1 + x2) / 2.0;
                wires.push(Wire {
                    x1,
                    y1,
                    x2: mid_x,
                    y2: y1,
                });
                wires.push(Wire {
                    x1: mid_x,
                    y1,
                    x2: mid_x,
                    y2,
                });
                wires.push(Wire {
                    x1: mid_x,
                    y1: y2,
                    x2,
                    y2,
                });
            }
        } else if sources.len() == 1 && targets.len() > 1 {
            // Fan-out (split)
            let s = &blocks[sources[0]];
            let sx = block_right_x(s.column);
            let sy = lane_center_y(s.lane);
            let mid_x = (sx + block_left_x(col + 1)) / 2.0;

            wires.push(Wire {
                x1: sx,
                y1: sy,
                x2: mid_x,
                y2: sy,
            });

            let min_y = targets
                .iter()
                .map(|&i| lane_center_y(blocks[i].lane))
                .fold(f32::MAX, f32::min);
            let max_y = targets
                .iter()
                .map(|&i| lane_center_y(blocks[i].lane))
                .fold(f32::MIN, f32::max);
            wires.push(Wire {
                x1: mid_x,
                y1: min_y,
                x2: mid_x,
                y2: max_y,
            });

            for &ti in &targets {
                let t = &blocks[ti];
                let tx = block_left_x(t.column);
                let ty = lane_center_y(t.lane);
                wires.push(Wire {
                    x1: mid_x,
                    y1: ty,
                    x2: tx,
                    y2: ty,
                });
            }
        } else if sources.len() > 1 && targets.len() == 1 {
            // Fan-in (merge)
            let t = &blocks[targets[0]];
            let tx = block_left_x(t.column);
            let ty = lane_center_y(t.lane);
            let mid_x = (block_right_x(col) + tx) / 2.0;

            for &si in &sources {
                let s = &blocks[si];
                let sx = block_right_x(s.column);
                let sy = lane_center_y(s.lane);
                wires.push(Wire {
                    x1: sx,
                    y1: sy,
                    x2: mid_x,
                    y2: sy,
                });
            }

            let min_y = sources
                .iter()
                .map(|&i| lane_center_y(blocks[i].lane))
                .fold(f32::MAX, f32::min);
            let max_y = sources
                .iter()
                .map(|&i| lane_center_y(blocks[i].lane))
                .fold(f32::MIN, f32::max);
            wires.push(Wire {
                x1: mid_x,
                y1: min_y,
                x2: mid_x,
                y2: max_y,
            });
            wires.push(Wire {
                x1: mid_x,
                y1: ty,
                x2: tx,
                y2: ty,
            });
        } else {
            // N:M fallback — match by lane
            for &si in &sources {
                let s = &blocks[si];
                let sx = block_right_x(s.column);
                let sy = lane_center_y(s.lane);
                let matched = targets
                    .iter()
                    .find(|&&ti| blocks[ti].lane == s.lane)
                    .or(targets.first());
                if let Some(&ti) = matched {
                    let t = &blocks[ti];
                    let tx = block_left_x(t.column);
                    let ty = lane_center_y(t.lane);
                    wires.push(Wire {
                        x1: sx,
                        y1: sy,
                        x2: tx,
                        y2: ty,
                    });
                }
            }
        }
    }

    wires
}

// endregion: --- Wire geometry

// region: --- SignalChainGrid

/// Read-only signal chain flow grid — Helix / Quad Cortex style.
///
/// Blocks are rendered as 2-wide colored cells with a dark header bar,
/// connected by SVG routing wires. Parallel splits fan out vertically.
/// Visual style ported from the legacy `SignalFlowGridView`.
#[component]
pub fn SignalChainGrid(
    /// Pre-laid-out blocks with (lane, column) positions.
    blocks: Vec<FlowBlock>,
    /// Total number of columns in the grid.
    total_columns: usize,
    /// Total number of lanes (rows) in the grid.
    total_lanes: usize,
) -> Element {
    if blocks.is_empty() || total_columns == 0 {
        return rsx! {
            div { class: "text-xs text-zinc-600 italic py-2", "Empty signal chain" }
        };
    }

    let cols = total_columns;
    let rows = total_lanes;

    // Grid columns: for each logical block column, we need BLOCK_SPAN cols + WIRE_SPAN gap.
    // Last block column doesn't need a trailing wire gap.
    let total_grid_cols = cols as u32 * COLS_PER_BLOCK - WIRE_SPAN;
    let total_w = total_grid_cols * (CELL_SIZE + CELL_GAP) + CELL_GAP;
    let total_h = rows as u32 * (CELL_SIZE + CELL_GAP) + CELL_GAP;

    let col_template = format!("repeat({total_grid_cols}, {CELL_SIZE}px)");
    let row_template = format!("repeat({rows}, {CELL_SIZE}px)");

    let wires = compute_wires(&blocks, cols, rows);

    rsx! {
        div { class: "relative w-full overflow-x-auto",
            div {
                class: "relative",
                style: "width: {total_w}px; height: {total_h}px;",

                // SVG routing wires (behind blocks)
                svg {
                    class: "absolute inset-0",
                    width: "{total_w}",
                    height: "{total_h}",
                    style: "pointer-events: none;",
                    for (i, wire) in wires.iter().enumerate() {
                        {
                            let x1 = wire.x1;
                            let y1 = wire.y1;
                            let x2 = wire.x2;
                            let y2 = wire.y2;
                            rsx! {
                                line {
                                    key: "w-{i}",
                                    x1: "{x1}",
                                    y1: "{y1}",
                                    x2: "{x2}",
                                    y2: "{y2}",
                                    stroke: "#52525B",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                }
                            }
                        }
                    }
                    // Junction dots at vertical wire tops
                    for (i, wire) in wires.iter().enumerate() {
                        {
                            let is_vert = (wire.x1 - wire.x2).abs() < 0.5
                                && (wire.y1 - wire.y2).abs() > 1.0;
                            if is_vert {
                                let cx = wire.x1;
                                let cy = wire.y1;
                                rsx! {
                                    circle {
                                        key: "jt-{i}",
                                        cx: "{cx}",
                                        cy: "{cy}",
                                        r: "3",
                                        fill: "#52525B",
                                    }
                                }
                            } else {
                                rsx! {}
                            }
                        }
                    }
                    // Junction dots at vertical wire bottoms
                    for (i, wire) in wires.iter().enumerate() {
                        {
                            let is_vert = (wire.x1 - wire.x2).abs() < 0.5
                                && (wire.y1 - wire.y2).abs() > 1.0;
                            if is_vert {
                                let cx = wire.x2;
                                let cy = wire.y2;
                                rsx! {
                                    circle {
                                        key: "jb-{i}",
                                        cx: "{cx}",
                                        cy: "{cy}",
                                        r: "3",
                                        fill: "#52525B",
                                    }
                                }
                            } else {
                                rsx! {}
                            }
                        }
                    }
                }

                // CSS Grid for blocks (on top of wires)
                div {
                    class: "absolute inset-0 inline-grid",
                    style: "grid-template-columns: {col_template}; \
                            grid-template-rows: {row_template}; \
                            gap: {CELL_GAP}px; \
                            padding: {CELL_GAP}px;",

                    for block in blocks.iter() {
                        { render_flow_block(block) }
                    }
                }
            }
        }
    }
}

/// Render a single block cell — ported from legacy `GridBlockCell` + `BlockHeader`.
///
/// Each block spans 2 grid columns (BLOCK_SPAN) giving ~132px width for labels.
/// Structure:
/// - `rounded-lg border-2` outer div with full-opacity `block_color()` background
/// - Dark overlay header bar (24px) with block name (truncated)
/// - Content area with block type label centered
fn render_flow_block(block: &FlowBlock) -> Element {
    let color = block_color(&block.block_type_key);
    let bg = color.bg;
    let fg = color.fg;
    let border = color.border;
    let label = &block.label;
    let short_label = &block.short_label;
    let block_type = &block.block_type_key;
    let id = &block.id;

    // CSS grid placement: each logical column maps to COLS_PER_BLOCK grid columns.
    let grid_col_start = block.column as u32 * COLS_PER_BLOCK + 1; // 1-indexed
    let grid_col_end = grid_col_start + BLOCK_SPAN;
    let grid_row = block.lane + 1; // 1-indexed

    // Block pixel width: BLOCK_SPAN cells + (BLOCK_SPAN-1) gaps
    let block_px_w = BLOCK_SPAN * CELL_SIZE + (BLOCK_SPAN - 1) * CELL_GAP;
    let is_compact = block_px_w < 100;

    // Content height below the header
    let content_h = CELL_SIZE.saturating_sub(HEADER_H);

    rsx! {
        div {
            key: "{id}",
            class: "relative rounded-lg border-2 overflow-hidden select-none \
                    flex flex-col transition-all duration-150 hover:brightness-110",
            style: "grid-column: {grid_col_start} / {grid_col_end}; \
                    grid-row: {grid_row}; \
                    background-color: {bg}; color: {fg}; border-color: {border};",
            title: "{label} ({block_type})",

            if is_compact {
                // Compact (1×1 legacy style): centered short label, no header/content split
                div {
                    class: "flex-1 flex items-center justify-center",
                    span { class: "text-xs font-bold select-none", "{short_label}" }
                }
            } else {
                // Multi-cell: header bar + content area (legacy BlockHeader style)
                div {
                    class: "flex items-center justify-between px-2 select-none truncate",
                    style: "height: {HEADER_H}px; background-color: rgba(0,0,0,0.2);",
                    span {
                        class: "text-[11px] font-semibold truncate",
                        "{label}"
                    }
                }
                div {
                    class: "flex-1 flex items-center justify-center select-none opacity-60",
                    style: "height: {content_h}px;",
                    span { class: "text-[9px] font-medium uppercase tracking-wider", "{block_type}" }
                }
            }
        }
    }
}

// endregion: --- SignalChainGrid
