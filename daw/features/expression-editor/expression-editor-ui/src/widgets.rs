//! Portable controls.
//!
//! `<input type="range">` renders as a thumbless bar under Blitz — no
//! track fill, no handle, no readout — which makes every slider in a
//! plugin window unreadable. These are drawn as SVG and dragged with
//! pointer events, so they look and behave the same standalone, in a
//! VST3/CLAP editor, and in a browser.

use dioxus::prelude::*;

use crate::theme;

/// A horizontal slider with a value readout.
///
/// `value`, `min` and `max` are in the caller's own units; the mapping
/// to 0..1 happens here so callers never pre-normalize.
#[component]
pub fn Slider(
    label: String,
    value: f64,
    min: f64,
    max: f64,
    #[props(default = 96.0)] width: f64,
    /// Rendered next to the handle. Defaults to the raw value.
    #[props(default)]
    readout: Option<String>,
    on_change: EventHandler<f64>,
) -> Element {
    let mut dragging = use_signal(|| false);
    let span = (max - min).max(1e-9);
    let frac = ((value - min) / span).clamp(0.0, 1.0);

    const H: f64 = 14.0;
    let track_w = width - H;
    let cx = H * 0.5 + frac * track_w;
    let text = readout.unwrap_or_else(|| format!("{value:.2}"));

    // Pointer x → value, shared by the initial press and the drag.
    let emit = move |x: f64| {
        let f = ((x - H * 0.5) / track_w.max(1.0)).clamp(0.0, 1.0);
        on_change.call(min + f * span);
    };

    rsx! {
        div {
            style: "display: flex; align-items: center; gap: 5px; \
                    color: {theme::TEXT_DIM}; font-size: 10px; user-select: none;",
            if !label.is_empty() {
                span { style: "min-width: 40px;", "{label}" }
            }
            svg {
                view_box: "0 0 {width:.0} {H:.0}",
                style: "width: {width:.0}px; height: {H:.0}px; flex: 0 0 auto; \
                        cursor: pointer; touch-action: none;",
                onpointerdown: move |e: PointerEvent| {
                    dragging.set(true);
                    emit(e.data().element_coordinates().x);
                },
                onpointermove: move |e: PointerEvent| {
                    if dragging() {
                        emit(e.data().element_coordinates().x);
                    }
                },
                onpointerup: move |_| dragging.set(false),
                onpointerleave: move |_| dragging.set(false),

                rect {
                    x: "{H * 0.5:.1}",
                    y: "{H * 0.5 - 2.0:.1}",
                    width: "{track_w:.1}",
                    height: "4",
                    rx: "2",
                    fill: theme::CONTROL_GROOVE,
                }
                rect {
                    x: "{H * 0.5:.1}",
                    y: "{H * 0.5 - 2.0:.1}",
                    width: "{(frac * track_w).max(0.0):.1}",
                    height: "4",
                    rx: "2",
                    fill: theme::ACCENT,
                }
                circle {
                    cx: "{cx:.1}",
                    cy: "{H * 0.5:.1}",
                    r: "5",
                    fill: if dragging() { theme::SELECTED } else { theme::HANDLE },
                    stroke: theme::BG,
                    stroke_width: "1",
                }
            }
            span {
                style: "min-width: 34px; text-align: right; \
                        font-family: ui-monospace, monospace; color: {theme::TEXT};",
                "{text}"
            }
        }
    }
}

/// A slider anchored at a centre value, filling outward from it.
///
/// For gains and blends where 1.0 (or 0) means "unchanged" and both
/// directions are meaningful — the fill shows deviation, not magnitude.
#[component]
pub fn CenterSlider(
    label: String,
    value: f64,
    min: f64,
    max: f64,
    center: f64,
    #[props(default = 96.0)] width: f64,
    #[props(default)] readout: Option<String>,
    on_change: EventHandler<f64>,
) -> Element {
    let mut dragging = use_signal(|| false);
    let span = (max - min).max(1e-9);
    let frac = ((value - min) / span).clamp(0.0, 1.0);
    let cfrac = ((center - min) / span).clamp(0.0, 1.0);

    const H: f64 = 14.0;
    let track_w = width - H;
    let x0 = H * 0.5 + cfrac.min(frac) * track_w;
    let x1 = H * 0.5 + cfrac.max(frac) * track_w;
    let cx = H * 0.5 + frac * track_w;
    let text = readout.unwrap_or_else(|| format!("{value:.2}"));

    let emit = move |x: f64| {
        let f = ((x - H * 0.5) / track_w.max(1.0)).clamp(0.0, 1.0);
        on_change.call(min + f * span);
    };

    rsx! {
        div {
            style: "display: flex; align-items: center; gap: 5px; \
                    color: {theme::TEXT_DIM}; font-size: 10px; user-select: none;",
            if !label.is_empty() {
                span { style: "min-width: 40px;", "{label}" }
            }
            svg {
                view_box: "0 0 {width:.0} {H:.0}",
                style: "width: {width:.0}px; height: {H:.0}px; flex: 0 0 auto; \
                        cursor: pointer; touch-action: none;",
                onpointerdown: move |e: PointerEvent| {
                    dragging.set(true);
                    emit(e.data().element_coordinates().x);
                },
                onpointermove: move |e: PointerEvent| {
                    if dragging() {
                        emit(e.data().element_coordinates().x);
                    }
                },
                onpointerup: move |_| dragging.set(false),
                onpointerleave: move |_| dragging.set(false),

                rect {
                    x: "{H * 0.5:.1}",
                    y: "{H * 0.5 - 2.0:.1}",
                    width: "{track_w:.1}",
                    height: "4",
                    rx: "2",
                    fill: theme::CONTROL_GROOVE,
                }
                rect {
                    x: "{x0:.1}",
                    y: "{H * 0.5 - 2.0:.1}",
                    width: "{(x1 - x0).max(0.0):.1}",
                    height: "4",
                    rx: "2",
                    fill: theme::ACCENT,
                }
                // The unchanged mark, so "back to as sung" is findable.
                line {
                    x1: "{H * 0.5 + cfrac * track_w:.1}",
                    y1: "{H * 0.5 - 5.0:.1}",
                    x2: "{H * 0.5 + cfrac * track_w:.1}",
                    y2: "{H * 0.5 + 5.0:.1}",
                    stroke: theme::TEXT_DIM,
                    stroke_width: "1",
                }
                circle {
                    cx: "{cx:.1}",
                    cy: "{H * 0.5:.1}",
                    r: "5",
                    fill: if dragging() { theme::SELECTED } else { theme::HANDLE },
                    stroke: theme::BG,
                    stroke_width: "1",
                }
            }
            span {
                style: "min-width: 34px; text-align: right; \
                        font-family: ui-monospace, monospace; color: {theme::TEXT};",
                "{text}"
            }
        }
    }
}
