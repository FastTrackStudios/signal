//! Full searchable parameter list — raw access to all block parameters.

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct BlockRawParamsProps {
    pub block: signal::Block,
    #[props(default)]
    pub block_type: signal::BlockType,
    #[props(default)]
    pub on_param_change: Option<EventHandler<(String, f32)>>,
}

/// Renders all block parameters as a searchable list with sliders.
#[component]
pub fn BlockRawParams(props: BlockRawParamsProps) -> Element {
    let mut search = use_signal(String::new);
    let params = props.block.parameters();
    let color = props.block_type.color();

    let search_term = search().to_lowercase();
    let filtered: Vec<_> = params
        .iter()
        .filter(|p| {
            search_term.is_empty()
                || p.name().to_lowercase().contains(&search_term)
                || p.id().to_lowercase().contains(&search_term)
        })
        .collect();

    rsx! {
        div { class: "p-3 space-y-3",
            // Search bar
            div { class: "relative",
                input {
                    r#type: "text",
                    class: "w-full px-3 py-1.5 text-xs rounded \
                            bg-zinc-800 border border-zinc-700 text-zinc-200 \
                            placeholder-zinc-600 \
                            focus:border-zinc-500 focus:outline-none",
                    placeholder: "Search parameters...",
                    value: "{search}",
                    oninput: move |evt: Event<FormData>| {
                        search.set(evt.value());
                    },
                }
                if !search().is_empty() {
                    button {
                        class: "absolute right-2 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-zinc-300 text-xs",
                        onclick: move |_| search.set(String::new()),
                        "\u{2715}"
                    }
                }
            }

            // Count
            div { class: "text-[10px] text-zinc-600",
                "{filtered.len()} of {params.len()} parameters"
            }

            // Parameter list
            div { class: "space-y-2",
                for param in filtered.iter() {
                    {
                        let param_id = param.id().to_string();
                        let param_name = param.name().to_string();
                        let value = param.value().get();
                        let accent = color.bg.to_string();
                        let on_change = props.on_param_change.clone();
                        rsx! {
                            RawParamSlider {
                                key: "{param_id}",
                                param_id: param_id,
                                param_name: param_name,
                                value,
                                accent_color: accent,
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

/// A single parameter slider with local state for immediate visual feedback.
#[component]
fn RawParamSlider(
    param_id: String,
    param_name: String,
    value: f32,
    accent_color: String,
    on_change: EventHandler<(String, f32)>,
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
                span { class: "text-[11px] text-zinc-400 truncate", "{param_name}" }
                span { class: "text-[10px] text-zinc-600 tabular-nums flex-shrink-0 ml-2",
                    "{pct}%"
                }
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
                        on_change.call((param_id.clone(), normalized));
                    }
                },
            }
        }
    }
}
