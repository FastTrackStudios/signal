//! Edit mode — the live rig resolved into the zoomable/pannable module/wire
//! graph (`signal_grid_ui::RigGridPanel`).
//!
//! The guitar-rig *template* is the full canvas (every module + slot); the
//! active patch's live blocks resolve their matching slots (by name) with
//! real bypass state, params, and live ids for control. Same logic as the
//! desktop shell — this component is what makes the browser remote a real
//! editor, not a viewer.

use crate::param_writer::WriteParam;
use dioxus::prelude::*;

use signal_grid::GridSlot;
use signal_grid::conversion::template_to_grid_slots;
use signal_grid_ui::RigGridPanel;
use signal_grid_ui::dynamic_grid::GridSelection;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{LiveBlock, LiveNode};
use signal_proto::defaults::guitar::guitar_rig_template;

/// Stable Uuid derived from a block's string id (so the grid keeps a
/// consistent identity across updates without re-diffing every frame).
fn slot_uuid(id: &str) -> uuid::Uuid {
    use std::hash::{Hash, Hasher};
    let mut h1 = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h1);
    let mut h2 = std::collections::hash_map::DefaultHasher::new();
    (id, 0x9e37_79b9u64).hash(&mut h2);
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&h1.finish().to_le_bytes());
    bytes[8..].copy_from_slice(&h2.finish().to_le_bytes());
    uuid::Uuid::from_bytes(bytes)
}

/// The live rig's own tree as grid slots — no template, no name matching.
///
/// The canvas used to be a fixed five-level template with live blocks
/// *matched onto it by name*, which meant the graph drew what the rig was
/// expected to be rather than what it is: a block the template did not
/// anticipate had nowhere to appear, and a renamed one silently fell back to
/// a dashed placeholder. Name matching is the hazard the node model exists to
/// remove, and it was still here after it had gone everywhere else.
///
/// Now each block node is a slot, grouped by the module it actually sits in,
/// carrying its real id. A block's id is a seeded UUID, so it is the slot's
/// identity directly rather than a hash of a string.
fn slots_from_nodes(nodes: &[LiveNode]) -> Vec<GridSlot> {
    let mut slots = Vec::new();
    // The container each block hangs from, tracked as the flat list descends
    // and ascends: `module` is the nearest container above the current row.
    let mut ancestors: Vec<(u32, String)> = Vec::new();
    let (mut col, mut row) = (0usize, 0usize);
    let mut current_group: Option<String> = None;

    for node in nodes {
        ancestors.retain(|(depth, _)| *depth < node.depth);
        if !node.is_block {
            ancestors.push((node.depth, node.name.clone()));
            continue;
        }
        let group = ancestors.last().map(|(_, name)| name.clone());
        // A new module starts a new row, so the graph reads as the chain does.
        if group != current_group {
            if current_group.is_some() {
                row += 1;
            }
            col = 0;
            current_group.clone_from(&group);
        }

        slots.push(GridSlot {
            id: uuid::Uuid::parse_str(&node.id).unwrap_or_else(|_| slot_uuid(&node.id)),
            block_type: node.block_type.unwrap_or_default(),
            block_preset_name: Some(node.name.clone()),
            plugin_name: Some(node.name.clone()),
            col,
            row,
            module_group: group.clone(),
            module_type: None,
            layer_group: None,
            engine_group: None,
            is_template: false,
            bypassed: node.bypassed,
            is_phantom: false,
            parameters: Vec::new(),
            preset_id: Some(node.id.clone()),
            snapshot_id: None,
        });
        col += 1;
    }
    slots
}

/// Overlay the live rig's active blocks onto the guitar-rig-template grid
/// slots: a live block *resolves* its matching slot (matched by name) —
/// filling in real bypass state, its param, and its live id (in
/// `preset_id`) for control. Unmatched slots stay dashed placeholders.
fn resolve_template(base: &[GridSlot], live: &[LiveBlock]) -> Vec<GridSlot> {
    let mut slots = base.to_vec();
    for slot in &mut slots {
        let Some(slot_name) = slot.block_preset_name.clone() else {
            continue;
        };
        if let Some(b) = live
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(&slot_name) && b.block_type == slot.block_type)
        {
            slot.is_template = false;
            slot.bypassed = b.bypassed;
            slot.id = slot_uuid(&b.id);
            slot.preset_id = Some(b.id.clone());
            slot.plugin_name = Some(b.name.clone());
            slot.parameters = b
                .param_name
                .as_ref()
                .map(|n| vec![(n.clone(), b.param_value)])
                .unwrap_or_default();
        }
    }
    slots
}

/// The rig graph: guitar-rig template canvas with the live chain resolved
/// in, wired back to the rig service (param edits). Renders read-only if no
/// client is in context.
#[component]
pub fn RigGraph(blocks: Vec<LiveBlock>, nodes: Vec<LiveNode>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let base_slots = use_hook(|| template_to_grid_slots(&guitar_rig_template()));

    // The rig's own tree when it has one; the template only as a fallback,
    // for a rig that has not resolved yet.
    let slots = if nodes.is_empty() {
        resolve_template(&base_slots, &blocks)
    } else {
        slots_from_nodes(&nodes)
    };
    // Keep a lookup for param edits: grid uuid → live block id.
    let by_uuid: std::collections::HashMap<uuid::Uuid, String> = slots
        .iter()
        .filter_map(|s| s.preset_id.clone().map(|id| (s.id, id)))
        .collect();

    rsx! {
        RigGridPanel {
            initial_slots: slots,
            on_selection_change: move |_sel: Option<GridSelection>| {},
            on_param_change: {
                let rig = rig;
                move |(uuid, name, value): (uuid::Uuid, String, f32)| {
                    if let (Some(r), Some(id)) = (rig.clone(), by_uuid.get(&uuid).cloned()) {
                        spawn(async move {
                            let _ = r.write_param(id, name, value).await;
                        });
                    }
                }
            },
        }
    }
}
