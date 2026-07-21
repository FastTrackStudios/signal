//! The FTS-Comp editor, detached — a 1:1 port of the comp-plugin layout:
//! the rolling peak waveform fills the panel (teal input fill from the
//! bottom, red gain-reduction fill from the top — Pro-C 3 style), with the
//! transfer curve + threshold line + input ball overlaid, and the two knob
//! rows floating at the bottom (Threshold/Ratio/Attack/Release large,
//! Knee/Range/Fold/Style small, centres column-aligned).
//!
//! The vello painter's geometry, gradients and colors are reproduced in
//! SVG (`audio-gui/src/viz/waveform.rs` is the reference); the telemetry
//! ring streams from the live DSP over `RigEvent::CompWave`, and the GR
//! readout is the detector's real `gain_reduction_db`.

use dioxus::prelude::*;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{BlockParam, LiveBlock};

use crate::knob::{Knob, KnobSize};

/// What a pointer drag on the display is editing.
#[derive(Clone, Copy, PartialEq)]
enum CompDrag {
    /// Vertical drag on the threshold line.
    Threshold,
    /// Vertical drag on the transfer curve above the knee — tilts the slope
    /// (ratio). Stores (start_y_graph, start_ratio).
    Ratio(f32, f32),
}

const W: f64 = 360.0;
const H: f64 = 360.0; // 1:1 — the comp widget is square
/// Waveform dB range. 60 dB: this compressor lives on DI guitar, not line
/// level — the −30 dB neighborhood sits mid-window where thresholds live.
const RANGE_DB: f64 = 60.0;

fn param(block: &LiveBlock, name: &str) -> Option<BlockParam> {
    block.params.iter().find(|p| p.name == name).cloned()
}

fn param_v(block: &LiveBlock, name: &str, dflt: f32) -> f32 {
    param(block, name).map(|p| p.value).unwrap_or(dflt)
}

/// Soft-knee transfer function — same math as audio-gui's `compress_transfer`.
fn compress_transfer(input_db: f32, threshold_db: f32, ratio: f32, knee_db: f32) -> f32 {
    if ratio <= 1.0 {
        return input_db;
    }
    let slope = 1.0 - 1.0 / ratio;
    let half_knee = knee_db * 0.5;
    if knee_db > 0.001 && (input_db - threshold_db).abs() < half_knee {
        let x = input_db - threshold_db + half_knee;
        input_db - slope * x * x / (2.0 * knee_db)
    } else if input_db > threshold_db {
        input_db - slope * (input_db - threshold_db)
    } else {
        input_db
    }
}

/// Smooth path through the samples (Catmull-Rom → cubic Bézier, the
/// painter's `build_smooth_path`), filled to `baseline`.
fn smooth_path(samples: &[f32], w: f64, h: f64, from_bottom: bool, close: bool) -> String {
    let n = samples.len();
    if n == 0 {
        return String::new();
    }
    let step = w / n as f64;
    let ys: Vec<f64> = samples
        .iter()
        .map(|&s| {
            let amp = s.clamp(0.0, 1.0) as f64;
            if from_bottom { h - amp * h } else { amp * h }
        })
        .collect();
    let baseline = if from_bottom { h } else { 0.0 };
    let mut d = String::new();
    if close {
        d.push_str(&format!("M 0 {baseline:.1} L 0 {:.1} ", ys[0]));
    } else {
        d.push_str(&format!("M 0 {:.1} ", ys[0]));
    }
    for i in 0..n - 1 {
        let x0 = i as f64 * step;
        let x1 = (i + 1) as f64 * step;
        let y_prev = if i > 0 { ys[i - 1] } else { ys[0] };
        let (y_curr, y_next) = (ys[i], ys[i + 1]);
        let y_next2 = if i + 2 < n { ys[i + 2] } else { ys[n - 1] };
        let t1 = (y_next - y_prev) / 2.0;
        let t2 = (y_next2 - y_curr) / 2.0;
        d.push_str(&format!(
            "C {:.1} {:.1} {:.1} {:.1} {:.1} {:.1} ",
            x0 + step / 3.0,
            y_curr + t1 / 3.0,
            x1 - step / 3.0,
            y_next - t2 / 3.0,
            x1,
            y_next,
        ));
    }
    if close {
        d.push_str(&format!("L {w:.1} {baseline:.1} Z"));
    }
    d
}

/// dB (0 top of range … −RANGE_DB) → y within the waveform area.
fn db_to_y(db: f64, h: f64) -> f64 {
    ((-db) / RANGE_DB).clamp(0.0, 1.0) * h
}

/// The detached FTS-Comp surface.
#[component]
pub fn CompSurface(
    block: LiveBlock,
    /// Rolling telemetry `(input 0..1, gr 0..1)`, oldest → newest.
    wave: (Vec<f32>, Vec<f32>),
    /// Live input level (dBFS) for the transfer-curve ball.
    in_db: f32,
    /// Real detector gain reduction (dB, positive).
    gr_db: f32,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut svg_el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let mut svg_rect = use_signal(|| None::<(f64, f64, f64)>); // (top, height, _)
    let mut dragging = use_signal(|| None::<CompDrag>);
    let (wave_in, wave_gr) = wave;

    let threshold = param_v(&block, "threshold", -18.0);
    let thr = threshold;
    let ratio = param_v(&block, "ratio", 4.0).max(1.0);
    let knee = param_v(&block, "knee", 6.0);

    // Input peaks arrive linear 0..1 — map through dB so the display is
    // log-scaled like the painter (its levels are pre-scaled; ours are raw).
    let wave_in_scaled: Vec<f32> = wave_in
        .iter()
        .map(|&p| {
            if p <= 0.0 {
                0.0
            } else {
                let db = 20.0 * p.log10();
                (1.0 + db / RANGE_DB as f32).clamp(0.0, 1.0)
            }
        })
        .collect();

    let in_fill = smooth_path(&wave_in_scaled, W, H, true, true);
    let in_edge = smooth_path(&wave_in_scaled, W, H, true, false);
    // GR ring is normalized to 30 dB FS; rescale onto the display range.
    let gr_scaled: Vec<f32> = wave_gr
        .iter()
        .map(|&g| (g * 30.0 / RANGE_DB as f32).clamp(0.0, 1.0))
        .collect();
    let gr_fill = smooth_path(&gr_scaled, W, H, false, true);
    let gr_edge = smooth_path(&gr_scaled, W, H, false, false);

    let thresh_y = db_to_y(threshold as f64, H);

    // Transfer curve overlay across the panel (yellow-green, Pro-C 3).
    let mut tc = String::new();
    for i in 0..=60 {
        let input = -(RANGE_DB) + (i as f64 / 60.0) * RANGE_DB;
        let output = compress_transfer(input as f32, threshold, ratio, knee) as f64;
        let x = (input + RANGE_DB) / RANGE_DB * W;
        let y = db_to_y(output, H);
        tc.push_str(if i == 0 { "M " } else { "L " });
        tc.push_str(&format!("{x:.1} {y:.1} "));
    }
    let ball = {
        let level = in_db.clamp(-(RANGE_DB as f32), 0.0);
        let out = compress_transfer(level, threshold, ratio, knee) as f64;
        let x = (level as f64 + RANGE_DB) / RANGE_DB * W;
        (x, db_to_y(out, H))
    };

    // Knob writes.
    let knob = |name: &'static str,
                label: &'static str,
                size: KnobSize,
                fmt: Option<crate::knob::FmtFn>|
     -> Element {
        let p = param(&block, name);
        let rig = rig.clone();
        let id = block.id.clone();
        match p {
            Some(p) => rsx! {
                Knob {
                    label: label.to_string(),
                    value: p.value,
                    min: p.min,
                    max: p.max,
                    size,
                    fmt,
                    on_change: Callback::new(move |v: f32| {
                        if let Some(r) = rig.clone() {
                            let (id, name) = (id.clone(), name.to_string());
                            spawn(async move { let _ = r.set_block_param(id, name, v).await; });
                        }
                    }),
                }
            },
            None => rsx! {},
        }
    };

    let _fmt_ratio: fn(f32) -> String = |v| format!("{v:.1}:1");
    let fmt_ms = crate::knob::FmtFn(|v| format!("{v:.1}ms"));
    let _fmt_db: fn(f32) -> String = |v| format!("{v:.1}dB");

    rsx! {
        div { class: "relative flex flex-col h-full min-h-0 overflow-hidden",
            style: "background: #080808;",

            // ── The rolling display — grab the threshold line to move it;
            // grab the transfer curve above the knee to tilt the ratio ──
            svg {
                class: "w-full flex-1 min-h-0 touch-none select-none",
                view_box: "0 0 360 360",
                preserve_aspect_ratio: "none",
                onmounted: move |e| svg_el.set(Some(e.data())),
                onpointerdown: {
                    let ratio0 = ratio;
                    move |e: PointerEvent| {
                        let coords = e.element_coordinates();
                        let el = svg_el();
                        spawn(async move {
                            let Some(el) = el else { return };
                            let Ok(rect) = el.get_client_rect().await else { return };
                            svg_rect.set(Some((rect.origin.y, rect.height(), rect.width())));
                            let y = (coords.y / rect.height()) as f32 * H as f32;
                            let ty = db_to_y(thr as f64, H) as f32;
                            if (y - ty).abs() < 22.0 {
                                dragging.set(Some(CompDrag::Threshold));
                            } else if y < ty {
                                // Above the threshold line = the compressed
                                // region — drag tilts the slope.
                                dragging.set(Some(CompDrag::Ratio(y, ratio0)));
                            }
                        });
                    }
                },

                defs {
                    // Input level — dark teal gradient (painter's stops).
                    linearGradient { id: "comp-in", x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0", stop_color: "rgba(20,110,115,0.82)" }
                        stop { offset: "0.45", stop_color: "rgba(14,80,85,0.51)" }
                        stop { offset: "0.80", stop_color: "rgba(8,50,55,0.22)" }
                        stop { offset: "1", stop_color: "rgba(4,25,28,0.06)" }
                    }
                    // Gain reduction — red gradient from the top.
                    linearGradient { id: "comp-gr", x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0", stop_color: "rgba(220,40,40,0.82)" }
                        stop { offset: "0.35", stop_color: "rgba(200,30,30,0.51)" }
                        stop { offset: "0.70", stop_color: "rgba(175,25,25,0.22)" }
                        stop { offset: "1", stop_color: "rgba(150,20,20,0.06)" }
                    }
                }

                // Input waveform: fill + glow edge + bright cyan edge.
                if !in_fill.is_empty() {
                    path { d: "{in_fill}", fill: "url(#comp-in)" }
                    path { d: "{in_edge}", fill: "none", stroke: "rgba(0,200,210,0.12)", stroke_width: "4" }
                    path { d: "{in_edge}", fill: "none", stroke: "rgba(60,210,220,0.78)", stroke_width: "1.5" }
                }
                // GR from the top: fill + glow + bright red edge. Hidden
                // while the detector is idle (a zero trace would still
                // paint its edge line across the top).
                if !gr_fill.is_empty() && gr_scaled.iter().any(|&g| g > 0.002) {
                    path { d: "{gr_fill}", fill: "url(#comp-gr)" }
                    path { d: "{gr_edge}", fill: "none", stroke: "rgba(255,60,60,0.12)", stroke_width: "4" }
                    path { d: "{gr_edge}", fill: "none", stroke: "rgba(255,80,80,0.82)", stroke_width: "1.5" }
                }

                // Threshold line — the grabbable control.
                line { x1: "0", y1: "{thresh_y:.1}", x2: "360", y2: "{thresh_y:.1}",
                    stroke: "rgba(255,120,120,0.55)", stroke_width: "2", stroke_dasharray: "6,4" }
                // Grab handle chip at the right end.
                rect { x: "328", y: "{thresh_y - 7.0:.1}", width: "30", height: "14", rx: "3",
                    fill: "rgba(255,120,120,0.15)", stroke: "rgba(255,120,120,0.5)", stroke_width: "1" }
                text { x: "343", y: "{thresh_y + 3.5:.1}", fill: "#ff9c9c", font_size: "9",
                    text_anchor: "middle", pointer_events: "none", "{thr:.0}" }
                // Transfer curve + input ball.
                path { d: "{tc}", fill: "none", stroke: "rgba(180,210,140,0.71)", stroke_width: "2" }
                circle { cx: "{ball.0:.1}", cy: "{ball.1:.1}", r: "3", fill: "rgba(255,255,255,0.78)" }
            }

            // Drag shield: threshold/ratio keep tracking outside the panel
            // until release.
            if dragging().is_some() {
                div {
                    class: "fixed inset-0",
                    style: "z-index: 1000; cursor: ns-resize;",
                    onpointermove: {
                        let rig = rig.clone();
                        let id = block.id.clone();
                        move |e: PointerEvent| {
                            let Some(mode) = dragging() else { return };
                            let Some((top, h, _)) = svg_rect() else { return };
                            let y = ((e.client_coordinates().y - top) / h) as f32 * H as f32;
                            let Some(r) = rig.clone() else { return };
                            let id = id.clone();
                            match mode {
                                CompDrag::Threshold => {
                                    let db = (-(y / H as f32) * RANGE_DB as f32).clamp(-60.0, 0.0);
                                    spawn(async move {
                                        let _ = r.set_block_param(id, "threshold".into(), db).await;
                                    });
                                }
                                CompDrag::Ratio(y0, r0) => {
                                    // Drag down = more ratio (harder tilt).
                                    let ratio = (r0 * ((y - y0) / 60.0).exp2()).clamp(1.0, 20.0);
                                    spawn(async move {
                                        let _ = r.set_block_param(id, "ratio".into(), ratio).await;
                                    });
                                }
                            }
                        }
                    },
                    onpointerup: move |_| dragging.set(None),
                }
            }

            // GR readout, top right (real detector value).
            div { class: "absolute top-1.5 right-2 flex items-baseline gap-1",
                span { style: "font-size:9px; font-weight:600; text-transform:uppercase; color:#8a8a92;", "GR" }
                span { style: "font-family:ui-monospace,monospace; font-size:12px; color:#ff9c9c;",
                    "{-gr_db:.1}"
                }
            }

            // ── Readouts + the two time knobs (threshold/ratio live on
            // the display itself) ──
            div {
                class: "absolute bottom-0 left-0 right-0 flex items-end px-2 py-1",
                style: "background: linear-gradient(to top, rgba(8,8,8,0.9), transparent);",
                div { class: "flex flex-col",
                    span { style: "font-size:8px; color:#8a8a92; text-transform:uppercase;", "Thr · Ratio" }
                    span { style: "font-family:ui-monospace,monospace; font-size:11px; color:#e8e8ec;",
                        "{thr:.1} dB · {ratio:.1}:1"
                    }
                }
                div { class: "ml-auto flex gap-2",
                    {knob("attack", "Atk", KnobSize::Small, Some(fmt_ms))}
                    {knob("release", "Rel", KnobSize::Small, Some(fmt_ms))}
                }
            }
        }
    }
}
