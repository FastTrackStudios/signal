//! The FTS-Comp editor, detached — a 1:1 port of the comp-plugin layout:
//! the rolling peak waveform fills the panel (neutral grey input fill from the
//! bottom, red gain-reduction fill from the top — Pro-C 3 style), with the
//! transfer curve + threshold line + input ball overlaid, and the two knob
//! rows floating at the bottom (Threshold/Ratio/Attack/Release large,
//! Knee/Range/Fold/Style small, centres column-aligned).
//!
//! The vello painter's geometry, gradients and colors are reproduced in
//! SVG (`audio-gui/src/viz/waveform.rs` is the reference); the telemetry
//! ring streams from the live DSP over `RigEvent::CompWave`, and the GR
//! readout is the detector's real `gain_reduction_db`.

use std::fmt::Write;

use dioxus::prelude::*;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{BlockParam, LiveBlock};

use dioxus::html::input_data::MouseButton;

use crate::knob::{Knob, KnobSize};

/// What a pointer drag on the display is editing.
#[derive(Clone, Copy, PartialEq)]
enum CompDrag {
    /// Vertical drag on the threshold line.
    Threshold,
    /// Vertical drag on the transfer curve above the knee — tilts the slope
    /// (ratio). Stores (`start_y_graph`, `start_ratio`).
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
    param(block, name).map_or(dflt, |p| p.value)
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
            let amp = f64::from(s.clamp(0.0, 1.0));
            if from_bottom { h - amp * h } else { amp * h }
        })
        .collect();
    let baseline = if from_bottom { h } else { 0.0 };
    let mut d = String::new();
    if close {
        let _ = write!(d, "M 0 {baseline:.1} L 0 {:.1} ", ys[0]);
    } else {
        let _ = write!(d, "M 0 {:.1} ", ys[0]);
    }
    for i in 0..n - 1 {
        let x0 = i as f64 * step;
        let x1 = (i + 1) as f64 * step;
        let y_prev = if i > 0 { ys[i - 1] } else { ys[0] };
        let (y_curr, y_next) = (ys[i], ys[i + 1]);
        let y_next2 = if i + 2 < n { ys[i + 2] } else { ys[n - 1] };
        let t1 = (y_next - y_prev) / 2.0;
        let t2 = (y_next2 - y_curr) / 2.0;
        let _ = write!(
            d,
            "C {:.1} {:.1} {:.1} {:.1} {:.1} {:.1} ",
            x0 + step / 3.0,
            y_curr + t1 / 3.0,
            x1 - step / 3.0,
            y_next - t2 / 3.0,
            x1,
            y_next,
        );
    }
    if close {
        let _ = write!(d, "L {w:.1} {baseline:.1} Z");
    }
    d
}

/// How close to the threshold line counts as grabbing it, in graph units.
const GRAB_PX: f32 = 22.0;

/// A pointer's y, in graph units, or `None` before the widget has painted.
///
/// `element_coordinates()` is already element-local and in CSS pixels; the
/// panel's height comes from the widget's own box. Nothing here is
/// asynchronous, which is the entire point — the press that started a drag
/// used to be decided inside an `await`, so it landed a frame late with its
/// first movement already gone.
fn graph_y(metrics: &comp_ui::viz::MetricsHandle, element_y: f64) -> Option<f32> {
    let h = metrics.get().css_height();
    if h < 1.0 {
        return None;
    }
    Some((element_y / h) as f32 * H as f32)
}

/// dB (0 top of range … −`RANGE_DB`) → y within the waveform area.
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
    // The widget publishes its own box here every paint, so a pointer maps
    // into graph space synchronously. The previous arrangement measured the
    // element with `get_client_rect().await` on pointer-down, which meant
    // the gesture did not know whether it had grabbed anything until the
    // await resolved — the press was dead for a frame or more, the first
    // movement was dropped, and the cached rect went stale for the rest of
    // the drag. `eq_graph` hit the same thing and says so in its own source.
    let metrics = use_hook(comp_ui::viz::MetricsHandle::new);
    let mut dragging = use_signal(|| None::<CompDrag>);
    // Whether the pointer is close enough to the threshold to take it. A
    // control you can grab should look like one before you try.
    let mut hot = use_signal(|| false);
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

    // GR ring is normalized to 30 dB FS; rescale onto the display range.
    let gr_scaled: Vec<f32> = wave_gr
        .iter()
        .map(|&g| (g * 30.0 / RANGE_DB as f32).clamp(0.0, 1.0))
        .collect();

    // The only geometry this component still computes itself: where to put
    // the threshold's grab chip and its number. The traces, the transfer
    // curve and the ball are `comp_ui::viz`'s, drawn on the GPU.
    let thresh_y = db_to_y(f64::from(threshold), H);

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

            // The picture, drawn by the compressor itself — the same widget
            // the plugin mounts, so the rig and the plugin cannot disagree
            // about what this block looks like. It paints and does not
            // listen; the svg above it owns every gesture.
            div { style: "position:absolute; inset:0;",
                comp_ui::viz::CompViz {
                    threshold,
                    ratio,
                    knee,
                    in_db,
                    gr_db,
                    wave: (wave_in_scaled.clone(), gr_scaled.clone()),
                    on: true,
                    grabbable: hot() || dragging().is_some(),
                    metrics: metrics.clone(),
                    // Grey, not a hue: the input is the signal itself rather
                    // than an effect's contribution to it.
                    color: [228u8, 228u8, 231u8],
                }
            }

            // ── The gestures — grab the threshold line to move it; grab the
            // transfer curve above the knee to tilt the ratio ──
            svg {
                class: "w-full flex-1 min-h-0 touch-none select-none",
                view_box: "0 0 360 360",
                preserve_aspect_ratio: "none",
                // Everything below is synchronous. `element_coordinates()`
                // is already element-local, and the panel's height comes
                // from the widget's last paint, so a press is decided in the
                // handler that received it.
                onpointerdown: {
                    let ratio0 = ratio;
                    let metrics = metrics.clone();
                    move |e: PointerEvent| {
                        let Some(y) = graph_y(&metrics, e.element_coordinates().y) else {
                            return;
                        };
                        let ty = db_to_y(f64::from(thr), H) as f32;
                        if (y - ty).abs() < GRAB_PX {
                            dragging.set(Some(CompDrag::Threshold));
                        } else if y < ty {
                            // Above the threshold line = the compressed
                            // region — drag tilts the slope.
                            dragging.set(Some(CompDrag::Ratio(y, ratio0)));
                        }
                        e.prevent_default();
                    }
                },
                onpointermove: {
                    let rig = rig.clone();
                    let id = block.id.clone();
                    let metrics = metrics.clone();
                    move |e: PointerEvent| {
                        let Some(y) = graph_y(&metrics, e.element_coordinates().y) else {
                            return;
                        };
                        let Some(mode) = *dragging.read() else {
                            // Not dragging: light the threshold when it is
                            // within reach, so the control announces itself.
                            let ty = db_to_y(f64::from(thr), H) as f32;
                            let near = (y - ty).abs() < GRAB_PX;
                            if near != *hot.peek() {
                                hot.set(near);
                            }
                            return;
                        };
                        // Released outside the panel: end the gesture on
                        // re-entry rather than resuming it (eq_graph idiom).
                        if !e.held_buttons().contains(MouseButton::Primary) {
                            dragging.set(None);
                            return;
                        }
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
                                // Drag down = more ratio (harder tilt). A
                                // doubling per eighth of the panel: at a
                                // sixth it took a twitch to cross the whole
                                // 1:1..20:1 range.
                                let span = H as f32 / 8.0;
                                let ratio = (r0 * ((y - y0) / span).exp2()).clamp(1.0, 20.0);
                                spawn(async move {
                                    let _ = r.set_block_param(id, "ratio".into(), ratio).await;
                                });
                            }
                        }
                    }
                },
                onpointerup: move |_| dragging.set(None),
                onpointerleave: move |_| {
                    if hot() {
                        hot.set(false);
                    }
                },

                // The traces, the transfer curve, the threshold rule and the
                // ball are all painted by `CompViz` below this svg. What is
                // left here is what a painted scene cannot do: the grab
                // handle's NUMBER. Text in a scene needs a font handle the
                // widget has not got, and the threshold is the one value on
                // this panel that has to be readable while it is dragged.
                rect { x: "328", y: "{thresh_y - 7.0:.1}", width: "30", height: "14", rx: "3",
                    fill: "rgba(255,120,120,0.15)", stroke: "rgba(255,120,120,0.5)", stroke_width: "1" }
                text { x: "343", y: "{thresh_y + 3.5:.1}", fill: "#ff9c9c", font_size: "9",
                    text_anchor: "middle", pointer_events: "none", "{thr:.0}" }
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

#[cfg(test)]
mod tests {
    use super::*;
    use comp_ui::viz::{CompMetrics, MetricsHandle};

    fn metrics(css_height: f32) -> MetricsHandle {
        // Two physical pixels per CSS pixel, so the test would catch a
        // mapping that forgot the scale — which is the mistake that makes a
        // drag track at half speed on a HiDPI screen and nowhere else.
        MetricsHandle::from(CompMetrics {
            width: 800.0,
            height: css_height * 2.0,
            scale: 2.0,
        })
    }

    /// The top of the panel is 0 dB and the bottom is the full range, in the
    /// pointer's own units.
    #[test]
    fn a_pointer_maps_through_the_widgets_own_box() {
        let m = metrics(500.0);
        let top = graph_y(&m, 0.0).expect("measured");
        let bottom = graph_y(&m, 500.0).expect("measured");
        assert!((top - 0.0).abs() < 1e-3);
        assert!((bottom - H as f32).abs() < 1e-3);
        // Halfway down is halfway through the range.
        let mid = graph_y(&m, 250.0).expect("measured");
        assert!((mid - H as f32 / 2.0).abs() < 1e-2, "got {mid}");
    }

    /// Before the first paint there is no box, and a gesture must decline
    /// rather than invent one — a drag mapped through a guessed height jumps
    /// the parameter the moment it starts.
    #[test]
    fn an_unmeasured_panel_refuses_to_map() {
        assert!(graph_y(&MetricsHandle::new(), 120.0).is_none());
    }

    /// Grabbing the threshold is a synchronous decision made from the press
    /// itself. This is the shape of that decision: near the line takes it,
    /// above the line tilts the slope, below it does nothing.
    #[test]
    fn the_press_decides_what_it_grabbed() {
        let m = metrics(500.0);
        let thr = -20.0_f64;
        let ty = db_to_y(thr, H) as f32;
        // The threshold's y in graph units, back into CSS pixels.
        let css_of = |graph_y_units: f32| f64::from(graph_y_units) / H * 500.0;

        let on_line = graph_y(&m, css_of(ty)).expect("measured");
        assert!((on_line - ty).abs() < GRAB_PX, "the line itself is grabbable");

        let just_above = graph_y(&m, css_of(ty) - 60.0).expect("measured");
        assert!(
            (just_above - ty).abs() >= GRAB_PX && just_above < ty,
            "well above the line is the compressed region, not the handle"
        );

        let well_below = graph_y(&m, css_of(ty) + 120.0).expect("measured");
        assert!(
            (well_below - ty).abs() >= GRAB_PX && well_below > ty,
            "below the line is neither"
        );
    }

    /// A doubling per eighth of the panel. At a sixth the whole 1:1..20:1
    /// range crossed in a twitch, which is what made the tilt feel unusable.
    #[test]
    fn the_ratio_tilt_doubles_over_an_eighth_of_the_panel() {
        let span = H as f32 / 8.0;
        let doubled = 4.0_f32 * ((span) / span).exp2();
        assert!((doubled - 8.0).abs() < 1e-3, "got {doubled}");
        // And the full useful range needs most of the panel, not a flick.
        let from_one = 1.0_f32 * ((H as f32 * 0.5) / span).exp2();
        assert!(from_one > 15.0, "half the panel should reach the top: {from_one}");
    }
}
