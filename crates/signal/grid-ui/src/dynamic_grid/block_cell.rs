//! Grid block cell component — occupied cell with block info, color dot, and connection ports.

use dioxus::prelude::*;
use uuid::Uuid;

use super::layout::PORT_SIZE;

#[derive(Props, Clone, PartialEq)]
pub(super) struct GridBlockCellProps {
    pub slot_id: Uuid,
    pub block_type_name: String,
    /// Glyph for the block's type (see `icons::block_icon`).
    pub icon: String,
    pub name: String,
    pub cell_style: String,
    pub cell_class: String,
    pub dot_color: String,
    pub port_color: String,
    pub port_opacity: String,
    pub port_half: f64,
    pub left_port_hovered: bool,
    pub right_port_hovered: bool,
    #[props(default)]
    pub outer_style: String,
    pub on_block_mousedown: EventHandler<MouseEvent>,
    pub on_context_menu: EventHandler<MouseEvent>,
    pub on_left_port_mousedown: EventHandler<MouseEvent>,
    pub on_right_port_mousedown: EventHandler<MouseEvent>,
    pub on_left_port_enter: EventHandler<()>,
    pub on_left_port_leave: EventHandler<()>,
    pub on_right_port_enter: EventHandler<()>,
    pub on_right_port_leave: EventHandler<()>,
}

#[component]
pub(super) fn GridBlockCell(props: GridBlockCellProps) -> Element {
    let port_half = props.port_half;

    // The block TYPE is the primary (centered) label; the block's own name —
    // its preset — is the secondary line, shown only when it actually adds
    // information (skip "Compressor / Compressor" duplication).
    let preset = (!props.name.trim().is_empty()
        && !props.name.eq_ignore_ascii_case(&props.block_type_name))
    .then(|| props.name.clone());

    let left_port_style = format!(
        "left: {}px; top: 50%; transform: translateY(-50%); \
         width: {}px; height: {}px; background-color: {}; opacity: {};",
        -port_half as i32, PORT_SIZE, PORT_SIZE, props.port_color, props.port_opacity,
    );
    let right_port_style = format!(
        "right: {}px; top: 50%; transform: translateY(-50%); \
         width: {}px; height: {}px; background-color: {}; opacity: {};",
        -port_half as i32, PORT_SIZE, PORT_SIZE, props.port_color, props.port_opacity,
    );

    rsx! {
        div {
            key: "{props.slot_id}",
            class: "relative aspect-square",
            style: "{props.outer_style}",
            div {
                class: "{props.cell_class}",
                style: "{props.cell_style}",
                onmousedown: move |evt| {
                    evt.stop_propagation();
                    props.on_block_mousedown.call(evt);
                },
                oncontextmenu: move |evt: MouseEvent| {
                    props.on_context_menu.call(evt);
                },
                span {
                    class: "text-[11px] leading-none flex-shrink-0",
                    style: "color: {props.dot_color};",
                    "{props.icon}"
                }
                span {
                    class: "text-[11px] font-semibold truncate max-w-full text-center px-1 leading-tight",
                    "{props.block_type_name}"
                }
                if let Some(preset) = preset {
                    span {
                        class: "text-[9px] font-mono truncate max-w-full text-center px-1 opacity-60 leading-none",
                        "{preset}"
                    }
                }
            }
            // Left input port
            div {
                class: if props.left_port_hovered {
                    "absolute rounded-full border-2 border-cyan-400 z-10 cursor-crosshair shadow-[0_0_8px_rgba(34,211,238,0.6)]"
                } else {
                    "absolute rounded-full border border-white/40 z-10 cursor-crosshair hover:border-white/70 hover:shadow-[0_0_6px_rgba(255,255,255,0.3)]"
                },
                style: "{left_port_style}",
                onmousedown: move |evt| {
                    evt.stop_propagation();
                    props.on_left_port_mousedown.call(evt);
                },
                onmouseenter: move |_| props.on_left_port_enter.call(()),
                onmouseleave: move |_| props.on_left_port_leave.call(()),
            }
            // Right output port
            div {
                class: if props.right_port_hovered {
                    "absolute rounded-full border-2 border-cyan-400 z-10 cursor-crosshair shadow-[0_0_8px_rgba(34,211,238,0.6)]"
                } else {
                    "absolute rounded-full border border-white/40 z-10 cursor-crosshair hover:border-white/70 hover:shadow-[0_0_6px_rgba(255,255,255,0.3)]"
                },
                style: "{right_port_style}",
                onmousedown: move |evt| {
                    evt.stop_propagation();
                    props.on_right_port_mousedown.call(evt);
                },
                onmouseenter: move |_| props.on_right_port_enter.call(()),
                onmouseleave: move |_| props.on_right_port_leave.call(()),
            }
        }
    }
}
