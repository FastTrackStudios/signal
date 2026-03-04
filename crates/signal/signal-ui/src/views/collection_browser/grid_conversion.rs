//! Grid slot conversion and the `RigGridPanel` wrapper component.
//!
//! Converts domain hierarchy data (`EngineFlowData`, `ModuleChainData`,
//! `SignalChain`) into flat `Vec<GridSlot>` for the `DynamicGridView`.

use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;
use signal::SignalChain;

use super::inspector::BlockInspectorPanel;
use super::types::{EngineFlowData, ModuleChainData};
use crate::components::dynamic_grid::{
    BlockPickerDropdown, DynamicGridView, GridConnection as DynGridConnection, GridContextMenu,
    GridContextMenuEvent, GridSelection, GridSlot, PICKER_CELL, PICKER_CLICK_POS,
};

/// Pre-resolved block parameters keyed by `(preset_id, snapshot_id)`.
/// Built during async data fetching, passed into synchronous grid conversion.
pub type ParamLookup = HashMap<(String, String), Vec<(String, f32)>>;

/// Extract parameters for a `ModuleBlock`.
///
/// 1. For `Inline { block }` sources, read parameters directly.
/// 2. For `PresetSnapshot` sources, look up in the pre-resolved map.
/// 3. For `PresetDefault` sources, look up with snapshot_id = "default".
/// 4. Apply any overrides on top.
fn extract_block_params(mb: &signal::ModuleBlock, lookup: &ParamLookup) -> Vec<(String, f32)> {
    let mut params: Vec<(String, f32)> = match mb.source() {
        signal::ModuleBlockSource::Inline { block } => block
            .parameters()
            .iter()
            .map(|p| (p.name().to_string(), p.value().get()))
            .collect(),
        signal::ModuleBlockSource::PresetSnapshot {
            preset_id,
            snapshot_id,
            ..
        } => lookup
            .get(&(preset_id.to_string(), snapshot_id.to_string()))
            .cloned()
            .unwrap_or_default(),
        signal::ModuleBlockSource::PresetDefault { preset_id, .. } => lookup
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
///
/// Layout strategy (matching legacy `unified_grid_editor`):
///  - Modules flow left-to-right across the row band
///  - A module is **never split** across rows — if it won't fit in the
///    remaining columns, the entire module wraps to the next row band
///  - Row bands are separated by `ROW_BAND_STRIDE` rows (2 empty gap rows)
///  - Split nodes fan out vertically within the module's row band
///  - Whole-module collision avoidance: if a module's footprint overlaps
///    existing blocks, the entire module shifts to a free row position
pub fn engines_to_grid_slots(engines: &[EngineFlowData], params: &ParamLookup) -> Vec<GridSlot> {
    let mut slots = Vec::new();
    let mut occupied = HashSet::new();
    let mut row: usize = 0;

    for engine in engines {
        let engine_key = engine.name.clone();

        // Two-pass layout: measure each layer, then pack them.
        // Pre-compute each layer's dimensions by laying it out into a
        // temporary slot list at origin (0,0).
        struct LayerMeasure {
            width: usize,  // max col + 1
            height: usize, // row count (accounts for split fan-out + wrapping)
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

        // Pack layers left-to-right, wrapping when a layer won't fit.
        let mut col: usize = 0;
        let mut band_start_row = row;
        let mut band_max_height: usize = 0;

        for (li, layer) in engine.layers.iter().enumerate() {
            let layer_key = format!("{}/{}", engine.name, layer.name);
            let measure = &layer_measures[li];

            // Wrap to next row band if this layer won't fit horizontally.
            if col > 0 && col + measure.width > LAYER_PACK_MAX_COLS {
                // Advance past the tallest layer in the current band.
                // +1 row gap only when stacking vertically.
                band_start_row += band_max_height + 1;
                band_max_height = 0;
                col = 0;
            }

            // Place this layer's modules starting at (col, band_start_row).
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

            // Use pre-measured height (from dry-run) for consistent packing.
            band_max_height = band_max_height.max(measure.height);

            // Advance col past this layer for the next one.
            col = col + measure.width + 1; // +1 col gap between side-by-side layers
        }

        // Advance row past this engine.
        // Always add 1 gap row so the engine title strip has clear space above.
        row = band_start_row + band_max_height + 1;
    }

    slots
}

/// Convert a list of module chains into grid slots for `DynamicGridView`.
/// Used for Engine/Layer detail where we show the module chains without
/// the full rig hierarchy.
pub(super) fn module_chains_to_grid_slots(
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
/// Used for Module snapshot detail.
pub(super) fn signal_chain_to_grid_slots(
    chain: &SignalChain,
    module_name: &str,
    module_type: Option<signal::ModuleType>,
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
fn count_chain_width(nodes: &[signal::SignalNode]) -> usize {
    let mut width = 0;
    for node in nodes {
        match node {
            signal::SignalNode::Block(_) => width += 1,
            signal::SignalNode::Split { lanes } => {
                // A split's width is the max width among its wet lanes.
                // Empty (dry) lanes have no cells — the cable layer draws
                // the pass-through.
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
///
/// Lays out blocks and splits relative to `(col_cursor, base_row)`.
/// Splits fan out symmetrically around `base_row`. This function does NOT
/// do collision checking — callers use `place_module` for that.
fn flatten_chain_nodes(
    nodes: &[signal::SignalNode],
    module_key: &str,
    layer_key: Option<&str>,
    engine_key: Option<&str>,
    module_type: Option<signal::ModuleType>,
    col_cursor: &mut usize,
    base_row: usize,
    slots: &mut Vec<GridSlot>,
    param_lookup: &ParamLookup,
) {
    for node in nodes {
        match node {
            signal::SignalNode::Block(mb) => {
                let parameters = extract_block_params(mb, param_lookup);
                let (preset_id, snapshot_id) = match mb.source() {
                    signal::ModuleBlockSource::PresetSnapshot {
                        preset_id,
                        snapshot_id,
                        ..
                    } => (Some(preset_id.to_string()), Some(snapshot_id.to_string())),
                    signal::ModuleBlockSource::PresetDefault { preset_id, .. } => {
                        (Some(preset_id.to_string()), None)
                    }
                    signal::ModuleBlockSource::Inline { .. } => (None, None),
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
            signal::SignalNode::Split { lanes } => {
                let split_start_col = *col_cursor;
                let mut max_col = split_start_col;

                let wet: Vec<&signal::SignalChain> =
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

/// Lay out a module's chain into temp slots, then shift the entire module
/// to a row position where it doesn't collide with already-placed blocks.
///
/// Returns the column cursor after the module (for advancing `layer_col`).
fn place_module(
    nodes: &[signal::SignalNode],
    module_key: &str,
    layer_key: Option<&str>,
    engine_key: Option<&str>,
    module_type: Option<signal::ModuleType>,
    start_col: usize,
    preferred_row: usize,
    slots: &mut Vec<GridSlot>,
    param_lookup: &ParamLookup,
    occupied: &mut HashSet<(usize, usize)>,
) -> usize {
    // 1. Dry-run: lay out at (start_col, preferred_row) into temp slots.
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

    // 2. Compute the module's bounding rows.
    let min_row = temp_slots.iter().map(|s| s.row).min().unwrap();

    // Collect relative (col, row_offset) for collision checking.
    let cells: Vec<(usize, usize)> = temp_slots
        .iter()
        .map(|s| (s.col, s.row - min_row))
        .collect();

    // 3. Find a row where the whole module fits without collision.
    let place_row = find_free_module_row(min_row, &cells, occupied);
    let row_shift = place_row as isize - min_row as isize;

    // 4. Shift all temp slots and commit them.
    for mut slot in temp_slots {
        slot.row = (slot.row as isize + row_shift) as usize;
        occupied.insert((slot.col, slot.row));
        slots.push(slot);
    }

    col_cursor
}

/// Find a row position where all cells of a module can be placed without
/// colliding with occupied positions. Searches outward from `preferred_start`,
/// trying the preferred position first, then alternating above and below.
fn find_free_module_row(
    preferred_start: usize,
    cells: &[(usize, usize)], // (col, row_offset) pairs
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

    // Try preferred position first.
    if fits_at(preferred_start) {
        return preferred_start;
    }

    // Search outward: try above first, then below.
    for offset in 1..50 {
        if preferred_start >= offset && fits_at(preferred_start - offset) {
            return preferred_start - offset;
        }
        if fits_at(preferred_start + offset) {
            return preferred_start + offset;
        }
    }

    // Fallback.
    preferred_start + 10
}

// endregion: --- Converters

// region: --- RigGridPanel

#[derive(Props, Clone, PartialEq)]
pub struct RigGridPanelProps {
    pub initial_slots: Vec<GridSlot>,
    #[props(default)]
    pub on_param_change: Option<EventHandler<(uuid::Uuid, String, f32)>>,
    #[props(default)]
    pub on_save: Option<EventHandler<GridSlot>>,
    #[props(default)]
    pub on_save_as_new: Option<EventHandler<(GridSlot, String)>>,
    #[props(default)]
    pub on_selection_change: Option<EventHandler<Option<GridSelection>>>,
    // Block snapshot callbacks
    #[props(default)]
    pub on_save_block_snapshot: Option<EventHandler<GridSlot>>,
    #[props(default)]
    pub on_save_block_snapshot_as: Option<EventHandler<(GridSlot, String)>>,
    // Module save callbacks
    #[props(default)]
    pub on_save_module_preset_as: Option<EventHandler<(Vec<GridSlot>, String, signal::ModuleType)>>,
    #[props(default)]
    pub on_save_module_snapshot: Option<EventHandler<Vec<GridSlot>>>,
    #[props(default)]
    pub on_save_module_snapshot_as:
        Option<EventHandler<(Vec<GridSlot>, String, signal::ModuleType)>>,
}

/// Stateful wrapper around `DynamicGridView` + `BlockPickerDropdown`.
///
/// Owns local signals for chain, selection, and connections so the
/// detail panel can render an interactive grid without lifting state further.
#[component]
pub fn RigGridPanel(props: RigGridPanelProps) -> Element {
    let mut chain = use_signal(|| props.initial_slots.clone());
    let mut selection = use_signal(|| Option::<GridSelection>::None);
    let mut connections = use_signal(Vec::<DynGridConnection>::new);

    // Sync when the parent passes new data (e.g. user selects a different preset).
    // We track the previous initial_slots to detect prop changes across renders,
    // because use_effect cannot reactively track plain (non-signal) props.
    let mut last_initial = use_signal(|| props.initial_slots.clone());
    if *last_initial.read() != props.initial_slots {
        tracing::info!(
            "RigGridPanel: syncing {} -> {} slots",
            last_initial.read().len(),
            props.initial_slots.len()
        );
        chain.set(props.initial_slots.clone());
        last_initial.set(props.initial_slots.clone());
        selection.set(None);
        connections.set(Vec::new());
    }

    // Context menu target — tracks which block/module was right-clicked.
    // Open/close and positioning are handled by lumen-blocks ContextMenuTrigger.
    // Name input state lives inside GridContextMenu.
    let mut ctx_menu_target = use_signal(|| None::<GridSelection>);

    let picker_cell = PICKER_CELL();
    let picker_pos = PICKER_CLICK_POS();

    let on_param_change_prop = props.on_param_change.clone();
    let on_save_prop = props.on_save.clone();
    let on_save_as_new_prop = props.on_save_as_new.clone();
    let on_selection_change_prop = props.on_selection_change.clone();
    let on_save_block_snapshot_prop = props.on_save_block_snapshot.clone();
    let on_save_block_snapshot_as_prop = props.on_save_block_snapshot_as.clone();
    let on_save_module_preset_as_prop = props.on_save_module_preset_as.clone();
    let on_save_module_snapshot_prop = props.on_save_module_snapshot.clone();
    let on_save_module_snapshot_as_prop = props.on_save_module_snapshot_as.clone();

    let current_chain = chain();
    let current_sel = selection();

    // Handler: update local chain when a parameter changes in the inspector.
    let param_change_handler = {
        EventHandler::new(move |(id, name, value): (uuid::Uuid, String, f32)| {
            let mut current = chain();
            if let Some(slot) = current.iter_mut().find(|s| s.id == id) {
                if let Some(p) = slot.parameters.iter_mut().find(|(n, _)| *n == name) {
                    p.1 = value;
                }
            }
            chain.set(current);
            if let Some(ref cb) = on_param_change_prop {
                cb.call((id, name, value));
            }
        })
    };

    rsx! {
        div {
            class: "flex-1 min-h-0 flex flex-col",
            DynamicGridView {
                chain: current_chain.clone(),
                selection: current_sel.clone(),
                connections: connections(),
                on_chain_change: move |new_chain: Vec<GridSlot>| {
                    chain.set(new_chain);
                },
                on_connections_change: move |new_conns: Vec<DynGridConnection>| {
                    connections.set(new_conns);
                },
                on_select: move |sel: Option<GridSelection>| {
                    selection.set(sel.clone());
                    if let Some(ref cb) = on_selection_change_prop {
                        cb.call(sel);
                    }
                },
                on_context_menu: move |evt: GridContextMenuEvent| {
                    ctx_menu_target.set(Some(evt.target));
                },
            }
            GridContextMenu {
                target: ctx_menu_target(),
                chain: current_chain.clone(),
                on_save: on_save_prop.clone(),
                on_save_as_new: on_save_as_new_prop.clone(),
                on_save_block_snapshot: on_save_block_snapshot_prop.clone(),
                on_save_block_snapshot_as: on_save_block_snapshot_as_prop.clone(),
                on_save_module_preset_as: on_save_module_preset_as_prop.clone(),
                on_save_module_snapshot: on_save_module_snapshot_prop.clone(),
                on_save_module_snapshot_as: on_save_module_snapshot_as_prop.clone(),
                on_close: move |_| { ctx_menu_target.set(None); },
            }
        }

        // Block picker rendered outside the transform context
        if let Some((col, row)) = picker_cell {
            BlockPickerDropdown {
                col: col,
                row: row,
                click_x: picker_pos.0,
                click_y: picker_pos.1,
                on_add_slot: move |slot: GridSlot| {
                    let mut current = chain();
                    current.push(slot);
                    chain.set(current);
                    *PICKER_CELL.write() = None;
                },
                on_add_slots: move |slots: Vec<GridSlot>| {
                    let mut current = chain();
                    current.extend(slots);
                    chain.set(current);
                    *PICKER_CELL.write() = None;
                },
                on_close: move |_| {
                    *PICKER_CELL.write() = None;
                },
            }
        }
        // Inspector panel for selected block / module
        BlockInspectorPanel {
            selection: current_sel,
            chain: current_chain,
            on_param_change: param_change_handler,
            on_save: on_save_prop.clone(),
            on_save_as_new: on_save_as_new_prop.clone(),
        }
    }
}

// endregion: --- RigGridPanel
