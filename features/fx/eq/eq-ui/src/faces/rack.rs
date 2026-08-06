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
use fts_ui_audio::hardware::button::{LedMeter, PanelButton};
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
    /// The editor's form. A panel is a fixed drawing, so when the window is a
    /// shape it was never for — a portrait 500-series module, a 1U sliver —
    /// the face flows its controls instead of shrinking the panel past
    /// legibility.
    form: fts_ui_audio::EditorForm,
) -> Element {
    let _ = frame;
    let shared = use_context::<SharedState>();
    let ui = shared.get::<EqUiState>().expect("EqUiState missing");
    let ctx = use_param_context();
    let params = ui.params.clone();
    let scale = panel_scale(design.w, design.h, RAIL_W);
    // The console's LED ladders read the plugin's own metering.
    let in_db = ui.input_peak_db.load(std::sync::atomic::Ordering::Relaxed);
    let out_db = ui.output_peak_db.load(std::sync::atomic::Ordering::Relaxed);

    // Panics rather than skipping: a control on a panel that resolves to no
    // parameter is a knob that does nothing, and `every_placed_control_resolves`
    // is the test that keeps it from shipping.
    let handle = move |id: &str| -> ParamHandle {
        // An empty id is a control the panel has and the DSP does not yet: it
        // draws and does not move. Anything else must resolve — a knob bound
        // to nothing by accident is the one failure a screenshot will not
        // show you, and `every_placed_control_resolves_to_a_parameter` is what
        // keeps that from shipping.
        if id.is_empty() {
            return ParamHandle::inert("Not wired", 0.5);
        }
        let ptr = control_ptr(&params, model, id)
            .unwrap_or_else(|| panic!("model {model} has no parameter for control {id}"));
        param_handle(ptr, ctx.clone())
    };

    let (win_w, win_h) = fts_ui_audio::hardware::panel::window_logical_size()
        .unwrap_or((design.w + RAIL_W, design.h));
    if !form.wants_panel(design.w, design.h, win_w - RAIL_W, win_h) {
        return rsx! {
            CompactEqRack { design, model, avail_h: win_h }
        };
    }

    rsx! {
        Panel {
            design_w: design.w,
            design_h: design.h,
            scale,
            background: design.paint.to_string(),
            chrome: design.chrome.to_string(),
            ends: design.ends,
            texture: design.texture,

            for item in design.items.iter().copied() {
                {
                    match item {
                        RackItem::Knob { id, legend, x, y, d, ring, tint, style } => {
                            let box_w = d * 2.0;
                            let knob_style = style.unwrap_or(design.knob);
                            let (ring_r, label_r, ticks) = ring.geometry();
                            // A pointer knob's nose reaches past its body, so
                            // its panel scale is printed further out.
                            let (ring_r, label_r) = (
                                ring_r + knob_style.ring_offset(),
                                label_r + knob_style.ring_offset(),
                            );
                            rsx! {
                                PanelSlot { scale, x, y, w: box_w, h: box_w,
                                    HardwareKnob {
                                        handle: handle(id),
                                        testid: id.replace('_', "-"),
                                        scale,
                                        diameter: d,
                                        style: knob_style,
                                        ink: design.ink.to_string(),
                                        marks: ring.marks(),
                                        tint: tint.map(str::to_string),
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
                        RackItem::Buttons { id, legend, x, y, labels, reverse } => rsx! {
                            PanelSlot { scale, x, y, w: 96.0, h: 210.0,
                                RatioButtons {
                                    handle: handle(id),
                                    testid: id.replace('_', "-"),
                                    scale,
                                    labels: labels.iter().map(|s| s.to_string()).collect(),
                                    ink: design.ink.to_string(),
                                    reverse,
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
                        RackItem::TintedText { x, y, text, size, color } => rsx! {
                            Silkscreen {
                                scale, x, y,
                                text: text.to_string(),
                                width: 220.0,
                                size,
                                weight: 700,
                                tracking: 0.12,
                                color: color.to_string(),
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
                        RackItem::Button { id, label, x, y, color, ink, led } => rsx! {
                            PanelSlot { scale, x, y, w: 64.0, h: 86.0,
                                PanelButton {
                                    // An empty id is a control the panel has
                                    // and the DSP does not yet: it draws, it
                                    // does not move. See the kit's docs.
                                    handle: (!id.is_empty()).then(|| handle(id)),
                                    testid: if id.is_empty() {
                                        label.to_lowercase().replace([' ', '/'], "-")
                                    } else {
                                        id.replace('_', "-")
                                    },
                                    scale,
                                    label: label.to_string(),
                                    color: color.to_string(),
                                    ink: ink.to_string(),
                                    led: led.to_string(),
                                }
                            }
                        },
                        RackItem::LedMeter { x, y, h, right } => {
                            let level = if right { out_db } else { in_db };
                            rsx! {
                                PanelSlot { scale, x, y, w: 22.0, h: h + 6.0,
                                    LedMeter { scale, level_db: level, h }
                                }
                            }
                        }
                        RackItem::Divider { x, y, h } => rsx! {
                            div {
                                style: format!(
                                    "position:absolute; left:{:.1}px; top:{:.1}px; \
                                     width:{:.1}px; height:{:.1}px; \
                                     background:rgba(255,255,255,0.10);",
                                    x * scale,
                                    (y - h / 2.0) * scale,
                                    (1.0 * scale).max(1.0),
                                    h * scale,
                                ),
                            }
                        },
                        // EQ panels carry no meter movement — the console's
                        // metering is an LED ladder, above — and no LED rows
                        // or section frames, which are the hybrid's idiom.
                        RackItem::Vu { .. }
                        | RackItem::LedBar { .. }
                        | RackItem::LedSelect { .. }
                        | RackItem::Frame { .. }
                        | RackItem::Region { .. } => rsx! {},
                    }
                }
            }
        }
    }
}

/// The same model, flowed into a window its panel does not fit.
///
/// The *same* [`RackDesign`] items as the panel, drawn as a wrapping row of
/// cells — so a 500-series or 1U view exists for every model without a second
/// layout table per unit. Levers become their underlying stepped control,
/// because a paddle needs the arc of panel a small window does not have.
#[component]
fn CompactEqRack(design: RackDesign, model: i32, avail_h: f64) -> Element {
    let shared = use_context::<SharedState>();
    let ui = shared.get::<EqUiState>().expect("EqUiState missing");
    let ctx = use_param_context();
    let params = ui.params.clone();

    let handle = move |id: &str| -> ParamHandle {
        // An empty id is a control the panel has and the DSP does not yet: it
        // draws and does not move. Anything else must resolve — a knob bound
        // to nothing by accident is the one failure a screenshot will not
        // show you, and `every_placed_control_resolves_to_a_parameter` is what
        // keeps that from shipping.
        if id.is_empty() {
            return ParamHandle::inert("Not wired", 0.5);
        }
        let ptr = control_ptr(&params, model, id)
            .unwrap_or_else(|| panic!("model {model} has no parameter for control {id}"));
        param_handle(ptr, ctx.clone())
    };

    let rows = if avail_h > 260.0 { 3.0 } else { 1.0 };
    let cell_h = (avail_h - 20.0) / rows;
    let knob_d = (cell_h * 0.58).clamp(26.0, 46.0);
    let show_legends = cell_h > 62.0;

    rsx! {
        div {
            "data-testid": "compact-rack",
            style: format!(
                "flex:1; min-height:0; display:flex; flex-wrap:wrap; \
                 align-content:center; justify-content:center; gap:10px 14px; \
                 padding:10px; overflow:hidden; background:{};",
                design.paint,
            ),

            for item in design.items.iter().copied() {
                {
                    let cell = |id: &'static str, legend: &'static str, inner: Element| {
                        let _ = id;
                        rsx! {
                            div {
                                style: "display:flex; flex-direction:column; align-items:center; gap:3px;",
                                {inner}
                                if show_legends && !legend.is_empty() {
                                    div {
                                        style: format!(
                                            "font-size:9px; font-weight:700; letter-spacing:0.06em; \
                                             text-transform:uppercase; color:{};",
                                            design.ink,
                                        ),
                                        "{legend}"
                                    }
                                }
                            }
                        }
                    };
                    match item {
                        RackItem::Knob { id, legend, ring, tint, style, .. } => cell(id, legend, rsx! {
                            HardwareKnob {
                                handle: handle(id),
                                testid: id.replace('_', "-"),
                                scale: 1.0,
                                diameter: knob_d,
                                style: style.unwrap_or(design.knob),
                                ink: design.ink.to_string(),
                                marks: ring.marks(),
                                tint: tint.map(str::to_string),
                            }
                        }),
                        // A lever needs an arc of panel to print its legends
                        // in; at this size the same stepped parameter reads
                        // better as a knob.
                        RackItem::Lever { id, legend, labels, .. } => cell(id, legend, rsx! {
                            HardwareKnob {
                                handle: handle(id),
                                testid: id.replace('_', "-"),
                                scale: 1.0,
                                diameter: knob_d,
                                style: design.knob,
                                ink: design.ink.to_string(),
                                marks: fts_ui_audio::hardware::rack::Ring::Detents(labels).marks(),
                            }
                        }),
                        RackItem::Switch { id, legend, labels, .. } => cell(id, legend, rsx! {
                            ToggleSwitch {
                                handle: handle(id),
                                testid: id.replace('_', "-"),
                                scale: 0.8,
                                labels: [labels[0].to_string(), labels[1].to_string()],
                                ink: design.ink.to_string(),
                            }
                        }),
                        RackItem::Buttons { id, legend, labels, .. } => cell(id, legend, rsx! {
                            RatioButtons {
                                handle: handle(id),
                                testid: id.replace('_', "-"),
                                scale: 0.8,
                                labels: labels.iter().map(|s| s.to_string()).collect(),
                                ink: design.ink.to_string(),
                            }
                        }),
                        // Panel text, readouts, lamps and meters are the
                        // panel's, not the controls'.
                        _ => rsx! {},
                    }
                }
            }
        }
    }
}
