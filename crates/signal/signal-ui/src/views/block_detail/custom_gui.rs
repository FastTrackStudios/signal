//! Curated parameter knob grid — shows only featured parameters.

use dioxus::prelude::*;

use crate::views::MiniKnob;

#[derive(Props, Clone, PartialEq)]
pub struct BlockCustomGuiProps {
    pub block: signal::Block,
    #[props(default)]
    pub block_type: signal::BlockType,
    #[props(default)]
    pub on_param_change: Option<EventHandler<(String, f32)>>,
}

/// Renders curated parameters as a knob grid.
///
/// If `block.param_curation` is set, only those parameters are shown.
/// Otherwise falls back to showing all parameters.
#[component]
pub fn BlockCustomGui(props: BlockCustomGuiProps) -> Element {
    let params = props.block.parameters();
    let curation = &props.block.param_curation;

    // Filter to featured params if curation exists, otherwise show all
    let featured: Vec<_> = if let Some(ref curation) = curation {
        params
            .iter()
            .filter(|p| curation.is_featured(p.id()))
            .collect()
    } else {
        params.iter().collect()
    };

    if featured.is_empty() {
        return rsx! {
            div { class: "flex flex-col items-center justify-center h-32 text-center px-4",
                div { class: "text-zinc-600 text-xs", "No curated parameters" }
                div { class: "text-zinc-700 text-[10px] mt-1",
                    "Add parameters to the curation list to build a custom GUI."
                }
            }
        };
    }

    let color = props.block_type.color();

    rsx! {
        div { class: "p-4",
            // Section header
            div { class: "flex items-center gap-2 mb-3",
                span {
                    class: "w-2 h-2 rounded-full",
                    style: "background-color: {color.bg};",
                }
                span { class: "text-[10px] font-bold uppercase tracking-widest text-zinc-400",
                    if curation.is_some() {
                        "Curated Parameters"
                    } else {
                        "All Parameters"
                    }
                }
                span { class: "text-[10px] text-zinc-600", "({featured.len()})" }
            }

            // Knob grid
            div { class: "grid grid-cols-4 gap-4",
                for param in featured.iter() {
                    {
                        let param_id = param.id().to_string();
                        let param_name = param.name().to_string();
                        let value = param.value().get();
                        let on_change = props.on_param_change.clone();
                        rsx! {
                            CuratedKnob {
                                key: "{param_id}",
                                param_id: param_id,
                                param_name: param_name,
                                value,
                                on_change: move |(id, val): (String, f32)| {
                                    if let Some(ref cb) = on_change {
                                        cb.call((id, val));
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

/// A single curated knob with local state for immediate value display.
#[component]
fn CuratedKnob(
    param_id: String,
    param_name: String,
    value: f32,
    on_change: EventHandler<(String, f32)>,
) -> Element {
    let mut local_value = use_signal(|| value);

    // Sync from props when value changes externally
    use_effect(move || {
        local_value.set(value);
    });

    let display = local_value();

    rsx! {
        div { class: "flex flex-col items-center gap-1",
            MiniKnob {
                value: display,
                on_change: move |new_val: f32| {
                    local_value.set(new_val);
                    on_change.call((param_id.clone(), new_val));
                },
            }
            span {
                class: "text-[10px] text-zinc-400 text-center truncate w-14",
                title: "{param_name}",
                "{param_name}"
            }
            span { class: "text-[10px] font-mono text-zinc-500",
                "{(display * 100.0) as i32}%"
            }
        }
    }
}
