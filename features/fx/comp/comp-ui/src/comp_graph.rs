//! The compressor graph — the Pro-C-style rolling display, as a Dioxus SVG
//! component for the Blitz plugin editor.
//!
//! Rendering is a port of the signal domain's `CompSurface`
//! (features/rigs/guitar/ui/src/comp_surface.rs): the input waveform fills
//! the panel from the bottom (teal), the gain-reduction trace fills from the
//! top (red), with the soft-knee transfer curve, threshold line + knee
//! markers, and the live input ball overlaid. All path math lives in the
//! portable [`crate::comp_graph_svg`] module.
//!
//! Interactions drive nice-plug parameters through the editor's
//! [`ParamContext`] begin/set/end idiom (the same host-gesture pattern as
//! eq-ui's graph):
//!
//! - **Drag the threshold line** (grab within ±16 px): absolute threshold.
//! - **Drag above the line** (the compressed region): tilts the slope —
//!   drag down = more ratio, up = less (60 px per doubling).
//! - **Mouse wheel**: knee width (±1 dB per notch).
//!
//! # Coordinate mapping
//!
//! The SVG's viewBox is `0 0 360 {height}` with `preserveAspectRatio: none`,
//! and the host container ([`crate::faces::control`]) pins the graph to
//! exactly that many CSS px, so element-relative pointer y IS viewBox y — no
//! async rect measurement (which is unreliable under Blitz, see eq-ui's
//! notes) and no scale drift. That is why the height is a prop rather than a
//! constant: it grows with the editor ([`graph_height`]) and the two uses have
//! to move together. The width stretches freely; no interaction depends on x.

use std::sync::atomic::Ordering;

use audiocore_core::prelude::*;

use crate::comp_graph_svg::{
    db_to_y, scale_gr_wave, scale_input_wave, smooth_path, transfer_ball, transfer_curve_path,
    y_to_db, RANGE_DB,
};
use crate::params::CompUiState;

/// Design viewBox width, and the fallback when no window size is in context.
pub const GRAPH_VB_W: f64 = 360.0;
/// Design height of the graph: the viewBox height AND the CSS pixel height of
/// the container at the editor's design size. Keeping the two equal is what
/// makes pointer y map 1:1 onto graph dB space — see [`graph_height`], which
/// preserves that property as the editor grows.
pub const GRAPH_H: f64 = 300.0;
/// The graph's size for the current editor window: the surface the shell rail
/// leaves, in CSS px.
///
/// Both numbers go into the viewBox as well as the container, because blitz
/// does not honour `preserveAspectRatio: none` — it scales the viewBox
/// uniformly and centres it, so a viewBox whose aspect does not match its box
/// gets letterboxed. Feeding it the real pixel size makes the scale exactly 1,
/// which both fills the surface and keeps pointer coordinates equal to viewBox
/// coordinates.
pub fn graph_size() -> (f64, f64) {
    match crate::hardware::panel::window_logical_size() {
        Some((win_w, win_h)) => (
            (win_w - crate::control_view::RAIL_W).round().max(1.0),
            win_h.round().max(1.0),
        ),
        None => (GRAPH_VB_W, GRAPH_H),
    }
}

/// The graph height for the current editor window.
///
/// The graph is no longer pinned to [`GRAPH_H`], and no longer a band under a
/// header: it *is* the control surface, so it takes the window's full height,
/// with the knobs and meters floating over it. The same number is used for the
/// container height and for the viewBox, so element-relative pointer y is
/// still viewBox y at any size. Falls back to the design height when no host
/// window size is in context (headless tests, non-plugin mounts).
pub fn graph_height() -> f64 {
    graph_size().1
}
/// Grab distance (px) around the threshold line.
const THRESHOLD_GRAB_PX: f64 = 16.0;
/// Vertical drag distance (px) that doubles/halves the ratio.
const RATIO_PX_PER_DOUBLING: f64 = 60.0;

/// What a pointer drag on the display is editing.
#[derive(Clone, Copy, PartialEq)]
enum CompDrag {
    /// Vertical drag on the threshold line — absolute position.
    Threshold,
    /// Vertical drag above the knee — tilts the slope (ratio).
    Ratio { start_y: f64, start_ratio: f32 },
}

/// The compressor graph. Consumes `SharedState` (for [`CompUiState`]) and
/// the editor's `ParamContext` from context, like the rest of the editor.
///
/// `width` and `height` are both the container's CSS size and the viewBox — the
/// caller owns them (it sizes the container) and passes them in so the two
/// cannot drift apart and break the 1:1 pointer mapping. See [`graph_size`].
#[component]
pub fn CompGraph(
    #[props(default = GRAPH_VB_W)] width: f64,
    #[props(default = GRAPH_H)] height: f64,
) -> Element {
    let shared = use_context::<SharedState>();
    let ui = shared.get::<CompUiState>().expect("CompUiState missing");
    let ctx = use_param_context();
    let params = ui.params.clone();

    let mut dragging = use_signal(|| None::<CompDrag>);

    // Redraw tick: the waveform rings advance without any signal write, so
    // mark this scope dirty from an OS thread — the same driver as
    // eq_graph.rs (schedule_update works outside the runtime per its docs).
    let frame_tick: Signal<u64> = use_signal(|| 0);
    use_hook(|| {
        let updater = dioxus_core::schedule_update();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(33));
                updater();
            }
        });
    });
    let _ = *frame_tick.read();

    // ── Current values ───────────────────────────────────────────────────
    let threshold = params.threshold_db.value();
    let ratio = params.ratio.value().max(1.0);
    let knee = params.knee_db.value();
    let gr_db = ui.gain_reduction_db.load(Ordering::Relaxed);
    let in_db = ui.input_peak_db.load(Ordering::Relaxed);

    // ── Paths (portable math) ────────────────────────────────────────────
    let in_scaled = scale_input_wave(&ui.input_wave.snapshot());
    let gr_scaled = scale_gr_wave(&ui.gr_wave.snapshot());
    let in_fill = smooth_path(&in_scaled, width, height, true, true);
    let in_edge = smooth_path(&in_scaled, width, height, true, false);
    let gr_active = gr_scaled.iter().any(|&g| g > 0.002);
    let gr_fill = smooth_path(&gr_scaled, width, height, false, true);
    let gr_edge = smooth_path(&gr_scaled, width, height, false, false);

    let tc = transfer_curve_path(threshold, ratio, knee, width, height);
    let ball = transfer_ball(in_db, threshold, ratio, knee, width, height);
    let thresh_y = db_to_y(threshold as f64, height);
    let knee_lo_y = db_to_y((threshold - knee * 0.5) as f64, height);
    let knee_hi_y = db_to_y((threshold + knee * 0.5) as f64, height);

    rsx! {
        div {
            style: "position:relative; width:100%; height:100%; overflow:hidden; \
                    background:#080808; user-select:none;",

            // ── Interactions: grab the threshold line, tilt the slope
            // above it, wheel for knee width ──
            onmousedown: {
                let params = params.clone();
                let ctx = ctx.clone();
                move |evt: MouseEvent| {
                    let y = evt.element_coordinates().y;
                    let ty = db_to_y(params.threshold_db.value() as f64, height);
                    if (y - ty).abs() < THRESHOLD_GRAB_PX {
                        ctx.begin_set_raw(params.threshold_db.as_ptr());
                        dragging.set(Some(CompDrag::Threshold));
                    } else if y < ty {
                        // Above the threshold line = the compressed region —
                        // dragging tilts the slope.
                        ctx.begin_set_raw(params.ratio.as_ptr());
                        dragging.set(Some(CompDrag::Ratio {
                            start_y: y,
                            start_ratio: params.ratio.value(),
                        }));
                    }
                    evt.prevent_default();
                }
            },
            onmousemove: {
                let params = params.clone();
                let ctx = ctx.clone();
                move |evt: MouseEvent| {
                    let Some(mode) = *dragging.read() else { return };
                    // Button released outside the graph: end the gesture on
                    // re-entry instead of resuming it (eq_graph idiom).
                    if evt.held_buttons().is_empty() {
                        let ptr = match mode {
                            CompDrag::Threshold => params.threshold_db.as_ptr(),
                            CompDrag::Ratio { .. } => params.ratio.as_ptr(),
                        };
                        ctx.end_set_raw(ptr);
                        dragging.set(None);
                        return;
                    }
                    let y = evt.element_coordinates().y;
                    match mode {
                        CompDrag::Threshold => {
                            let db = y_to_db(y, height).clamp(-(RANGE_DB as f64), 0.0) as f32;
                            ctx.set_normalized_raw(
                                params.threshold_db.as_ptr(),
                                params.threshold_db.preview_normalized(db),
                            );
                        }
                        CompDrag::Ratio { start_y, start_ratio } => {
                            // Drag down = more ratio (harder tilt).
                            let factor = ((y - start_y) / RATIO_PX_PER_DOUBLING) as f32;
                            let ratio = (start_ratio * factor.exp2()).clamp(1.0, 20.0);
                            ctx.set_normalized_raw(
                                params.ratio.as_ptr(),
                                params.ratio.preview_normalized(ratio),
                            );
                        }
                    }
                }
            },
            onmouseup: {
                let params = params.clone();
                let ctx = ctx.clone();
                move |_| {
                    let mode = { *dragging.read() };
                    if let Some(mode) = mode {
                        let ptr = match mode {
                            CompDrag::Threshold => params.threshold_db.as_ptr(),
                            CompDrag::Ratio { .. } => params.ratio.as_ptr(),
                        };
                        ctx.end_set_raw(ptr);
                        dragging.set(None);
                    }
                }
            },
            onwheel: {
                let params = params.clone();
                let ctx = ctx.clone();
                move |evt: WheelEvent| {
                    evt.prevent_default();
                    let delta = evt.delta().strip_units().y;
                    if delta == 0.0 {
                        return;
                    }
                    // Scroll up = wider knee, ±1 dB per notch.
                    let step = if delta < 0.0 { 1.0 } else { -1.0 };
                    let knee = (params.knee_db.value() + step).clamp(0.0, 24.0);
                    let ptr = params.knee_db.as_ptr();
                    ctx.begin_set_raw(ptr);
                    ctx.set_normalized_raw(ptr, params.knee_db.preview_normalized(knee));
                    ctx.end_set_raw(ptr);
                }
            },

            svg {
                style: "position:absolute; top:0; left:0; right:0; bottom:0; \
                        width:100%; height:100%; display:block; pointer-events:none;",
                view_box: "0 0 {width} {height}",
                preserve_aspect_ratio: "none",

                defs {
                    // Input level — dark teal gradient (painter's stops).
                    linearGradient { id: "compui-in", x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0", stop_color: "rgba(20,110,115,0.82)" }
                        stop { offset: "0.45", stop_color: "rgba(14,80,85,0.51)" }
                        stop { offset: "0.80", stop_color: "rgba(8,50,55,0.22)" }
                        stop { offset: "1", stop_color: "rgba(4,25,28,0.06)" }
                    }
                    // Gain reduction — red gradient from the top.
                    linearGradient { id: "compui-gr", x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0", stop_color: "rgba(220,40,40,0.82)" }
                        stop { offset: "0.35", stop_color: "rgba(200,30,30,0.51)" }
                        stop { offset: "0.70", stop_color: "rgba(175,25,25,0.22)" }
                        stop { offset: "1", stop_color: "rgba(150,20,20,0.06)" }
                    }
                }

                // Input waveform: fill + glow edge + bright cyan edge.
                if !in_fill.is_empty() {
                    path { d: "{in_fill}", fill: "url(#compui-in)" }
                    path { d: "{in_edge}", fill: "none", stroke: "rgba(0,200,210,0.12)", stroke_width: "4" }
                    path { d: "{in_edge}", fill: "none", stroke: "rgba(60,210,220,0.78)", stroke_width: "1.5" }
                }
                // GR from the top. Hidden while the detector is idle (a zero
                // trace would still paint its edge line across the top).
                if !gr_fill.is_empty() && gr_active {
                    path { d: "{gr_fill}", fill: "url(#compui-gr)" }
                    path { d: "{gr_edge}", fill: "none", stroke: "rgba(255,60,60,0.12)", stroke_width: "4" }
                    path { d: "{gr_edge}", fill: "none", stroke: "rgba(255,80,80,0.82)", stroke_width: "1.5" }
                }

                // Knee markers — the soft-knee window around the threshold.
                if knee > 0.05 {
                    line { x1: "0", y1: "{knee_hi_y:.1}", x2: "{width:.1}", y2: "{knee_hi_y:.1}",
                        stroke: "rgba(255,180,120,0.18)", stroke_width: "1", stroke_dasharray: "2,4" }
                    line { x1: "0", y1: "{knee_lo_y:.1}", x2: "{width:.1}", y2: "{knee_lo_y:.1}",
                        stroke: "rgba(255,180,120,0.18)", stroke_width: "1", stroke_dasharray: "2,4" }
                }

                // Threshold line — the grabbable control.
                line { x1: "0", y1: "{thresh_y:.1}", x2: "{width:.1}", y2: "{thresh_y:.1}",
                    stroke: "rgba(255,120,120,0.55)", stroke_width: "2", stroke_dasharray: "6,4" }
                // Grab handle chip at the right end.
                rect { x: "6", y: "{thresh_y - 7.0:.1}", width: "30", height: "14", rx: "3",
                    fill: "rgba(255,120,120,0.15)", stroke: "rgba(255,120,120,0.5)", stroke_width: "1" }
                text { x: "21", y: "{thresh_y + 3.5:.1}", fill: "#ff9c9c", font_size: "9",
                    text_anchor: "middle", pointer_events: "none", "{threshold:.0}" }

                // Transfer curve + live input ball.
                path { "data-testid": "transfer-curve",
                    d: "{tc}", fill: "none", stroke: "rgba(180,210,140,0.71)", stroke_width: "2" }
                circle { cx: "{ball.0:.1}", cy: "{ball.1:.1}", r: "3", fill: "rgba(255,255,255,0.78)" }
            }

            // Readouts sit top-left, and so does the threshold line's grab
            // chip: the floating meter and control panels own the right-hand
            // and bottom edges of the surface.
            div {
                style: "position:absolute; top:8px; left:10px; display:flex; \
                        align-items:baseline; gap:4px; pointer-events:none;",
                span {
                    style: "font-size:9px; font-weight:600; text-transform:uppercase; color:#8a8a92;",
                    "GR"
                }
                span {
                    style: "font-family:ui-monospace,monospace; font-size:12px; color:#ff9c9c;",
                    "{-gr_db:.1}"
                }
            }

            div {
                style: "position:absolute; top:26px; left:10px; display:flex; \
                        flex-direction:column; pointer-events:none;",
                span {
                    style: "font-size:8px; color:#8a8a92; text-transform:uppercase;",
                    "Thr · Ratio · Knee"
                }
                span {
                    style: "font-family:ui-monospace,monospace; font-size:11px; color:#e8e8ec;",
                    "{threshold:.1} dB · {ratio:.1}:1 · {knee:.1} dB"
                }
            }
        }
    }
}
