//! SVG wire-layer renderer for the node graph canvas.
//!
//! Renders committed wire connections as bezier paths and the in-progress
//! draft wire when the user drags from a port.

use dioxus::prelude::*;
use uuid::Uuid;

use super::models::NodePosition;
use super::wire::{wire_path_d, ResolvedWire, WirePath};

/// Transient state for a wire being dragged from a port but not yet connected.
#[derive(Debug, Clone, PartialEq)]
pub struct WireDraft {
    pub from_entity: Uuid,
    pub from_port: String,
    pub from_pos: NodePosition,
    pub is_from_output: bool,
    pub mouse_pos: NodePosition,
}

/// Props for the SVG wire layer.
#[derive(Props, Clone, PartialEq)]
pub struct WireLayerProps {
    pub canvas_w: f64,
    pub canvas_h: f64,
    pub wires: Vec<ResolvedWire>,
    pub wire_draft: Option<WireDraft>,
    pub hovered_port: Option<(Uuid, String, bool)>,
    pub selected_wire_id: Option<Uuid>,
    pub on_wire_click: Callback<Uuid>,
}

#[component]
pub fn WireLayer(props: WireLayerProps) -> Element {
    let selected_wire_id = props.selected_wire_id;

    rsx! {
        svg {
            style: "position: absolute; top: 0; left: 0; \
                    pointer-events: none; overflow: visible;",
            width: "{props.canvas_w}",
            height: "{props.canvas_h}",

            for wire in &props.wires {
                WirePath {
                    key: "{wire.wire_id}",
                    from: wire.from,
                    to: wire.to,
                    color: wire.color.clone(),
                    wire_id: wire.wire_id,
                    is_selected: selected_wire_id.map_or(false, |id| id == wire.wire_id),
                    on_click: {
                        move |wire_id: Uuid| {
                            props.on_wire_click.call(wire_id);
                        }
                    },
                }
            }

            // Draft wire (in-progress connection)
            if let Some(draft) = &props.wire_draft {
                {
                    let (from, to) = if draft.is_from_output {
                        (draft.from_pos, draft.mouse_pos)
                    } else {
                        (draft.mouse_pos, draft.from_pos)
                    };
                    let d = wire_path_d(&from, &to);
                    let draft_color = if props.hovered_port.is_some() {
                        "#22d3ee"
                    } else {
                        "#ffffff"
                    };
                    rsx! {
                        path {
                            d: "{d}",
                            fill: "none",
                            stroke: "{draft_color}",
                            stroke_width: "2.5",
                            stroke_opacity: "0.8",
                            stroke_linecap: "round",
                            stroke_dasharray: "8 4",
                        }
                    }
                }
            }
        }
    }
}
