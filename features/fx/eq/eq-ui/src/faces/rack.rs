//! Drawing a [`RackDesign`] for the EQ.
//!
//! The description of a panel is shared with the compressor
//! ([`fts_ui_audio::hardware::rack`]); what is local is where a control id
//! turns into a [`ParamHandle`] — here, one parameter per control through
//! [`crate::faces::params_map`].
//!
//! An EQ panel has no meter movement, so [`RackItem::Vu`] is not used: the
//! only thing worth watching on an equalizer is the curve, and that lives on
//! the Main face.

use audiocore_core::prelude::*;
use fts_ui_audio::hardware::knob::HardwareKnob;
use fts_ui_audio::hardware::panel::{panel_scale, Panel, PanelSlot, Silkscreen};
use fts_ui_audio::hardware::rack::{RackDesign, RackItem};
use fts_ui_audio::hardware::switches::{RatioButtons, ToggleSwitch};
use fts_ui_audio::shell::RAIL_W;
use fts_ui_audio::ParamHandle;

use crate::faces::params_map::control_ptr;
use crate::param_adapter::param_handle;
use crate::params::EqUiState;

/// Where a knob's legend sits below its centre, in design px.
const LEGEND_DROP: f64 = 58.0;
/// Legend box width — narrow enough that neighbours on a seven-control row do
/// not run into each other.
const LEGEND_W: f64 = 118.0;

/// Draw an EQ model's front panel.
#[component]
pub fn EqRackFace(
    design: RackDesign,
    /// `model` parameter value this panel is for — control ids resolve against
    /// it, so a Pultec knob can never bind an SSL parameter.
    model: i32,
    /// The shell's redraw tick. Not read; its only job is to change, so the
    /// panel re-renders against fresh parameter values rather than being
    /// memoized away.
    frame: u64,
) -> Element {
    let _ = frame;
    let shared = use_context::<SharedState>();
    let ui = shared.get::<EqUiState>().expect("EqUiState missing");
    let ctx = use_param_context();
    let params = ui.params.clone();
    let scale = panel_scale(design.w, design.h, RAIL_W);

    // Panics rather than skipping: a control on a panel that resolves to no
    // parameter is a knob that does nothing, and `every_placed_control_resolves`
    // is the test that keeps it from shipping.
    let handle = move |id: &str| -> ParamHandle {
        let ptr = control_ptr(&params, model, id)
            .unwrap_or_else(|| panic!("model {model} has no parameter for control {id}"));
        param_handle(ptr, ctx.clone())
    };

    rsx! {
        Panel {
            design_w: design.w,
            design_h: design.h,
            scale,
            background: design.paint.to_string(),
            chrome: design.chrome.to_string(),

            for item in design.items.iter().copied() {
                {
                    match item {
                        RackItem::Knob { id, legend, x, y, d, ring } => {
                            let box_w = d * 2.0;
                            rsx! {
                                PanelSlot { scale, x, y, w: box_w, h: box_w,
                                    HardwareKnob {
                                        handle: handle(id),
                                        testid: id.replace('_', "-"),
                                        scale,
                                        diameter: d,
                                        style: design.knob,
                                        ink: design.ink.to_string(),
                                        marks: ring.marks(),
                                    }
                                }
                                Silkscreen {
                                    scale, x, y: y + LEGEND_DROP, width: LEGEND_W,
                                    text: legend.to_string(), color: design.ink.to_string(),
                                }
                            }
                        }
                        RackItem::Buttons { id, legend, x, y, labels } => rsx! {
                            PanelSlot { scale, x, y, w: 90.0, h: 180.0,
                                RatioButtons {
                                    handle: handle(id),
                                    testid: id.replace('_', "-"),
                                    scale,
                                    labels: labels.iter().map(|s| s.to_string()).collect(),
                                    ink: design.ink.to_string(),
                                }
                            }
                            Silkscreen {
                                scale, x, y: y + LEGEND_DROP + 12.0, width: LEGEND_W,
                                text: legend.to_string(), color: design.ink.to_string(),
                            }
                        },
                        RackItem::Switch { id, legend, x, y, labels } => rsx! {
                            PanelSlot { scale, x, y, w: 110.0, h: 100.0,
                                ToggleSwitch {
                                    handle: handle(id),
                                    testid: id.replace('_', "-"),
                                    scale,
                                    labels: [labels[0].to_string(), labels[1].to_string()],
                                    ink: design.ink.to_string(),
                                }
                            }
                            Silkscreen {
                                scale, x, y: y + LEGEND_DROP, width: LEGEND_W,
                                text: legend.to_string(), color: design.ink.to_string(),
                            }
                        },
                        RackItem::Text { x, y, text, size, strong } => rsx! {
                            Silkscreen {
                                scale, x, y,
                                text: text.to_string(),
                                width: 380.0,
                                size,
                                weight: if strong { 700 } else { 600 },
                                tracking: if strong { 0.22 } else { 0.14 },
                                color: if strong {
                                    design.ink.to_string()
                                } else {
                                    design.dim_ink.to_string()
                                },
                            }
                        },
                        // EQ panels carry no meter movement — see the module
                        // docs.
                        RackItem::Vu { .. } => rsx! {},
                    }
                }
            }
        }
    }
}
