//! FX Binding Status UI — live DAW FX chain connection status and management.
//!
//! Shows a toolbar icon (green/yellow/red) indicating DAW connection state,
//! with a side-sheet panel listing bound tracks, FX tree, and binding controls.

use dioxus::prelude::*;


/// Connection health state for the DAW FX binding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BindingHealth {
    /// All modules have live FX bindings.
    Connected,
    /// Some modules are unbound or stale.
    Partial,
    /// No DAW connection or all bindings lost.
    Disconnected,
}

impl BindingHealth {
    fn color_class(self) -> &'static str {
        match self {
            Self::Connected => "bg-signal-safe",
            Self::Partial => "bg-signal-warn",
            Self::Disconnected => "bg-signal-danger",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Connected => "Connected",
            Self::Partial => "Partial",
            Self::Disconnected => "Disconnected",
        }
    }
}

/// A single FX binding row in the status panel.
#[derive(Clone, PartialEq)]
pub struct FxBindingRow {
    pub module_name: String,
    pub fx_name: String,
    pub is_bound: bool,
    pub track_name: Option<String>,
}

/// Toolbar indicator dot showing overall binding health.
#[derive(Props, Clone, PartialEq)]
pub struct FxBindingIndicatorProps {
    /// Overall health status.
    health: BindingHealth,

    /// Callback to open the detail panel.
    #[props(default)]
    on_click: Option<Callback<()>>,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn FxBindingIndicator(props: FxBindingIndicatorProps) -> Element {
    let color = props.health.color_class();
    let label = props.health.label();

    rsx! {
        button {
            class: format!(
                "inline-flex items-center gap-1.5 px-2 py-1 rounded text-xs font-medium text-foreground hover:bg-muted transition-colors {}",
                props.class
            ),
            title: "DAW FX Binding: {label}",
            onclick: move |_| {
                if let Some(cb) = &props.on_click {
                    cb.call(());
                }
            },

            // Status dot
            div {
                class: format!("w-2 h-2 rounded-full {color}"),
            }
            span { "FX" }
        }
    }
}

/// Detail panel showing all FX bindings with refresh and auto-bind controls.
#[derive(Props, Clone, PartialEq)]
pub struct FxBindingPanelProps {
    /// List of FX binding rows.
    bindings: Vec<FxBindingRow>,

    /// Overall health.
    health: BindingHealth,

    /// Callback to refresh bindings from the DAW.
    #[props(default)]
    on_refresh: Option<Callback<()>>,

    /// Callback to auto-bind all unbound modules.
    #[props(default)]
    on_auto_bind: Option<Callback<()>>,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

#[component]
pub fn FxBindingPanel(props: FxBindingPanelProps) -> Element {
    let bound_count = props.bindings.iter().filter(|b| b.is_bound).count();
    let total = props.bindings.len();

    rsx! {
        div {
            class: format!("flex flex-col gap-3 p-4 {}", props.class),

            // Header
            div {
                class: "flex items-center justify-between",
                h3 {
                    class: "text-xs uppercase tracking-widest text-muted-foreground font-semibold",
                    "FX Bindings"
                }
                span {
                    class: "text-xs text-muted-foreground",
                    "{bound_count}/{total} bound"
                }
            }

            // Status bar
            div {
                class: "flex items-center gap-2",
                div {
                    class: format!("w-2 h-2 rounded-full {}", props.health.color_class()),
                }
                span {
                    class: "text-xs text-muted-foreground",
                    {props.health.label()}
                }
            }

            // Action buttons
            div {
                class: "flex gap-2",
                button {
                    class: "px-3 py-1 text-xs rounded bg-secondary text-secondary-foreground hover:bg-secondary/80",
                    onclick: move |_| {
                        if let Some(cb) = &props.on_refresh {
                            cb.call(());
                        }
                    },
                    "Refresh"
                }
                button {
                    class: "px-3 py-1 text-xs rounded bg-primary text-primary-foreground hover:bg-primary/90",
                    onclick: move |_| {
                        if let Some(cb) = &props.on_auto_bind {
                            cb.call(());
                        }
                    },
                    "Auto-Bind"
                }
            }

            // Binding list
            div {
                class: "flex flex-col gap-1 max-h-64 overflow-y-auto",
                for binding in props.bindings.iter() {
                    div {
                        class: "flex items-center justify-between px-2 py-1.5 rounded text-xs hover:bg-muted",
                        div {
                            class: "flex items-center gap-2",
                            div {
                                class: format!(
                                    "w-1.5 h-1.5 rounded-full {}",
                                    if binding.is_bound { "bg-signal-safe" } else { "bg-signal-danger" }
                                ),
                            }
                            span {
                                class: "font-medium",
                                "{binding.module_name}"
                            }
                        }
                        span {
                            class: "text-muted-foreground truncate max-w-32",
                            if let Some(track) = &binding.track_name {
                                "{track} \u{2192} {binding.fx_name}"
                            } else {
                                "Unbound"
                            }
                        }
                    }
                }
            }
        }
    }
}
