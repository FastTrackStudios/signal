//! MIDI Learn UI — interactive MIDI CC assignment and monitoring.
//!
//! Shows a list of assignable parameters with MIDI learn mode:
//! click a parameter, twiddle a MIDI CC, and the mapping is created.

use dioxus::prelude::*;

/// A MIDI CC mapping.
#[derive(Clone, PartialEq)]
pub struct MidiMapping {
    pub id: String,
    pub param_name: String,
    pub module_name: String,
    pub cc_number: u8,
    pub channel: u8,
    pub min_value: f64,
    pub max_value: f64,
}

/// MIDI learn mode state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LearnState {
    /// Not learning — normal operation.
    Idle,
    /// Waiting for MIDI input to map to a parameter.
    Learning,
}

/// MIDI Learn panel.
#[derive(Props, Clone, PartialEq)]
pub struct MidiLearnPanelProps {
    /// Existing MIDI mappings.
    mappings: Vec<MidiMapping>,

    /// Current learn state.
    #[props(default = LearnState::Idle)]
    learn_state: LearnState,

    /// Parameter currently being learned (if any).
    #[props(default)]
    learning_param: Option<String>,

    /// Last received MIDI CC (for live feedback).
    #[props(default)]
    last_cc: Option<(u8, u8)>,

    /// Callback to toggle learn mode.
    #[props(default)]
    on_toggle_learn: Option<Callback<()>>,

    /// Callback to delete a mapping by ID.
    #[props(default)]
    on_delete_mapping: Option<Callback<String>>,

    /// Callback to update a mapping's range.
    #[props(default)]
    on_update_range: Option<Callback<(String, f64, f64)>>,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn MidiLearnPanel(props: MidiLearnPanelProps) -> Element {
    let is_learning = props.learn_state == LearnState::Learning;
    let learn_btn_class = if is_learning {
        "bg-signal-danger text-white hover:bg-signal-danger/90 animate-pulse"
    } else {
        "bg-secondary text-secondary-foreground hover:bg-secondary/80"
    };

    rsx! {
        div {
            class: format!("flex flex-col gap-3 p-4 {}", props.class),

            // Header
            div {
                class: "flex items-center justify-between",
                h3 {
                    class: "text-xs uppercase tracking-widest text-muted-foreground font-semibold",
                    "MIDI Learn"
                }
                button {
                    class: format!("px-3 py-1 text-xs rounded font-medium transition-colors {learn_btn_class}"),
                    onclick: move |_| {
                        if let Some(cb) = &props.on_toggle_learn {
                            cb.call(());
                        }
                    },
                    if is_learning { "Stop Learning" } else { "Learn" }
                }
            }

            // Learning indicator
            if is_learning {
                div {
                    class: "flex items-center gap-2 px-3 py-2 rounded bg-signal-danger/10 border border-signal-danger/30",
                    div {
                        class: "w-2 h-2 rounded-full bg-signal-danger animate-pulse",
                    }
                    div {
                        class: "text-xs",
                        if let Some(param) = &props.learning_param {
                            "Waiting for MIDI input for: {param}"
                        } else {
                            "Click a parameter, then move a MIDI controller..."
                        }
                    }
                    if let Some((ch, cc)) = props.last_cc {
                        span {
                            class: "text-[10px] text-muted-foreground ml-auto",
                            "Last: Ch{ch} CC{cc}"
                        }
                    }
                }
            }

            // Mapping list
            div {
                class: "flex flex-col gap-1 max-h-80 overflow-y-auto",

                if props.mappings.is_empty() {
                    div {
                        class: "text-xs text-muted-foreground text-center py-4",
                        "No MIDI mappings. Enable Learn mode to create one."
                    }
                }

                for mapping in props.mappings.iter() {
                    {
                        let mapping_id = mapping.id.clone();
                        rsx! {
                            div {
                                class: "flex items-center justify-between px-2 py-1.5 rounded hover:bg-muted group",

                                div {
                                    class: "flex items-center gap-2 flex-1 min-w-0",
                                    // CC badge
                                    span {
                                        class: "px-1.5 py-0.5 rounded text-[10px] font-mono bg-signal-mod/20 text-signal-mod",
                                        "CC{mapping.cc_number}"
                                    }
                                    div {
                                        class: "flex flex-col min-w-0",
                                        span {
                                            class: "text-xs font-medium truncate",
                                            "{mapping.param_name}"
                                        }
                                        span {
                                            class: "text-[10px] text-muted-foreground truncate",
                                            "{mapping.module_name} \u{2022} Ch{mapping.channel}"
                                        }
                                    }
                                }

                                // Range display
                                span {
                                    class: "text-[10px] text-muted-foreground",
                                    "{mapping.min_value:.0}%\u{2013}{mapping.max_value:.0}%"
                                }

                                // Delete button
                                button {
                                    class: "ml-2 px-1 text-[10px] text-destructive opacity-0 group-hover:opacity-100 transition-opacity",
                                    onclick: move |_| {
                                        if let Some(cb) = &props.on_delete_mapping {
                                            cb.call(mapping_id.clone());
                                        }
                                    },
                                    "\u{2715}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
