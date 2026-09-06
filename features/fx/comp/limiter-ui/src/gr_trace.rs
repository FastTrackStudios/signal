//! The limiter's scrolling gain-reduction trace.
//!
//! Reads the two history rings the audio thread writes and emits SVG from
//! [`crate::gr_trace_svg`]'s geometry. Display only — the limiter has nothing
//! to drag here, unlike the compressor's transfer curve.

use audiocore_core::prelude::*;
use fts_plug_ui::prelude::*;
use nice_plug_dioxus::SharedState;
use std::sync::atomic::Ordering;

use crate::gr_trace_svg::{
    GR_RANGE_DB, TRACE_H, TRACE_W, gr_area_path, gr_gridlines, waveform_path,
};
use crate::params::LimiterUiState;

pub use crate::gr_trace_svg::TRACE_H as GRAPH_H;

/// `frame` is not read — it exists to defeat memoization.
///
/// Dioxus skips re-rendering a component whose props compare equal, and this
/// component's only real prop (`skin`) never changes. Its data comes from the
/// audio thread through atomics, which Dioxus cannot see, so without a prop
/// that moves every tick the trace renders once and then sits frozen no matter
/// what the limiter is doing. Feeding the redraw counter in makes the props
/// differ each tick, which is what actually drives the animation.
#[component]
pub fn GrTrace(skin: Skin, frame: u64) -> Element {
    let _ = frame;
    let shared = use_context::<SharedState>();
    let ui = shared
        .get::<LimiterUiState>()
        .expect("LimiterUiState missing");

    let gr = ui.gr_wave.snapshot();
    let peaks = ui.output_wave.snapshot();
    let gr_now = ui.gain_reduction_db.load(Ordering::Relaxed);

    let wave_d = waveform_path(&peaks, TRACE_W, TRACE_H);
    let gr_d = gr_area_path(&gr, TRACE_W, TRACE_H);
    let grid = gr_gridlines(TRACE_H);

    rsx! {
        svg {
            width: "100%",
            height: "100%",
            view_box: "0 0 {TRACE_W} {TRACE_H}",
            preserve_aspect_ratio: "none",

            rect { x: 0, y: 0, width: TRACE_W, height: TRACE_H, fill: "rgba(0,0,0,0.28)" }

            // dB gridlines for the reduction scale.
            for (y, label) in grid.iter() {
                line {
                    x1: 0, y1: "{y}", x2: TRACE_W, y2: "{y}",
                    stroke: "rgba(255,255,255,0.08)", stroke_width: 1,
                }
                text {
                    x: 6, y: "{y - 3.0}",
                    fill: "rgba(255,255,255,0.35)", font_size: 9,
                    "{label}"
                }
            }

            // Output waveform behind the reduction.
            if !wave_d.is_empty() {
                path {
                    "data-testid": "limiter-waveform",
                    d: "{wave_d}",
                    fill: "rgba(255,255,255,0.16)",
                }
            }

            // Gain reduction hanging from the top edge.
            if !gr_d.is_empty() {
                path {
                    "data-testid": "limiter-gr-area",
                    d: "{gr_d}",
                    fill: "{skin.accent}",
                    fill_opacity: 0.30,
                    stroke: "{skin.accent}",
                    stroke_width: 1.2,
                }
            }

            text {
                x: TRACE_W - 8.0, y: 14,
                text_anchor: "end",
                fill: "{skin.accent}", font_size: 11, font_weight: 700,
                "GR {gr_now:.1} dB"
            }
            text {
                x: TRACE_W - 8.0, y: TRACE_H - 6.0,
                text_anchor: "end",
                fill: "rgba(255,255,255,0.35)", font_size: 9,
                "range {GR_RANGE_DB:.0} dB"
            }
        }
    }
}
