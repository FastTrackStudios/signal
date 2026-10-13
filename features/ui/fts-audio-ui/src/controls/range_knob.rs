//! Range knob (iced_audio `RangeKnob` parity).
//!
//! Two-handle rotary control: `min_handle` and `max_handle`. The arc between
//! them fills with the accent color. Vertical drag on the left half adjusts
//! the min handle; right half adjusts the max handle.

use crate::drag::DragState;
use crate::gesture::{self, KNOB_SENSITIVITY};
use crate::param::ParamHandle;
use crate::theme::*;
use dioxus::prelude::*;
use std::f64::consts::PI;

const START_ANGLE: f64 = 135.0;
const SWEEP: f64 = 270.0;
const SENSITIVITY: f64 = KNOB_SENSITIVITY;

fn angle_for(v: f64) -> f64 {
    START_ANGLE + v.clamp(0.0, 1.0) * SWEEP
}

fn arc_pt(cx: f64, cy: f64, r: f64, a: f64) -> (f64, f64) {
    let rad = a * PI / 180.0;
    (cx + r * rad.cos(), cy + r * rad.sin())
}

fn svg_arc(cx: f64, cy: f64, r: f64, s: f64, e: f64) -> String {
    let (x1, y1) = arc_pt(cx, cy, r, s);
    let (x2, y2) = arc_pt(cx, cy, r, e);
    let large = if (e - s).abs() > 180.0 { 1 } else { 0 };
    format!("M {x1:.1} {y1:.1} A {r:.1} {r:.1} 0 {large} 1 {x2:.1} {y2:.1}")
}

#[component]
pub fn RangeKnob(
    min_handle: ParamHandle,
    max_handle: ParamHandle,
    #[props(default = 48)] diameter: u32,
    #[props(default)] label: Option<String>,
    #[props(default)] color: Option<String>,
    #[props(default)] disabled: bool,
) -> Element {
    let mut drag: Signal<DragState> = use_context();
    let _ = drag.read().move_count;

    let lo = min_handle.normalized() as f64;
    let hi = max_handle.normalized() as f64;

    let df = diameter as f64;
    let cx = df / 2.0;
    let cy = df / 2.0;
    let r = df / 2.0 - 4.0;

    let track = svg_arc(cx, cy, r, START_ANGLE, START_ANGLE + SWEEP);
    let lo_a = angle_for(lo);
    let hi_a = angle_for(hi);
    let fill = svg_arc(cx, cy, r, lo_a.min(hi_a), lo_a.max(hi_a));

    let (lo_x, lo_y) = arc_pt(cx, cy, r, lo_a);
    let (hi_x, hi_y) = arc_pt(cx, cy, r, hi_a);

    let accent = color.as_deref().unwrap_or(ACCENT);
    let opacity = if disabled { "0.5" } else { "1.0" };

    rsx! {
        div {
            style: format!(
                "display:inline-flex; flex-direction:column; align-items:center; gap:4px; \
                 opacity:{opacity}; position:relative;"
            ),
            svg {
                width: "{diameter}", height: "{diameter}", view_box: "0 0 {df} {df}",
                path { d: "{track}", fill: "none", stroke: "{BORDER}", stroke_width: "3.5", stroke_linecap: "round" }
                path { d: "{fill}",  fill: "none", stroke: "{accent}", stroke_width: "4",   stroke_linecap: "round" }
                circle { cx: "{lo_x:.1}", cy: "{lo_y:.1}", r: "3", fill: "{TEXT}" }
                circle { cx: "{hi_x:.1}", cy: "{hi_y:.1}", r: "3", fill: "{TEXT}" }
            }

            if !disabled {
                div {
                    style: "position:absolute; left:0; top:0; width:50%; height:100%; cursor:ns-resize;",
                    onmousedown: {
                        let h = min_handle.clone();
                        move |evt: MouseEvent| {
                            let _ = gesture::press_vertical(&evt, &mut drag, &h, SENSITIVITY);
                        }
                    },
                    ondoubleclick: {
                        let h = min_handle.clone();
                        move |_| gesture::double_click(&mut drag, &h)
                    },
                    onwheel: {
                        let h = min_handle.clone();
                        move |evt: WheelEvent| gesture::wheel(&evt, &h)
                    },
                }
                div {
                    style: "position:absolute; right:0; top:0; width:50%; height:100%; cursor:ns-resize;",
                    onmousedown: {
                        let h = max_handle.clone();
                        move |evt: MouseEvent| {
                            let _ = gesture::press_vertical(&evt, &mut drag, &h, SENSITIVITY);
                        }
                    },
                    ondoubleclick: {
                        let h = max_handle.clone();
                        move |_| gesture::double_click(&mut drag, &h)
                    },
                    onwheel: {
                        let h = max_handle.clone();
                        move |evt: WheelEvent| gesture::wheel(&evt, &h)
                    },
                }
            }

            if let Some(lbl) = label {
                span { style: format!("font-size:10px; color:{TEXT_DIM};"), "{lbl}" }
            }
        }
    }
}
