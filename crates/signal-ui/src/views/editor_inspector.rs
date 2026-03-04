//! Editor inspector panel with gradient "lumen block" accent sections.
//!
//! Designed for the Editor tab's right panel — shows detailed block info,
//! interactive parameter sliders, morph point controls, and snapshot management.

use dioxus::prelude::*;

use crate::components::dynamic_grid::{GridSelection, GridSlot};

#[derive(Props, Clone, PartialEq)]
pub struct EditorInspectorPanelProps {
    pub selection: Option<GridSelection>,
    pub chain: Vec<GridSlot>,
    #[props(default)]
    pub on_param_change: Option<EventHandler<(uuid::Uuid, String, f32)>>,
    #[props(default)]
    pub on_save: Option<EventHandler<GridSlot>>,
    #[props(default)]
    pub on_save_as_new: Option<EventHandler<(GridSlot, String)>>,
    /// Callback to expand the full block detail panel.
    #[props(default)]
    pub on_expand_detail: Option<EventHandler<()>>,
}

/// Rich inspector panel with gradient-accented "lumen block" sections.
///
/// Sections:
/// 1. Block Identity (amber) — type, module, bypass, template
/// 2. Parameters (emerald) — interactive sliders
/// 3. Morph Points (violet) — A/B morph position
/// 4. Snapshot (sky) — save/capture controls
#[component]
pub fn EditorInspectorPanel(props: EditorInspectorPanelProps) -> Element {
    let Some(ref sel) = props.selection else {
        return rsx! {
            div { class: "flex flex-col items-center justify-center h-full text-center px-4",
                div { class: "text-zinc-600 text-xs mb-2", "No Selection" }
                div { class: "text-zinc-700 text-[10px] leading-relaxed",
                    "Select a block or module in the grid to inspect and edit its parameters."
                }
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
                let bypassed = slot.bypassed;
                let slot_clone = slot.clone();
                let has_preset = slot.preset_id.is_some();
                let mut show_save_as_new = use_signal(|| false);
                let mut save_as_new_name = use_signal(|| String::new());

                rsx! {
                    div { class: "p-3 space-y-3",
                        // ── Section 1: Block Identity (amber accent) ──
                        div { class: "relative overflow-hidden rounded-lg border border-zinc-800/60 bg-zinc-900/40",
                            div { class: "absolute left-0 top-0 bottom-0 w-1 bg-gradient-to-b from-amber-500 via-orange-400 to-red-500" }
                            div { class: "pl-4 pr-3 py-3",
                                h4 { class: "text-[10px] font-bold uppercase tracking-widest text-zinc-400 mb-2", "Block Identity" }
                                div { class: "space-y-1.5",
                                    // Type badge
                                    div { class: "flex items-center gap-2",
                                        span {
                                            class: "w-2.5 h-2.5 rounded-full inline-block flex-shrink-0",
                                            style: "background-color: {color.bg};",
                                        }
                                        span { class: "text-xs font-semibold text-zinc-200", "{bt_display}" }
                                    }
                                    // Preset name
                                    div { class: "flex justify-between text-xs",
                                        span { class: "text-zinc-500", "Preset" }
                                        span { class: "text-zinc-300 truncate ml-2", "{preset}" }
                                    }
                                    // Module group
                                    div { class: "flex justify-between text-xs",
                                        span { class: "text-zinc-500", "Module" }
                                        span { class: "text-zinc-300 truncate ml-2", "{module}" }
                                    }
                                    // Position
                                    div { class: "flex justify-between text-xs",
                                        span { class: "text-zinc-500", "Position" }
                                        span { class: "text-zinc-400", "col {slot.col}, row {slot.row}" }
                                    }
                                    // Bypass toggle
                                    div { class: "flex justify-between items-center text-xs",
                                        span { class: "text-zinc-500", "Bypassed" }
                                        span {
                                            class: if bypassed {
                                                "px-1.5 py-0.5 rounded text-[10px] font-medium bg-amber-900/40 text-amber-400"
                                            } else {
                                                "px-1.5 py-0.5 rounded text-[10px] font-medium bg-zinc-800 text-zinc-500"
                                            },
                                            if bypassed { "Yes" } else { "No" }
                                        }
                                    }
                                    // Template indicator
                                    if slot.is_template {
                                        div { class: "flex justify-between items-center text-xs",
                                            span { class: "text-zinc-500", "Template" }
                                            span { class: "px-1.5 py-0.5 rounded text-[10px] font-medium bg-violet-900/40 text-violet-400",
                                                "Yes"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // ── Section 2: Parameters (emerald accent) ──
                        if !slot.parameters.is_empty() {
                            div { class: "relative overflow-hidden rounded-lg border border-zinc-800/60 bg-zinc-900/40",
                                div { class: "absolute left-0 top-0 bottom-0 w-1 bg-gradient-to-b from-emerald-500 via-teal-400 to-cyan-500" }
                                div { class: "pl-4 pr-3 py-3",
                                    h4 { class: "text-[10px] font-bold uppercase tracking-widest text-zinc-400 mb-2",
                                        "Parameters ({slot.parameters.len()})"
                                    }
                                    div { class: "space-y-2",
                                        for (name, value) in slot.parameters.iter() {
                                            {
                                                let name = name.clone();
                                                let slot_id = slot.id;
                                                let on_change = props.on_param_change.clone();
                                                let initial_value = *value;
                                                rsx! {
                                                    InspectorParamSlider {
                                                        key: "{name}",
                                                        name: name.clone(),
                                                        value: initial_value,
                                                        accent_color: color.bg.to_string(),
                                                        on_change: move |normalized: f32| {
                                                            if let Some(ref cb) = on_change {
                                                                cb.call((slot_id, name.clone(), normalized));
                                                            }
                                                        },
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // ── Section 2b: Expand Detail (cyan accent) ──
                        if let Some(ref on_expand) = props.on_expand_detail {
                            {
                                let on_expand = on_expand.clone();
                                rsx! {
                                    button {
                                        class: "w-full px-3 py-2 text-xs rounded-lg \
                                                bg-gradient-to-r from-cyan-900/30 to-blue-900/30 \
                                                hover:from-cyan-900/50 hover:to-blue-900/50 \
                                                text-cyan-400 font-medium \
                                                border border-cyan-800/40 \
                                                transition-all duration-150 \
                                                flex items-center justify-center gap-2",
                                        onclick: move |_| on_expand.call(()),
                                        span { "\u{2197}" }
                                        span { "Expand Block Detail" }
                                    }
                                }
                            }
                        }

                        // ── Section 3: Morph Points (violet accent) ──
                        div { class: "relative overflow-hidden rounded-lg border border-zinc-800/60 bg-zinc-900/40",
                            div { class: "absolute left-0 top-0 bottom-0 w-1 bg-gradient-to-b from-violet-500 via-purple-400 to-fuchsia-500" }
                            div { class: "pl-4 pr-3 py-3",
                                h4 { class: "text-[10px] font-bold uppercase tracking-widest text-zinc-400 mb-2", "Morph Points" }
                                div { class: "space-y-2",
                                    div { class: "flex justify-between text-[10px] text-zinc-500 mb-1",
                                        span { "Scene A" }
                                        span { "Scene B" }
                                    }
                                    input {
                                        r#type: "range",
                                        min: "0",
                                        max: "100",
                                        value: "50",
                                        class: "w-full h-1.5",
                                        style: "accent-color: #8B5CF6;",
                                    }
                                    // Easing curve
                                    div { class: "flex justify-between items-center text-xs mt-1",
                                        span { class: "text-zinc-500", "Easing" }
                                        select {
                                            class: "bg-zinc-800 border border-zinc-700 rounded px-1.5 py-0.5 text-[11px] text-zinc-300",
                                            option { value: "linear", "Linear" }
                                            option { value: "ease-in", "Ease In" }
                                            option { value: "ease-out", "Ease Out" }
                                            option { value: "ease-in-out", "Ease In/Out" }
                                        }
                                    }
                                }
                            }
                        }

                        // ── Section 4: Snapshot (sky accent) ──
                        div { class: "relative overflow-hidden rounded-lg border border-zinc-800/60 bg-zinc-900/40",
                            div { class: "absolute left-0 top-0 bottom-0 w-1 bg-gradient-to-b from-sky-500 via-blue-400 to-indigo-500" }
                            div { class: "pl-4 pr-3 py-3",
                                h4 { class: "text-[10px] font-bold uppercase tracking-widest text-zinc-400 mb-2", "Snapshot" }
                                div { class: "space-y-2",
                                    div { class: "flex items-center gap-2",
                                        span { class: "text-xs text-zinc-300 truncate flex-1", "{preset}" }
                                        if slot.snapshot_id.is_some() {
                                            span { class: "px-1.5 py-0.5 rounded text-[9px] font-medium bg-sky-900/40 text-sky-400 flex-shrink-0",
                                                "v1"
                                            }
                                        }
                                    }
                                    if has_preset {
                                        {
                                            let on_save = props.on_save.clone();
                                            let save_slot = slot_clone.clone();
                                            rsx! {
                                                button {
                                                    class: "w-full px-3 py-1.5 text-xs rounded \
                                                            bg-gradient-to-r from-sky-600 to-blue-600 \
                                                            hover:from-sky-500 hover:to-blue-500 \
                                                            text-white font-medium \
                                                            transition-all duration-150",
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
                            }
                        }

                        // ── Section 5: Save As New Preset (amber accent) ──
                        {
                            let on_save_as_new = props.on_save_as_new.clone();
                            let new_slot = slot_clone.clone();
                            let default_name = format!("{:?} Preset", slot.block_type);
                            rsx! {
                                div { class: "relative overflow-hidden rounded-lg border border-zinc-800/60 bg-zinc-900/40",
                                    div { class: "absolute left-0 top-0 bottom-0 w-1 bg-gradient-to-b from-amber-500 via-yellow-400 to-orange-500" }
                                    div { class: "pl-4 pr-3 py-3",
                                        h4 { class: "text-[10px] font-bold uppercase tracking-widest text-zinc-400 mb-2", "New Preset" }
                                        if show_save_as_new() {
                                            div { class: "space-y-2",
                                                input {
                                                    r#type: "text",
                                                    class: "w-full px-2 py-1.5 text-xs rounded \
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
                                                        class: "flex-1 px-2 py-1.5 text-xs rounded \
                                                                bg-gradient-to-r from-amber-600 to-orange-600 \
                                                                hover:from-amber-500 hover:to-orange-500 \
                                                                text-white font-medium \
                                                                transition-all duration-150",
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
                                                        class: "flex-1 px-2 py-1.5 text-xs rounded \
                                                                bg-zinc-700 hover:bg-zinc-600 text-zinc-300 \
                                                                transition-all duration-150",
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
                                                        transition-all duration-150",
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
            let block_count = module_slots.len();
            let total_params: usize = module_slots.iter().map(|s| s.parameters.len()).sum();

            rsx! {
                div { class: "p-3 space-y-3",
                    // ── Module Identity (amber accent) ──
                    div { class: "relative overflow-hidden rounded-lg border border-zinc-800/60 bg-zinc-900/40",
                        div { class: "absolute left-0 top-0 bottom-0 w-1 bg-gradient-to-b from-amber-500 via-orange-400 to-red-500" }
                        div { class: "pl-4 pr-3 py-3",
                            h4 { class: "text-[10px] font-bold uppercase tracking-widest text-zinc-400 mb-2", "Module" }
                            div { class: "space-y-1.5",
                                div { class: "flex items-center gap-2",
                                    span {
                                        class: "w-2.5 h-2.5 rounded-full inline-block flex-shrink-0",
                                        style: "background-color: {color.bg};",
                                    }
                                    span { class: "text-xs font-semibold text-zinc-200", "{display_name}" }
                                    span { class: "text-[10px] text-zinc-500", "{mt_display}" }
                                }
                                div { class: "flex justify-between text-xs",
                                    span { class: "text-zinc-500", "Blocks" }
                                    span { class: "text-zinc-300", "{block_count}" }
                                }
                                div { class: "flex justify-between text-xs",
                                    span { class: "text-zinc-500", "Total Params" }
                                    span { class: "text-zinc-300", "{total_params}" }
                                }
                            }
                        }
                    }

                    // ── Block list (emerald accent) ──
                    div { class: "relative overflow-hidden rounded-lg border border-zinc-800/60 bg-zinc-900/40",
                        div { class: "absolute left-0 top-0 bottom-0 w-1 bg-gradient-to-b from-emerald-500 via-teal-400 to-cyan-500" }
                        div { class: "pl-4 pr-3 py-3",
                            h4 { class: "text-[10px] font-bold uppercase tracking-widest text-zinc-400 mb-2", "Blocks" }
                            div { class: "space-y-1",
                                for slot in module_slots.iter() {
                                    {
                                        let bt = format!("{:?}", slot.block_type);
                                        let preset = slot.block_preset_name.as_deref().unwrap_or("—");
                                        let sc = slot.block_type.color();
                                        rsx! {
                                            div { class: "flex items-center gap-2",
                                                span {
                                                    class: "w-1.5 h-1.5 rounded-full inline-block flex-shrink-0",
                                                    style: "background-color: {sc.bg};",
                                                }
                                                span { class: "text-[11px] text-zinc-500 truncate", "{bt}" }
                                                span { class: "text-[11px] text-zinc-300 truncate", "{preset}" }
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
    }
}

// ── InspectorParamSlider ────────────────────────────────────────

/// A single parameter slider with local state for immediate value display.
///
/// The percentage text and slider position update instantly during drag
/// without waiting for the parent's re-render cycle.
#[component]
fn InspectorParamSlider(
    name: String,
    value: f32,
    accent_color: String,
    on_change: EventHandler<f32>,
) -> Element {
    let mut local_value = use_signal(|| value);

    // Sync from props when value changes externally
    use_effect(move || {
        local_value.set(value);
    });

    let display = local_value();
    let pct = (display * 100.0).round() as u32;

    rsx! {
        div { class: "space-y-0.5",
            div { class: "flex justify-between items-baseline",
                span { class: "text-[11px] text-zinc-400 truncate", "{name}" }
                span { class: "text-[10px] text-zinc-600 tabular-nums flex-shrink-0 ml-2", "{pct}%" }
            }
            input {
                r#type: "range",
                min: "0",
                max: "100",
                value: "{pct}",
                class: "w-full h-1.5",
                style: "accent-color: {accent_color};",
                oninput: move |evt: Event<FormData>| {
                    if let Ok(v) = evt.value().parse::<f32>() {
                        let normalized = (v / 100.0).clamp(0.0, 1.0);
                        local_value.set(normalized);
                        on_change.call(normalized);
                    }
                },
            }
        }
    }
}
