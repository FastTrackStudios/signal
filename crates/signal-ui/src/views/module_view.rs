//! Module view -- displays a module's blocks in compact or detail mode.
//!
//! Compact mode shows colored pills (Quad Cortex style).
//! Detail mode shows full parameter cards with knobs.

use dioxus::prelude::*;
use signal::{BlockType, Module, SignalChain};

use super::block_editor::BlockCard;
use crate::components::{block_bypassed_style, block_style};

// region: --- Types

/// View mode for the module display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModuleViewMode {
    /// Colored pills with name + bypass dot.
    #[default]
    Compact,
    /// Full block cards with parameter knobs.
    Detail,
}

/// Parameter change event from a module view.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamChange {
    /// Block identifier within the module.
    pub block_id: String,
    /// Parameter index within the block.
    pub param_index: usize,
    /// New value (0.0-1.0).
    pub value: f32,
}

/// Extracted block data for rendering (avoids passing domain types to inner components).
struct BlockDisplay {
    id: String,
    label: String,
    block_type: BlockType,
    bypassed: bool,
    parameters: Vec<(String, f32)>,
}

// endregion: --- Types

// region: --- ModuleView

/// Displays a module's signal chain in compact or detail mode.
///
/// Extracts blocks from the module's `SignalChain` and renders them
/// using the appropriate view mode.
#[component]
pub fn ModuleView(
    /// The module to display.
    module: Module,
    /// View mode (Compact or Detail).
    #[props(default)]
    view_mode: ModuleViewMode,
    /// Callback when a block's bypass is toggled.
    on_toggle_bypass: EventHandler<String>,
    /// Callback when a parameter value changes.
    on_param_change: EventHandler<ParamChange>,
) -> Element {
    // Extract blocks from the signal chain
    let blocks = extract_blocks(module.chain());

    if blocks.is_empty() {
        return rsx! {
            div { class: "text-xs text-zinc-500 text-center py-4", "No blocks in this module" }
        };
    }

    match view_mode {
        ModuleViewMode::Compact => rsx! {
            div { class: "flex flex-col gap-1 p-2",
                for block in blocks {
                    CompactBlockPill {
                        key: "{block.id}",
                        id: block.id,
                        label: block.label,
                        block_type: block.block_type,
                        bypassed: block.bypassed,
                        on_toggle_bypass: on_toggle_bypass.clone(),
                    }
                }
            }
        },
        ModuleViewMode::Detail => rsx! {
            div { class: "flex flex-col gap-2 p-2",
                for block in blocks {
                    BlockCard {
                        key: "{block.id}",
                        block_type_key: block.block_type.as_str().to_string(),
                        name: block.label.clone(),
                        bypassed: block.bypassed,
                        parameters: block.parameters.clone(),
                        on_toggle_bypass: {
                            let id = block.id.clone();
                            move |_| on_toggle_bypass.call(id.clone())
                        },
                        on_param_change: {
                            let id = block.id.clone();
                            move |(idx, val): (usize, f32)| {
                                on_param_change.call(ParamChange {
                                    block_id: id.clone(),
                                    param_index: idx,
                                    value: val,
                                });
                            }
                        },
                    }
                }
            }
        },
    }
}

// endregion: --- ModuleView

// region: --- CompactBlockPill

/// A single colored pill representing a block in compact mode.
#[component]
fn CompactBlockPill(
    id: String,
    label: String,
    block_type: BlockType,
    #[props(default)] bypassed: bool,
    on_toggle_bypass: EventHandler<String>,
) -> Element {
    let style = if bypassed {
        block_bypassed_style(block_type.as_str())
    } else {
        block_style(block_type.as_str())
    };

    let display_name = truncate_name(&label, 12);

    rsx! {
        button {
            class: "flex items-center justify-between px-2 py-1.5 rounded text-xs font-medium \
                    border transition-all cursor-pointer hover:brightness-110 active:brightness-90",
            style: "{style}",
            onclick: {
                let id = id.clone();
                move |_| on_toggle_bypass.call(id.clone())
            },
            title: "{label}",

            span { class: "truncate", "{display_name}" }

            div {
                class: if bypassed {
                    "w-2 h-2 rounded-full bg-red-500 flex-shrink-0 ml-1"
                } else {
                    "w-2 h-2 rounded-full bg-green-500 flex-shrink-0 ml-1"
                },
            }
        }
    }
}

// endregion: --- CompactBlockPill

// region: --- Helpers

/// Extract display-ready block data from a signal chain.
fn extract_blocks(chain: &SignalChain) -> Vec<BlockDisplay> {
    chain
        .blocks()
        .iter()
        .map(|mb| {
            let params = mb
                .overrides()
                .iter()
                .map(|o| (o.parameter_id().to_string(), o.value().get()))
                .collect();

            BlockDisplay {
                id: mb.id().to_string(),
                label: mb.label().to_string(),
                block_type: mb.block_type(),
                bypassed: false, // overrides don't track bypass at ModuleBlock level
                parameters: params,
            }
        })
        .collect()
}

/// Truncate a name to fit in compact view.
fn truncate_name(name: &str, max_len: usize) -> String {
    if name.len() <= max_len {
        name.to_string()
    } else {
        format!("{}\u{2026}", &name[..max_len - 1])
    }
}

// endregion: --- Helpers
