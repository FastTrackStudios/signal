//! `RigGridPanel` — the Dioxus wrapper around `DynamicGridView` + inspector.
//!
//! The grid-conversion logic that produces the `Vec<GridSlot>` consumed
//! here lives in the headless `signal-browser` crate.

use dioxus::prelude::*;

use crate::dynamic_grid::{
    BlockPickerDropdown, DynamicGridView, GridConnection as DynGridConnection, GridContextMenu,
    GridContextMenuEvent, GridSelection, GridSlot, PICKER_CELL, PICKER_CLICK_POS,
};
use crate::inspector::BlockInspectorPanel;

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
    pub on_save_module_preset_as:
        Option<EventHandler<(Vec<GridSlot>, String, signal_proto::ModuleType)>>,
    #[props(default)]
    pub on_save_module_snapshot: Option<EventHandler<Vec<GridSlot>>>,
    #[props(default)]
    pub on_save_module_snapshot_as:
        Option<EventHandler<(Vec<GridSlot>, String, signal_proto::ModuleType)>>,
}

/// Stateful wrapper around `DynamicGridView` + `BlockPickerDropdown`.
#[component]
pub fn RigGridPanel(props: RigGridPanelProps) -> Element {
    let mut chain = use_signal(|| props.initial_slots.clone());
    let mut selection = use_signal(|| Option::<GridSelection>::None);
    let mut connections = use_signal(Vec::<DynGridConnection>::new);

    let mut last_initial = use_signal(|| props.initial_slots.clone());
    if *last_initial.read() != props.initial_slots {
        tracing::info!(
            "RigGridPanel: syncing {} -> {} slots",
            last_initial.read().len(),
            props.initial_slots.len()
        );
        chain.set(props.initial_slots.clone());
        last_initial.set(props.initial_slots.clone());
        // Preserve the selection across live-state updates (bypass / param
        // changes keep the same slot ids) so the inspector and Space-toggle
        // don't flicker; only drop it if the selected block actually vanished.
        let sel_gone = match selection() {
            Some(GridSelection::Block(id)) => !props.initial_slots.iter().any(|s| s.id == id),
            _ => false,
        };
        if sel_gone {
            selection.set(None);
        }
        connections.set(Vec::new());
    }

    let mut ctx_menu_target = use_signal(|| None::<GridSelection>);

    let picker_cell = PICKER_CELL();
    let picker_pos = PICKER_CLICK_POS();

    let on_param_change_prop = props.on_param_change;
    let on_save_prop = props.on_save;
    let on_save_as_new_prop = props.on_save_as_new;
    let on_selection_change_prop = props.on_selection_change;
    let on_save_block_snapshot_prop = props.on_save_block_snapshot;
    let on_save_block_snapshot_as_prop = props.on_save_block_snapshot_as;
    let on_save_module_preset_as_prop = props.on_save_module_preset_as;
    let on_save_module_snapshot_prop = props.on_save_module_snapshot;
    let on_save_module_snapshot_as_prop = props.on_save_module_snapshot_as;

    let current_chain = chain();
    let current_sel = selection();

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
                on_save: on_save_prop,
                on_save_as_new: on_save_as_new_prop,
                on_save_block_snapshot: on_save_block_snapshot_prop,
                on_save_block_snapshot_as: on_save_block_snapshot_as_prop,
                on_save_module_preset_as: on_save_module_preset_as_prop,
                on_save_module_snapshot: on_save_module_snapshot_prop,
                on_save_module_snapshot_as: on_save_module_snapshot_as_prop,
                on_close: move |_| { ctx_menu_target.set(None); },
            }
        }

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
        // The inspector only exists while something is selected — no idle
        // "select a block…" placeholder bar eating canvas space.
        if current_sel.is_some() {
            BlockInspectorPanel {
                selection: current_sel,
                chain: current_chain,
                on_param_change: param_change_handler,
                on_save: on_save_prop,
                on_save_as_new: on_save_as_new_prop,
            }
        }
    }
}
