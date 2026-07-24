//! Macro knob for the layer-zoom panels — a 270° arc with the same drag feel
//! as the guitar rig's knob (150 px per sweep), sized for dense panels.

use std::f64::consts::PI;

use dioxus::prelude::*;

const START_ANGLE: f64 = 135.0;
const SWEEP: f64 = 270.0;
const SENSITIVITY: f64 = 150.0;

fn arc_point(cx: f64, cy: f64, r: f64, deg: f64) -> (f64, f64) {
    let rad = deg * PI / 180.0;
    (cx + r * rad.cos(), cy + r * rad.sin())
}

fn arc_path(cx: f64, cy: f64, r: f64, from: f64, to: f64) -> String {
    let (x1, y1) = arc_point(cx, cy, r, from);
    let (x2, y2) = arc_point(cx, cy, r, to);
    let large = if (to - from).abs() > 180.0 { 1 } else { 0 };
    format!("M {x1:.1} {y1:.1} A {r:.1} {r:.1} 0 {large} 1 {x2:.1} {y2:.1}")
}

/// Format a macro value for its unit.
pub fn fmt_value(value: f32, unit: &str) -> String {
    match unit {
        "Hz" if value >= 1000.0 => format!("{:.1}k", value / 1000.0),
        "Hz" => format!("{value:.0}"),
        "ms" if value >= 1000.0 => format!("{:.2}s", value / 1000.0),
        "ms" => format!("{value:.0}"),
        "dB" => format!("{value:+.1}"),
        "st" => format!("{value:+.0}"),
        "v" => format!("{value:.0}"),
        "" if value.abs() < 10.0 => format!("{value:.2}"),
        _ => format!("{value:.1}"),
    }
}

/// One macro knob. `live: false` renders dimmed — the parameter is authored
/// and persisted but its block has no DSP yet.
#[component]
pub fn Knob(
    label: String,
    value: f32,
    min: f32,
    max: f32,
    unit: String,
    #[props(default = true)] live: bool,
    #[props(default = "#38bdf8".to_string())] accent: String,
    on_change: EventHandler<f32>,
) -> Element {
    let mut drag = use_signal(|| None::<(f64, f32)>);

    let span = (max - min).max(1e-6);
    let norm = ((value - min) / span).clamp(0.0, 1.0);
    let end = START_ANGLE + SWEEP * norm as f64;
    let (cx, cy, r) = (22.0, 22.0, 16.0);
    let track = arc_path(cx, cy, r, START_ANGLE, START_ANGLE + SWEEP);
    let fill = arc_path(cx, cy, r, START_ANGLE, end.max(START_ANGLE + 0.1));
    let (ix, iy) = arc_point(cx, cy, r - 5.0, end);
    let (bx, by) = arc_point(cx, cy, 5.0, end);
    let color = if live { accent.clone() } else { "#52525b".to_string() };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; align-items: center; gap: 2px; \
                    width: 62px; touch-action: none;",
            svg {
                width: "44", height: "44", view_box: "0 0 44 44",
                onpointerdown: move |e: PointerEvent| {
                    drag.set(Some((e.client_coordinates().y, norm)));
                },
                path { d: "{track}", fill: "none", stroke: "#26262b", stroke_width: "3", stroke_linecap: "round" }
                path { d: "{fill}", fill: "none", stroke: "{color}", stroke_width: "3", stroke_linecap: "round" }
                circle { cx: "{cx}", cy: "{cy}", r: "11", fill: "#1a1a1e", stroke: "#2c2c32" }
                line {
                    x1: "{bx}", y1: "{by}", x2: "{ix}", y2: "{iy}",
                    stroke: "{color}", stroke_width: "2", stroke_linecap: "round",
                }
            }
            span {
                style: format!(
                    "font-size: 9px; font-weight: 600; color: {}; text-align: center; line-height: 1.1;",
                    if live { "#d4d4d8" } else { "#52525b" },
                ),
                "{label}"
            }
            span {
                style: "font-size: 9px; font-variant-numeric: tabular-nums; color: #71717a;",
                {format!("{}{}", fmt_value(value, &unit), if unit == "dB" || unit.is_empty() { String::new() } else { format!(" {unit}") })}
            }
            if drag().is_some() {
                div {
                    style: "position: fixed; inset: 0; z-index: 999; cursor: ns-resize;",
                    onpointermove: move |e: PointerEvent| {
                        if let Some((y0, n0)) = drag() {
                            let dy = y0 - e.client_coordinates().y;
                            let next = (n0 as f64 + dy / SENSITIVITY).clamp(0.0, 1.0) as f32;
                            on_change.call(min + next * span);
                        }
                    },
                    onpointerup: move |_| drag.set(None),
                    onpointerleave: move |_| drag.set(None),
                }
            }
        }
    }
}
