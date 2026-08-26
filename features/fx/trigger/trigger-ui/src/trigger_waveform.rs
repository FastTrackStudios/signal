//! The trigger analysis waveform — scrolling input peaks with a draggable
//! threshold line and per-hit markers, as a Dioxus SVG component for the
//! Blitz plugin editor.
//!
//! Rendering is a port of the legacy FTS-Trigger `TriggerWaveform`
//! (Slate-Trigger-style analysis window): vertical peak-bar columns scroll
//! right→left, a dB scale grid sits behind them, every detected hit gets a
//! full-height highlight column + a velocity-sized marker dot, and a red
//! threshold line floats over everything. The legacy linear-amplitude y axis
//! is remapped to the comp graph's dB-linear axis so the threshold line is
//! exact in dB. All path math lives in the portable
//! [`crate::trigger_waveform_svg`] module.
//!
//! Interactions drive nice-plug parameters through the editor's
//! `ParamContext` begin/set/end idiom (the same host-gesture pattern as
//! comp-ui's graph):
//!
//! - **Drag the threshold line** (grab within ±16 px): absolute threshold,
//!   landing exactly on the pointer's dB.
//!
//! # Coordinate mapping
//!
//! The SVG uses a fixed `0 0 360 260` viewBox with
//! `preserveAspectRatio: none`. The host container
//! ([`crate::control_view`]) pins the graph height to exactly [`GRAPH_H`]
//! CSS px, so element-relative pointer y IS viewBox y — no async rect
//! measurement (unreliable under Blitz, see eq-ui's notes) and no scale
//! drift. The width stretches freely; no interaction depends on x.

use audiocore_core::prelude::*;

use crate::params::{TriggerUiState, WAVE_HISTORY_LEN};
use crate::trigger_waveform_svg::{
    bars_path, db_to_y, marker_columns, scale_peaks, threshold_line_y, y_to_db, DB_MARKERS,
    RANGE_DB,
};

/// viewBox width. Stretched to the container width (visual only).
pub const GRAPH_VB_W: f64 = 360.0;
/// viewBox height AND the required CSS pixel height of the container —
/// keeping them equal makes pointer y map 1:1 onto graph dB space.
pub const GRAPH_H: f64 = 260.0;
/// Grab distance (px) around the threshold line.
const THRESHOLD_GRAB_PX: f64 = 16.0;

/// The trigger waveform. Consumes `SharedState` (for [`TriggerUiState`]) and
/// the editor's `ParamContext` from context, like the rest of the editor —
/// no props, so it renders identically standalone and embedded.
#[component]
pub fn TriggerWaveform() -> Element {
    let shared = use_context::<SharedState>();
    let ui = shared
        .get::<TriggerUiState>()
        .expect("TriggerUiState missing");
    let ctx = use_param_context();
    let params = ui.params.clone();

    let mut dragging = use_signal(|| false);

    // Redraw tick: the waveform/hit rings advance without any signal write,
    // so mark this scope dirty from an OS thread — the same driver as
    // comp_graph.rs (schedule_update works outside the runtime per its docs).
    let frame_tick: Signal<u64> = use_signal(|| 0);
    use_hook(|| {
        let updater = dioxus_core::schedule_update();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(33));
            updater();
        });
    });
    let _ = *frame_tick.read();

    // ── Current values ───────────────────────────────────────────────────
    let threshold = params.threshold_db.value();

    // ── Paths (portable math) ────────────────────────────────────────────
    let bars = bars_path(&scale_peaks(&ui.input_wave.snapshot()), GRAPH_VB_W, GRAPH_H);
    let head = ui.input_wave.head();
    let markers = marker_columns(&ui.hits.snapshot(), head, WAVE_HISTORY_LEN, GRAPH_VB_W);
    let thresh_y = threshold_line_y(threshold, GRAPH_H);
    let grid: Vec<(f64, &str)> = DB_MARKERS
        .iter()
        .map(|&(db, label)| (db_to_y(db as f64, GRAPH_H), label))
        .collect();

    rsx! {
        div {
            style: "position:relative; width:100%; height:100%; overflow:hidden; \
                    background:#080810; user-select:none; cursor:ns-resize;",

            // ── Interactions: grab the threshold line ──
            onmousedown: {
                let params = params.clone();
                let ctx = ctx.clone();
                move |evt: MouseEvent| {
                    let y = evt.element_coordinates().y;
                    let ty = threshold_line_y(params.threshold_db.value(), GRAPH_H);
                    if (y - ty).abs() < THRESHOLD_GRAB_PX {
                        ctx.begin_set_raw(params.threshold_db.as_ptr());
                        dragging.set(true);
                    }
                    evt.prevent_default();
                }
            },
            onmousemove: {
                let params = params.clone();
                let ctx = ctx.clone();
                move |evt: MouseEvent| {
                    if !*dragging.read() {
                        return;
                    }
                    // Button released outside the graph: end the gesture on
                    // re-entry instead of resuming it (comp_graph idiom).
                    if evt.held_buttons().is_empty() {
                        ctx.end_set_raw(params.threshold_db.as_ptr());
                        dragging.set(false);
                        return;
                    }
                    let y = evt.element_coordinates().y;
                    let db = y_to_db(y, GRAPH_H).clamp(-(RANGE_DB as f64), 0.0) as f32;
                    ctx.set_normalized_raw(
                        params.threshold_db.as_ptr(),
                        params.threshold_db.preview_normalized(db),
                    );
                }
            },
            onmouseup: {
                let params = params.clone();
                let ctx = ctx.clone();
                move |_| {
                    if *dragging.read() {
                        ctx.end_set_raw(params.threshold_db.as_ptr());
                        dragging.set(false);
                    }
                }
            },

            svg {
                style: "position:absolute; top:0; left:0; right:0; bottom:0; \
                        width:100%; height:100%; display:block; pointer-events:none;",
                view_box: "0 0 360 260",
                preserve_aspect_ratio: "none",

                defs {
                    // Peak bars — the legacy blue, brightened toward the top.
                    linearGradient { id: "trigui-bars", x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0", stop_color: "rgba(90,180,240,0.75)" }
                        stop { offset: "0.5", stop_color: "rgba(60,140,200,0.48)" }
                        stop { offset: "1", stop_color: "rgba(40,100,160,0.25)" }
                    }
                }

                // dB scale grid lines (labels are HTML overlays below —
                // crisper text than stretched-viewBox SVG glyphs).
                for (y, _label) in grid.iter().copied() {
                    line {
                        x1: "0", y1: "{y:.1}",
                        x2: "360", y2: "{y:.1}",
                        stroke: "rgba(255,255,255,0.06)", stroke_width: "1",
                    }
                }

                // Scrolling peak bars.
                if !bars.is_empty() {
                    path { "data-testid": "wave-bars", d: "{bars}", fill: "url(#trigui-bars)" }
                }

                // Hit markers: full-height highlight column + a dot sized and
                // brightened by velocity, riding just under the top edge.
                for (x, vel) in markers.iter().copied() {
                    rect {
                        x: "{x - 1.5:.1}", y: "0", width: "3", height: "260",
                        fill: "rgba(100,200,255,{0.04 + 0.10 * vel:.3})",
                    }
                    circle {
                        cx: "{x:.1}", cy: "12", r: "{2.0 + 3.0 * vel:.1}",
                        fill: "rgba(120,210,255,{0.35 + 0.60 * vel:.3})",
                    }
                }

                // Threshold line — the grabbable control (legacy red + glow).
                line { "data-testid": "threshold-line",
                    x1: "0", y1: "{thresh_y:.1}", x2: "360", y2: "{thresh_y:.1}",
                    stroke: "rgba(248,113,113,0.25)", stroke_width: "6" }
                line { x1: "0", y1: "{thresh_y:.1}", x2: "360", y2: "{thresh_y:.1}",
                    stroke: "rgba(248,113,113,0.85)", stroke_width: "2" }
                // Grab handle chip at the right end.
                rect { x: "328", y: "{thresh_y - 7.0:.1}", width: "30", height: "14", rx: "3",
                    fill: "rgba(248,113,113,0.15)", stroke: "rgba(248,113,113,0.5)", stroke_width: "1" }
                text { x: "343", y: "{thresh_y + 3.5:.1}", fill: "#f8b4b4", font_size: "9",
                    text_anchor: "middle", pointer_events: "none", "{threshold:.0}" }
            }

            // dB scale labels down the right edge (legacy placement).
            for (y, label) in grid.iter().copied() {
                div {
                    style: format!(
                        "position:absolute; right:4px; top:{:.0}px; font-size:8px; \
                         color:rgba(255,255,255,0.25); pointer-events:none;",
                        y + 2.0,
                    ),
                    "{label}"
                }
            }

            // Threshold readout, bottom left.
            div {
                style: "position:absolute; bottom:4px; left:8px; display:flex; \
                        flex-direction:column; pointer-events:none;",
                span {
                    style: "font-size:8px; color:#8a8a92; text-transform:uppercase;",
                    "Threshold"
                }
                span {
                    style: "font-family:ui-monospace,monospace; font-size:11px; color:#f8b4b4;",
                    "{threshold:.1} dB"
                }
            }
        }
    }
}
