//! Node block component for the node graph view.
//!
//! Renders a single processing node as an absolutely positioned div with:
//! - Color-coded rounded rect background (cyan glow when selected)
//! - Header with name and bypass indicator (draggable)
//! - Content area with widget visualization
//! - Port circles on left (inputs) and right (outputs) edges

use dioxus::prelude::*;
use uuid::Uuid;

use super::models::{Node, NodePort, NodeWidget};

/// Callback data when a port drag starts.
#[derive(Debug, Clone)]
pub struct PortDragStart {
    pub port_name: String,
    pub is_output: bool,
}

/// Callback data when a port is hovered during wire drafting.
#[derive(Debug, Clone)]
pub struct PortHoverEvent {
    pub node_id: Uuid,
    pub port_name: String,
    pub is_hovering: bool,
}

/// Props for a node block.
#[derive(Props, Clone, PartialEq)]
pub struct NodeBlockProps {
    pub node: Node,
    #[props(default)]
    pub offset_x: f64,
    #[props(default)]
    pub offset_y: f64,
    #[props(default)]
    pub compact: bool,
    #[props(default)]
    pub is_selected: bool,
    #[props(default)]
    pub on_select: Option<Callback<Uuid>>,
    #[props(default)]
    pub on_header_drag_start: Option<EventHandler<MouseEvent>>,
    #[props(default)]
    pub on_port_drag_start: Option<Callback<PortDragStart>>,
    #[props(default)]
    pub on_port_hover: Option<Callback<PortHoverEvent>>,
    #[props(default)]
    pub on_port_hover_end: Option<Callback<()>>,
    #[props(default)]
    pub on_double_click: Option<Callback<Uuid>>,
    #[props(default)]
    pub wire_draft_active: bool,
    #[props(default)]
    pub hovered_port: Option<(Uuid, String, bool)>,
}

/// Renders a node block on the canvas.
#[component]
pub fn NodeBlock(props: NodeBlockProps) -> Element {
    let node = &props.node;
    let x = node.position.x + props.offset_x;
    let y = node.position.y + props.offset_y;
    let w = node.size.width;
    let h = node.size.height;

    let color = node.block_type.color();
    let style_str = format!(
        "background-color: {}; color: {}; border-color: {};",
        color.bg, color.fg, color.border
    );
    let opacity = if node.bypassed { "0.5" } else { "1.0" };

    let header_h = if props.compact { h } else { 28.0 };
    let content_h = if props.compact { 0.0 } else { h - 28.0 };

    let on_header_drag = props.on_header_drag_start.clone();
    let on_select = props.on_select.clone();
    let on_dbl_click = props.on_double_click.clone();
    let node_id = node.id;
    let header_cursor = if props.wire_draft_active {
        "crosshair"
    } else {
        "grab"
    };

    let display_label = node.short_label.as_deref().unwrap_or(&node.name);

    let tooltip = node
        .description
        .as_deref()
        .or_else(|| {
            if node.short_label.is_some() {
                Some(node.name.as_str())
            } else {
                None
            }
        })
        .unwrap_or("");

    let selection_style = if props.is_selected {
        "box-shadow: 0 0 12px 2px rgba(34,211,238,0.5); border-color: #22d3ee !important;"
    } else {
        ""
    };

    let node_class = if node.is_placeholder {
        if props.compact {
            "absolute rounded overflow-hidden border border-dashed transition-shadow duration-150 opacity-40"
        } else {
            "absolute rounded-lg overflow-hidden shadow-md border-2 border-dashed transition-shadow duration-150 opacity-40"
        }
    } else if props.compact {
        "absolute rounded overflow-hidden border transition-shadow duration-150"
    } else {
        "absolute rounded-lg overflow-hidden shadow-md border-2 transition-shadow duration-150"
    };

    let header_class = if props.compact {
        "flex items-center justify-between px-1.5 text-[9px] font-semibold select-none truncate"
    } else {
        "flex items-center justify-between px-2 py-1 text-[11px] font-semibold select-none"
    };

    rsx! {
        div {
            class: "{node_class}",
            style: "left: {x}px; top: {y}px; width: {w}px; height: {h}px; \
                    {style_str} opacity: {opacity}; {selection_style}",
            title: "{tooltip}",
            onmousedown: move |evt| {
                evt.stop_propagation();
                if let Some(ref cb) = on_select {
                    cb.call(node_id);
                }
            },
            ondoubleclick: move |evt| {
                evt.stop_propagation();
                if let Some(ref cb) = on_dbl_click {
                    cb.call(node_id);
                }
            },

            // Header
            div {
                class: "{header_class}",
                style: "height: {header_h}px; cursor: {header_cursor};",
                onmousedown: {
                    move |evt: MouseEvent| {
                        evt.stop_propagation();
                        if let Some(ref handler) = on_header_drag {
                            handler.call(evt);
                        }
                    }
                },
                if node.is_placeholder {
                    span { class: "flex-shrink-0 mr-1 text-[10px] opacity-70", "+" }
                }
                span { class: "truncate", "{display_label}" }
                if node.bypassed {
                    span { class: "text-[8px] opacity-50 flex-shrink-0 ml-1", "BYP" }
                }
            }

            // Content area
            if !props.compact {
                if node.is_placeholder {
                    div {
                        class: "flex-1 flex items-center justify-center overflow-hidden",
                        style: "height: {content_h}px;",
                        div {
                            class: "flex flex-col items-center gap-1 opacity-60",
                            span { class: "text-xl font-light leading-none", "+" }
                            span { class: "text-[9px] uppercase tracking-wider", "Assign" }
                        }
                    }
                } else {
                    div {
                        class: "flex-1 overflow-hidden",
                        style: "height: {content_h}px;",
                        NodeWidgetContent {
                            widget: node.widget,
                            width: w as u32,
                            height: content_h as u32,
                            bypassed: node.bypassed,
                        }
                    }
                }
            }

            // Ports (hide in compact mode)
            if !props.compact {
                for (idx, port) in node.inputs.iter().enumerate() {
                    PortCircle {
                        key: "{port.id}",
                        port: port.clone(),
                        index: idx,
                        total: node.inputs.len(),
                        node_height: h,
                        is_input: true,
                        entity_id: node.id,
                        on_port_drag_start: props.on_port_drag_start.clone(),
                        on_port_hover: props.on_port_hover.clone(),
                        on_port_hover_end: props.on_port_hover_end.clone(),
                        wire_draft_active: props.wire_draft_active,
                        is_hovered: props.hovered_port.as_ref().map_or(false, |(eid, pid, _)| {
                            *eid == node.id && pid == &port.id
                        }),
                    }
                }
                for (idx, port) in node.outputs.iter().enumerate() {
                    PortCircle {
                        key: "{port.id}",
                        port: port.clone(),
                        index: idx,
                        total: node.outputs.len(),
                        node_height: h,
                        node_width: w,
                        is_input: false,
                        entity_id: node.id,
                        on_port_drag_start: props.on_port_drag_start.clone(),
                        on_port_hover: props.on_port_hover.clone(),
                        on_port_hover_end: props.on_port_hover_end.clone(),
                        wire_draft_active: props.wire_draft_active,
                        is_hovered: props.hovered_port.as_ref().map_or(false, |(eid, pid, _)| {
                            *eid == node.id && pid == &port.id
                        }),
                    }
                }
            }
        }
    }
}

// ── Port Circle ──────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct PortCircleProps {
    port: NodePort,
    index: usize,
    total: usize,
    node_height: f64,
    #[props(default)]
    node_width: f64,
    is_input: bool,
    entity_id: Uuid,
    #[props(default)]
    on_port_drag_start: Option<Callback<PortDragStart>>,
    #[props(default)]
    on_port_hover: Option<Callback<PortHoverEvent>>,
    #[props(default)]
    on_port_hover_end: Option<Callback<()>>,
    #[props(default)]
    wire_draft_active: bool,
    #[props(default)]
    is_hovered: bool,
}

#[component]
fn PortCircle(props: PortCircleProps) -> Element {
    let base_size = 10.0;
    let port_size = if props.is_hovered {
        base_size + 4.0
    } else {
        base_size
    };
    let size_offset = if props.is_hovered { -2.0 } else { 0.0 };

    let spacing = props.node_height / (props.total + 1) as f64;
    let port_y = spacing * (props.index + 1) as f64 - base_size / 2.0 + size_offset;

    let port_x = if props.is_input {
        -base_size / 2.0 + size_offset
    } else {
        props.node_width - base_size / 2.0 + size_offset
    };

    let color = props.port.color.as_deref().unwrap_or("#ffffff");

    let glow = if props.is_hovered {
        "0 0 8px 3px #22d3ee"
    } else if props.wire_draft_active {
        "0 0 4px 1px rgba(255,255,255,0.3)"
    } else {
        "none"
    };

    let on_port_start = props.on_port_drag_start.clone();
    let on_hover = props.on_port_hover.clone();
    let on_hover_end = props.on_port_hover_end.clone();
    let port_id = props.port.id.clone();
    let entity_id = props.entity_id;
    let is_input = props.is_input;
    let wire_active = props.wire_draft_active;

    rsx! {
        div {
            class: "absolute rounded-full border border-white/50 transition-all duration-150",
            style: "left: {port_x}px; top: {port_y}px; \
                    width: {port_size}px; height: {port_size}px; \
                    background-color: {color}; \
                    box-shadow: {glow}; \
                    cursor: crosshair; \
                    pointer-events: auto;",
            title: "{props.port.label}",
            onmousedown: {
                let port_id = port_id.clone();
                move |evt: MouseEvent| {
                    evt.stop_propagation();
                    if let Some(ref cb) = on_port_start {
                        cb.call(PortDragStart { port_name: port_id.clone(), is_output: !is_input });
                    }
                }
            },
            onmouseenter: {
                let port_id = port_id.clone();
                move |_| {
                    if wire_active {
                        if let Some(ref cb) = on_hover {
                            cb.call(PortHoverEvent { node_id: entity_id, port_name: port_id.clone(), is_hovering: is_input });
                        }
                    }
                }
            },
            onmouseleave: move |_| {
                if let Some(ref cb) = on_hover_end {
                    cb.call(());
                }
            },
        }
    }
}

// ── Widget Content ───────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct NodeWidgetContentProps {
    widget: NodeWidget,
    width: u32,
    height: u32,
    bypassed: bool,
}

#[component]
fn NodeWidgetContent(props: NodeWidgetContentProps) -> Element {
    if props.bypassed {
        return rsx! {
            div { class: "flex items-center justify-center h-full text-xs opacity-30",
                "BYPASSED"
            }
        };
    }

    let has_room = props.width >= 80 && props.height >= 50;

    match props.widget {
        NodeWidget::EqGraph if has_room => rsx! { WidgetPlaceholder { label: "EQ" } },
        NodeWidget::CompressorGraph if has_room => rsx! { WidgetPlaceholder { label: "COMP" } },
        NodeWidget::GateGraph if has_room => rsx! { WidgetPlaceholder { label: "GATE" } },
        NodeWidget::AmpCab if has_room => rsx! { WidgetPlaceholder { label: "AMP" } },
        NodeWidget::DelayGraph if has_room => rsx! { WidgetPlaceholder { label: "DLY" } },
        NodeWidget::ReverbGraph if has_room => rsx! { WidgetPlaceholder { label: "REV" } },
        NodeWidget::ModulationGraph if has_room => rsx! { WidgetPlaceholder { label: "MOD" } },
        NodeWidget::DriveGraph if has_room => rsx! { WidgetPlaceholder { label: "DRV" } },
        NodeWidget::Tuner => rsx! { WidgetPlaceholder { label: "TUNE" } },
        NodeWidget::Looper => rsx! { WidgetPlaceholder { label: "LOOP" } },
        _ => rsx! {},
    }
}

/// Simple SVG placeholder widget visualization.
#[derive(Props, Clone, PartialEq)]
struct WidgetPlaceholderProps {
    label: &'static str,
}

#[component]
fn WidgetPlaceholder(props: WidgetPlaceholderProps) -> Element {
    rsx! {
        div {
            class: "flex items-center justify-center h-full w-full",
            style: "opacity: 0.3;",
            svg {
                width: "100%",
                height: "100%",
                view_box: "0 0 100 40",
                // Simple waveform hint
                path {
                    d: "M 5,20 Q 15,5 25,20 T 45,20 T 65,20 T 85,20 L 95,20",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "1.5",
                    stroke_linecap: "round",
                    opacity: "0.6",
                }
                text {
                    x: "50",
                    y: "36",
                    text_anchor: "middle",
                    font_size: "8",
                    fill: "currentColor",
                    opacity: "0.5",
                    "{props.label}"
                }
            }
        }
    }
}
