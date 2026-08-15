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
//! [`fts_audio_ui::hardware::rack`]); this is the compressor's *drawing* of
//! one. Every control is looked up on the unit's `comp-profiles` profile by id
//! and driven through [`crate::profile_handle`], so a design cannot silently
//! reference a control the profile does not have — it panics at mount, in a
//! test, rather than rendering a knob that does nothing.

use std::sync::atomic::Ordering;

use audiocore_core::prelude::*;

use crate::faces::use_face_context;
use crate::hardware::knob::HardwareKnob;
use fts_audio_ui::hardware::button::{Lamp, LedBar, LedMeter, LedSelect, PanelButton};
use crate::hardware::rack::{RackDesign, RackItem};
use crate::hardware::panel::{panel_scale, Panel, PanelSlot, Silkscreen};
use crate::hardware::switches::{RatioButtons, ToggleSwitch};
use crate::hardware::vu::{VuMeter, VuMode};

/// Where a knob's legend sits below its centre, in design px.
///
/// Computed from where the printed numerals actually reach, not picked: the
/// ring styles print at different radii, so a fixed drop that clears one
/// strikes another. (The 1176's dotted scale put "∞" and "0" straight through
/// the word INPUT.) Same function the EQ's faces use.
fn legend_drop(d: f64, label_r: f64) -> f64 {
    let numerals = d * (label_r / 60.0) * 0.866;
    let text = d * (7.0 / 110.0) * 0.5 + 5.5;
    (numerals + text + 4.0).max(30.0)
}
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
    form: fts_audio_ui::EditorForm,
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

    let (win_w, win_h) = fts_audio_ui::hardware::panel::window_logical_size()
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
                avail_w: win_w - crate::control_view::RAIL_W,
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
            ends: design.ends,
            texture: design.texture,

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
                                    bezel_style: design.vu_frame,
                                    card: design.vu_card,
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
                                    scale, x, y: y + legend_drop(d, label_r), width: LEGEND_W,
                                    text: legend.to_string(), color: design.ink.to_string(),
                                }
                            }
                        }
                        RackItem::Buttons { id, legend, x, y, labels, reverse } => rsx! {
                            PanelSlot { scale, x, y, w: 96.0, h: 210.0,
                                RatioButtons {
                                    handle: face.handle(id),
                                    testid: id.replace('_', "-"),
                                    scale,
                                    labels: labels.iter().map(|s| s.to_string()).collect(),
                                    ink: design.ink.to_string(),
                                    reverse,
                                    cap_h: 30.0,
                                    cap_w: 12.0,
                                }
                            }
                            Silkscreen {
                                scale, x, y: y + 72.0, width: LEGEND_W,
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
                                scale, x, y: y + 60.0, width: LEGEND_W,
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
                        RackItem::Button { id, label, x, y, color, ink, led, style } => rsx! {
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
                                    style: style.unwrap_or_default(),
                                    w: 40.0,
                                    h: 22.0,
                                }
                            }
                        },
                        RackItem::Region { x, y, w, h, color } => rsx! {
                            div {
                                style: format!(
                                    "position:absolute; left:{:.1}px; top:{:.1}px; \
                                     width:{:.1}px; height:{:.1}px; background:{color}; \
                                     pointer-events:none;",
                                    (x - w / 2.0) * scale,
                                    (y - h / 2.0) * scale,
                                    w * scale,
                                    h * scale,
                                ),
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
                            PanelSlot { scale, x, y, w: 30.0, h: 30.0,
                                Lamp { scale, color: color.to_string(), d: 17.0 }
                            }
                        },
                        RackItem::Lever { .. }
                        | RackItem::Glyph { .. }
                        | RackItem::Concentric { .. }
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

/// How a compact view flows its cells.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CompactFlow {
    /// One row, no wrapping — a 1U sliver. A wrapped row in a window that
    /// short is a row cut in half.
    Row,
    /// One column — a 500-series slot, which is what the hardware is.
    Column,
    /// Wrapped rows, for boxes that are neither.
    Grid,
}

/// The largest control size that actually fits, and how to flow them.
///
/// Solved rather than assumed. The knob size decides how many cells fit per
/// row, the row count decides how much height each cell gets, and that decides
/// the knob size — so guessing one and hoping produced a Mini view with three
/// rows of controls in a two-row box, clipped, because blitz does not shrink
/// what does not fit, it cuts it.
///
/// So: try every size from generous down to tiny and take the first that fits
/// both axes. It is a dozen iterations of arithmetic, once per render, and it
/// cannot be wrong about whether the result fits.
fn fit_cells(cells: usize, avail_w: f64, avail_h: f64) -> CompactFit {
    /// A knob's box is nearly twice its diameter — `HardwareKnob` sizes its
    /// viewBox for the printed ring outside the body.
    const KNOB_BOX_RATIO: f64 = 110.0 / 60.0;
    const LEGEND_H: f64 = 13.0;
    const GAP: f64 = 12.0;
    const PAD: f64 = 16.0;

    let (inner_w, inner_h) = ((avail_w - PAD * 2.0).max(1.0), (avail_h - PAD).max(1.0));
    let flow = if avail_h < 200.0 {
        CompactFlow::Row
    } else if avail_w < avail_h {
        CompactFlow::Column
    } else {
        CompactFlow::Grid
    };

    let mut best = CompactFit { knob_d: 16.0, show_legends: false, flow };
    for step in 0..=30 {
        let knob_d = 46.0 - step as f64;
        let box_px = knob_d * KNOB_BOX_RATIO;
        for show_legends in [true, false] {
            let cell_w = box_px + GAP;
            let cell_h = box_px + if show_legends { LEGEND_H } else { 0.0 } + GAP;
            let cols = match flow {
                CompactFlow::Row => cells,
                CompactFlow::Column => 1,
                CompactFlow::Grid => ((inner_w / cell_w).floor() as usize).max(1),
            };
            let rows = cells.div_ceil(cols);
            if cell_w * cols as f64 <= inner_w && cell_h * rows as f64 <= inner_h {
                return CompactFit { knob_d, show_legends, flow };
            }
            best = CompactFit { knob_d, show_legends, flow };
        }
    }
    best
}

struct CompactFit {
    knob_d: f64,
    show_legends: bool,
    flow: CompactFlow,
}

/// How many of a design's items are controls a compact view will draw.
///
/// Silkscreen, regions and lamps are panel *drawing*: they have no cell in a
/// flowed layout, so counting them would divide the height into rows that
/// nothing occupies.
fn design_control_count(design: &RackDesign) -> usize {
    design
        .items
        .iter()
        .filter(|item| {
            matches!(
                item,
                RackItem::Knob { .. } | RackItem::Buttons { .. } | RackItem::Switch { .. }
            )
        })
        .count()
}

/// Gain reduction, in the panel's own ink.
///
/// The shared [`GrMeter`](fts_audio_ui::prelude::GrMeter) is a dark-theme
/// widget — surface, border and text all come from the app palette — and on a
/// grey leveling-amplifier plate it reads as a black sticker someone left on
/// the panel. This one is drawn from the design: the plate's ink for the fill,
/// its dim ink for the well, and a needle-sized reading beside it.
#[component]
fn CompactGrMeter(
    gain_reduction_db: f32,
    ink: String,
    dim_ink: String,
    /// Short windows lay the meter along the row rather than across it.
    horizontal: bool,
    /// Length of the meter's travel, in px.
    extent: f64,
) -> Element {
    const MAX_GR_DB: f32 = 24.0;
    let filled = (gain_reduction_db.clamp(0.0, MAX_GR_DB) / MAX_GR_DB) * 100.0;
    let thickness = 9.0;
    let (well, fill) = if horizontal {
        (
            format!("width:{extent:.0}px; height:{thickness:.0}px;"),
            format!("top:0; bottom:0; right:0; width:{filled:.1}%;"),
        )
    } else {
        (
            format!("width:{thickness:.0}px; height:{extent:.0}px;"),
            format!("left:0; right:0; top:0; height:{filled:.1}%;"),
        )
    };

    rsx! {
        div {
            "data-testid": "compact-gr",
            // Column either way: the "GR" caption sits above the meter
            // whichever way the meter itself runs. (`horizontal` does
            // still pick the well/fill geometry above.)
            style: "display:flex; flex-direction:column; align-items:center; gap:5px;",
            div {
                style: format!(
                    "font-size:8px; font-weight:700; letter-spacing:0.10em; \
                     text-transform:uppercase; color:{dim_ink};"
                ),
                "GR"
            }
            div {
                style: format!(
                    "{well} position:relative; border-radius:2px; \
                     background:rgba(0,0,0,0.30); \
                     box-shadow:inset 0 1px 2px rgba(0,0,0,0.45); overflow:hidden;"
                ),
                div {
                    style: format!("position:absolute; {fill} background:{ink}; opacity:0.85;"),
                }
            }
            div {
                style: format!("font-size:8px; font-family:monospace; color:{dim_ink};"),
                "{-gain_reduction_db:.1}"
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
fn CompactRack(design: RackDesign, profile_id: String, avail_w: f64, avail_h: f64) -> Element {
    let profile = comp_profiles::profile_by_id(&profile_id)
        .unwrap_or_else(|| panic!("rack design names unknown profile {profile_id}"));
    let face = use_face_context(profile);
    let gr_db = face.ui.gain_reduction_db.load(Ordering::Relaxed);

    // A 1U window is 89px tall, a module is 984, and Mini is 260x200: no
    // fixed cell size or row count serves all three. So the layout is solved
    // rather than guessed — see [`fit_cells`].
    let cells = design_control_count(&design) + 1; // +1 for the meter
    let fit = fit_cells(cells, avail_w, avail_h);
    let (knob_d, show_legends) = (fit.knob_d, fit.show_legends);
    // Numerals are legible down to about here; below it they are grey noise
    // around a knob, and the knob is the useful part.
    let marks_fit = knob_d >= 30.0;

    rsx! {
        div {
            "data-testid": "compact-rack",
            style: format!(
                "flex:1; min-height:0; display:flex; {} \
                 align-items:center; align-content:center; justify-content:{}; \
                 gap:{}; padding:8px 12px; overflow:hidden; background:{};",
                match fit.flow {
                    CompactFlow::Row => "flex-wrap:nowrap;",
                    CompactFlow::Column => "flex-direction:column; flex-wrap:nowrap;",
                    CompactFlow::Grid => "flex-wrap:wrap;",
                },
                if fit.flow == CompactFlow::Column { "space-evenly" } else { "center" },
                if fit.flow == CompactFlow::Row { "0 10px" } else { "12px 16px" },
                design.paint,
            ),

            CompactGrMeter {
                gain_reduction_db: gr_db,
                ink: design.ink.to_string(),
                dim_ink: design.dim_ink.to_string(),
                horizontal: fit.flow == CompactFlow::Row,
                extent: match fit.flow {
                    CompactFlow::Row => 96.0,
                    _ => (avail_h * 0.18).clamp(48.0, 140.0),
                },
            }

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
                                    marks: if marks_fit { ring.marks() } else { Vec::new() },
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
