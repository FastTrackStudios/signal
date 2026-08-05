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
                        RackItem::Vu { x, y, w, mode, legend } => rsx! {
                            PanelSlot { scale, x, y, w: w + 14.0, h: w * 0.72,
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
                                }
                            }
                        },
                        RackItem::Knob { id, legend, x, y, d, ring } => {
                            // The knob's box is wider than the knob: the
                            // printed ring lives outside it.
                            let box_w = d * 2.0;
                            rsx! {
                                PanelSlot { scale, x, y, w: box_w, h: box_w,
                                    HardwareKnob {
                                        handle: face.handle(id),
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
                        RackItem::Lever { .. }
                        | RackItem::Readout { .. }
                        | RackItem::Lamp { .. } => rsx! {},
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
