//! Popup and context-menu controls for the EQ graph.

use architect_ui::prelude::{Button, ButtonSize, ButtonVariant, Select, SelectContent, SelectItem};
use nice_plug_dioxus::prelude::*;

use super::eq_graph_model::{
    EqBand, EqBandShape, MAX_BANDS, StereoMode, q_to_slope_db,
};

fn shape_to_int(s: EqBandShape) -> i32 {
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
        format!("{:.0}", frequency)
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

fn int_to_shape(v: i32) -> EqBandShape {
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

fn shape_icon(s: EqBandShape) -> &'static str {
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
pub fn band_popup_rect(
    bx: f64,
    by: f64,
    graph_w: f64,
    graph_h: f64,
    is_dragging: bool,
) -> (f64, f64, f64, f64) {
    let w = if is_dragging { 178.0 } else { 196.0 };
    let h = if is_dragging { 34.0 } else { 72.0 };
    let x = (bx - w / 2.0).clamp(0.0, (graph_w - w).max(0.0));
    let y = if by - h - POPUP_GAP >= 0.0 {
        by - h - POPUP_GAP
    } else {
        by + POPUP_GAP
    }
    .clamp(0.0, (graph_h - h).max(0.0));
    (x, y, w, h)
}

/// Is `(px, py)` inside the band's "keep the panel up" region?
///
/// That region is the union of the panel's box and the band node itself, so
/// the empty [`POPUP_GAP`] the pointer must cross to reach the panel is part
/// of it. Without this the panel fades out from under the cursor on the way
/// there and its controls can never be clicked.
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

fn next_stereo_mode(mode: StereoMode) -> StereoMode {
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
    let q_str = if band.shape.uses_slope() {
        format!("{:.0} dB/oct", q_to_slope_db(band.q))
    } else {
        format!("Q {:.2}", band.q)
    };
    let band_color = crate::eq_graph_model::freq_to_color(band.frequency as f64);
    let band_enabled = band.enabled;
    let band_solo = band.solo;
    let band_gain = band.gain;
    let band_shape = band.shape;

    let stereo_mode = band.stereo_mode;

    let (popup_x, popup_y, popup_w, popup_h) =
        band_popup_rect(bx, by, graph_w, graph_h, is_dragging);

    let current_shape_value = shape_to_int(band_shape).to_string();
    let mut shape_value_sig = use_signal(|| current_shape_value.clone());
    if *shape_value_sig.read() != current_shape_value {
        shape_value_sig.set(current_shape_value);
    }
    let shape_options: Vec<(usize, String, String)> = EqBandShape::all()
        .iter()
        .enumerate()
        .map(|(i, sh)| {
            (
                i,
                shape_to_int(*sh).to_string(),
                format!("{} {}", shape_icon(*sh), sh.label()),
            )
        })
        .collect();

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

            div {
                class: "rounded-md border bg-card text-foreground shadow-md p-1.5 flex flex-col gap-1.5",
                style: format!("border-top: 2px solid {band_color};"),
                onmousedown: move |evt| { evt.stop_propagation(); },

                div {
                    class: "flex items-baseline justify-between gap-2 px-1 h-5",
                    span {
                        class: "text-xs font-semibold tabular-nums tracking-tight",
                        "{freq_str} Hz"
                    }
                    span {
                        class: "text-xs font-semibold tabular-nums tracking-tight",
                        "{band_gain:+.1} dB"
                    }
                    span {
                        class: "text-[10px] tabular-nums text-muted-foreground",
                        "{q_str}"
                    }
                }

                if !is_dragging {
                    div {
                        class: "grid items-center gap-1",
                        style: "grid-template-columns: 28px minmax(72px, 1fr) 28px 28px 28px;",

                        Button {
                            size: ButtonSize::Small,
                            variant: if band_enabled { ButtonVariant::Outline } else { ButtonVariant::Secondary },
                            class: "px-0".to_string(),
                            on_click: {
                                let cb = on_band_change;
                                move |_| {
                                    let updated = {
                                        let mut bv = bands.write();
                                        if band_idx < bv.len() {
                                            bv[band_idx].enabled = !bv[band_idx].enabled;
                                            Some(bv[band_idx].clone())
                                        } else {
                                            None
                                        }
                                    };
                                    if let (Some(b), Some(c)) = (updated, &cb) {
                                        c.call((band_idx, b));
                                    }
                                }
                            },
                            if band_enabled { "On" } else { "Byp" }
                        }

                        div { class: "min-w-0",
                            Select {
                                value: shape_value_sig,
                                placeholder: "Filter".to_string(),
                                on_change: {
                                    let cb = on_band_change;
                                    move |v: String| {
                                        let Ok(idx) = v.parse::<i32>() else { return };
                                        let new_shape = int_to_shape(idx);
                                        let updated = {
                                            let mut bv = bands.write();
                                            if band_idx < bv.len() {
                                                bv[band_idx].shape = new_shape;
                                                if new_shape.uses_slope() {
                                                    bv[band_idx].q = 1.0;
                                                }
                                                Some(bv[band_idx].clone())
                                            } else {
                                                None
                                            }
                                        };
                                        if let (Some(b), Some(c)) = (updated, &cb) {
                                            c.call((band_idx, b));
                                        }
                                    }
                                },
                                SelectContent {
                                    for (idx , value , label) in shape_options {
                                        SelectItem { value, index: idx, "{label}" }
                                    }
                                }
                            }
                        }

                        Button {
                            size: ButtonSize::Small,
                            variant: ButtonVariant::Outline,
                            class: "px-0".to_string(),
                            on_click: {
                                let cb = on_band_change;
                                move |_| {
                                    let updated = {
                                        let mut bv = bands.write();
                                        if band_idx < bv.len() {
                                            bv[band_idx].stereo_mode = next_stereo_mode(bv[band_idx].stereo_mode);
                                            Some(bv[band_idx].clone())
                                        } else {
                                            None
                                        }
                                    };
                                    if let (Some(b), Some(c)) = (updated, &cb) {
                                        c.call((band_idx, b));
                                    }
                                }
                            },
                            "{stereo_mode.short_label()}"
                        }

                        Button {
                            size: ButtonSize::Small,
                            variant: if band_solo { ButtonVariant::Primary } else { ButtonVariant::Outline },
                            class: "px-0".to_string(),
                            on_click: {
                                let cb = on_band_change;
                                move |_| {
                                    let updated = {
                                        let mut bv = bands.write();
                                        if band_idx < bv.len() {
                                            bv[band_idx].solo = !bv[band_idx].solo;
                                            Some(bv[band_idx].clone())
                                        } else {
                                            None
                                        }
                                    };
                                    if let (Some(b), Some(c)) = (updated, &cb) {
                                        c.call((band_idx, b));
                                    }
                                }
                            },
                            "S"
                        }

                        Button {
                            size: ButtonSize::Small,
                            variant: ButtonVariant::Destructive,
                            class: "px-0".to_string(),
                            on_click: {
                                let cb = on_band_remove;
                                move |_| {
                                    if let Some(c) = &cb {
                                        c.call(band_idx);
                                    }
                                    on_dismiss.call(());
                                }
                            },
                            "X"
                        }
                    }
                }
            }
        }
    }
}

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
    on_dismiss: EventHandler<()>,
) -> Element {
    let Some(band) = bands.read().get(band_idx).cloned() else {
        return rsx! {};
    };

    let shapes = EqBandShape::all();
    let menu_w: f64 = 130.0;
    let item_h: f64 = 20.0;
    let menu_h = (5.0 + shapes.len() as f64) * item_h + 16.0;
    let menu_x = x.min((graph_w - menu_w).max(0.0));
    let menu_y = y.min((graph_h - menu_h).max(0.0));

    let band_color = crate::eq_graph_model::freq_to_color(band.frequency as f64);
    let is_enabled = band.enabled;
    let is_solo = band.solo;
    let cur_shape = band.shape;

    rsx! {
        div {
            style: "position:absolute; inset:0; z-index:20;",
            onmousedown: {
                let dismiss = on_dismiss;
                move |evt: MouseEvent| { evt.stop_propagation(); dismiss.call(()); }
            },
        }

        div {
            style: format!(
                "position:absolute; left:{menu_x}px; top:{menu_y}px; width:{menu_w}px;                  background:rgba(15,15,18,0.97); border:1px solid rgba(80,80,85,0.6);                  border-radius:4px; z-index:21; font-size:9px; color:#aaa;                  box-sizing:border-box;",
            ),
            onmousedown: move |evt| { evt.stop_propagation(); },

            div { style: format!("padding:6px 10px 4px; color:{band_color}; font-weight:700;"), "Band {band_idx + 1}" }

            div {
                style: format!("padding:4px 10px; cursor:pointer; color:{};",
                    if is_enabled { "#aaa" } else { "#f66" }),
                onclick: {
                    let cb = on_band_change;
                    let dismiss = on_dismiss;
                    move |evt: MouseEvent| {
                        evt.stop_propagation();
                        let upd = { let mut bv = bands.write(); if band_idx < bv.len() { bv[band_idx].enabled = !bv[band_idx].enabled; Some(bv[band_idx].clone()) } else { None } };
                        if let (Some(b), Some(c)) = (upd, &cb) { c.call((band_idx, b)); }
                        dismiss.call(());
                    }
                },
                if is_enabled { "Bypass" } else { "Enable" }
            }

            div {
                style: format!("padding:4px 10px; cursor:pointer; color:{};",
                    if is_solo { "#fc0" } else { "#aaa" }),
                onclick: {
                    let cb = on_band_change;
                    let dismiss = on_dismiss;
                    move |evt: MouseEvent| {
                        evt.stop_propagation();
                        let upd = { let mut bv = bands.write(); if band_idx < bv.len() { bv[band_idx].solo = !bv[band_idx].solo; Some(bv[band_idx].clone()) } else { None } };
                        if let (Some(b), Some(c)) = (upd, &cb) { c.call((band_idx, b)); }
                        dismiss.call(());
                    }
                },
                if is_solo { "Unsolo" } else { "Solo" }
            }

            div { style: "height:1px; background:rgba(80,80,85,0.5); margin:2px 6px;" }

            div {
                style: "padding:4px 10px; cursor:pointer;",
                onclick: {
                    let cb = on_band_change;
                    let dismiss = on_dismiss;
                    move |evt: MouseEvent| {
                        evt.stop_propagation();
                        let upd = { let mut bv = bands.write(); if band_idx < bv.len() { bv[band_idx].gain = 0.0; Some(bv[band_idx].clone()) } else { None } };
                        if let (Some(b), Some(c)) = (upd, &cb) { c.call((band_idx, b)); }
                        dismiss.call(());
                    }
                },
                "Reset Gain"
            }

            div { style: "height:1px; background:rgba(80,80,85,0.5); margin:2px 6px;" }

            div { style: "padding:3px 10px 2px; font-size:8px; color:#555;", "Filter Type" }

            for shape in shapes.iter() {
                {
                    let sc = *shape;
                    let is_cur = sc == cur_shape;
                    rsx! {
                        div {
                            style: format!(
                                "padding:3px 10px 3px {}px; cursor:pointer; color:{}; background:{};",
                                if is_cur { 18 } else { 10 },
                                if is_cur { "#fff" } else { "#bbb" },
                                if is_cur { "rgba(100,150,255,0.2)" } else { "transparent" },
                            ),
                            onclick: {
                                let cb = on_band_change;
                                let dismiss = on_dismiss;
                                move |evt: MouseEvent| {
                                    evt.stop_propagation();
                                    let upd = {
                                        let mut bv = bands.write();
                                        if band_idx < bv.len() {
                                            bv[band_idx].shape = sc;
                                            if sc.uses_slope() { bv[band_idx].q = 1.0; }
                                            Some(bv[band_idx].clone())
                                        } else { None }
                                    };
                                    if let (Some(b), Some(c)) = (upd, &cb) { c.call((band_idx, b)); }
                                    dismiss.call(());
                                }
                            },
                            if is_cur { "● " }
                            "{sc.label()}"
                        }
                    }
                }
            }

            div { style: "height:1px; background:rgba(80,80,85,0.5); margin:2px 6px;" }

            div {
                style: "padding:4px 10px; cursor:pointer; color:rgba(255,80,80,0.8);",
                onclick: {
                    let cb = on_band_remove;
                    let dismiss = on_dismiss;
                    move |evt: MouseEvent| {
                        evt.stop_propagation();
                        if let Some(c) = &cb { c.call(band_idx); }
                        dismiss.call(());
                    }
                },
                "Delete Band"
            }
        }
    }
}
