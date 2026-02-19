//! Layout helpers for positioning signal chain blocks on a 2D grid.
//!
//! These functions walk a [`SignalChain`] and produce positioned [`FlowBlock`]s
//! for rendering by [`SignalChainGrid`]. Extracted here so both the
//! `collection_browser` and rig-level views can share the same layout logic.

use signal::{SignalChain, SignalNode};

use super::collection_browser::{EngineFlowData, ModuleChainData};
use crate::components::FlowBlock;

/// Walk a `SignalChain` and produce positioned `FlowBlock`s for grid rendering.
///
/// Returns `(blocks, total_columns, total_lanes)`.
pub fn layout_signal_chain(chain: &SignalChain) -> (Vec<FlowBlock>, usize, usize) {
    let mut blocks = Vec::new();
    let mut max_lane: usize = 0;
    let end_col = layout_nodes(chain.nodes(), 0, 0, &mut blocks, &mut max_lane);
    let total_cols = if end_col == 0 { 1 } else { end_col };
    let total_lanes = max_lane + 1;
    (blocks, total_cols, total_lanes)
}

/// Recursive helper: lay out a slice of nodes starting at (base_lane, start_col).
/// Returns the column index after the last placed node.
fn layout_nodes(
    nodes: &[SignalNode],
    base_lane: usize,
    start_col: usize,
    out: &mut Vec<FlowBlock>,
    max_lane: &mut usize,
) -> usize {
    let mut col = start_col;
    for node in nodes {
        match node {
            SignalNode::Block(mb) => {
                let label = mb.label().to_string();
                let short_label = make_short_label(&label);
                let params: Vec<(String, f32)> = mb
                    .overrides()
                    .iter()
                    .map(|o| (o.parameter_id().to_string(), o.value().get()))
                    .collect();
                out.push(FlowBlock {
                    id: mb.id().to_string(),
                    label,
                    short_label,
                    block_type_key: mb.block_type().as_str().to_string(),
                    params,
                    lane: base_lane,
                    column: col,
                });
                if base_lane > *max_lane {
                    *max_lane = base_lane;
                }
                col += 1;
            }
            SignalNode::Split { lanes } => {
                let split_start = col;
                let mut split_end = col;
                for (lane_idx, lane_chain) in lanes.iter().enumerate() {
                    let lane_row = base_lane + lane_idx;
                    let lane_end =
                        layout_nodes(lane_chain.nodes(), lane_row, split_start, out, max_lane);
                    if lane_end > split_end {
                        split_end = lane_end;
                    }
                }
                col = split_end;
            }
        }
    }
    col
}

/// Lay out a full rig's engines into a single unified grid.
///
/// All module chains across all engines and layers are concatenated into one
/// continuous horizontal chain. Each engine's blocks flow left-to-right with
/// wires connecting them — matching the Helix / Quad Cortex single-chain view.
///
/// Returns `(blocks, total_columns, total_lanes)`.
#[allow(dead_code)]
pub fn layout_rig_engines(engines: &[EngineFlowData]) -> (Vec<FlowBlock>, usize, usize) {
    let mut blocks = Vec::new();
    let mut max_lane: usize = 0;
    let mut col: usize = 0;

    for engine in engines {
        for layer in &engine.layers {
            for mc in &layer.module_chains {
                let end_col = layout_nodes(mc.chain.nodes(), 0, col, &mut blocks, &mut max_lane);
                col = end_col;
            }
        }
    }

    let total_cols = if col == 0 { 1 } else { col };
    let total_lanes = max_lane + 1;
    (blocks, total_cols, total_lanes)
}

/// Lay out a list of module chains into a single unified grid.
///
/// Returns `(blocks, total_columns, total_lanes)`.
pub fn layout_module_chains(chains: &[ModuleChainData]) -> (Vec<FlowBlock>, usize, usize) {
    let mut blocks = Vec::new();
    let mut max_lane: usize = 0;
    let mut col: usize = 0;

    for mc in chains {
        let end_col = layout_nodes(mc.chain.nodes(), 0, col, &mut blocks, &mut max_lane);
        col = end_col;
    }

    let total_cols = if col == 0 { 1 } else { col };
    let total_lanes = max_lane + 1;
    (blocks, total_cols, total_lanes)
}

/// Generate a compact 3-character label for 1×1 block display.
///
/// Tries to produce a meaningful abbreviation:
/// - "Compressor" → "CMP"
/// - "TS9 Drive" → "TS9"
/// - "EQ" → "EQ"
fn make_short_label(label: &str) -> String {
    let trimmed = label.trim();
    if trimmed.len() <= 3 {
        return trimmed.to_uppercase();
    }
    // Use first 3 uppercase characters, or first 3 chars if not enough uppercase
    let uppers: String = trimmed
        .chars()
        .filter(|c| c.is_uppercase() || c.is_ascii_digit())
        .collect();
    if uppers.len() >= 2 {
        uppers.chars().take(3).collect()
    } else {
        trimmed.chars().take(3).collect::<String>().to_uppercase()
    }
}
