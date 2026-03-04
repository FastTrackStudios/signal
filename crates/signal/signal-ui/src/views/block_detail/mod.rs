//! Block detail panel — full-featured editor with sub-tab navigation.
//!
//! Provides a rich editing experience for a single block:
//! - **Custom GUI**: Curated parameter knobs (if `ParamCuration` is set)
//! - **Raw Params**: Full searchable parameter list
//! - **Macros**: Assignable macro knobs with parameter bindings
//! - **Modulation**: LFO, envelope, MIDI CC routing to parameters

mod custom_gui;
mod macros;
mod modulation;
mod raw_params;

use dioxus::prelude::*;

pub use custom_gui::BlockCustomGui;
pub use macros::BlockMacros;
pub use modulation::BlockModulation;
pub use raw_params::BlockRawParams;

/// Which sub-tab is active in the block detail panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockDetailTab {
    CustomGui,
    RawParams,
    Macros,
    Modulation,
}

impl BlockDetailTab {
    pub const ALL: &'static [BlockDetailTab] = &[
        Self::CustomGui,
        Self::RawParams,
        Self::Macros,
        Self::Modulation,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::CustomGui => "Custom",
            Self::RawParams => "Params",
            Self::Macros => "Macros",
            Self::Modulation => "Mod",
        }
    }
}

/// Props for the full block detail panel.
#[derive(Props, Clone, PartialEq)]
pub struct BlockDetailPanelProps {
    /// The block being edited.
    pub block: signal::Block,
    /// Block type for color theming.
    #[props(default)]
    pub block_type: signal::BlockType,
    /// Callback when a parameter value changes: (param_id, new_value).
    #[props(default)]
    pub on_param_change: Option<EventHandler<(String, f32)>>,
    /// Callback when a macro knob value changes: (knob_id, new_value).
    #[props(default)]
    pub on_macro_change: Option<EventHandler<(String, f32)>>,
    /// Callback to save the block.
    #[props(default)]
    pub on_save: Option<EventHandler<signal::Block>>,
    /// Callback to close the detail panel.
    #[props(default)]
    pub on_close: Option<EventHandler<()>>,
}

/// Full block detail panel with sub-tab navigation.
///
/// Designed to appear as an expanded overlay or right-side panel,
/// giving users access to all block editing capabilities.
#[component]
pub fn BlockDetailPanel(props: BlockDetailPanelProps) -> Element {
    let mut active_tab = use_signal(|| {
        // Default to CustomGui if curation exists, otherwise RawParams
        if props.block.param_curation.is_some() {
            BlockDetailTab::CustomGui
        } else {
            BlockDetailTab::RawParams
        }
    });

    let color = props.block_type.color();

    rsx! {
        div { class: "flex flex-col h-full bg-zinc-950/95 border-l border-zinc-800",
            // ── Header with close button ──
            div { class: "flex items-center justify-between px-4 py-2 border-b border-zinc-800/60",
                h3 { class: "text-sm font-semibold text-zinc-200", "Block Detail" }
                if let Some(ref on_close) = props.on_close {
                    {
                        let on_close = on_close.clone();
                        rsx! {
                            button {
                                class: "text-zinc-500 hover:text-zinc-300 text-lg leading-none px-1",
                                onclick: move |_| on_close.call(()),
                                "\u{2715}"
                            }
                        }
                    }
                }
            }

            // ── Sub-tab pill bar ──
            div { class: "flex gap-1 px-4 py-2 border-b border-zinc-800/40",
                for &tab in BlockDetailTab::ALL {
                    {
                        let is_active = active_tab() == tab;
                        rsx! {
                            button {
                                key: "{tab:?}",
                                class: if is_active {
                                    "px-3 py-1 text-[11px] font-medium rounded-full transition-colors"
                                } else {
                                    "px-3 py-1 text-[11px] text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 rounded-full transition-colors"
                                },
                                style: if is_active {
                                    format!("background-color: {}; color: {};", color.bg, color.fg)
                                } else {
                                    String::new()
                                },
                                onclick: move |_| active_tab.set(tab),
                                "{tab.label()}"
                            }
                        }
                    }
                }
            }

            // ── Content area ──
            div { class: "flex-1 overflow-y-auto",
                match active_tab() {
                    BlockDetailTab::CustomGui => rsx! {
                        BlockCustomGui {
                            block: props.block.clone(),
                            block_type: props.block_type,
                            on_param_change: move |(id, val)| {
                                if let Some(ref cb) = props.on_param_change {
                                    cb.call((id, val));
                                }
                            },
                        }
                    },
                    BlockDetailTab::RawParams => rsx! {
                        BlockRawParams {
                            block: props.block.clone(),
                            block_type: props.block_type,
                            on_param_change: move |(id, val)| {
                                if let Some(ref cb) = props.on_param_change {
                                    cb.call((id, val));
                                }
                            },
                        }
                    },
                    BlockDetailTab::Macros => rsx! {
                        BlockMacros {
                            block: props.block.clone(),
                            on_macro_change: move |(id, val)| {
                                if let Some(ref cb) = props.on_macro_change {
                                    cb.call((id, val));
                                }
                            },
                        }
                    },
                    BlockDetailTab::Modulation => rsx! {
                        BlockModulation {
                            block: props.block.clone(),
                        }
                    },
                }
            }
        }
    }
}
