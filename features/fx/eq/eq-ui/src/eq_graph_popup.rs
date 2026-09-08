//! Popup and context-menu controls for the EQ graph.

use architect_ui::prelude::{Button, ButtonSize, ButtonVariant};
use nice_plug_dioxus::prelude::*;

use super::eq_graph_model::{EqBand, EqBandShape, MAX_BANDS, StereoMode, slope_db};
use crate::dynamics::PanelKnob;

/// A shape's canonical id — the value the `type` parameter persists.
///
/// Deliberately NOT the shape's position in [`EqBandShape::all`]; the two
/// orderings differ (see `shape_id_tests`), and conflating them is what made
/// the band panel unable to display its own shape.
#[allow(dead_code, reason = "the forward half of the shape-id bijection")]
const fn shape_to_int(s: EqBandShape) -> i32 {
    match s {
        EqBandShape::Bell => 0,
        EqBandShape::LowShelf => 1,
        EqBandShape::LowCut => 2,
        EqBandShape::HighShelf => 3,
        EqBandShape::HighCut => 4,
        EqBandShape::Notch => 5,
        EqBandShape::BandPass => 6,
        EqBandShape::TiltShelf => 7,
        EqBandShape::FlatTilt => 8,
        EqBandShape::AllPass => 9,
    }
}

#[component]
pub fn EmptyGraphContextMenu(
    x: f64,
    y: f64,
    graph_w: f64,
    graph_h: f64,
    next_index: usize,
    frequency: f32,
    gain: f32,
    shape: EqBandShape,
    on_band_add: Option<EventHandler<EqBand>>,
    on_dismiss: EventHandler<()>,
) -> Element {
    let menu_w: f64 = 150.0;
    let menu_h: f64 = 76.0;
    let menu_x = x.min((graph_w - menu_w).max(0.0));
    let menu_y = y.min((graph_h - menu_h).max(0.0));
    let can_add = next_index < MAX_BANDS;
    let freq_str = if frequency >= 1000.0 {
        format!("{:.1}k", frequency / 1000.0)
    } else {
        format!("{frequency:.0}")
    };

    rsx! {
        div {
            style: "position:absolute; inset:0; z-index:20;",
            onmousedown: {
                let dismiss = on_dismiss;
                move |evt: MouseEvent| {
                    evt.stop_propagation();
                    dismiss.call(());
                }
            },
        }

        div {
            style: format!(
                "position:absolute; left:{menu_x}px; top:{menu_y}px; width:{menu_w}px; \
                 background:rgba(15,15,18,0.97); border:1px solid rgba(80,80,85,0.6); \
                 border-radius:4px; z-index:21; font-size:9px; color:#bbb; \
                 box-sizing:border-box;",
            ),
            onmousedown: move |evt| { evt.stop_propagation(); },

            div {
                style: "padding:6px 10px 2px; color:#eee; font-weight:700;",
                "Graph"
            }
            div {
                style: "padding:0 10px 4px; color:#777;",
                "{freq_str} Hz / {gain:+.1} dB"
            }
            div {
                style: format!(
                    "padding:5px 10px; cursor:{}; color:{};",
                    if can_add { "pointer" } else { "not-allowed" },
                    if can_add { "#ddd" } else { "#666" },
                ),
                onclick: {
                    let dismiss = on_dismiss;
                    move |evt: MouseEvent| {
                        evt.stop_propagation();
                        if can_add {
                            if let Some(cb) = &on_band_add {
                                cb.call(EqBand {
                                    index: next_index,
                                    used: true,
                                    enabled: true,
                                    frequency,
                                    gain,
                                    q: 1.0,
                                    slope: None,
                                    shape,
                                    solo: false,
                                    stereo_mode: StereoMode::default(),
                                    name: String::new(),
                                });
                            }
                        }
                        dismiss.call(());
                    }
                },
                if can_add {
                    "Add {shape.label()}"
                } else {
                    "No free band slots"
                }
            }
        }
    }
}

/// The inverse of [`shape_to_int`].
///
/// Kept although the panel no longer needs it: it is the other half of the
/// canonical shape-id mapping, and `shape_id_tests` uses it to prove the two
/// `match` arms are a bijection — which is the only thing standing between a
/// future edit and every high shelf silently becoming a high-pass.
#[allow(dead_code, reason = "the round-trip half of the shape-id bijection")]
const fn int_to_shape(v: i32) -> EqBandShape {
    match v {
        1 => EqBandShape::LowShelf,
        2 => EqBandShape::LowCut,
        3 => EqBandShape::HighShelf,
        4 => EqBandShape::HighCut,
        5 => EqBandShape::Notch,
        6 => EqBandShape::BandPass,
        7 => EqBandShape::TiltShelf,
        8 => EqBandShape::FlatTilt,
        9 => EqBandShape::AllPass,
        _ => EqBandShape::Bell,
    }
}

const fn shape_icon(s: EqBandShape) -> &'static str {
    match s {
        EqBandShape::Bell => "Bell",
        EqBandShape::LowShelf => "LS",
        EqBandShape::HighShelf => "HS",
        EqBandShape::LowCut => "LC",
        EqBandShape::HighCut => "HC",
        EqBandShape::Notch => "N",
        EqBandShape::BandPass => "BP",
        EqBandShape::TiltShelf => "TS",
        EqBandShape::FlatTilt => "FT",
        EqBandShape::AllPass => "AP",
    }
}

/// Vertical gap between the band node and the detail panel, in graph pixels.
pub const POPUP_GAP: f64 = 18.0;

/// Slack around the popup/node region that still counts as "on the band".
const POPUP_REGION_PAD: f64 = 10.0;

/// Geometry of the band detail panel in graph-element pixels: `(x, y, w, h)`.
///
/// Shared with `EqGraph`'s focus logic — the graph needs the same rect to know
/// that a pointer heading for the panel has not left the band.
#[must_use]
pub fn band_popup_rect(
    bx: f64,
    by: f64,
    graph_w: f64,
    graph_h: f64,
    is_dragging: bool,
) -> (f64, f64, f64, f64) {
    // A Pro-Q-style band strip: three dials across the middle, flanked by the
    // shape/routing clusters, with the dynamics row beneath.
    let w = if is_dragging { 178.0 } else { 300.0 };
    let h = if is_dragging { 34.0 } else { 150.0 };

    // DOCKED to the bottom of the graph, and only tracking the band
    // horizontally.
    //
    // It used to sit above the node and flip below when the node was near the
    // top, which meant the panel moved — often jumping the full height of
    // itself — while you were dragging the very band it describes. Pinning the
    // vertical edge keeps it still: the readouts stay where your eye already
    // is, and the panel cannot land under the cursor mid-drag or shove itself
    // off the top of the display.
    // `by` is deliberately unused now: the panel's vertical position no longer
    // depends on the band's. It stays in the signature because
    // `point_in_popup_region` needs it to union the node into the keep-alive
    // region, and callers pass both together.
    let _ = by;
    let x = (bx - w / 2.0).clamp(0.0, (graph_w - w).max(0.0));
    let y = (graph_h - h - POPUP_GAP).max(0.0);
    (x, y, w, h)
}

/// Is `(px, py)` inside the band's "keep the panel up" region?
///
/// That region is the union of the panel's box and the band node itself, so
/// the empty [`POPUP_GAP`] the pointer must cross to reach the panel is part
/// of it. Without this the panel fades out from under the cursor on the way
/// there and its controls can never be clicked.
#[must_use]
pub fn point_in_popup_region(
    px: f64,
    py: f64,
    bx: f64,
    by: f64,
    graph_w: f64,
    graph_h: f64,
    is_dragging: bool,
) -> bool {
    let (x, y, w, h) = band_popup_rect(bx, by, graph_w, graph_h, is_dragging);
    let x0 = x.min(bx) - POPUP_REGION_PAD;
    let x1 = (x + w).max(bx) + POPUP_REGION_PAD;
    let y0 = y.min(by) - POPUP_REGION_PAD;
    let y1 = (y + h).max(by) + POPUP_REGION_PAD;
    px >= x0 && px <= x1 && py >= y0 && py <= y1
}

const fn next_stereo_mode(mode: StereoMode) -> StereoMode {
    match mode {
        StereoMode::Stereo => StereoMode::Left,
        StereoMode::Left => StereoMode::Right,
        StereoMode::Right => StereoMode::Mid,
        StereoMode::Mid => StereoMode::Side,
        StereoMode::Side => StereoMode::Stereo,
    }
}

#[component]
pub fn BandPopup(
    band_idx: usize,
    bx: f64,
    by: f64,
    graph_w: f64,
    graph_h: f64,
    is_dragging: bool,
    bands: Signal<Vec<EqBand>>,
    /// Timestamp (ms) of the last pointer event the panel handled itself.
    /// `EqGraph` reads this to suppress its fade timer while the user is
    /// actually on the panel — the panel stops those events from bubbling, so
    /// the graph would otherwise see no activity at all (or worse, coordinates
    /// relative to the panel rather than the graph).
    mut popup_activity: Signal<f64>,
    on_band_change: Option<EventHandler<(usize, EqBand)>>,
    on_band_remove: Option<EventHandler<usize>>,
    /// This band's dynamics, when the editor supplied them. `None` degrades to
    /// the popup as it was.
    #[props(default)]
    dyn_state: Option<crate::dynamics::DynState>,
    /// This band's freq/gain/Q handles, when the editor supplied them.
    #[props(default)]
    handles: Option<crate::dynamics::BandHandles>,
    on_dismiss: EventHandler<()>,
) -> Element {
    let Some(band) = bands.read().get(band_idx).cloned() else {
        return rsx! {};
    };

    let freq_str = if band.frequency >= 1000.0 {
        format!("{:.1}k", band.frequency / 1000.0)
    } else {
        format!("{:.0}", band.frequency)
    };
    let q_str = if band.shape.uses_slope() && band.slope.is_some() {
        if band.slope.unwrap_or(2.0) >= 10.0 {
            "Brickwall".to_string()
        } else {
            format!("{:.0} dB/oct", slope_db(band.slope.unwrap_or(2.0)))
        }
    } else {
        format!("Q {:.2}", band.q)
    };
    let band_color = crate::eq_graph_model::freq_to_color(f64::from(band.frequency));
    let band_enabled = band.enabled;
    let band_solo = band.solo;
    let band_gain = band.gain;

    let stereo_mode = band.stereo_mode;

    let (popup_x, popup_y, popup_w, popup_h) =
        band_popup_rect(bx, by, graph_w, graph_h, is_dragging);


    rsx! {
        div {
            // While dragging, the popup sits right under the cursor next to the
            // band; if it intercepts pointer events it becomes the event target,
            // making `element_coordinates()` element-relative to the popup (≈0,0)
            // for a frame → the band snaps to the graph's top-left and back. So
            // make the popup click-through while dragging.
            style: format!(
                // Height is pinned to what `band_popup_rect` reports rather
                // than left to the content, so the region the graph treats as
                // "still on the band" and the box the pointer actually hits are
                // the same rectangle.
                "position:absolute; left:{popup_x}px; top:{popup_y}px; \
                 width:{popup_w}px; height:{popup_h}px; \
                 z-index:10; pointer-events:{pe};",
                pe = if is_dragging { "none" } else { "auto" },
            ),
            "data-testid": "eq-band-popup",
            "data-band": "{band_idx}",

            // Pointer events that land on the panel must NOT bubble to the
            // graph surface: `element_coordinates()` is relative to the event
            // *target*, so a move over the panel reaches the graph's handler as
            // a point near the panel's own origin — far from the band — and the
            // graph fades the panel out from under the cursor before anything
            // in it can be clicked. Stopping them here also records the moment
            // of contact, which `EqGraph` uses to hold focus open.
            onmousemove: move |evt: MouseEvent| {
                evt.stop_propagation();
                popup_activity.set(crate::eq_graph::now_ms());
            },
            onmouseup: move |evt: MouseEvent| {
                evt.stop_propagation();
                popup_activity.set(crate::eq_graph::now_ms());
            },
            onmousedown: move |evt: MouseEvent| {
                evt.stop_propagation();
                popup_activity.set(crate::eq_graph::now_ms());
            },
            onwheel: move |evt: WheelEvent| {
                evt.stop_propagation();
                popup_activity.set(crate::eq_graph::now_ms());
            },

            // ── Band panel ──────────────────────────────────────────────
            //
            // Laid out the way Pro-Q 4 lays out its band strip: the routing
            // and shape decisions on the left, the three continuous controls
            // as real dials across the middle with their VALUE above and their
            // NAME below, and the per-band actions on the right. The band's
            // own colour carries the top edge and the dial arcs, so which band
            // you are editing is legible without reading the frequency.
            div {
                class: "text-foreground",
                style: format!(
                    "border:1px solid #2a2a30; border-top:2px solid {band_color}; \
                     border-radius:10px; background:#14141a; \
                     box-shadow:0 6px 18px rgba(0,0,0,0.55), 0 0 0 1px rgba(0,0,0,0.3); \
                     padding:6px 8px; display:flex; flex-direction:column; gap:5px;"
                ),
                onmousedown: move |evt| { evt.stop_propagation(); },

                if is_dragging {
                    // Mid-drag the panel shrinks to the numbers — anything
                    // clickable would be unreachable anyway while the pointer
                    // owns the node.
                    div {
                        style: "display:flex; align-items:baseline; justify-content:space-between; gap:8px;",
                        span { class: "text-xs font-semibold tabular-nums", "{freq_str} Hz" }
                        span { class: "text-xs font-semibold tabular-nums", "{band_gain:+.1} dB" }
                        span { class: "text-[10px] tabular-nums text-muted-foreground", "{q_str}" }
                    }
                } else {
                    div {
                        style: "display:flex; align-items:flex-start; gap:8px;",

                        // ── Left: bypass, shape, slope ──
                        div {
                            style: "display:flex; flex-direction:column; gap:4px; width:82px; flex:0 0 auto;",
                            Button {
                                size: ButtonSize::Small,
                                variant: if band_enabled { ButtonVariant::Outline } else { ButtonVariant::Secondary },
                                class: "px-0 h-6".to_string(),
                                on_click: {
                                    let cb = on_band_change;
                                    move |_| {
                                        let updated = {
                                            let mut bv = bands.write();
                                            if band_idx < bv.len() {
                                                bv[band_idx].enabled = !bv[band_idx].enabled;
                                                Some(bv[band_idx].clone())
                                            } else { None }
                                        };
                                        if let (Some(b), Some(c)) = (updated, &cb) { c.call((band_idx, b)); }
                                    }
                                },
                                if band_enabled { "On" } else { "Byp" }
                            }
                            // Pro-Q shows the shape as a labelled button
                            // ("∧ Bell"), not a dropdown — and a button can
                            // actually display which shape the band is, which
                            // the Select could not: it resolves its label by
                            // index, and the canonical shape ids are not the
                            // positions in `EqBandShape::all()`, so it fell
                            // back to the placeholder for every band. Clicking
                            // steps to the next shape; the full list is one
                            // right-click away.
                            button {
                                style: format!(
                                    "display:flex; align-items:center; gap:5px; width:100%; height:22px; \
                                     padding:0 6px; border:1px solid #2a2a30; border-radius:4px; \
                                     background:transparent; color:{band_color}; font-size:10px; \
                                     cursor:pointer; box-sizing:border-box;"
                                ),
                                title: "{band.shape.label()} — click for the next shape, right-click the band for the full list",
                                onclick: {
                                    let cb = on_band_change;
                                    move |evt: MouseEvent| {
                                        evt.stop_propagation();
                                        let updated = {
                                            let mut bv = bands.write();
                                            if band_idx < bv.len() {
                                                let all = EqBandShape::all();
                                                let cur = all.iter().position(|s| *s == bv[band_idx].shape).unwrap_or(0);
                                                let next = all[(cur + 1) % all.len()];
                                                bv[band_idx].shape = next;
                                                if next.uses_slope() { bv[band_idx].q = 0.707; }
                                                Some(bv[band_idx].clone())
                                            } else { None }
                                        };
                                        if let (Some(b), Some(c)) = (updated, &cb) { c.call((band_idx, b)); }
                                    }
                                },
                                span { style: "flex:1; text-align:left;", "{band.shape.label()}" }
                            }
                            div {
                                style: "font-size:9px; color:#737380; text-align:center; letter-spacing:0.04em;",
                                "{q_str}"
                            }
                        }

                        // ── Centre: the three dials ──
                        if let Some(h) = handles.clone() {
                            div {
                                style: "display:flex; align-items:flex-start; gap:6px; flex:1 1 auto; justify-content:center;",
                                PanelKnob {
                                    label: "FREQ".to_string(),
                                    handle: h.freq.clone(),
                                    accent: band_color.clone(),
                                }
                                PanelKnob {
                                    label: "GAIN".to_string(),
                                    handle: h.gain.clone(),
                                    accent: band_color.clone(),
                                    dynamics: dyn_state.clone(),
                                }
                                PanelKnob {
                                    label: "Q".to_string(),
                                    handle: h.q.clone(),
                                    accent: band_color.clone(),
                                }
                            }
                        } else {
                            // No handles supplied (embedded surfaces): keep the
                            // readout rather than dropping the information.
                            div {
                                style: "display:flex; flex-direction:column; gap:2px; flex:1 1 auto; align-items:center; justify-content:center;",
                                span { class: "text-xs font-semibold tabular-nums", "{freq_str} Hz" }
                                span { class: "text-xs font-semibold tabular-nums", "{band_gain:+.1} dB" }
                            }
                        }

                        // ── Right: per-band actions ──
                        div {
                            style: "display:flex; flex-direction:column; gap:3px; width:30px; flex:0 0 auto;",
                            Button {
                                size: ButtonSize::Small,
                                variant: ButtonVariant::Outline,
                                class: "px-0 h-5".to_string(),
                                on_click: {
                                    let cb = on_band_change;
                                    move |_| {
                                        let updated = {
                                            let mut bv = bands.write();
                                            if band_idx < bv.len() {
                                                bv[band_idx].stereo_mode = next_stereo_mode(bv[band_idx].stereo_mode);
                                                Some(bv[band_idx].clone())
                                            } else { None }
                                        };
                                        if let (Some(b), Some(c)) = (updated, &cb) { c.call((band_idx, b)); }
                                    }
                                },
                                "{stereo_mode.short_label()}"
                            }
                            Button {
                                size: ButtonSize::Small,
                                variant: if band_solo { ButtonVariant::Secondary } else { ButtonVariant::Outline },
                                class: "px-0 h-5".to_string(),
                                on_click: {
                                    let cb = on_band_change;
                                    move |_| {
                                        let updated = {
                                            let mut bv = bands.write();
                                            if band_idx < bv.len() {
                                                bv[band_idx].solo = !bv[band_idx].solo;
                                                Some(bv[band_idx].clone())
                                            } else { None }
                                        };
                                        if let (Some(b), Some(c)) = (updated, &cb) { c.call((band_idx, b)); }
                                    }
                                },
                                "S"
                            }
                            Button {
                                size: ButtonSize::Small,
                                variant: ButtonVariant::Destructive,
                                class: "px-0 h-5".to_string(),
                                on_click: {
                                    let cb = on_band_remove;
                                    move |_| {
                                        if let Some(c) = &cb { c.call(band_idx); }
                                        on_dismiss.call(());
                                    }
                                },
                                "X"
                            }
                        }
                    }

                    // ── Dynamics ────────────────────────────────────────
                    //
                    // Pro-Q hangs dynamics off the GAIN dial, which is what the
                    // ring above does; this row is its expanded form — the mode
                    // it is in, the range it may travel, and what it is doing
                    // right now.
                    if let Some(ds) = dyn_state.clone() {
                        {
                            let mode = ds.mode();
                            let range_db = ds.range_db();
                            let colour = mode.live_colour();
                            let live = ds.live_db;
                            let step = 1.0_f32 / 60.0;
                            let ds_dn = ds.clone();
                            let ds_up = ds.clone();
                            let ds_cycle = ds.clone();
                            rsx! {
                                div {
                                    style: "display:flex; align-items:center; gap:4px; border-top:1px solid #23232a; padding-top:4px;",
                                    button {
                                        style: format!(
                                            "font-size:9px; text-transform:uppercase; letter-spacing:0.06em; \
                                             border:1px solid {colour}; border-radius:3px; padding:0 5px; height:15px; \
                                             color:{}; background:{}; cursor:pointer;",
                                            if mode == crate::dynamics::DynMode::Static { colour } else { "#14141a" },
                                            if mode == crate::dynamics::DynMode::Static { "transparent" } else { colour },
                                        ),
                                        title: "Dynamics mode — click to cycle",
                                        onclick: move |e: MouseEvent| {
                                            e.stop_propagation();
                                            ds_cycle.set_mode(ds_cycle.mode().next());
                                        },
                                        "{mode.label()}"
                                    }

                                    if mode != crate::dynamics::DynMode::Static {
                                        button {
                                            style: "font-size:10px; border:1px solid #2a2a30; border-radius:3px; width:16px; height:15px; color:#d4d4d8; background:transparent; cursor:pointer;",
                                            title: "Range −1 dB",
                                            onclick: move |e: MouseEvent| {
                                                e.stop_propagation();
                                                let n = (ds_dn.range.normalized() - step).clamp(0.0, 1.0);
                                                ds_dn.range.set_as_gesture(n);
                                            },
                                            "−"
                                        }
                                        span {
                                            style: format!("font-size:9px; color:{colour}; min-width:36px; text-align:center;"),
                                            "{range_db:+.0} dB"
                                        }
                                        button {
                                            style: "font-size:10px; border:1px solid #2a2a30; border-radius:3px; width:16px; height:15px; color:#d4d4d8; background:transparent; cursor:pointer;",
                                            title: "Range +1 dB",
                                            onclick: move |e: MouseEvent| {
                                                e.stop_propagation();
                                                let n = (ds_up.range.normalized() + step).clamp(0.0, 1.0);
                                                ds_up.range.set_as_gesture(n);
                                            },
                                            "+"
                                        }
                                        span {
                                            style: format!("font-size:9px; color:{colour}; margin-left:auto; font-variant-numeric:tabular-nums;"),
                                            "{live:+.1} dB"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Placement options, in Pro-Q's order, with the dot colour each one reads by.
///
/// The colour is the whole point of the dot: five rows of near-identical text
/// are hard to scan, and "which of these is Mid" should be answerable without
/// reading. Stereo is the amber default; Mid/Side take green/blue because they
/// are the pair you switch between most; Left/Right stay neutral.
const PLACEMENTS: [(StereoMode, &str, &str); 5] = [
    (StereoMode::Left, "Left", "#d4d4d8"),
    (StereoMode::Right, "Right", "#d4d4d8"),
    (StereoMode::Stereo, "Stereo", "#f0a030"),
    (StereoMode::Mid, "Mid", "#4ade80"),
    (StereoMode::Side, "Side", "#60a5fa"),
];

/// Slope steps, as the index the `slope` parameter stores and the dB/oct the
/// engineer thinks in. Mirrors `control_view::slope_options` — one list, two
/// surfaces, so the menu cannot offer a slope the inspector refuses.
const SLOPES: [(f32, &str); 11] = [
    (0.0, "0 dB/oct"),
    (1.0, "6 dB/oct"),
    (2.0, "12 dB/oct"),
    (3.0, "18 dB/oct"),
    (4.0, "24 dB/oct"),
    (5.0, "30 dB/oct"),
    (6.0, "36 dB/oct"),
    (7.0, "48 dB/oct"),
    (8.0, "72 dB/oct"),
    (9.0, "96 dB/oct"),
    (10.0, "Brickwall"),
];

const MENU_BG: &str = "rgba(18,18,22,0.98)";
const MENU_BORDER: &str = "rgba(90,90,98,0.55)";
const MENU_TEXT: &str = "#d4d4d8";
const MENU_DIM: &str = "#6b6b76";
const MENU_HOVER: &str = "rgba(120,150,255,0.16)";

fn menu_shell(x: f64, y: f64, w: f64) -> String {
    format!(
        "position:absolute; left:{x}px; top:{y}px; width:{w}px; \
         background:{MENU_BG}; border:1px solid {MENU_BORDER}; border-radius:6px; \
         box-shadow:0 8px 24px rgba(0,0,0,0.6); padding:4px 0; \
         font-size:10px; color:{MENU_TEXT}; box-sizing:border-box; z-index:21;"
    )
}

fn menu_row(hovered: bool, danger: bool) -> String {
    let colour = if danger { "rgba(255,110,110,0.95)" } else { MENU_TEXT };
    let bg = if hovered { MENU_HOVER } else { "transparent" };
    format!(
        "display:flex; align-items:center; gap:6px; padding:4px 10px; height:20px; \
         cursor:pointer; color:{colour}; background:{bg}; box-sizing:border-box;"
    )
}

fn menu_divider() -> String {
    format!("height:1px; background:{MENU_BORDER}; margin:4px 8px;")
}

/// The band's right-click menu.
///
/// Structured the way Pro-Q 4 structures its own: the toggles that act
/// immediately at the top, the three "what kind of filter is this" choices as
/// submenus in the middle, and the destructive actions last. Everything a band
/// can be is reachable from here — before this, placement, slope and the
/// dynamics modes had no menu at all, so mid/side EQ was a parameter the
/// engine supported and the editor could not ask for.
#[component]
pub fn BandContextMenu(
    band_idx: usize,
    x: f64,
    y: f64,
    graph_w: f64,
    graph_h: f64,
    bands: Signal<Vec<EqBand>>,
    on_band_change: Option<EventHandler<(usize, EqBand)>>,
    on_band_remove: Option<EventHandler<usize>>,
    /// The band's dynamics, so "Make Dynamic" / "Make Spectral" are real
    /// actions rather than menu entries that do nothing.
    #[props(default)]
    dyn_state: Option<crate::dynamics::DynState>,
    on_dismiss: EventHandler<()>,
) -> Element {
    let Some(band) = bands.read().get(band_idx).cloned() else {
        return rsx! {};
    };

    // Which row the pointer is on, and which submenu is open. One signal each:
    // a submenu stays open while the pointer is on its parent OR inside the
    // flyout, which is what makes a diagonal trip into it survivable.
    let mut hover: Signal<Option<usize>> = use_signal(|| None);
    let mut open_sub: Signal<Option<usize>> = use_signal(|| None);

    let shapes = EqBandShape::all();
    let menu_w: f64 = 172.0;
    let menu_h: f64 = 246.0;
    let menu_x = x.min((graph_w - menu_w).max(0.0));
    let menu_y = y.min((graph_h - menu_h).max(0.0));

    let band_color = crate::eq_graph_model::freq_to_color(f64::from(band.frequency));
    let cur_shape = band.shape;
    let cur_slope = band.slope.unwrap_or(2.0);
    let cur_place = band.stereo_mode;
    let mode = dyn_state.as_ref().map_or(crate::dynamics::DynMode::Static, |d| d.mode());

    // Flyouts open to the right unless that would leave the graph, in which
    // case they hinge to the left of the menu instead.
    let sub_w: f64 = 132.0;
    let sub_left = if menu_x + menu_w + sub_w <= graph_w {
        menu_x + menu_w - 2.0
    } else {
        (menu_x - sub_w + 2.0).max(0.0)
    };
    // A flyout opens level with the row that spawned it, but the slope list is
    // eleven items tall — level with its row it runs off the bottom of a short
    // graph. Push it back up by whatever it overhangs.
    let sub_top = |offset: f64, rows: usize| -> f64 {
        let h = (rows as f64).mul_add(20.0, 8.0);
        (menu_y + offset).min((graph_h - h).max(0.0))
    };

    // Commit a mutation to the band and notify the editor.
    //
    // Takes its own copy of the signal so the closure stays `Fn` rather than
    // `FnMut` — every menu row needs to call this from its own `move`
    // handler, and an `FnMut` could only be owned by one of them.
    let apply = move |f: &dyn Fn(&mut EqBand)| {
        let mut bands = bands;
        let updated = {
            let mut bv = bands.write();
            if band_idx < bv.len() {
                f(&mut bv[band_idx]);
                Some(bv[band_idx].clone())
            } else {
                None
            }
        };
        if let (Some(b), Some(c)) = (updated, &on_band_change) {
            c.call((band_idx, b));
        }
    };

    rsx! {
        div {
            // Click-catcher: anywhere outside closes the menu.
            style: "position:absolute; inset:0; z-index:20;",
            onmousedown: move |evt: MouseEvent| {
                evt.stop_propagation();
                on_dismiss.call(());
            },
            oncontextmenu: move |evt: MouseEvent| { evt.prevent_default(); },

            div {
                style: menu_shell(menu_x, menu_y, menu_w),
                onmousedown: move |evt: MouseEvent| { evt.stop_propagation(); },

                // ── Header ──
                div {
                    style: format!(
                        "padding:4px 10px 5px; color:{band_color}; font-weight:700; \
                         font-size:9px; letter-spacing:0.06em; text-transform:uppercase;"
                    ),
                    "Band {band_idx + 1} · {freq_short(band.frequency)}"
                }
                div { style: menu_divider() }

                // ── Immediate toggles ──
                div {
                    style: menu_row(hover() == Some(0), false),
                    onmouseenter: move |_| { hover.set(Some(0)); open_sub.set(None); },
                    onclick: move |_| { apply(&|b: &mut EqBand| b.enabled = !b.enabled); on_dismiss.call(()); },
                    span { style: "width:12px;", if band.enabled { "" } else { "✓" } }
                    span { if band.enabled { "Bypass" } else { "Enable" } }
                }
                div {
                    style: menu_row(hover() == Some(1), false),
                    onmouseenter: move |_| { hover.set(Some(1)); open_sub.set(None); },
                    onclick: move |_| { apply(&|b: &mut EqBand| b.solo = !b.solo); on_dismiss.call(()); },
                    span { style: "width:12px;", if band.solo { "✓" } else { "" } }
                    span { "Solo" }
                }
                div {
                    style: menu_row(hover() == Some(2), false),
                    onmouseenter: move |_| { hover.set(Some(2)); open_sub.set(None); },
                    onclick: move |_| { apply(&|b: &mut EqBand| b.gain = -b.gain); on_dismiss.call(()); },
                    span { style: "width:12px;" }
                    span { "Invert Gain" }
                }

                if let Some(ds) = dyn_state.clone() {
                    {
                        let ds_dyn = ds.clone();
                        let ds_spec = ds.clone();
                        rsx! {
                            div {
                                style: menu_row(hover() == Some(3), false),
                                onmouseenter: move |_| { hover.set(Some(3)); open_sub.set(None); },
                                onclick: move |_| {
                                    let target = if mode == crate::dynamics::DynMode::Dynamic {
                                        crate::dynamics::DynMode::Static
                                    } else {
                                        crate::dynamics::DynMode::Dynamic
                                    };
                                    ds_dyn.set_mode(target);
                                    on_dismiss.call(());
                                },
                                span {
                                    style: "width:12px;",
                                    if mode == crate::dynamics::DynMode::Dynamic { "✓" } else { "" }
                                }
                                // Contextual, the way Pro-Q's is: once a band
                                // is dynamic the entry that created it becomes
                                // the one that clears it, so the menu never
                                // offers you a state you are already in.
                                span {
                                    if mode == crate::dynamics::DynMode::Dynamic {
                                        "Clear Dynamics"
                                    } else {
                                        "Make Dynamic"
                                    }
                                }
                            }
                            div {
                                style: menu_row(hover() == Some(4), false),
                                onmouseenter: move |_| { hover.set(Some(4)); open_sub.set(None); },
                                onclick: move |_| {
                                    let target = if mode == crate::dynamics::DynMode::Spectral {
                                        crate::dynamics::DynMode::Static
                                    } else {
                                        crate::dynamics::DynMode::Spectral
                                    };
                                    ds_spec.set_mode(target);
                                    on_dismiss.call(());
                                },
                                span {
                                    style: "width:12px;",
                                    if mode == crate::dynamics::DynMode::Spectral { "✓" } else { "" }
                                }
                                span {
                                    if mode == crate::dynamics::DynMode::Spectral {
                                        "Clear Spectral"
                                    } else {
                                        "Make Spectral"
                                    }
                                }
                            }
                        }
                    }
                }

                div { style: menu_divider() }

                // ── Submenus: Shape / Slope / Stereo Placement ──
                div {
                    style: menu_row(hover() == Some(5) || open_sub() == Some(5), false),
                    onmouseenter: move |_| { hover.set(Some(5)); open_sub.set(Some(5)); },
                    span { style: "width:12px;" }
                    span { style: "flex:1;", "Shape" }
                    span { style: format!("color:{MENU_DIM};"), "{shape_icon(cur_shape)} ›" }
                }
                div {
                    style: menu_row(hover() == Some(6) || open_sub() == Some(6), false),
                    onmouseenter: move |_| { hover.set(Some(6)); open_sub.set(Some(6)); },
                    span { style: "width:12px;" }
                    span { style: "flex:1;", "Slope" }
                    span { style: format!("color:{MENU_DIM};"), "{slope_db(cur_slope):.0} ›" }
                }
                div {
                    style: menu_row(hover() == Some(7) || open_sub() == Some(7), false),
                    onmouseenter: move |_| { hover.set(Some(7)); open_sub.set(Some(7)); },
                    span { style: "width:12px;" }
                    span { style: "flex:1;", "Stereo Placement" }
                    span { style: format!("color:{MENU_DIM};"), "{cur_place.short_label()} ›" }
                }

                div { style: menu_divider() }

                // ── Destructive ──
                div {
                    style: menu_row(hover() == Some(8), false),
                    onmouseenter: move |_| { hover.set(Some(8)); open_sub.set(None); },
                    onclick: move |_| { apply(&|b: &mut EqBand| b.gain = 0.0); on_dismiss.call(()); },
                    span { style: "width:12px;" }
                    span { "Reset Gain" }
                }
                div {
                    style: menu_row(hover() == Some(9), true),
                    onmouseenter: move |_| { hover.set(Some(9)); open_sub.set(None); },
                    onclick: move |_| {
                        if let Some(c) = &on_band_remove { c.call(band_idx); }
                        on_dismiss.call(());
                    },
                    span { style: "width:12px;" }
                    span { "Delete Band" }
                }
            }

            // ── Flyouts ──
            if open_sub() == Some(5) {
                div {
                    style: menu_shell(sub_left, sub_top(92.0, shapes.len()), sub_w),
                    onmousedown: move |evt: MouseEvent| { evt.stop_propagation(); },
                    onmouseenter: move |_| { open_sub.set(Some(5)); },
                    for (i , s) in shapes.iter().enumerate() {
                        div {
                            key: "shape{i}",
                            style: menu_row(false, false),
                            onclick: {
                                let s = *s;
                                move |_| {
                                    apply(&|b: &mut EqBand| {
                                        b.shape = s;
                                        if s.uses_slope() { b.q = 0.707; }
                                    });
                                    on_dismiss.call(());
                                }
                            },
                            span {
                                style: format!("width:12px; color:{band_color};"),
                                if *s == cur_shape { "✓" } else { "" }
                            }
                            span { "{s.label()}" }
                        }
                    }
                }
            }

            if open_sub() == Some(6) {
                div {
                    style: menu_shell(sub_left, sub_top(112.0, SLOPES.len()), sub_w),
                    onmousedown: move |evt: MouseEvent| { evt.stop_propagation(); },
                    onmouseenter: move |_| { open_sub.set(Some(6)); },
                    for (i , (value , label)) in SLOPES.iter().enumerate() {
                        div {
                            key: "slope{i}",
                            style: menu_row(false, false),
                            onclick: {
                                let v = *value;
                                move |_| {
                                    apply(&|b: &mut EqBand| b.slope = Some(v));
                                    on_dismiss.call(());
                                }
                            },
                            span {
                                style: format!("width:12px; color:{band_color};"),
                                if (*value - cur_slope).abs() < 0.01 { "✓" } else { "" }
                            }
                            span { "{label}" }
                        }
                    }
                }
            }

            if open_sub() == Some(7) {
                div {
                    style: menu_shell(sub_left, sub_top(132.0, PLACEMENTS.len()), sub_w),
                    onmousedown: move |evt: MouseEvent| { evt.stop_propagation(); },
                    onmouseenter: move |_| { open_sub.set(Some(7)); },
                    for (i , (place , label , dot)) in PLACEMENTS.iter().enumerate() {
                        div {
                            key: "place{i}",
                            style: menu_row(false, false),
                            onclick: {
                                let pm = *place;
                                move |_| {
                                    apply(&|b: &mut EqBand| b.stereo_mode = pm);
                                    on_dismiss.call(());
                                }
                            },
                            span {
                                style: format!("width:12px; color:{band_color};"),
                                if *place == cur_place { "✓" } else { "" }
                            }
                            // The colour dot: what makes five near-identical
                            // rows scannable without reading them.
                            span {
                                style: format!(
                                    "width:7px; height:7px; border-radius:4px; background:{dot}; \
                                     flex:0 0 auto;"
                                ),
                            }
                            span {
                                style: if *place == cur_place { "font-weight:700;".to_string() } else { String::new() },
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Compact frequency for the menu header.
fn freq_short(hz: f32) -> String {
    if hz >= 1000.0 {
        format!("{:.1} kHz", hz / 1000.0)
    } else {
        format!("{hz:.0} Hz")
    }
}

#[cfg(test)]
mod shape_id_tests {
    use super::{int_to_shape, shape_to_int};
    use crate::eq_graph_model::EqBandShape;

    /// `shape_to_int` and `int_to_shape` are a bijection.
    ///
    /// They are written as two independent `match` arms, so nothing but a test
    /// stops them drifting — and a drift here silently turns every high shelf
    /// into a high-pass somewhere downstream.
    #[test]
    fn shape_ids_round_trip() {
        for shape in EqBandShape::all() {
            let id = shape_to_int(*shape);
            assert_eq!(int_to_shape(id), *shape, "{shape:?} does not round-trip");
        }
    }

    /// The canonical ids are dense and unique, which is what lets the band
    /// panel use them directly as select indices.
    #[test]
    fn shape_ids_are_dense_and_unique() {
        let all = EqBandShape::all();
        let mut seen = vec![false; all.len()];
        for shape in all {
            let id = shape_to_int(*shape);
            let i = usize::try_from(id).expect("shape id is never negative");
            assert!(i < all.len(), "{shape:?} has id {id}, past the shape count");
            assert!(!seen[i], "{shape:?} collides on id {id}");
            seen[i] = true;
        }
        assert!(seen.iter().all(|s| *s), "shape ids are not contiguous");
    }

    /// The ordering trap itself, pinned: the position a shape occupies in
    /// `all()` is NOT its id, so anything that needs an id must ask for one.
    #[test]
    fn position_in_all_is_not_the_canonical_id() {
        let all = EqBandShape::all();
        let mismatched = all
            .iter()
            .enumerate()
            .filter(|(i, sh)| shape_to_int(**sh) as usize != *i)
            .count();
        assert!(
            mismatched > 0,
            "all() and shape_to_int now agree — if that was deliberate, delete \
             this test and simplify the callers that work around the difference"
        );
    }
}
