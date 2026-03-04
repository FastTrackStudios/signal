//! Dynamic Grid View — auto-expanding 2D grid for block composition.
//!
//! Renders GridSlots on a CSS grid that grows as blocks are added.
//! Empty cells show a faint dashed border with hover highlight and click-to-add
//! via a searchable block type picker dropdown.

mod block_cell;
pub mod block_picker;
pub(crate) mod cables;
pub mod context_menu;
mod empty_cell;
pub(crate) mod interaction;
pub(crate) mod layout;
pub mod minimap;
mod module_bg;
pub(crate) mod types;

use dioxus::html::input_data::MouseButton;
use dioxus::prelude::dioxus_elements::geometry::WheelDelta;
use dioxus::prelude::*;
use std::rc::Rc;
use uuid::Uuid;

pub use types::GridSlot;

use interaction::*;
pub use interaction::{
    GridConnection, GridContextMenuEvent, GridSelection, GRID_CONNECTIONS, PICKER_CELL,
    PICKER_CLICK_POS,
};

pub use block_picker::BlockPickerDropdown;
pub(crate) use cables::*;
pub use context_menu::GridContextMenu;
pub(crate) use layout::*;

use types::{BlockVisualState, ModuleVisualState};

use block_cell::GridBlockCell;
use empty_cell::EmptyGridCell;
use module_bg::{ContainerBackground, ModuleBackground};

// ─────────────────────────────────────────────────────────────────────────────
// DynamicGridView
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct DynamicGridViewProps {
    pub chain: Vec<GridSlot>,
    pub selection: Option<GridSelection>,
    #[props(default)]
    pub connections: Vec<GridConnection>,
    pub on_chain_change: EventHandler<Vec<GridSlot>>,
    pub on_connections_change: EventHandler<Vec<GridConnection>>,
    pub on_select: EventHandler<Option<GridSelection>>,
    #[props(default)]
    pub on_group_reorder: Option<EventHandler<(String, String)>>,
    #[props(default)]
    pub on_bypass_toggle: Option<EventHandler<(GridSelection, bool)>>,
    #[props(default)]
    pub on_context_menu: Option<EventHandler<GridContextMenuEvent>>,
}

#[component]
pub fn DynamicGridView(props: DynamicGridViewProps) -> Element {
    let selection = props.selection.clone();
    let selection_for_key = selection.clone();

    // ── Viewport state (pan/zoom) ────────────────────────────────
    let mut pan_x = use_signal(|| 0.0f64);
    let mut pan_y = use_signal(|| 0.0f64);
    let mut zoom = use_signal(|| 1.0f64);
    let mut viewport_left = use_signal(|| 0.0f64);
    let mut viewport_top = use_signal(|| 0.0f64);
    let mut viewport_w = use_signal(|| 0.0f64);
    let mut viewport_h = use_signal(|| 0.0f64);

    // ── Interaction state machine ───
    let mut interaction = use_signal(|| InteractionMode::Idle);
    let mut hovered_port_slot = use_signal(|| None::<(Uuid, bool)>);
    let mut last_valid_group_delta = use_signal(|| (0isize, 0isize));
    let mut committed_chain = use_signal(|| None::<Vec<GridSlot>>);

    // ── Mounted element for viewport measurement ──
    let mut mounted_el: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    let update_viewport = move || {
        if let Some(el) = mounted_el.read().as_ref() {
            let el_clone = el.clone();
            spawn(async move {
                if let Ok(rect) = el_clone.get_client_rect().await {
                    let w = rect.width();
                    let h = rect.height();
                    let x = rect.origin.x;
                    let y = rect.origin.y;
                    if w > 0.0 && h > 0.0 {
                        viewport_w.set(w);
                        viewport_h.set(h);
                        viewport_left.set(x);
                        viewport_top.set(y);
                    }
                }
            });
        }
    };

    // Use committed_chain if it exists, otherwise use props.chain
    let chain_snapshot: Vec<GridSlot> = {
        let committed = committed_chain.read();
        if let Some(ref cc) = *committed {
            if *cc == props.chain {
                drop(committed);
                committed_chain.set(None);
                props.chain.clone()
            } else {
                cc.clone()
            }
        } else {
            props.chain.clone()
        }
    };

    let mut chain_signal = use_signal(|| Vec::<GridSlot>::new());
    if *chain_signal.read() != chain_snapshot {
        chain_signal.set(chain_snapshot.clone());
    }

    // Build virtual chain reflecting current drag state
    let drag_chain: Vec<GridSlot> = if let Some(gd) = interaction().group_drag().cloned() {
        let step = (CELL_SIZE + CELL_GAP) as f64;
        let cz = zoom();
        let dx_px = (gd.mouse_x - gd.start_mouse_x) / cz;
        let dy_px = (gd.mouse_y - gd.start_mouse_y) / cz;
        let raw_col_delta = (dx_px / step).round() as isize;
        let raw_row_delta = (dy_px / step).round() as isize;

        let (col_delta, row_delta) = if group_move_is_valid(
            &chain_snapshot,
            &gd.group_name,
            raw_col_delta,
            raw_row_delta,
        ) {
            last_valid_group_delta.set((raw_col_delta, raw_row_delta));
            (raw_col_delta, raw_row_delta)
        } else {
            last_valid_group_delta()
        };

        if gd.shift_held {
            chain_snapshot
                .iter()
                .filter(|s| s.module_group.as_deref() != Some(&gd.group_name))
                .cloned()
                .collect()
        } else {
            let moved_positions: Vec<(isize, isize)> = chain_snapshot
                .iter()
                .map(|s| {
                    if s.module_group.as_deref() == Some(&gd.group_name) {
                        (s.col as isize + col_delta, s.row as isize + row_delta)
                    } else {
                        (s.col as isize, s.row as isize)
                    }
                })
                .collect();
            let min_col = moved_positions
                .iter()
                .map(|p| p.0)
                .min()
                .unwrap_or(0)
                .min(0);
            let min_row = moved_positions
                .iter()
                .map(|p| p.1)
                .min()
                .unwrap_or(0)
                .min(0);
            chain_snapshot
                .iter()
                .zip(moved_positions.iter())
                .map(|(s, &(mc, mr))| {
                    let mut slot = s.clone();
                    slot.col = (mc - min_col) as usize;
                    slot.row = (mr - min_row) as usize;
                    slot
                })
                .collect()
        }
    } else {
        chain_snapshot.clone()
    };

    let is_any_drag = interaction().is_any_drag();
    let (base_cols, base_rows) = compute_grid_bounds(&drag_chain);
    let (cols, rows) = if is_any_drag {
        (base_cols + 1, base_rows + 1)
    } else {
        (base_cols, base_rows)
    };

    let nat_w = grid_natural_width(cols);
    let nat_h = grid_natural_height(rows);
    let col_template = format!("repeat({cols}, {CELL_SIZE}px)");
    let row_template = format!("repeat({rows}, {CELL_SIZE}px)");
    let grid_style = format!(
        "grid-template-columns: {col_template}; grid-template-rows: {row_template}; gap: {CELL_GAP}px; \
         width: {nat_w}px; height: {nat_h}px;",
    );

    let cables = resolve_cables_or_connections(&drag_chain, &props.connections);
    let module_ports = compute_module_ports(&drag_chain);
    let module_groups = compute_module_groups(&drag_chain);
    let container_groups = compute_container_groups(&drag_chain);

    // Cells that overlap a layer's left-side fader panel should not be interactive.
    let layer_blocked_cells: std::collections::HashSet<(usize, usize)> = {
        let step = (CELL_SIZE + CELL_GAP) as f64;
        let cs = CELL_SIZE as f64;
        let mut blocked = std::collections::HashSet::new();
        for cg in container_groups.iter() {
            if cg.level != ContainerLevel::Layer {
                continue;
            }
            // The fader panel occupies [cg.x, cg.x + LAYER_LEFT_PAD] × [cg.y, cg.y + cg.h]
            let panel_right = cg.x + LAYER_LEFT_PAD as f64;
            let panel_bottom = cg.y + cg.h;
            for r in 0..rows {
                let cell_top = r as f64 * step;
                let cell_bottom = cell_top + cs;
                if cell_top >= panel_bottom || cell_bottom <= cg.y {
                    continue;
                }
                for c in 0..cols {
                    let cell_left = c as f64 * step;
                    let cell_right = cell_left + cs;
                    if cell_left < panel_right && cell_right > cg.x {
                        blocked.insert((c, r));
                    }
                }
            }
        }
        blocked
    };

    let current_zoom = zoom();
    let block_drag = interaction().dragged_slot_id().and_then(|_| {
        if let InteractionMode::BlockDrag(ref d) = interaction() {
            Some(d.clone())
        } else {
            None
        }
    });
    let hover_cell: Option<(usize, usize)> = block_drag.as_ref().map(|d| {
        let step = (CELL_SIZE + CELL_GAP) as f64;
        let dx = (d.mouse_x - d.start_mouse_x) / current_zoom;
        let dy = (d.mouse_y - d.start_mouse_y) / current_zoom;
        let col_delta = (dx / step).round() as isize;
        let row_delta = (dy / step).round() as isize;
        let new_col = (d.origin_col as isize + col_delta).max(0) as usize;
        let new_row = (d.origin_row as isize + row_delta).max(0) as usize;
        (
            new_col.min(cols.saturating_sub(1)),
            new_row.min(rows.saturating_sub(1)),
        )
    });

    let dragged_slot_id = interaction().dragged_slot_id();

    let group_drop_target: Option<GroupDropTarget> = interaction().group_drag().map(|gd| {
        let cz = zoom();
        let dx_px = (gd.mouse_x - gd.start_mouse_x) / cz;
        let dy_px = (gd.mouse_y - gd.start_mouse_y) / cz;
        let step = (CELL_SIZE + CELL_GAP) as f64;
        let _col_delta = (dx_px / step).round() as isize;
        let _row_delta = (dy_px / step).round() as isize;

        if let Some(dragged) = module_groups.iter().find(|g| g.name == gd.group_name) {
            let target_cx = dragged.x + dragged.w * 0.5;
            let target_cy = dragged.y + dragged.h * 0.5;

            for g in module_groups.iter() {
                if g.name != gd.group_name
                    && target_cx >= g.x
                    && target_cx <= g.x + g.w
                    && target_cy >= g.y
                    && target_cy <= g.y + g.h
                {
                    return GroupDropTarget::SwapWith(g.name.clone());
                }
            }
        }

        let (valid_dc, valid_dr) = last_valid_group_delta();
        GroupDropTarget::MoveDelta(valid_dc, valid_dr)
    });

    // ── Compute actual content bounds ─────────────────────────
    let content_bounds = {
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = 0.0f64;
        let mut max_y = 0.0f64;

        let step = (CELL_SIZE + CELL_GAP) as f64;
        for slot in chain_snapshot.iter() {
            let cx = slot.col as f64 * step;
            let cy = slot.row as f64 * step;
            min_x = min_x.min(cx);
            min_y = min_y.min(cy);
            max_x = max_x.max(cx + CELL_SIZE as f64);
            max_y = max_y.max(cy + CELL_SIZE as f64);
        }

        for group in &module_groups {
            min_x = min_x.min(group.x);
            min_y = min_y.min(group.y);
            max_x = max_x.max(group.x + group.w);
            max_y = max_y.max(group.y + group.h);
        }
        for cg in &container_groups {
            min_x = min_x.min(cg.x);
            min_y = min_y.min(cg.y);
            max_x = max_x.max(cg.x + cg.w);
            max_y = max_y.max(cg.y + cg.h);
        }

        if min_x == f64::MAX {
            (0.0, 0.0, nat_w as f64, nat_h as f64)
        } else {
            (min_x, min_y, max_x - min_x, max_y - min_y)
        }
    };
    let (content_offset_x, content_offset_y, content_w, content_h) = content_bounds;

    // ── Auto-fit ──────────────────────────────────────────────
    let mut last_fit_state = use_signal(|| (0.0f64, 0.0f64, 0.0f64, 0.0f64));
    {
        let vw = viewport_w();
        let vh = viewport_h();
        let (last_vw, last_vh, last_cw, last_ch) = last_fit_state();
        let viewport_changed = (last_vw - vw).abs() > 10.0 || (last_vh - vh).abs() > 10.0;
        let content_changed =
            (last_cw - content_w).abs() > 1.0 || (last_ch - content_h).abs() > 1.0;
        if (viewport_changed || content_changed)
            && vw > 1.0
            && vh > 1.0
            && content_w > 0.0
            && content_h > 0.0
        {
            last_fit_state.set((vw, vh, content_w, content_h));
            let padding = 20.0;
            let avail_w = vw - padding * 2.0;
            let avail_h = vh - padding * 2.0;
            let fit_zoom = (avail_w / content_w)
                .min(avail_h / content_h)
                .clamp(0.1, 3.0);
            let scaled_w = content_w * fit_zoom;
            let scaled_h = content_h * fit_zoom;
            pan_x.set((vw - scaled_w) / 2.0 - content_offset_x * fit_zoom);
            pan_y.set((vh - scaled_h) / 2.0 - content_offset_y * fit_zoom);
            zoom.set(fit_zoom);
        }
    }

    let port_half = PORT_SIZE / 2.0;

    let cursor = match interaction() {
        InteractionMode::Pan { .. }
        | InteractionMode::BlockDrag(_)
        | InteractionMode::GroupDrag(_) => "grabbing",
        InteractionMode::WireDraft(_) => "crosshair",
        InteractionMode::Idle => "default",
    };

    let fit_content_w = content_w;
    let fit_content_h = content_h;
    let fit_content_offset_x = content_offset_x;
    let fit_content_offset_y = content_offset_y;

    rsx! {
        div {
            class: "relative h-full w-full overflow-hidden select-none outline-none",
            tabindex: "0",
            onkeydown: move |evt: KeyboardEvent| {
                if evt.key() == Key::Character(" ".to_string()) {
                    evt.prevent_default();
                    if let Some(ref sel) = selection_for_key {
                        let new_bypass = match sel {
                            GridSelection::Block(id) => {
                                chain_snapshot.iter()
                                    .find(|s| s.id == *id)
                                    .map(|s| !s.bypassed)
                            }
                            GridSelection::Module(name) => {
                                let module_slots: Vec<&GridSlot> = chain_snapshot.iter()
                                    .filter(|s| s.module_group.as_deref() == Some(name.as_str()))
                                    .collect();
                                Some(!module_slots.iter().all(|s| s.bypassed))
                            }
                        };
                        if let Some(bypassed) = new_bypass {
                            if let Some(ref cb) = props.on_bypass_toggle {
                                cb.call((sel.clone(), bypassed));
                            }
                        }
                    }
                }
            },
            onmounted: move |evt: MountedEvent| {
                mounted_el.set(Some(evt.data()));
                update_viewport();
                spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    update_viewport();
                });
            },
            style: "cursor: {cursor}; \
                    background-color: #000000; \
                    background-image: radial-gradient(circle, #111 1px, transparent 1px); \
                    background-size: 20px 20px;",

            onmousedown: move |evt| {
                update_viewport();
                if evt.trigger_button() == Some(MouseButton::Auxiliary) {
                    evt.prevent_default();
                    interaction.set(InteractionMode::Pan {
                        start_mouse_x: evt.client_coordinates().x,
                        start_mouse_y: evt.client_coordinates().y,
                        start_pan_x: pan_x(),
                        start_pan_y: pan_y(),
                    });
                } else if evt.trigger_button() == Some(MouseButton::Primary) {
                    props.on_select.call(None);
                    *PICKER_CELL.write() = None;
                }
            },

            onmousemove: move |evt| {
                let mx = evt.client_coordinates().x;
                let my = evt.client_coordinates().y;

                match interaction() {
                    InteractionMode::Pan { start_mouse_x, start_mouse_y, start_pan_x, start_pan_y } => {
                        pan_x.set(start_pan_x + (mx - start_mouse_x));
                        pan_y.set(start_pan_y + (my - start_mouse_y));
                    }
                    InteractionMode::BlockDrag(d) => {
                        interaction.set(InteractionMode::BlockDrag(GridDragState {
                            mouse_x: mx, mouse_y: my, ..d
                        }));
                    }
                    InteractionMode::GroupDrag(gd) => {
                        interaction.set(InteractionMode::GroupDrag(GroupDragState {
                            mouse_x: mx, mouse_y: my, ..gd
                        }));
                    }
                    InteractionMode::WireDraft(draft) => {
                        interaction.set(InteractionMode::WireDraft(GridWireDraft {
                            mouse_pos: (mx, my), ..draft
                        }));
                    }
                    InteractionMode::Idle => {}
                }
            },

            onmouseup: move |_evt| {
                match interaction() {
                    InteractionMode::BlockDrag(d) => {
                        let step = (CELL_SIZE + CELL_GAP) as f64;
                        let cz = zoom();
                        let dx = (d.mouse_x - d.start_mouse_x) / cz;
                        let dy = (d.mouse_y - d.start_mouse_y) / cz;
                        let col_delta = (dx / step).round() as isize;
                        let row_delta = (dy / step).round() as isize;
                        let new_col = (d.origin_col as isize + col_delta).max(0) as usize;
                        let new_row = (d.origin_row as isize + row_delta).max(0) as usize;
                        let target_col = new_col.min(cols.saturating_sub(1));
                        let target_row = new_row.min(rows.saturating_sub(1));

                        if target_col != d.origin_col || target_row != d.origin_row {
                            let mut new_chain = chain_signal();
                            let target_occupant = new_chain.iter().position(|s| s.col == target_col && s.row == target_row);
                            let dragged_idx = new_chain.iter().position(|s| s.id == d.slot_id);

                            if let Some(drag_idx) = dragged_idx {
                                if let Some(target_idx) = target_occupant {
                                    let orig_col = new_chain[drag_idx].col;
                                    let orig_row = new_chain[drag_idx].row;
                                    new_chain[drag_idx].col = new_chain[target_idx].col;
                                    new_chain[drag_idx].row = new_chain[target_idx].row;
                                    new_chain[target_idx].col = orig_col;
                                    new_chain[target_idx].row = orig_row;
                                } else {
                                    new_chain[drag_idx].col = target_col;
                                    new_chain[drag_idx].row = target_row;
                                }
                            }
                            committed_chain.set(Some(new_chain.clone()));
                            props.on_chain_change.call(new_chain);
                        }
                    }
                    InteractionMode::GroupDrag(gd) => {
                        if let Some(ref target) = group_drop_target {
                            let mut new_chain = chain_signal();
                            match target {
                                GroupDropTarget::SwapWith(ref target_name) => {
                                    let dragged_min_col = new_chain.iter()
                                        .filter(|s| s.module_group.as_deref() == Some(&gd.group_name))
                                        .map(|s| s.col).min().unwrap_or(0);
                                    let dragged_min_row = new_chain.iter()
                                        .filter(|s| s.module_group.as_deref() == Some(&gd.group_name))
                                        .map(|s| s.row).min().unwrap_or(0);
                                    let target_min_col = new_chain.iter()
                                        .filter(|s| s.module_group.as_deref() == Some(target_name.as_str()))
                                        .map(|s| s.col).min().unwrap_or(0);
                                    let target_min_row = new_chain.iter()
                                        .filter(|s| s.module_group.as_deref() == Some(target_name.as_str()))
                                        .map(|s| s.row).min().unwrap_or(0);

                                    let dc = target_min_col as isize - dragged_min_col as isize;
                                    let dr = target_min_row as isize - dragged_min_row as isize;

                                    for s in new_chain.iter_mut() {
                                        if s.module_group.as_deref() == Some(&gd.group_name) {
                                            s.col = (s.col as isize + dc).max(0) as usize;
                                            s.row = (s.row as isize + dr).max(0) as usize;
                                        } else if s.module_group.as_deref() == Some(target_name.as_str()) {
                                            s.col = (s.col as isize - dc).max(0) as usize;
                                            s.row = (s.row as isize - dr).max(0) as usize;
                                        }
                                    }
                                    committed_chain.set(Some(new_chain.clone()));
                                    props.on_chain_change.call(new_chain);
                                }
                                GroupDropTarget::MoveDelta(dc, dr) => {
                                    if *dc != 0 || *dr != 0 {
                                        let positions: Vec<(isize, isize)> = new_chain
                                            .iter()
                                            .map(|s| {
                                                if s.module_group.as_deref() == Some(&gd.group_name) {
                                                    (s.col as isize + dc, s.row as isize + dr)
                                                } else {
                                                    (s.col as isize, s.row as isize)
                                                }
                                            })
                                            .collect();
                                        let min_c = positions.iter().map(|p| p.0).min().unwrap_or(0).min(0);
                                        let min_r = positions.iter().map(|p| p.1).min().unwrap_or(0).min(0);
                                        for (s, &(mc, mr)) in new_chain.iter_mut().zip(positions.iter()) {
                                            s.col = (mc - min_c) as usize;
                                            s.row = (mr - min_r) as usize;
                                        }
                                        committed_chain.set(Some(new_chain.clone()));
                                        props.on_chain_change.call(new_chain);
                                    }
                                }
                            }
                        }
                    }
                    InteractionMode::WireDraft(draft) => {
                        if let Some((target_id, is_input)) = hovered_port_slot() {
                            let new_conn = if draft.is_from_output && is_input && target_id != draft.from_slot_id {
                                Some(GridConnection {
                                    from_slot_id: draft.from_slot_id,
                                    to_slot_id: target_id,
                                })
                            } else if !draft.is_from_output && !is_input && target_id != draft.from_slot_id {
                                Some(GridConnection {
                                    from_slot_id: target_id,
                                    to_slot_id: draft.from_slot_id,
                                })
                            } else {
                                None
                            };
                            if let Some(conn) = new_conn {
                                let mut new_conns = props.connections.clone();
                                new_conns.push(conn);
                                props.on_connections_change.call(new_conns);
                            }
                        }
                    }
                    _ => {}
                }
                interaction.set(InteractionMode::Idle);
                hovered_port_slot.set(None);
            },

            onmouseleave: move |_| {
                interaction.set(InteractionMode::Idle);
                hovered_port_slot.set(None);
            },

            onwheel: move |evt| {
                evt.prevent_default();
                update_viewport();
                let delta = evt.delta();
                let damp = 0.35;
                let (raw_dx, raw_dy) = match delta {
                    WheelDelta::Pixels(p) => (p.x * damp, p.y * damp),
                    WheelDelta::Lines(l) => (l.x * 16.0, l.y * 16.0),
                    WheelDelta::Pages(p) => (p.x * 160.0, p.y * 160.0),
                };

                let modifiers = evt.modifiers();
                let is_ctrl = modifiers.contains(keyboard_types::Modifiers::CONTROL)
                    || modifiers.contains(keyboard_types::Modifiers::META);
                let is_shift = modifiers.contains(keyboard_types::Modifiers::SHIFT);

                if is_ctrl {
                    let old_zoom = zoom();
                    let zoom_factor = if raw_dy < 0.0 { 1.08 } else { 1.0 / 1.08 };
                    let new_zoom = (old_zoom * zoom_factor).clamp(0.1, 3.0);

                    let local_x = evt.client_coordinates().x - viewport_left();
                    let local_y = evt.client_coordinates().y - viewport_top();
                    let canvas_x = (local_x - pan_x()) / old_zoom;
                    let canvas_y = (local_y - pan_y()) / old_zoom;
                    pan_x.set(local_x - canvas_x * new_zoom);
                    pan_y.set(local_y - canvas_y * new_zoom);
                    zoom.set(new_zoom);
                } else if is_shift {
                    pan_x.set(pan_x() - raw_dy);
                    pan_y.set(pan_y() - raw_dx);
                } else {
                    pan_y.set(pan_y() - raw_dy);
                    pan_x.set(pan_x() - raw_dx);
                }
            },

            // ── Canvas layer ───────────────
            div {
                style: "position: absolute; left: {pan_x()}px; top: {pan_y()}px; \
                        transform: scale({zoom()}); transform-origin: 0 0;",

                div {
                    class: "relative",
                    style: "width: {nat_w}px; height: {nat_h}px; isolation: isolate;",

                // Layer 0: SVG cables + port dots
                CableLayer {
                    cables: cables.clone(),
                    module_ports: module_ports.clone(),
                    nat_w: nat_w as f64,
                    nat_h: nat_h as f64,
                }

                // Layer 1-2: Engine and Layer container backgrounds
                for cg in container_groups.iter().filter(|c| c.level != ContainerLevel::Module) {
                    {
                        rsx! {
                            ContainerBackground {
                                name: cg.display_name.clone(),
                                bg_color: cg.color.bg.to_string(),
                                fg_color: cg.color.fg.to_string(),
                                x: cg.x,
                                y: cg.y,
                                w: cg.w,
                                h: cg.h,
                                level: cg.level,
                            }
                        }
                    }
                }

                // Layer 3: Module group backgrounds
                for group in module_groups.iter() {
                    {
                        let name = group.name.clone();
                        let is_being_dragged = interaction().group_drag().map_or(false, |gd| gd.group_name == name);
                        let is_module_selected = selection == Some(GridSelection::Module(name.clone()));

                        let module_slots: Vec<&GridSlot> = drag_chain.iter()
                            .filter(|s| s.module_group.as_deref() == Some(&name))
                            .collect();
                        let module_vs = ModuleVisualState::from_slots(&module_slots, is_module_selected, is_being_dragged);

                        rsx! {
                            ModuleBackground {
                                name: group.display_name.clone(),
                                bg_color: group.color.bg.to_string(),
                                fg_color: group.color.fg.to_string(),
                                x: group.x,
                                y: group.y,
                                w: group.w,
                                h: group.h,
                                visual_state: module_vs,
                            }
                        }
                    }
                }

                // Ghost preview for group drag target
                if let (Some(ref gd), Some(ref target)) = (interaction().group_drag().cloned(), &group_drop_target) {
                    {
                        let ghost_style = match target {
                            GroupDropTarget::SwapWith(ref target_name) => {
                                if let Some(tg) = module_groups.iter().find(|g| &g.name == target_name) {
                                    let label = if gd.shift_held { "extract" } else { "swap" };
                                    let border_color = if gd.shift_held { "#22d3ee" } else { "#60a5fa" };
                                    Some((tg.x, tg.y, tg.w, tg.h, border_color, label))
                                } else {
                                    None
                                }
                            }
                            GroupDropTarget::MoveDelta(_dc, _dr) => {
                                if let Some(dragged) = module_groups.iter().find(|g| g.name == gd.group_name) {
                                    let label = if gd.shift_held { "extract" } else { "move" };
                                    let border_color = if gd.shift_held { "#22d3ee" } else { "#60a5fa" };
                                    Some((dragged.x, dragged.y, dragged.w, dragged.h, border_color, label))
                                } else {
                                    None
                                }
                            }
                        };
                        if let Some((gx, gy, gw, gh, border_color, label)) = ghost_style {
                            rsx! {
                                div {
                                    class: "absolute",
                                    style: "left: {gx}px; top: {gy}px; width: {gw}px; height: {gh}px; \
                                            border: 2px dashed {border_color}; border-radius: 10px; \
                                            background-color: {border_color}10; \
                                            z-index: 45; pointer-events: none;",
                                    div {
                                        class: "absolute -top-5 left-1/2 -translate-x-1/2 px-2 py-0.5 rounded text-[9px] font-mono whitespace-nowrap",
                                        style: "background-color: {border_color}; color: #000;",
                                        "{label}"
                                    }
                                }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                }

                // Layer 2: CSS Grid cells
                div {
                    style: "position: absolute; left: 0; top: 0; display: grid; {grid_style} z-index: 10;",
                    for row in 0..rows {
                        for col in 0..cols {
                            {
                                let slot = drag_chain.iter().find(|s| s.col == col && s.row == row && !s.is_phantom);
                                let is_drag_target = hover_cell == Some((col, row)) && dragged_slot_id.is_some();
                                let is_being_dragged = slot.as_ref().map_or(false, |s| dragged_slot_id == Some(s.id));

                                if let Some(slot) = slot {
                                    let slot_id = slot.id;
                                    let bt = slot.block_type;
                                    let color = bt.color();
                                    let name = slot.block_preset_name.as_deref()
                                        .unwrap_or(bt.display_name()).to_string();
                                    let is_selected = selection == Some(GridSelection::Block(slot_id));
                                    let slot_col = slot.col;
                                    let slot_row = slot.row;

                                    let visual_state = BlockVisualState::resolve(
                                        is_being_dragged, is_drag_target, slot.bypassed, slot.is_template, is_selected,
                                    );
                                    let cell_style = visual_state.cell_style(&color);
                                    let cell_class = visual_state.cell_class().to_string();
                                    let port_opacity = visual_state.port_opacity().to_string();
                                    let port_color = color.bg.to_string();

                                    let left_port_hovered = hovered_port_slot() == Some((slot_id, true));
                                    let right_port_hovered = hovered_port_slot() == Some((slot_id, false));

                                    let outer_style = if interaction().group_drag().map_or(false, |gd| {
                                        slot.module_group.as_deref() == Some(&gd.group_name)
                                    }) {
                                        "z-index: 50; opacity: 0.85;".to_string()
                                    } else {
                                        String::new()
                                    };

                                    rsx! {
                                        GridBlockCell {
                                            slot_id: slot_id,
                                            block_type_name: bt.display_name().to_string(),
                                            name: name,
                                            cell_style: cell_style,
                                            cell_class: cell_class,
                                            dot_color: color.bg.to_string(),
                                            port_color: port_color,
                                            port_opacity: port_opacity,
                                            port_half: port_half,
                                            left_port_hovered: left_port_hovered,
                                            right_port_hovered: right_port_hovered,
                                            outer_style: outer_style,
                                            on_block_mousedown: move |evt: MouseEvent| {
                                                if PICKER_CELL.read().is_some() { return; }
                                                if is_selected {
                                                    props.on_select.call(None);
                                                } else {
                                                    props.on_select.call(Some(GridSelection::Block(slot_id)));
                                                }
                                                interaction.set(InteractionMode::BlockDrag(GridDragState {
                                                    slot_id,
                                                    origin_col: slot_col,
                                                    origin_row: slot_row,
                                                    start_mouse_x: evt.client_coordinates().x,
                                                    start_mouse_y: evt.client_coordinates().y,
                                                    mouse_x: evt.client_coordinates().x,
                                                    mouse_y: evt.client_coordinates().y,
                                                }));
                                                *PICKER_CELL.write() = None;
                                            },
                                            on_context_menu: {
                                                let on_ctx = props.on_context_menu.clone();
                                                move |evt: MouseEvent| {
                                                    if !is_selected {
                                                        props.on_select.call(Some(GridSelection::Block(slot_id)));
                                                    }
                                                    if let Some(ref cb) = on_ctx {
                                                        cb.call(GridContextMenuEvent {
                                                            target: GridSelection::Block(slot_id),
                                                            client_x: evt.client_coordinates().x,
                                                            client_y: evt.client_coordinates().y,
                                                        });
                                                    }
                                                }
                                            },
                                            on_left_port_mousedown: move |evt: MouseEvent| {
                                                let pos = input_port_pos(slot_col, slot_row);
                                                interaction.set(InteractionMode::WireDraft(GridWireDraft {
                                                    from_slot_id: slot_id,
                                                    from_pos: pos,
                                                    is_from_output: false,
                                                    mouse_pos: (evt.client_coordinates().x, evt.client_coordinates().y),
                                                }));
                                            },
                                            on_right_port_mousedown: move |evt: MouseEvent| {
                                                let pos = output_port_pos(slot_col, slot_row);
                                                interaction.set(InteractionMode::WireDraft(GridWireDraft {
                                                    from_slot_id: slot_id,
                                                    from_pos: pos,
                                                    is_from_output: true,
                                                    mouse_pos: (evt.client_coordinates().x, evt.client_coordinates().y),
                                                }));
                                            },
                                            on_left_port_enter: move |_| {
                                                if interaction().wire_draft().is_some() {
                                                    hovered_port_slot.set(Some((slot_id, true)));
                                                }
                                            },
                                            on_left_port_leave: move |_| {
                                                if hovered_port_slot() == Some((slot_id, true)) {
                                                    hovered_port_slot.set(None);
                                                }
                                            },
                                            on_right_port_enter: move |_| {
                                                if interaction().wire_draft().is_some() {
                                                    hovered_port_slot.set(Some((slot_id, false)));
                                                }
                                            },
                                            on_right_port_leave: move |_| {
                                                if hovered_port_slot() == Some((slot_id, false)) {
                                                    hovered_port_slot.set(None);
                                                }
                                            },
                                        }
                                    }
                                } else if layer_blocked_cells.contains(&(col, row)) {
                                    // Cell overlaps a layer's fader panel — render
                                    // an invisible placeholder so the CSS grid stays
                                    // aligned but the cell is non-interactive.
                                    rsx! {
                                        div {
                                            style: "pointer-events: none;",
                                        }
                                    }
                                } else {
                                    let picker_open_here = *PICKER_CELL.read() == Some((col, row));

                                    rsx! {
                                        EmptyGridCell {
                                            col: col,
                                            row: row,
                                            is_drag_target: is_drag_target,
                                            is_any_drag: is_any_drag,
                                            picker_open_here: picker_open_here,
                                            on_click: move |evt: MouseEvent| {
                                                if interaction().is_idle() {
                                                    *PICKER_CELL.write() = Some((col, row));
                                                    *PICKER_CLICK_POS.write() = (
                                                        evt.client_coordinates().x,
                                                        evt.client_coordinates().y,
                                                    );
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Layer 3: Interactive module title bar hit zones
                for group in module_groups.iter() {
                    {
                        let gname = group.name.clone();
                        let select_name = group.name.clone();
                        let hit_x = group.x;
                        let hit_y = group.y;
                        let hit_w = group.w;
                        let hit_h = GROUP_TITLE_H;

                        let is_this_module_selected = selection == Some(GridSelection::Module(select_name.clone()));

                        rsx! {
                            div {
                                key: "titlehit-{gname}",
                                class: "absolute cursor-grab active:cursor-grabbing",
                                style: "left: {hit_x}px; top: {hit_y}px; width: {hit_w}px; height: {hit_h}px; \
                                        z-index: 20;",
                                onmousedown: move |evt: MouseEvent| {
                                    evt.stop_propagation();
                                    if is_this_module_selected {
                                        props.on_select.call(None);
                                    } else {
                                        props.on_select.call(Some(GridSelection::Module(select_name.clone())));
                                    }
                                    last_valid_group_delta.set((0, 0));
                                    let mods = evt.modifiers();
                                    let shift = mods.contains(keyboard_types::Modifiers::SHIFT);
                                    interaction.set(InteractionMode::GroupDrag(GroupDragState {
                                        group_name: select_name.clone(),
                                        start_mouse_x: evt.client_coordinates().x,
                                        start_mouse_y: evt.client_coordinates().y,
                                        mouse_x: evt.client_coordinates().x,
                                        mouse_y: evt.client_coordinates().y,
                                        shift_held: shift,
                                    }));
                                },
                                oncontextmenu: {
                                    let on_ctx = props.on_context_menu.clone();
                                    let ctx_name = gname.clone();
                                    move |evt: MouseEvent| {
                                        if !is_this_module_selected {
                                            props.on_select.call(Some(GridSelection::Module(ctx_name.clone())));
                                        }
                                        if let Some(ref cb) = on_ctx {
                                            cb.call(GridContextMenuEvent {
                                                target: GridSelection::Module(ctx_name.clone()),
                                                client_x: evt.client_coordinates().x,
                                                client_y: evt.client_coordinates().y,
                                            });
                                        }
                                    }
                                },
                            }
                        }
                    }
                }

                } // close inner natural-size div
            } // close canvas layer div

            // ── Zoom controls overlay ─────────
            div {
                class: "absolute bottom-3 right-3 flex items-center gap-1.5 select-none",
                onmousedown: move |evt| evt.stop_propagation(),

                button {
                    class: "px-2.5 py-1 rounded-lg text-[10px] font-medium \
                            text-zinc-300 hover:text-white transition-colors",
                    style: "background-color: rgba(0,0,0,0.6); backdrop-filter: blur(8px);",
                    title: "Fit grid to view",
                    onclick: move |_| {
                        update_viewport();
                        let padding = 20.0;
                        let avail_w = viewport_w() - padding * 2.0;
                        let avail_h = viewport_h() - padding * 2.0;
                        if fit_content_w > 0.0 && fit_content_h > 0.0 {
                            let fz = (avail_w / fit_content_w)
                                .min(avail_h / fit_content_h)
                                .clamp(0.1, 3.0);
                            let scaled_w = fit_content_w * fz;
                            let scaled_h = fit_content_h * fz;
                            pan_x.set((viewport_w() - scaled_w) / 2.0 - fit_content_offset_x * fz);
                            pan_y.set((viewport_h() - scaled_h) / 2.0 - fit_content_offset_y * fz);
                            zoom.set(fz);
                        }
                    },
                    "Fit"
                }

                button {
                    class: "px-1.5 py-1 rounded-lg text-[10px] font-medium \
                            text-zinc-300 hover:text-white transition-colors",
                    style: "background-color: rgba(0,0,0,0.6); backdrop-filter: blur(8px);",
                    onclick: move |_| {
                        zoom.set((zoom() / 1.2).clamp(0.1, 3.0));
                    },
                    "-"
                }

                div {
                    class: "px-2 py-1 rounded-lg text-[10px] font-mono text-zinc-400",
                    style: "background-color: rgba(0,0,0,0.6); backdrop-filter: blur(8px); \
                            min-width: 40px; text-align: center;",
                    "{(zoom() * 100.0) as i32}%"
                }

                button {
                    class: "px-1.5 py-1 rounded-lg text-[10px] font-medium \
                            text-zinc-300 hover:text-white transition-colors",
                    style: "background-color: rgba(0,0,0,0.6); backdrop-filter: blur(8px);",
                    onclick: move |_| {
                        zoom.set((zoom() * 1.2).clamp(0.1, 3.0));
                    },
                    "+"
                }
            }
        }
    }
}
