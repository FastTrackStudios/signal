//! Grid slot conversion (headless).
//!
//! Converts domain hierarchy data (`EngineFlowData`, `ModuleChainData`,
//! `SignalChain`) into flat `Vec<GridSlot>` for downstream rendering.
//!
//! Rendering itself (the `RigGridPanel` Dioxus component) lives in
//! `signal-ui::views::collection_browser`.

use std::collections::{HashMap, HashSet};

use signal_proto::signal_chain::SignalChain;
use signal_proto::template::{RigTemplate, SignalChainTemplate, SignalNodeTemplate};

use crate::{EngineFlowData, GridSlot, ModuleChainData};

/// Pre-resolved block parameters keyed by `(preset_id, snapshot_id)`.
/// Built during async data fetching, passed into synchronous grid conversion.
pub type ParamLookup = HashMap<(String, String), Vec<(String, f32)>>;

/// Extract parameters for a `ModuleBlock`.
///
/// 1. For `Inline { block }` sources, read parameters directly.
/// 2. For `PresetSnapshot` sources, look up in the pre-resolved map.
/// 3. For `PresetDefault` sources, look up with snapshot_id = "default".
/// 4. Apply any overrides on top.
fn extract_block_params(
    mb: &signal_proto::ModuleBlock,
    lookup: &ParamLookup,
) -> Vec<(String, f32)> {
    let mut params: Vec<(String, f32)> = match mb.source() {
        signal_proto::ModuleBlockSource::Inline { block } => block
            .parameters()
            .iter()
            .map(|p| (p.name().to_string(), p.value().get()))
            .collect(),
        signal_proto::ModuleBlockSource::PresetSnapshot {
            preset_id,
            snapshot_id,
            ..
        } => lookup
            .get(&(preset_id.to_string(), snapshot_id.to_string()))
            .cloned()
            .unwrap_or_default(),
        signal_proto::ModuleBlockSource::PresetDefault { preset_id, .. } => lookup
            .get(&(preset_id.to_string(), "default".to_string()))
            .cloned()
            .unwrap_or_default(),
    };
    // Apply overrides
    for ov in mb.overrides() {
        if let Some(p) = params
            .iter_mut()
            .find(|(name, _)| name == ov.parameter_id())
        {
            p.1 = ov.value().get();
        }
    }
    params
}

// region: --- Constants

/// Preferred max columns before wrapping modules to the next row band
/// *within* a single layer.
const SOFT_MAX_COLS: usize = 14;

/// Max columns before layers wrap to the next vertical band.
/// Wider than SOFT_MAX_COLS because horizontal scrolling handles overflow,
/// and side-by-side layers are much more compact than stacking vertically.
const LAYER_PACK_MAX_COLS: usize = 24;

/// Gap rows when a module wraps within a layer.
/// Must be >= max split fan-out (typically 2 wet lanes = 1 extra row)
/// since splits fan upward into this gap space.
const ROW_BAND_STRIDE: usize = 2;

// endregion: --- Constants

// region: --- Converters

/// Flatten the full rig hierarchy (engines → layers → modules → blocks)
/// into a single `Vec<GridSlot>` for the interactive `DynamicGridView`.
pub fn engines_to_grid_slots(engines: &[EngineFlowData], params: &ParamLookup) -> Vec<GridSlot> {
    let mut slots = Vec::new();
    let mut occupied = HashSet::new();
    let mut row: usize = 0;

    for engine in engines {
        let engine_key = engine.name.clone();

        struct LayerMeasure {
            width: usize,
            height: usize,
        }

        let mut layer_measures: Vec<LayerMeasure> = Vec::new();
        for layer in &engine.layers {
            let mut temp_slots = Vec::new();
            let mut temp_col: usize = 0;
            let temp_row: usize = 0;
            let mut temp_base_row = temp_row;
            for mc in &layer.module_chains {
                let module_width = count_chain_width(mc.chain.nodes());
                if temp_col > 0 && temp_col + module_width > SOFT_MAX_COLS {
                    temp_col = 0;
                    temp_base_row += ROW_BAND_STRIDE;
                }
                let mut col_cursor = temp_col;
                flatten_chain_nodes(
                    mc.chain.nodes(),
                    "measure",
                    None,
                    None,
                    None,
                    &mut col_cursor,
                    temp_base_row,
                    &mut temp_slots,
                    params,
                );
                temp_col = col_cursor;
            }
            let max_col = temp_slots.iter().map(|s| s.col).max().unwrap_or(0);
            let max_row = temp_slots.iter().map(|s| s.row).max().unwrap_or(0);
            layer_measures.push(LayerMeasure {
                width: max_col + 1,
                height: max_row + 1,
            });
        }

        let mut col: usize = 0;
        let mut band_start_row = row;
        let mut band_max_height: usize = 0;

        for (li, layer) in engine.layers.iter().enumerate() {
            let layer_key = format!("{}/{}", engine.name, layer.name);
            let measure = &layer_measures[li];

            if col > 0 && col + measure.width > LAYER_PACK_MAX_COLS {
                band_start_row += band_max_height + 1;
                band_max_height = 0;
                col = 0;
            }

            let layer_base_row = band_start_row;
            let mut layer_col = col;
            let mut layer_row = layer_base_row;

            for mc in &layer.module_chains {
                let module_key = format!("{}/{}/{}", engine.name, layer.name, mc.name);
                let mt = mc.module_type;
                let module_width = count_chain_width(mc.chain.nodes());

                if layer_col > col && layer_col + module_width > col + SOFT_MAX_COLS {
                    layer_col = col;
                    layer_row += ROW_BAND_STRIDE;
                }

                layer_col = place_module(
                    mc.chain.nodes(),
                    &module_key,
                    Some(&layer_key),
                    Some(&engine_key),
                    mt,
                    layer_col,
                    layer_row,
                    &mut slots,
                    params,
                    &mut occupied,
                );
            }

            band_max_height = band_max_height.max(measure.height);

            col = col + measure.width + 1;
        }

        row = band_start_row + band_max_height + 1;
    }

    slots
}

/// Convert a list of module chains into grid slots for `DynamicGridView`.
pub fn module_chains_to_grid_slots(
    chains: &[ModuleChainData],
    params: &ParamLookup,
) -> Vec<GridSlot> {
    let mut slots = Vec::new();
    let mut occupied = HashSet::new();
    let mut col: usize = 0;
    let mut row: usize = 0;

    for mc in chains {
        let module_key = mc.name.clone();
        let mt = mc.module_type;
        let module_width = count_chain_width(mc.chain.nodes());

        if col > 0 && col + module_width > SOFT_MAX_COLS {
            col = 0;
            row += ROW_BAND_STRIDE;
        }

        col = place_module(
            mc.chain.nodes(),
            &module_key,
            None,
            None,
            mt,
            col,
            row,
            &mut slots,
            params,
            &mut occupied,
        );
    }
    slots
}

/// Convert a single signal chain into grid slots for `DynamicGridView`.
pub fn signal_chain_to_grid_slots(
    chain: &SignalChain,
    module_name: &str,
    module_type: Option<signal_proto::ModuleType>,
    params: &ParamLookup,
) -> Vec<GridSlot> {
    let mut slots = Vec::new();
    let mut col_cursor = 0;
    flatten_chain_nodes(
        chain.nodes(),
        module_name,
        None,
        None,
        module_type,
        &mut col_cursor,
        0,
        &mut slots,
        params,
    );
    slots
}

/// Count the number of columns a chain of nodes needs (for wrapping decisions).
fn count_chain_width(nodes: &[signal_proto::SignalNode]) -> usize {
    let mut width = 0;
    for node in nodes {
        match node {
            signal_proto::SignalNode::Block(_) => width += 1,
            signal_proto::SignalNode::Split { lanes } => {
                let max_lane_width = lanes
                    .iter()
                    .filter(|lane| !lane.is_empty())
                    .map(|lane| count_chain_width(lane.nodes()))
                    .max()
                    .unwrap_or(0);
                width += max_lane_width;
            }
        }
    }
    width
}

/// Recursively flatten SignalNodes into GridSlots, handling splits.
#[allow(clippy::too_many_arguments)]
fn flatten_chain_nodes(
    nodes: &[signal_proto::SignalNode],
    module_key: &str,
    layer_key: Option<&str>,
    engine_key: Option<&str>,
    module_type: Option<signal_proto::ModuleType>,
    col_cursor: &mut usize,
    base_row: usize,
    slots: &mut Vec<GridSlot>,
    param_lookup: &ParamLookup,
) {
    for node in nodes {
        match node {
            signal_proto::SignalNode::Block(mb) => {
                let parameters = extract_block_params(mb, param_lookup);
                let (preset_id, snapshot_id) = match mb.source() {
                    signal_proto::ModuleBlockSource::PresetSnapshot {
                        preset_id,
                        snapshot_id,
                        ..
                    } => (Some(preset_id.to_string()), Some(snapshot_id.to_string())),
                    signal_proto::ModuleBlockSource::PresetDefault { preset_id, .. } => {
                        (Some(preset_id.to_string()), None)
                    }
                    signal_proto::ModuleBlockSource::Inline { .. } => (None, None),
                };
                slots.push(GridSlot {
                    id: uuid::Uuid::new_v4(),
                    block_type: mb.block_type(),
                    block_preset_name: Some(mb.label().to_string()),
                    plugin_name: None,
                    col: *col_cursor,
                    row: base_row,
                    module_group: Some(module_key.to_string()),
                    module_type,
                    layer_group: layer_key.map(|s| s.to_string()),
                    engine_group: engine_key.map(|s| s.to_string()),
                    is_template: false,
                    bypassed: false,
                    is_phantom: false,
                    parameters,
                    preset_id,
                    snapshot_id,
                });
                *col_cursor += 1;
            }
            signal_proto::SignalNode::Split { lanes } => {
                let split_start_col = *col_cursor;
                let mut max_col = split_start_col;

                let wet: Vec<&signal_proto::SignalChain> =
                    lanes.iter().filter(|l| !l.is_empty()).collect();
                let wet_count = wet.len();
                let vert_offset = wet_count.saturating_sub(1) / 2;

                for (i, lane) in wet.iter().enumerate() {
                    let lane_row = (base_row + i).saturating_sub(vert_offset);
                    let mut lane_col = split_start_col;
                    flatten_chain_nodes(
                        lane.nodes(),
                        module_key,
                        layer_key,
                        engine_key,
                        module_type,
                        &mut lane_col,
                        lane_row,
                        slots,
                        param_lookup,
                    );
                    if lane_col > max_col {
                        max_col = lane_col;
                    }
                }

                *col_cursor = max_col;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn place_module(
    nodes: &[signal_proto::SignalNode],
    module_key: &str,
    layer_key: Option<&str>,
    engine_key: Option<&str>,
    module_type: Option<signal_proto::ModuleType>,
    start_col: usize,
    preferred_row: usize,
    slots: &mut Vec<GridSlot>,
    param_lookup: &ParamLookup,
    occupied: &mut HashSet<(usize, usize)>,
) -> usize {
    let mut temp_slots = Vec::new();
    let mut col_cursor = start_col;
    flatten_chain_nodes(
        nodes,
        module_key,
        layer_key,
        engine_key,
        module_type,
        &mut col_cursor,
        preferred_row,
        &mut temp_slots,
        param_lookup,
    );

    if temp_slots.is_empty() {
        return col_cursor;
    }

    let min_row = temp_slots.iter().map(|s| s.row).min().unwrap();

    let cells: Vec<(usize, usize)> = temp_slots
        .iter()
        .map(|s| (s.col, s.row - min_row))
        .collect();

    let place_row = find_free_module_row(min_row, &cells, occupied);
    let row_shift = place_row as isize - min_row as isize;

    for mut slot in temp_slots {
        slot.row = (slot.row as isize + row_shift) as usize;
        occupied.insert((slot.col, slot.row));
        slots.push(slot);
    }

    col_cursor
}

fn find_free_module_row(
    preferred_start: usize,
    cells: &[(usize, usize)],
    occupied: &HashSet<(usize, usize)>,
) -> usize {
    let fits_at = |start_row: usize| -> bool {
        for &(col, row_off) in cells {
            if occupied.contains(&(col, start_row + row_off)) {
                return false;
            }
        }
        true
    };

    if fits_at(preferred_start) {
        return preferred_start;
    }

    for offset in 1..50 {
        if preferred_start >= offset && fits_at(preferred_start - offset) {
            return preferred_start - offset;
        }
        if fits_at(preferred_start + offset) {
            return preferred_start + offset;
        }
    }

    preferred_start + 10
}

// endregion: --- Converters

// region: --- Template converters

/// Flatten a rig *template* (an all-unassigned structural blueprint) into grid
/// slots for the interactive `DynamicGridView`.
///
/// Unlike [`engines_to_grid_slots`], which operates on resolved rig data, this
/// walks the template hierarchy directly (engines → layers → modules → template
/// nodes). Every emitted slot is marked `is_template = true` so the grid renders
/// each block as a dashed placeholder awaiting a plugin/preset assignment.
///
/// Layout mirrors the resolved converters: modules flow left→right, wrap at
/// `SOFT_MAX_COLS`, and `Split` nodes fan their wet lanes into parallel rows.
/// Empty lanes (dry pass-through) are skipped, exactly as in
/// `flatten_chain_nodes`.
pub fn template_to_grid_slots(rig: &RigTemplate) -> Vec<GridSlot> {
    let mut slots = Vec::new();
    let mut engine_base_row: usize = 0;

    for engine in &rig.engines {
        let engine_key = engine.name.clone();
        let mut engine_height: usize = 1;
        // Each layer is laid out in its own horizontal band, offset to the right
        // of the previous layer within this engine.
        let mut layer_col_offset: usize = 0;

        for layer in &engine.layers {
            let layer_key = format!("{}/{}", engine.name, layer.name);
            let mut col = layer_col_offset;
            let mut base_row = engine_base_row;

            for module in &layer.modules {
                let module_key = format!("{}/{}/{}", engine.name, layer.name, module.name);
                let width = count_template_width(&module.chain.nodes);

                // Wrap to the next row band when this module would overflow.
                if col > layer_col_offset && col + width > layer_col_offset + SOFT_MAX_COLS {
                    col = layer_col_offset;
                    base_row += ROW_BAND_STRIDE;
                }

                let before = slots.len();
                let mut cursor = col;
                flatten_template_nodes(
                    &module.chain.nodes,
                    &module_key,
                    Some(&layer_key),
                    Some(&engine_key),
                    Some(module.module_type),
                    &mut cursor,
                    base_row,
                    &mut slots,
                );
                col = cursor;

                for s in &slots[before..] {
                    engine_height = engine_height.max((s.row + 1).saturating_sub(engine_base_row));
                }
            }

            // Next layer starts to the right of everything placed so far.
            let placed_max_col = slots.iter().map(|s| s.col).max();
            layer_col_offset = placed_max_col.map_or(layer_col_offset, |c| c + 2);
        }

        engine_base_row += engine_height + 1;
    }

    slots
}

/// Column width a template chain needs (for wrap decisions). Mirror of
/// [`count_chain_width`] for template nodes.
fn count_template_width(nodes: &[SignalNodeTemplate]) -> usize {
    let mut width = 0;
    for node in nodes {
        match node {
            SignalNodeTemplate::Block(_) => width += 1,
            SignalNodeTemplate::Split { lanes } => {
                let max_lane_width = lanes
                    .iter()
                    .filter(|lane| !lane.nodes.is_empty())
                    .map(|lane| count_template_width(&lane.nodes))
                    .max()
                    .unwrap_or(0);
                width += max_lane_width;
            }
        }
    }
    width
}

/// Recursively flatten template nodes into template placeholder slots. Mirror of
/// [`flatten_chain_nodes`] but reads `SignalNodeTemplate` and emits
/// `is_template = true` slots with no parameters.
#[allow(clippy::too_many_arguments)]
fn flatten_template_nodes(
    nodes: &[SignalNodeTemplate],
    module_key: &str,
    layer_key: Option<&str>,
    engine_key: Option<&str>,
    module_type: Option<signal_proto::ModuleType>,
    col_cursor: &mut usize,
    base_row: usize,
    slots: &mut Vec<GridSlot>,
) {
    for node in nodes {
        match node {
            SignalNodeTemplate::Block(bt) => {
                slots.push(GridSlot {
                    id: uuid::Uuid::new_v4(),
                    block_type: bt.block_type,
                    block_preset_name: Some(bt.name.clone()),
                    plugin_name: None,
                    col: *col_cursor,
                    row: base_row,
                    module_group: Some(module_key.to_string()),
                    module_type,
                    layer_group: layer_key.map(|s| s.to_string()),
                    engine_group: engine_key.map(|s| s.to_string()),
                    is_template: true,
                    bypassed: false,
                    is_phantom: false,
                    parameters: Vec::new(),
                    preset_id: None,
                    snapshot_id: None,
                });
                *col_cursor += 1;
            }
            SignalNodeTemplate::Split { lanes } => {
                let split_start_col = *col_cursor;
                let mut max_col = split_start_col;

                let wet: Vec<&SignalChainTemplate> =
                    lanes.iter().filter(|l| !l.nodes.is_empty()).collect();
                let wet_count = wet.len();
                let vert_offset = wet_count.saturating_sub(1) / 2;

                for (i, lane) in wet.iter().enumerate() {
                    let lane_row = (base_row + i).saturating_sub(vert_offset);
                    let mut lane_col = split_start_col;
                    flatten_template_nodes(
                        &lane.nodes,
                        module_key,
                        layer_key,
                        engine_key,
                        module_type,
                        &mut lane_col,
                        lane_row,
                        slots,
                    );
                    if lane_col > max_col {
                        max_col = lane_col;
                    }
                }

                *col_cursor = max_col;
            }
        }
    }
}

// endregion: --- Template converters
