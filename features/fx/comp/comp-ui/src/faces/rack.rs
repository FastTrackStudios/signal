//! A rack unit's front panel, as data.
//!
//! Nine compressors share one anatomy: a painted panel with rack ears, a meter
//! somewhere on the left, a row of knobs with silkscreened legends, and the
//! occasional switch or bank of buttons. Hand-writing each one as its own
//! component produced five near-identical files that drifted apart in spacing
//! and colour, so a face is now a [`RackDesign`] — a table of placements — and
//! [`RackFace`] draws it.
//!
//! What stays per-unit is what should: the paint, the meter's lamp, the
//! numbers printed around each knob, and where things sit. What is shared is
//! everything that should be identical across nine panels anyway.
//!
//! The tables themselves are [`RackDesign`]s from the shared hardware kit (see
//! [`fts_ui_audio::hardware::rack`]); this is the compressor's *drawing* of
//! one. Every control is looked up on the unit's `comp-profiles` profile by id
//! and driven through [`crate::profile_handle`], so a design cannot silently
//! reference a control the profile does not have — it panics at mount, in a
//! test, rather than rendering a knob that does nothing.

use std::sync::atomic::Ordering;

use audiocore_core::prelude::*;

use crate::faces::use_face_context;
use crate::hardware::knob::HardwareKnob;
use fts_ui_audio::hardware::button::{Lamp, LedBar, LedMeter, LedSelect, PanelButton};
use fts_ui_audio::prelude::GrMeter;
use crate::hardware::rack::{RackDesign, RackItem};
use crate::hardware::panel::{panel_scale, Panel, PanelSlot, Silkscreen};
use crate::hardware::switches::{RatioButtons, ToggleSwitch};
use crate::hardware::vu::{VuMeter, VuMode};

/// Where a knob's legend sits below its centre, in design px.
const LEGEND_DROP: f64 = 60.0;
/// Legend text box width. Narrow enough that neighbouring legends on a
/// six-control row do not run into each other, which the panels are spaced for.
const LEGEND_W: f64 = 124.0;

/// Draw a [`RackDesign`].
#[component]
pub fn RackFace(
    design: RackDesign,
    /// The editor's form. A panel is a fixed drawing, so when the window is a
    /// shape it was never for — a portrait 500-series module, a 1U sliver —
    /// the face flows its controls instead of shrinking the panel past
    /// legibility. Same controls, same handles, different rendering.
    form: fts_ui_audio::EditorForm,
    /// The shell's redraw tick — see [`crate::faces::Face`]. Not read; its only
    /// job is to change, so the panel re-renders against fresh param and meter
    /// values instead of being memoized away.
    frame: u64,
) -> Element {
    let _ = frame;
    let profile = comp_profiles::profile_by_id(design.id)
        .unwrap_or_else(|| panic!("rack design names unknown profile {}", design.id));
    let face = use_face_context(profile);
    let gr_db = face.ui.gain_reduction_db.load(Ordering::Relaxed);
    let out_db = face.ui.output_peak_db.load(Ordering::Relaxed);
    let scale = panel_scale(design.w, design.h, crate::control_view::RAIL_W);

    let (win_w, win_h) = fts_ui_audio::hardware::panel::window_logical_size()
        .unwrap_or((design.w + crate::control_view::RAIL_W, design.h));
    if !form.wants_panel(
        design.w,
        design.h,
        win_w - crate::control_view::RAIL_W,
        win_h,
    ) {
        return rsx! {
            CompactRack {
                design,
                profile_id: design.id.to_string(),
                avail_h: win_h,
            }
        };
    }

    rsx! {
        Panel {
            design_w: design.w,
            design_h: design.h,
            scale,
            background: design.paint.to_string(),
            chrome: design.chrome.to_string(),

            for (index , item) in design.items.iter().copied().enumerate() {
                div {
                    // Keyed and uniform: the arms below produce different
                    // shapes (a slot, a pair, a bare div), and a list whose
                    // entries change shape between two designs is what walks
                    // blitz's mutator off the end of a template path. The
                    // wrapper is inert — the items inside are absolutely
                    // positioned on the panel, as before.
                    key: "{design.id}-{index}",
                    // Not `display:contents` — that leaves blitz a node with
                    // no children, which is the very panic this is fixing. A
                    // plain static div is zero-height (everything inside is
                    // absolutely positioned against the panel) and gives the
                    // diff a stable node to land on.
                    match item {
                        RackItem::Vu { x, y, w, mode, legend } => rsx! {
                            PanelSlot { scale, x, y, w: w + 30.0, h: w * 0.72 + if design.vu_bezel { 34.0 } else { 0.0 },
                                VuMeter {
                                    scale,
                                    width: w,
                                    face: design.vu,
                                    mode,
                                    value_db: match mode {
                                        VuMode::GainReduction => gr_db,
                                        VuMode::Level => out_db,
                                    },
                                    legend: legend.to_string(),
                                    bezel: design.vu_bezel,
                                }
                            }
                        },
                        RackItem::Knob { id, legend, x, y, d, ring, tint, style } => {
                            // The knob's box is wider than the knob: the
                            // printed ring lives outside it.
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
                                        handle: face.handle(id),
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
                                Silkscreen {
                                    scale, x, y: y + LEGEND_DROP, width: LEGEND_W,
                                    text: legend.to_string(), color: design.ink.to_string(),
                                }
                            }
                        }
                        RackItem::Buttons { id, legend, x, y, labels } => rsx! {
                            PanelSlot { scale, x, y, w: 90.0, h: 180.0,
                                RatioButtons {
                                    handle: face.handle(id),
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
                            PanelSlot { scale, x, y, w: 120.0, h: 110.0,
                                ToggleSwitch {
                                    handle: face.handle(id),
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
                        // Levers, readouts and lamps are the EQ panels' idiom;
                        // no compressor face places one.
                        // A hybrid meters with LEDs rather than a movement.
                        RackItem::LedMeter { x, y, h, right } => rsx! {
                            PanelSlot { scale, x, y, w: 22.0, h: h + 6.0,
                                LedMeter {
                                    scale,
                                    // Gain reduction reads downward, so the
                                    // ladder fills as the compressor works.
                                    level_db: if right { -gr_db } else { out_db },
                                    h,
                                }
                            }
                        },
                        RackItem::LedBar { x, y, steps, pitch } => rsx! {
                            PanelSlot { scale, x, y, w: steps.len() as f64 * pitch + 8.0, h: 34.0,
                                LedBar {
                                    scale,
                                    value_db: gr_db,
                                    steps: steps.to_vec(),
                                    pitch,
                                    ink: design.ink.to_string(),
                                }
                            }
                        },
                        RackItem::LedSelect { id, x, y, labels, pitch } => rsx! {
                            PanelSlot { scale, x, y, w: labels.len() as f64 * pitch + 8.0, h: 34.0,
                                LedSelect {
                                    handle: face.handle(id),
                                    testid: id.replace('_', "-"),
                                    scale,
                                    labels: labels.iter().map(|s| s.to_string()).collect(),
                                    pitch,
                                    ink: design.ink.to_string(),
                                }
                            }
                        },
                        RackItem::Button { id, label, x, y, color, ink, led } => rsx! {
                            PanelSlot { scale, x, y, w: 62.0, h: 62.0,
                                PanelButton {
                                    handle: (!id.is_empty()).then(|| face.handle(id)),
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
                                    w: 40.0,
                                    h: 22.0,
                                }
                            }
                        },
                        RackItem::Frame { x, y, w, h } => rsx! {
                            div {
                                style: format!(
                                    "position:absolute; left:{:.1}px; top:{:.1}px; \
                                     width:{:.1}px; height:{:.1}px; border-radius:{:.1}px; \
                                     border:{:.1}px solid rgba(255,255,255,0.16); \
                                     pointer-events:none;",
                                    (x - w / 2.0) * scale,
                                    (y - h / 2.0) * scale,
                                    w * scale,
                                    h * scale,
                                    8.0 * scale,
                                    (1.0 * scale).max(1.0),
                                ),
                            }
                        },
                        RackItem::Lamp { x, y, color } => rsx! {
                            PanelSlot { scale, x, y, w: 26.0, h: 26.0,
                                Lamp { scale, color: color.to_string(), d: 13.0 }
                            }
                        },
                        RackItem::Lever { .. }
                        | RackItem::Readout { .. }
                        | RackItem::Divider { .. } => rsx! {},
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
                                width: 360.0,
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
                    }
                }
            }
        }
    }
}

/// The same unit, flowed into a window its panel does not fit.
///
/// Not a second layout table: the *same* [`RackDesign`] items, drawn as a
/// wrapping row of cells. That is what makes a 500-series or 1U view available
/// to every unit the moment it exists, rather than something each one has to
/// be drawn for twice. Silkscreen, rack ears and placement are what a small
/// window has no room for anyway; the controls are what it is for.
#[component]
fn CompactRack(design: RackDesign, profile_id: String, avail_h: f64) -> Element {
    let profile = comp_profiles::profile_by_id(&profile_id)
        .unwrap_or_else(|| panic!("rack design names unknown profile {profile_id}"));
    let face = use_face_context(profile);
    let gr_db = face.ui.gain_reduction_db.load(Ordering::Relaxed);

    // A 1U window is 89 px tall, a module is 984: the same cell cannot serve
    // both. Size the controls from the height, and drop the legends when there
    // is not room for a knob *and* a word under it.
    let rows = if avail_h > 260.0 { 3.0 } else { 1.0 };
    let cell_h = (avail_h - 20.0) / rows;
    let knob_d = (cell_h * 0.62).clamp(26.0, 46.0);
    let show_legends = cell_h > 62.0;
    let meter_h = (avail_h - 28.0).clamp(40.0, 120.0);

    rsx! {
        div {
            "data-testid": "compact-rack",
            style: format!(
                "flex:1; min-height:0; display:flex; flex-wrap:wrap; \
                 align-content:center; justify-content:center; gap:10px 14px; \
                 padding:10px; overflow:hidden; background:{};",
                design.paint,
            ),

            GrMeter { gain_reduction_db: gr_db, height: meter_h as f32 }

            for (index , item) in design.items.iter().copied().enumerate() {
                div {
                    // Keyed and uniform: the arms below produce different
                    // shapes (a slot, a pair, a bare div), and a list whose
                    // entries change shape between two designs is what walks
                    // blitz's mutator off the end of a template path. The
                    // wrapper is inert — the items inside are absolutely
                    // positioned on the panel, as before.
                    key: "{design.id}-{index}",
                    // Not `display:contents` — that leaves blitz a node with
                    // no children, which is the very panic this is fixing. A
                    // plain static div is zero-height (everything inside is
                    // absolutely positioned against the panel) and gives the
                    // diff a stable node to land on.
                    match item {
                        RackItem::Knob { id, legend, ring, style, .. } => rsx! {
                            div {
                                style: "display:flex; flex-direction:column; align-items:center; gap:3px;",
                                HardwareKnob {
                                    handle: face.handle(id),
                                    testid: id.replace('_', "-"),
                                    scale: 1.0,
                                    diameter: knob_d,
                                    style: style.unwrap_or(design.knob),
                                    ink: design.ink.to_string(),
                                    marks: ring.marks(),
                                }
                                if show_legends {
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
                        },
                        RackItem::Buttons { id, legend, labels, .. } => rsx! {
                            div {
                                style: "display:flex; flex-direction:column; align-items:center; gap:3px;",
                                RatioButtons {
                                    handle: face.handle(id),
                                    testid: id.replace('_', "-"),
                                    scale: 0.8,
                                    labels: labels.iter().map(|s| s.to_string()).collect(),
                                    ink: design.ink.to_string(),
                                }
                                if show_legends {
                                    div {
                                        style: format!("font-size:9px; font-weight:700; color:{};", design.ink),
                                        "{legend}"
                                    }
                                }
                            }
                        },
                        RackItem::Switch { id, legend, labels, .. } => rsx! {
                            div {
                                style: "display:flex; flex-direction:column; align-items:center; gap:3px;",
                                ToggleSwitch {
                                    handle: face.handle(id),
                                    testid: id.replace('_', "-"),
                                    scale: 0.8,
                                    labels: [labels[0].to_string(), labels[1].to_string()],
                                    ink: design.ink.to_string(),
                                }
                                if show_legends {
                                    div {
                                        style: format!("font-size:9px; font-weight:700; color:{};", design.ink),
                                        "{legend}"
                                    }
                                }
                            }
                        },
                        // The meter is drawn once above; panel text and the EQ
                        // items have no place in a compact view.
                        _ => rsx! {},
                    }
                }
            }
        }
    }
}
