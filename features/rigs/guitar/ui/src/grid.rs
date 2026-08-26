//! Edit mode — the live rig resolved into the zoomable/pannable module/wire
//! graph (`signal_grid_ui::RigGridPanel`).
//!
//! The guitar-rig *template* is the full canvas (every module + slot); the
//! active patch's live blocks resolve their matching slots (by name) with
//! real bypass state, params, and live ids for control. Same logic as the
//! desktop shell — this component is what makes the browser remote a real
//! editor, not a viewer.

use dioxus::prelude::*;

use signal_grid::conversion::template_to_grid_slots;
use signal_grid::GridSlot;
use signal_grid_ui::dynamic_grid::GridSelection;
use signal_grid_ui::RigGridPanel;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::LiveBlock;
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

/// Overlay the live rig's active blocks onto the guitar-rig-template grid
/// slots: a live block *resolves* its matching slot (matched by name) —
/// filling in real bypass state, its param, and its live id (in
/// `preset_id`) for control. Unmatched slots stay dashed placeholders.
fn resolve_template(base: &[GridSlot], live: &[LiveBlock]) -> Vec<GridSlot> {
    let mut slots = base.to_vec();
    for slot in slots.iter_mut() {
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
pub fn RigGraph(blocks: Vec<LiveBlock>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let base_slots = use_hook(|| template_to_grid_slots(&guitar_rig_template()));

    let slots = resolve_template(&base_slots, &blocks);
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
                let rig = rig.clone();
                move |(uuid, name, value): (uuid::Uuid, String, f32)| {
                    if let (Some(r), Some(id)) = (rig.clone(), by_uuid.get(&uuid).cloned()) {
                        spawn(async move {
                            let _ = r.set_block_param(id, name, value).await;
                        });
                    }
                }
            },
        }
    }
}
