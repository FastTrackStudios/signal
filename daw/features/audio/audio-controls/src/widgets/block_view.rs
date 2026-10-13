//! Block view component for rendering audio effect blocks at different LODs.
//!
//! Provides adaptive rendering based on form factor and level of detail.

use crate::prelude::*;

use crate::core::{
    block::{BlockDefinition, BlockInstance, BlockParameter},
    layout::{FormFactor, FormFactorCategory, LayoutConstraints, LevelOfDetail},
};
use crate::widgets::knob::Knob;

/// Props for the BlockView component.
#[derive(Props, Clone, PartialEq)]
pub struct BlockViewProps {
    /// The block definition (type info, parameters, macro mappings).
    pub definition: BlockDefinition,
    /// The block instance (current state, values).
    pub instance: BlockInstance,
    /// Layout constraints (form factor, LOD, dimensions).
    #[props(default)]
    pub constraints: Option<LayoutConstraints>,
    /// Callback when a parameter value changes.
    #[props(default)]
    pub on_param_change: Option<EventHandler<(String, f32)>>,
    /// Callback when macro value changes.
    #[props(default)]
    pub on_macro_change: Option<EventHandler<f32>>,
    /// Callback when bypass state changes.
    #[props(default)]
    pub on_bypass_toggle: Option<EventHandler<bool>>,
}

/// Main block view component that adapts to different form factors and LODs.
#[component]
pub fn BlockView(props: BlockViewProps) -> Element {
    let constraints = props
        .constraints
        .unwrap_or_else(|| LayoutConstraints::from_form_factor(props.instance.form_factor));

    let lod = constraints.lod;
    let form_factor = constraints.form_factor;
    let category = form_factor.style_category();

    // Container styling based on form factor category
    let container_class = match category {
        FormFactorCategory::Plugin => "bg-zinc-800 rounded-lg border border-zinc-700",
        FormFactorCategory::Rack => "bg-zinc-900 rounded border-2 border-zinc-600",
        FormFactorCategory::Pedal => "bg-zinc-900 rounded-xl border-4 border-zinc-700",
        FormFactorCategory::Minimal => "bg-zinc-800 rounded",
    };

    let (width, height) = (constraints.available_width, constraints.available_height);

    rsx! {
        div {
            class: "block-view flex flex-col {container_class}",
            style: "width: {width}px; height: {height}px; overflow: hidden;",

            // Render based on LOD
            match lod {
                LevelOfDetail::Mini => rsx! {
                    BlockMiniView {
                        definition: props.definition.clone(),
                        instance: props.instance.clone(),
                        form_factor,
                        on_macro_change: props.on_macro_change,
                        on_bypass_toggle: props.on_bypass_toggle,
                    }
                },
                LevelOfDetail::Compact => rsx! {
                    BlockCompactView {
                        definition: props.definition.clone(),
                        instance: props.instance.clone(),
                        form_factor,
                        on_param_change: props.on_param_change,
                        on_bypass_toggle: props.on_bypass_toggle,
                    }
                },
                LevelOfDetail::Standard => rsx! {
                    BlockStandardView {
                        definition: props.definition.clone(),
                        instance: props.instance.clone(),
                        form_factor,
                        on_param_change: props.on_param_change,
                        on_macro_change: props.on_macro_change,
                        on_bypass_toggle: props.on_bypass_toggle,
                    }
                },
                LevelOfDetail::Full => rsx! {
                    BlockFullView {
                        definition: props.definition.clone(),
                        instance: props.instance.clone(),
                        form_factor,
                        on_param_change: props.on_param_change,
                        on_macro_change: props.on_macro_change,
                        on_bypass_toggle: props.on_bypass_toggle,
                    }
                },
            }
        }
    }
}

/// Props for LOD-specific views.
#[derive(Props, Clone, PartialEq)]
struct LodViewProps {
    definition: BlockDefinition,
    instance: BlockInstance,
    form_factor: FormFactor,
    #[props(default)]
    on_param_change: Option<EventHandler<(String, f32)>>,
    #[props(default)]
    on_macro_change: Option<EventHandler<f32>>,
    #[props(default)]
    on_bypass_toggle: Option<EventHandler<bool>>,
}

/// Mini view - single macro knob + bypass.
#[component]
fn BlockMiniView(props: LodViewProps) -> Element {
    let mut macro_value = use_signal(|| props.instance.macro_value);
    let mut bypassed = use_signal(|| props.instance.bypassed);

    let category_icon = props.definition.category.icon();
    let is_pedal = props.form_factor.is_pedal();

    rsx! {
        div {
            class: "flex flex-col items-center justify-center h-full p-2 gap-1",

            // Category icon
            div {
                class: "text-xs text-zinc-500 font-mono",
                "{category_icon}"
            }

            // Single macro knob
            Knob {
                value: macro_value,
                size: if is_pedal { 48 } else { 40 },
                label: Some(props.definition.name.clone()),
                on_change: move |v| {
                    macro_value.set(v);
                    if let Some(cb) = &props.on_macro_change {
                        cb.call(v);
                    }
                },
            }

            // Bypass button (footswitch style for pedals)
            BypassButton {
                bypassed: *bypassed.read(),
                is_pedal,
                on_toggle: move |b| {
                    bypassed.set(b);
                    if let Some(cb) = &props.on_bypass_toggle {
                        cb.call(b);
                    }
                },
            }
        }
    }
}

/// Compact view - key parameters only.
/// For pedals: vintage-style layout with knobs at top, name in middle, footswitch at bottom
#[component]
fn BlockCompactView(props: LodViewProps) -> Element {
    let mut bypassed = use_signal(|| props.instance.bypassed);
    let params = props.definition.parameters_for_lod(LevelOfDetail::Compact);
    let is_pedal = props.form_factor.is_pedal();

    // Limit to max 6 params for pedal view (can show more in advanced panel)
    let display_params: Vec<_> = params.into_iter().take(6).collect();

    if is_pedal {
        // Vintage pedal style layout
        rsx! {
            div {
                class: "flex flex-col h-full",

                // Top section: Knobs in a row with labels
                div {
                    class: "flex-1 flex flex-col justify-start pt-4 px-3",

                    // Knobs row - evenly distributed
                    div {
                        class: "flex justify-center gap-2 flex-wrap",

                        for param in display_params {
                            ParameterKnob {
                                param: param.clone(),
                                value: props.instance.get_param(&param.id),
                                size: 40,
                                show_label: true,
                                show_value: false,
                                on_change: {
                                    let param_id = param.id.clone();
                                    let on_param_change = props.on_param_change;
                                    move |v| {
                                        if let Some(cb) = &on_param_change {
                                            cb.call((param_id.clone(), v));
                                        }
                                    }
                                },
                            }
                        }
                    }
                }

                // Middle section: Pedal name (large, stylized)
                div {
                    class: "py-3 text-center",
                    h2 {
                        class: "text-xl font-bold text-zinc-100 tracking-tight",
                        style: "font-family: Georgia, serif; font-style: italic;",
                        "{props.definition.name}"
                    }
                }

                // Bottom section: Footswitch with ON/OFF label
                div {
                    class: "pb-4 flex flex-col items-center gap-1",
                    BypassButton {
                        bypassed: *bypassed.read(),
                        is_pedal: true,
                        on_toggle: move |b| {
                            bypassed.set(b);
                            if let Some(cb) = &props.on_bypass_toggle {
                                cb.call(b);
                            }
                        },
                    }
                    span {
                        class: "text-[10px] text-zinc-400 font-medium tracking-wider",
                        "ON/OFF"
                    }
                }
            }
        }
    } else {
        // Non-pedal compact view (cards, rack, etc.)
        // Separate input/output params from main params
        let input_param = display_params.iter().find(|p| p.id == "input").cloned();
        let output_param = display_params.iter().find(|p| p.id == "output").cloned();
        let main_params: Vec<_> = display_params
            .into_iter()
            .filter(|p| p.id != "input" && p.id != "output")
            .collect();

        rsx! {
            div {
                class: "flex flex-col h-full relative",

                // Header with name and bypass
                div {
                    class: "flex items-center justify-between px-3 py-2 border-b border-zinc-700",

                    span {
                        class: "text-sm font-semibold text-zinc-200 truncate",
                        "{props.definition.name}"
                    }

                    BypassButton {
                        bypassed: *bypassed.read(),
                        is_pedal: false,
                        on_toggle: move |b| {
                            bypassed.set(b);
                            if let Some(cb) = &props.on_bypass_toggle {
                                cb.call(b);
                            }
                        },
                    }
                }

                // Main content area with I/O in corners
                div {
                    class: "flex-1 flex",

                    // Left side - Input (if exists)
                    if let Some(param) = input_param {
                        div {
                            class: "flex flex-col items-center justify-center px-2 border-r border-zinc-700/50",
                            ParameterKnob {
                                param: param.clone(),
                                value: props.instance.get_param(&param.id),
                                size: 28,
                                show_label: true,
                                show_value: false,
                                on_change: {
                                    let param_id = param.id.clone();
                                    let on_param_change = props.on_param_change;
                                    move |v| {
                                        if let Some(cb) = &on_param_change {
                                            cb.call((param_id.clone(), v));
                                        }
                                    }
                                },
                            }
                        }
                    }

                    // Center - Main parameters
                    div {
                        class: "flex-1 flex flex-wrap items-center justify-center gap-3 p-2",

                        for param in main_params {
                            ParameterKnob {
                                param: param.clone(),
                                value: props.instance.get_param(&param.id),
                                size: 36,
                                show_label: true,
                                show_value: false,
                                on_change: {
                                    let param_id = param.id.clone();
                                    let on_param_change = props.on_param_change;
                                    move |v| {
                                        if let Some(cb) = &on_param_change {
                                            cb.call((param_id.clone(), v));
                                        }
                                    }
                                },
                            }
                        }
                    }

                    // Right side - Output (if exists)
                    if let Some(param) = output_param {
                        div {
                            class: "flex flex-col items-center justify-center px-2 border-l border-zinc-700/50",
                            ParameterKnob {
                                param: param.clone(),
                                value: props.instance.get_param(&param.id),
                                size: 28,
                                show_label: true,
                                show_value: false,
                                on_change: {
                                    let param_id = param.id.clone();
                                    let on_param_change = props.on_param_change;
                                    move |v| {
                                        if let Some(cb) = &on_param_change {
                                            cb.call((param_id.clone(), v));
                                        }
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

/// Standard view - essential parameters with labels.
#[component]
fn BlockStandardView(props: LodViewProps) -> Element {
    let mut macro_value = use_signal(|| props.instance.macro_value);
    let mut bypassed = use_signal(|| props.instance.bypassed);
    let params = props.definition.parameters_for_lod(LevelOfDetail::Standard);
    let is_pedal = props.form_factor.is_pedal();

    rsx! {
        div {
            class: "flex flex-col h-full",

            // Header
            div {
                class: "flex items-center justify-between px-3 py-2 border-b border-zinc-700",

                div {
                    class: "flex items-center gap-2",

                    span {
                        class: "text-xs text-zinc-500 font-mono",
                        "{props.definition.category.icon()}"
                    }

                    span {
                        class: "text-sm font-semibold text-zinc-200",
                        "{props.definition.name}"
                    }
                }

                if !is_pedal {
                    BypassButton {
                        bypassed: *bypassed.read(),
                        is_pedal: false,
                        on_toggle: move |b| {
                            bypassed.set(b);
                            if let Some(cb) = &props.on_bypass_toggle {
                                cb.call(b);
                            }
                        },
                    }
                }
            }

            // Parameters area
            div {
                class: "flex-1 flex flex-wrap items-start justify-center gap-3 p-3",

                // Macro knob (slightly larger, highlighted)
                div {
                    class: "flex flex-col items-center",

                    Knob {
                        value: macro_value,
                        size: 52,
                        label: Some("Macro".to_string()),
                        on_change: move |v| {
                            macro_value.set(v);
                            if let Some(cb) = &props.on_macro_change {
                                cb.call(v);
                            }
                        },
                    }
                }

                // Divider
                div {
                    class: "w-px h-12 bg-zinc-700 self-center",
                }

                // Individual parameters
                for param in params {
                    ParameterKnob {
                        param: param.clone(),
                        value: props.instance.get_param(&param.id),
                        size: 44,
                        show_label: true,
                        show_value: true,
                        on_change: {
                            let param_id = param.id.clone();
                            let on_param_change = props.on_param_change;
                            move |v| {
                                if let Some(cb) = &on_param_change {
                                    cb.call((param_id.clone(), v));
                                }
                            }
                        },
                    }
                }
            }

            // Pedal footswitch
            if is_pedal {
                div {
                    class: "flex justify-center py-3 border-t border-zinc-700",
                    BypassButton {
                        bypassed: *bypassed.read(),
                        is_pedal: true,
                        on_toggle: move |b| {
                            bypassed.set(b);
                            if let Some(cb) = &props.on_bypass_toggle {
                                cb.call(b);
                            }
                        },
                    }
                }
            }
        }
    }
}

/// Full view - all parameters, visualizations, meters.
#[component]
fn BlockFullView(props: LodViewProps) -> Element {
    let mut macro_value = use_signal(|| props.instance.macro_value);
    let mut bypassed = use_signal(|| props.instance.bypassed);
    let params = props.definition.parameters_for_lod(LevelOfDetail::Full);
    let is_pedal = props.form_factor.is_pedal();

    // Group parameters by priority for layout
    let primary_params: Vec<_> = params.iter().filter(|p| p.priority <= 2).collect();
    let secondary_params: Vec<_> = params.iter().filter(|p| p.priority > 2).collect();

    rsx! {
        div {
            class: "flex flex-col h-full",

            // Header with name, category, and bypass
            div {
                class: "flex items-center justify-between px-4 py-2 border-b border-zinc-700 bg-zinc-800/50",

                div {
                    class: "flex items-center gap-3",

                    // Category badge
                    div {
                        class: "px-2 py-0.5 bg-zinc-700 rounded text-xs font-mono text-zinc-400",
                        "{props.definition.category.icon()}"
                    }

                    span {
                        class: "text-base font-semibold text-zinc-100",
                        "{props.definition.name}"
                    }
                }

                div {
                    class: "flex items-center gap-2",

                    // Macro knob in header for quick access
                    div {
                        class: "flex items-center gap-1",

                        span {
                            class: "text-xs text-zinc-500",
                            "Macro"
                        }

                        Knob {
                            value: macro_value,
                            size: 32,
                            on_change: move |v| {
                                macro_value.set(v);
                                if let Some(cb) = &props.on_macro_change {
                                    cb.call(v);
                                }
                            },
                        }
                    }

                    if !is_pedal {
                        BypassButton {
                            bypassed: *bypassed.read(),
                            is_pedal: false,
                            on_toggle: move |b| {
                                bypassed.set(b);
                                if let Some(cb) = &props.on_bypass_toggle {
                                    cb.call(b);
                                }
                            },
                        }
                    }
                }
            }

            // Main parameter area
            div {
                class: "flex-1 flex flex-col gap-4 p-4 overflow-auto",

                // Primary parameters (larger knobs)
                if !primary_params.is_empty() {
                    div {
                        class: "flex flex-wrap items-start justify-center gap-4",

                        for param in primary_params {
                            ParameterKnob {
                                param: (*param).clone(),
                                value: props.instance.get_param(&param.id),
                                size: 56,
                                show_label: true,
                                show_value: true,
                                on_change: {
                                    let param_id = param.id.clone();
                                    let on_param_change = props.on_param_change;
                                    move |v| {
                                        if let Some(cb) = &on_param_change {
                                            cb.call((param_id.clone(), v));
                                        }
                                    }
                                },
                            }
                        }
                    }
                }

                // Secondary parameters (smaller knobs)
                if !secondary_params.is_empty() {
                    div {
                        class: "flex flex-wrap items-start justify-center gap-3 pt-2 border-t border-zinc-700/50",

                        for param in secondary_params {
                            ParameterKnob {
                                param: (*param).clone(),
                                value: props.instance.get_param(&param.id),
                                size: 40,
                                show_label: true,
                                show_value: true,
                                on_change: {
                                    let param_id = param.id.clone();
                                    let on_param_change = props.on_param_change;
                                    move |v| {
                                        if let Some(cb) = &on_param_change {
                                            cb.call((param_id.clone(), v));
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
            }

            // Pedal footswitch
            if is_pedal {
                div {
                    class: "flex justify-center py-4 border-t border-zinc-700 bg-zinc-800/30",
                    BypassButton {
                        bypassed: *bypassed.read(),
                        is_pedal: true,
                        on_toggle: move |b| {
                            bypassed.set(b);
                            if let Some(cb) = &props.on_bypass_toggle {
                                cb.call(b);
                            }
                        },
                    }
                }
            }
        }
    }
}

/// Props for the parameter knob component.
#[derive(Props, Clone, PartialEq)]
struct ParameterKnobProps {
    param: BlockParameter,
    value: f32,
    #[props(default = 48)]
    size: u32,
    #[props(default = true)]
    show_label: bool,
    #[props(default = true)]
    show_value: bool,
    on_change: EventHandler<f32>,
}

/// A knob bound to a block parameter.
#[component]
fn ParameterKnob(props: ParameterKnobProps) -> Element {
    let mut value = use_signal(|| props.value);

    // Update signal when props change
    use_effect(move || {
        value.set(props.value);
    });

    let formatted_value = props.param.format.format(*value.read());
    let label_upper = props.param.short_name.to_uppercase();

    rsx! {
        div {
            class: "flex flex-col items-center gap-1",

            // Knob first
            Knob {
                value,
                size: props.size,
                default: Some(props.param.default),
                on_change: move |v| {
                    value.set(v);
                    props.on_change.call(v);
                },
            }

            // Label below the knob (uppercase, like vintage pedals)
            if props.show_label {
                div {
                    class: "text-[9px] text-zinc-300 font-semibold tracking-wide text-center",
                    "{label_upper}"
                }
            }

            // Value below the label (optional)
            if props.show_value {
                div {
                    class: "text-[9px] text-zinc-500 font-mono whitespace-nowrap",
                    "{formatted_value}"
                }
            }
        }
    }
}

/// Props for the bypass button.
#[derive(Props, Clone, PartialEq)]
struct BypassButtonProps {
    bypassed: bool,
    #[props(default = false)]
    is_pedal: bool,
    on_toggle: EventHandler<bool>,
}

/// Bypass button (footswitch style for pedals).
#[component]
fn BypassButton(props: BypassButtonProps) -> Element {
    if props.is_pedal {
        // Vintage footswitch style - metallic/bronze look
        let outer_style = if props.bypassed {
            // Off state - darker, muted
            "background: linear-gradient(145deg, #52525b, #3f3f46); box-shadow: inset 0 2px 4px rgba(0,0,0,0.3), 0 1px 2px rgba(255,255,255,0.1);"
        } else {
            // On state - warm bronze/gold
            "background: linear-gradient(145deg, #b8860b, #8b6914); box-shadow: inset 0 2px 4px rgba(255,255,255,0.2), 0 2px 8px rgba(184,134,11,0.4);"
        };

        rsx! {
            button {
                class: "w-10 h-10 rounded-full border-2 border-zinc-500
                        transition-all duration-150 active:scale-95
                        hover:brightness-110",
                style: "{outer_style}",
                onclick: move |_| props.on_toggle.call(!props.bypassed),

                // Inner circle detail
                div {
                    class: "w-full h-full rounded-full flex items-center justify-center",
                    style: "background: radial-gradient(circle at 30% 30%, rgba(255,255,255,0.3), transparent 60%);",
                }
            }
        }
    } else {
        // Small toggle button for non-pedal views
        let active_class = if props.bypassed {
            "bg-zinc-700 text-zinc-500"
        } else {
            "bg-green-600 text-white shadow-lg shadow-green-600/30"
        };

        rsx! {
            button {
                class: "px-2 py-1 rounded text-xs font-semibold {active_class}
                        transition-all duration-150 hover:opacity-80",
                onclick: move |_| props.on_toggle.call(!props.bypassed),

                if props.bypassed { "BYP" } else { "ON" }
            }
        }
    }
}

/// A pedalboard container for arranging pedals.
#[derive(Props, Clone, PartialEq)]
pub struct PedalboardProps {
    /// Children (BlockView components).
    children: Element,
    /// Number of slots in the pedalboard.
    #[props(default = 5)]
    pub slots: u8,
    /// Form factor for pedals in this board.
    #[props(default = FormFactor::Pedal)]
    pub pedal_size: FormFactor,
}

/// A pedalboard container that arranges pedals horizontally.
#[component]
pub fn Pedalboard(props: PedalboardProps) -> Element {
    let pedal_width = props.pedal_size.min_width();
    let total_width = pedal_width * props.slots as u32 + (props.slots as u32 - 1) * 8; // 8px gap

    rsx! {
        div {
            class: "pedalboard flex items-end gap-2 p-4
                    bg-gradient-to-b from-zinc-800 to-zinc-900
                    rounded-xl border-2 border-zinc-700",
            style: "min-width: {total_width}px;",

            {props.children}
        }
    }
}

/// A rack container for arranging rack units.
#[derive(Props, Clone, PartialEq)]
pub struct RackProps {
    /// Children (BlockView components).
    children: Element,
    /// Number of rack units (1U each).
    #[props(default = 4)]
    pub units: u8,
}

/// A rack container that arranges units vertically.
#[component]
pub fn Rack(props: RackProps) -> Element {
    rsx! {
        div {
            class: "rack flex flex-col gap-1 p-2
                    bg-zinc-950 rounded border-4 border-zinc-700",

            // Rack rails decoration
            div {
                class: "absolute left-0 top-0 bottom-0 w-4
                        bg-gradient-to-r from-zinc-600 to-zinc-700",
            }
            div {
                class: "absolute right-0 top-0 bottom-0 w-4
                        bg-gradient-to-l from-zinc-600 to-zinc-700",
            }

            {props.children}
        }
    }
}
