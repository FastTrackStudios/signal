//! Macro knob bank viewer — assignable knobs with parameter bindings.
//!
//! Supports macro groups: when groups are configured, shows a group indicator
//! bar and renders only the shared + active-group knobs.

use dioxus::prelude::*;

use crate::views::MiniKnob;

#[derive(Props, Clone, PartialEq)]
pub struct BlockMacrosProps {
    pub block: signal::Block,
    #[props(default)]
    pub on_macro_change: Option<EventHandler<(String, f32)>>,
    /// Current selector parameter value — determines which macro group is active.
    /// Pass `None` when groups aren't in use or the selector value isn't known.
    #[props(default)]
    pub active_group_selector_value: Option<f32>,
}

/// Renders the macro knob bank with binding configuration and group support.
///
/// Shows up to 8 macro knobs in a horizontal row. When macro groups are
/// configured, a group indicator bar appears and only shared + active-group
/// knobs are rendered. Clicking a knob reveals its bindings below.
#[component]
pub fn BlockMacros(props: BlockMacrosProps) -> Element {
    let mut selected_knob = use_signal(|| None::<String>);

    let macro_bank = &props.block.macro_bank;

    let Some(ref bank) = macro_bank else {
        return rsx! {
            div { class: "flex flex-col items-center justify-center h-32 text-center px-4",
                div { class: "text-zinc-600 text-xs", "No Macros" }
                div { class: "text-zinc-700 text-[10px] mt-1",
                    "Add macro knobs to create assignable controls for this block's parameters."
                }
                button {
                    class: "mt-3 px-3 py-1.5 text-[11px] rounded \
                            bg-zinc-800 hover:bg-zinc-700 text-zinc-400 \
                            hover:text-zinc-200 border border-zinc-700 border-dashed \
                            transition-all duration-150",
                    "Add Macro Bank"
                }
            }
        };
    };

    let has_groups = bank.has_groups();
    // Use the prop override if provided, otherwise read from the selector knob
    let active_group = if let Some(val) = props.active_group_selector_value {
        bank.active_group_for(val)
    } else {
        bank.active_group()
    };
    let active_group_color = active_group.map(|g| g.color.clone());

    // Visible knobs: shared + active group
    let visible_knobs: Vec<&signal::macro_bank::MacroKnob> = if let Some(val) = props.active_group_selector_value {
        bank.visible_knobs_for(val)
    } else {
        bank.visible_knobs()
    };
    let shared_count = bank.knobs.len();
    let total_knob_count = visible_knobs.len();
    let selector_knob_id = bank.group_selector.as_ref().map(|s| s.knob_id.clone());

    rsx! {
        div { class: "p-4 space-y-4",
            // ── Group indicator bar (only when groups are configured) ──
            if has_groups {
                div { class: "flex items-center gap-2 mb-2",
                    for group in bank.groups.iter() {
                        {
                            let is_active = active_group.map(|g| g.id == group.id).unwrap_or(false);
                            let color = group.color.clone();
                            rsx! {
                                div {
                                    key: "indicator-{group.id}",
                                    class: if is_active {
                                        "px-2 py-0.5 rounded text-[9px] font-medium border"
                                    } else {
                                        "px-2 py-0.5 rounded text-[9px] text-zinc-600 border border-zinc-800"
                                    },
                                    style: if is_active { "background: {color}22; border-color: {color}; color: {color};" } else { "" },
                                    "{group.label}"
                                }
                            }
                        }
                    }
                }
            }

            // ── Macro knob row ──
            div {
                div { class: "flex items-center gap-2 mb-3",
                    span { class: "text-[10px] font-bold uppercase tracking-widest text-zinc-400",
                        "Macro Knobs"
                    }
                    span { class: "text-[10px] text-zinc-600",
                        "({total_knob_count}/{signal::macro_bank::MacroBank::MAX_KNOBS})"
                    }
                }
                div { class: "flex gap-3 flex-wrap",
                    for (i, knob) in visible_knobs.iter().enumerate() {
                        {
                            let knob_id = knob.id.clone();
                            let knob_id_for_change = knob_id.clone();
                            let knob_label = knob.label.clone();
                            let value = knob.value;
                            let is_selected = selected_knob() == Some(knob_id.clone());
                            let is_group_knob = i >= shared_count;
                            let accent = if is_group_knob {
                                active_group_color.as_deref().unwrap_or("#3B82F6")
                            } else {
                                knob.color.as_deref().unwrap_or("#3B82F6")
                            };
                            let on_change = props.on_macro_change.clone();
                            let binding_count = knob.bindings.len();
                            let is_selector = selector_knob_id.as_deref() == Some(knob_id.as_str());
                            let readout = if is_selector {
                                bank.active_group_for(value)
                                    .map(|g| g.label.clone())
                                    .unwrap_or_else(|| format!("{:.0}%", value * 100.0))
                            } else {
                                format!("{:.0}%", value * 100.0)
                            };
                            rsx! {
                                // Separator between shared and group knobs
                                if i == shared_count && shared_count > 0 {
                                    div { class: "w-px h-16 bg-zinc-700 mx-1 self-center" }
                                }
                                div {
                                    key: "{knob_id}",
                                    class: if is_selected {
                                        "flex flex-col items-center gap-1 p-2 rounded-lg bg-zinc-800/60 border border-zinc-600 cursor-pointer"
                                    } else {
                                        "flex flex-col items-center gap-1 p-2 rounded-lg hover:bg-zinc-800/40 cursor-pointer"
                                    },
                                    onclick: {
                                        let kid = knob_id.clone();
                                        move |_| {
                                            if selected_knob() == Some(kid.clone()) {
                                                selected_knob.set(None);
                                            } else {
                                                selected_knob.set(Some(kid.clone()));
                                            }
                                        }
                                    },
                                    MiniKnob {
                                        value,
                                        on_change: move |new_val: f32| {
                                            if let Some(ref cb) = on_change {
                                                cb.call((knob_id_for_change.clone(), new_val));
                                            }
                                        },
                                    }
                                    span {
                                        class: "text-[10px] font-medium text-center truncate w-14",
                                        style: "color: {accent};",
                                        "{knob_label}"
                                    }
                                    span {
                                        class: "text-[9px] font-mono tabular-nums text-zinc-400",
                                        "{readout}"
                                    }
                                    {
                                        let suffix = if binding_count != 1 { "s" } else { "" };
                                        rsx! {
                                            span { class: "text-[9px] text-zinc-600",
                                                "{binding_count} binding{suffix}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Add knob button
                    if total_knob_count < signal::macro_bank::MacroBank::MAX_KNOBS {
                        div {
                            class: "flex flex-col items-center justify-center gap-1 p-2 w-[72px] h-[88px] \
                                    rounded-lg border border-zinc-700 border-dashed \
                                    hover:bg-zinc-800/40 hover:border-zinc-600 \
                                    cursor-pointer transition-colors",
                            span { class: "text-zinc-600 text-lg", "+" }
                            span { class: "text-[9px] text-zinc-600", "Add" }
                        }
                    }
                }
            }

            // ── Selected knob bindings ──
            if let Some(ref sel_id) = selected_knob() {
                if let Some(knob) = bank.get_knob(sel_id) {
                    div { class: "relative overflow-hidden rounded-lg border border-zinc-800/60 bg-zinc-900/40",
                        // Accent bar — use group color for group knobs
                        {
                            let knob_in_group = bank.groups.iter().any(|g| g.knobs.iter().any(|k| &k.id == sel_id));
                            let bar_color = if knob_in_group {
                                active_group_color.as_deref().unwrap_or("#3B82F6")
                            } else {
                                "#3B82F6"
                            };
                            rsx! {
                                div {
                                    class: "absolute left-0 top-0 bottom-0 w-1",
                                    style: "background: {bar_color};",
                                }
                            }
                        }
                        div { class: "pl-4 pr-3 py-3",
                            h4 { class: "text-[10px] font-bold uppercase tracking-widest text-zinc-400 mb-2",
                                "Bindings for \"{knob.label}\""
                            }
                            if knob.bindings.is_empty() {
                                div { class: "text-[10px] text-zinc-600 py-2",
                                    "No bindings. Assign this macro to parameters to control them."
                                }
                            } else {
                                div { class: "space-y-2",
                                    for (i, binding) in knob.bindings.iter().enumerate() {
                                        div {
                                            key: "{i}",
                                            class: "flex items-center gap-2 text-xs",
                                            // Target info
                                            div { class: "flex-1 min-w-0",
                                                div { class: "text-zinc-300 truncate",
                                                    "{binding.target.param_id}"
                                                }
                                                div { class: "text-[10px] text-zinc-600",
                                                    "Block: {binding.target.block_id}"
                                                }
                                            }
                                            // Range
                                            div { class: "flex-shrink-0 text-[10px] text-zinc-500 tabular-nums",
                                                "{binding.min:.0}–{binding.max:.0}"
                                            }
                                            // Curve
                                            span {
                                                class: "flex-shrink-0 px-1.5 py-0.5 rounded text-[9px] bg-zinc-800 text-zinc-500",
                                                "{binding.curve.display_name()}"
                                            }
                                        }
                                    }
                                }
                            }
                            // Add binding button
                            button {
                                class: "mt-2 w-full px-2 py-1 text-[10px] rounded \
                                        bg-zinc-800 hover:bg-zinc-700 text-zinc-500 \
                                        hover:text-zinc-300 border border-zinc-700 border-dashed \
                                        transition-all duration-150",
                                "Add Binding..."
                            }
                        }
                    }
                }
            }
        }
    }
}
