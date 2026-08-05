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
use fts_ui_audio::hardware::lever::LeverSwitch;
use fts_ui_audio::hardware::panel::{panel_scale, Panel, PanelSlot, Silkscreen};
use fts_ui_audio::hardware::rack::{RackDesign, RackItem};
use fts_ui_audio::hardware::switches::{RatioButtons, ToggleSwitch};
use fts_ui_audio::shell::RAIL_W;
use fts_ui_audio::ParamHandle;

use crate::faces::params_map::control_ptr;
use crate::param_adapter::param_handle;
use crate::params::EqUiState;

/// Where a knob's legend sits below its centre, in design px.
///
/// Proportional to the knob, because the numerals it has to clear are printed
/// at a radius proportional to the knob: a fixed drop puts "BOOST" through the
/// "0" and "10" of a 96 px Pultec knob.
fn legend_drop(d: f64, label_r: f64) -> f64 {
    // Where the printed numerals actually reach: `label_r` is in the knob's
    // 110-unit viewBox, the box is 110/60 of the knob's diameter, and the
    // lowest numerals sit at cos(30°) of that radius. Then clear their text
    // and the legend's own half-height.
    //
    // It has to be computed rather than picked, because the two ring styles
    // print at different radii — a fixed drop that clears a Pultec's tight
    // numerals strikes the SSL's wider ones.
    let numerals = d * (label_r / 60.0) * 0.866;
    let text = d * (7.0 / 110.0) * 0.5 + 5.5;
    (numerals + text + 4.0).max(30.0)
}
/// Legend box width — narrow enough that neighbours on a seven-control row do
/// not run into each other, and that a knob's legend clears the printed arc of
/// a lever standing between it and the next knob.
const LEGEND_W: f64 = 88.0;

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
                            let (ring_r, label_r, ticks) = ring.geometry();
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
                                        ring_r,
                                        label_r,
                                        ticks,
                                    }
                                }
                                if !legend.is_empty() {
                                    Silkscreen {
                                        scale, x, y: y + legend_drop(d, label_r), width: LEGEND_W,
                                        text: legend.to_string(), color: design.ink.to_string(),
                                    }
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
                                scale, x, y: y + 70.0, width: LEGEND_W,
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
                                scale, x, y: y + 58.0, width: LEGEND_W,
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
                        RackItem::Lever { id, legend, unit, x, y, labels } => rsx! {
                            PanelSlot { scale, x, y, w: 132.0, h: 132.0,
                                LeverSwitch {
                                    length: 41.0,
                                    handle: handle(id),
                                    testid: id.replace('_', "-"),
                                    scale,
                                    labels: labels.iter().map(|s| s.to_string()).collect(),
                                    unit: unit.to_string(),
                                    ink: design.ink.to_string(),
                                }
                            }
                            Silkscreen {
                                scale, x, y: y + 62.0, width: 170.0,
                                text: legend.to_string(), color: design.ink.to_string(),
                            }
                        },
                        RackItem::Readout { id, x, y } => {
                            // The panel's own 0–10 scale, which is what the
                            // numerals around the knob mean too — not the
                            // engine's dB.
                            let value = handle(id).normalized() * 10.0;
                            rsx! {
                                Silkscreen {
                                    scale, x, y, width: 90.0, size: 10.0,
                                    tracking: 0.02, weight: 600,
                                    text: format!("{value:.1}"),
                                    color: design.ink.to_string(),
                                }
                            }
                        }
                        RackItem::Lamp { x, y, color } => rsx! {
                            PanelSlot { scale, x, y, w: 30.0, h: 30.0,
                                div {
                                    style: format!(
                                        "width:{:.1}px; height:{:.1}px; border-radius:50%; \
                                         background:radial-gradient(circle at 40% 34%, {color}, \
                                         rgba(0,0,0,0.75)); \
                                         box-shadow:0 0 {:.1}px {color}, \
                                         inset 0 0 {:.1}px rgba(0,0,0,0.5); \
                                         border:{:.1}px solid rgba(0,0,0,0.55);",
                                        17.0 * scale,
                                        17.0 * scale,
                                        7.0 * scale,
                                        4.0 * scale,
                                        (1.5 * scale).max(1.0),
                                    ),
                                }
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
