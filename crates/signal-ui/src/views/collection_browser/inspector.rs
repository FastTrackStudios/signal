//! Block / module inspector panel for the collection browser detail pane.

use dioxus::prelude::*;

use crate::components::dynamic_grid::{GridSelection, GridSlot};

#[derive(Props, Clone, PartialEq)]
pub(super) struct BlockInspectorPanelProps {
    pub selection: Option<GridSelection>,
    pub chain: Vec<GridSlot>,
    #[props(default)]
    pub on_param_change: Option<EventHandler<(uuid::Uuid, String, f32)>>,
    #[props(default)]
    pub on_save: Option<EventHandler<GridSlot>>,
    #[props(default)]
    pub on_save_as_new: Option<EventHandler<(GridSlot, String)>>,
}

/// Shows properties of the currently selected block or module in the grid.
#[component]
pub(super) fn BlockInspectorPanel(props: BlockInspectorPanelProps) -> Element {
    let Some(ref sel) = props.selection else {
        return rsx! {
            div { class: "mt-3 px-3 py-2 text-xs text-zinc-600 italic",
                "Select a block or module to inspect"
            }
        };
    };

    match sel {
        GridSelection::Block(id) => {
            let slot = props.chain.iter().find(|s| s.id == *id);
            if let Some(slot) = slot {
                let bt_display = format!("{:?}", slot.block_type);
                let color = slot.block_type.color();
                let preset = slot.block_preset_name.as_deref().unwrap_or("—");
                let module = slot
                    .module_group
                    .as_deref()
                    .and_then(|k| k.rsplit('/').next())
                    .unwrap_or("—");
                let bypassed = if slot.bypassed { "Yes" } else { "No" };
                let slot_clone = slot.clone();
                let has_preset = slot.preset_id.is_some();
                let mut show_save_as_new = use_signal(|| false);
                let mut save_as_new_name = use_signal(|| String::new());

                rsx! {
                    div { class: "mt-3 rounded border border-zinc-800 bg-zinc-900/60 overflow-hidden",
                        div { class: "px-3 py-1.5 border-b border-zinc-800 flex items-center gap-2",
                            span {
                                class: "w-2.5 h-2.5 rounded-full inline-block",
                                style: "background-color: {color.bg};",
                            }
                            span { class: "text-xs font-semibold text-zinc-200", "{bt_display}" }
                            span { class: "text-[10px] text-zinc-500", "{preset}" }
                        }
                        div { class: "px-3 py-2 text-xs text-zinc-400 space-y-1",
                            div { class: "flex justify-between",
                                span { class: "text-zinc-500", "Module" }
                                span { "{module}" }
                            }
                            div { class: "flex justify-between",
                                span { class: "text-zinc-500", "Position" }
                                span { "col {slot.col}, row {slot.row}" }
                            }
                            div { class: "flex justify-between",
                                span { class: "text-zinc-500", "Bypassed" }
                                span { "{bypassed}" }
                            }
                            if slot.is_template {
                                div { class: "flex justify-between",
                                    span { class: "text-zinc-500", "Template" }
                                    span { "Yes" }
                                }
                            }
                        }
                        // Interactive parameter sliders
                        if !slot.parameters.is_empty() {
                            div { class: "px-3 py-2 border-t border-zinc-800 space-y-1.5",
                                h4 { class: "text-[10px] font-semibold text-zinc-500 uppercase tracking-wider mb-1",
                                    "Parameters ({slot.parameters.len()})"
                                }
                                for (name, value) in slot.parameters.iter() {
                                    {
                                        let pct = (value * 100.0).round() as u32;
                                        let name = name.clone();
                                        let slot_id = slot.id;
                                        let on_change = props.on_param_change.clone();
                                        rsx! {
                                            div { class: "flex items-center gap-2",
                                                span { class: "text-[11px] text-zinc-400 w-24 truncate flex-shrink-0", "{name}" }
                                                input {
                                                    r#type: "range",
                                                    min: "0",
                                                    max: "100",
                                                    value: "{pct}",
                                                    class: "flex-1 h-1.5",
                                                    style: "accent-color: {color.bg};",
                                                    oninput: move |evt: Event<FormData>| {
                                                        if let Ok(v) = evt.value().parse::<f32>() {
                                                            let normalized = (v / 100.0).clamp(0.0, 1.0);
                                                            if let Some(ref cb) = on_change {
                                                                cb.call((slot_id, name.clone(), normalized));
                                                            }
                                                        }
                                                    },
                                                }
                                                span { class: "text-[10px] text-zinc-600 w-8 text-right flex-shrink-0", "{pct}%" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        // Save button (only when slot has a preset_id to save back to)
                        if has_preset {
                            {
                                let on_save = props.on_save.clone();
                                let save_slot = slot_clone.clone();
                                rsx! {
                                    div { class: "px-3 py-2 border-t border-zinc-800",
                                        button {
                                            class: "w-full px-3 py-1.5 text-xs rounded \
                                                    bg-zinc-700 hover:bg-zinc-600 text-zinc-200 \
                                                    transition-colors duration-150",
                                            onclick: move |_| {
                                                if let Some(ref cb) = on_save {
                                                    cb.call(save_slot.clone());
                                                }
                                            },
                                            "Save Snapshot"
                                        }
                                    }
                                }
                            }
                        }
                        // Save As New Preset
                        {
                            let on_save_as_new = props.on_save_as_new.clone();
                            let new_slot = slot_clone.clone();
                            let default_name = format!("{:?} Preset", slot.block_type);
                            rsx! {
                                div { class: "px-3 py-2 border-t border-zinc-800",
                                    if show_save_as_new() {
                                        div { class: "space-y-2",
                                            input {
                                                r#type: "text",
                                                class: "w-full px-2 py-1 text-xs rounded \
                                                        bg-zinc-800 border border-zinc-600 text-zinc-200 \
                                                        focus:border-amber-500 focus:outline-none",
                                                placeholder: "Preset name...",
                                                value: "{save_as_new_name}",
                                                oninput: move |evt: Event<FormData>| {
                                                    save_as_new_name.set(evt.value());
                                                },
                                            }
                                            div { class: "flex gap-2",
                                                button {
                                                    class: "flex-1 px-2 py-1 text-xs rounded \
                                                            bg-amber-600 hover:bg-amber-500 text-white \
                                                            transition-colors duration-150",
                                                    onclick: {
                                                        let on_save_as_new = on_save_as_new.clone();
                                                        let new_slot = new_slot.clone();
                                                        move |_| {
                                                            let name = save_as_new_name();
                                                            if !name.trim().is_empty() {
                                                                if let Some(ref cb) = on_save_as_new {
                                                                    cb.call((new_slot.clone(), name));
                                                                }
                                                                show_save_as_new.set(false);
                                                                save_as_new_name.set(String::new());
                                                            }
                                                        }
                                                    },
                                                    "Save"
                                                }
                                                button {
                                                    class: "flex-1 px-2 py-1 text-xs rounded \
                                                            bg-zinc-700 hover:bg-zinc-600 text-zinc-300 \
                                                            transition-colors duration-150",
                                                    onclick: move |_| {
                                                        show_save_as_new.set(false);
                                                        save_as_new_name.set(String::new());
                                                    },
                                                    "Cancel"
                                                }
                                            }
                                        }
                                    } else {
                                        button {
                                            class: "w-full px-3 py-1.5 text-xs rounded \
                                                    bg-zinc-800 hover:bg-zinc-700 text-zinc-400 \
                                                    hover:text-zinc-200 border border-zinc-700 border-dashed \
                                                    transition-colors duration-150",
                                            onclick: move |_| {
                                                save_as_new_name.set(default_name.clone());
                                                show_save_as_new.set(true);
                                            },
                                            "Save As New Preset..."
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                rsx! {}
            }
        }
        GridSelection::Module(name) => {
            let module_slots: Vec<&GridSlot> = props
                .chain
                .iter()
                .filter(|s| s.module_group.as_deref() == Some(name) && !s.is_phantom)
                .collect();
            let display_name = name.rsplit('/').next().unwrap_or(name);
            let mt_display = module_slots
                .first()
                .and_then(|s| s.module_type)
                .map(|mt| format!("{mt:?}"))
                .unwrap_or_else(|| "Custom".to_string());
            let color = module_slots
                .first()
                .map(|s| s.block_type.color())
                .unwrap_or_else(|| signal::BlockType::Custom.color());

            rsx! {
                div { class: "mt-3 rounded border border-zinc-800 bg-zinc-900/60 overflow-hidden",
                    div { class: "px-3 py-1.5 border-b border-zinc-800 flex items-center gap-2",
                        span {
                            class: "w-2.5 h-2.5 rounded-full inline-block",
                            style: "background-color: {color.bg};",
                        }
                        span { class: "text-xs font-semibold text-zinc-200", "{display_name}" }
                        span { class: "text-[10px] text-zinc-500", "{mt_display}" }
                    }
                    div { class: "px-3 py-2 text-xs text-zinc-400 space-y-1",
                        div { class: "flex justify-between",
                            span { class: "text-zinc-500", "Blocks" }
                            span { "{module_slots.len()}" }
                        }
                        for slot in module_slots.iter() {
                            {
                                let bt = format!("{:?}", slot.block_type);
                                let preset = slot.block_preset_name.as_deref().unwrap_or("—");
                                let sc = slot.block_type.color();
                                rsx! {
                                    div { class: "flex items-center gap-2 pl-2",
                                        span {
                                            class: "w-1.5 h-1.5 rounded-full inline-block flex-shrink-0",
                                            style: "background-color: {sc.bg};",
                                        }
                                        span { class: "text-zinc-500 truncate", "{bt}" }
                                        span { class: "text-zinc-300 truncate", "{preset}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
