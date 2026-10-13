//! Ramp curve editor (iced_audio `Ramp` parity).
//!
//! Renders a curve from one corner to the opposite corner and lets the user
//! drag vertically to reshape it. Normalized value 0.5 = linear, <0.5 = ease
//! out, >0.5 = ease in. Direction selects which diagonal the curve follows.

use crate::drag::DragState;
use crate::gesture::{self, KNOB_SENSITIVITY};
use crate::param::ParamHandle;
use crate::theme::*;
use dioxus::prelude::*;

const SENSITIVITY: f64 = KNOB_SENSITIVITY;
const SAMPLES: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RampDirection {
    /// Bottom-left to top-right.
    #[default]
    Up,
    /// Top-left to bottom-right.
    Down,
}

#[component]
pub fn Ramp(
    handle: ParamHandle,
    #[props(default)] direction: RampDirection,
    #[props(default = 96)] width: u32,
    #[props(default = 64)] height: u32,
    #[props(default)] color: Option<String>,
    #[props(default)] disabled: bool,
) -> Element {
    let mut drag: Signal<DragState> = use_context();
    let _ = drag.read().move_count;

    let n = handle.normalized().clamp(0.0, 1.0) as f64;
    // Map [0,1] → exponent in [0.25, 4.0]. 0.5 → 1.0 (linear).
    let exponent = 4.0_f64.powf((n - 0.5) * 2.0);

    let w = width as f64;
    let h = height as f64;

    let mut points = String::new();
    for i in 0..=SAMPLES {
        let t = i as f64 / SAMPLES as f64;
        let y_norm = t.powf(exponent);
        let x = t * w;
        let y = match direction {
            RampDirection::Up => h - y_norm * h,
            RampDirection::Down => y_norm * h,
        };
        if i > 0 {
            points.push(' ');
        }
        points.push_str(&format!("{x:.1},{y:.1}"));
    }

    let accent = color.as_deref().unwrap_or(ACCENT);
    let opacity = if disabled { "0.5" } else { "1.0" };
    let cursor = if disabled { "not-allowed" } else { "ns-resize" };

    rsx! {
        div {
            style: format!(
                "position:relative; width:{width}px; height:{height}px; \
                 border:1px solid {BORDER}; border-radius:4px; background:{SURFACE}; \
                 opacity:{opacity}; cursor:{cursor}; user-select:none;"
            ),
            onmousedown: {
                let handle = handle.clone();
                move |evt: MouseEvent| {
                    if disabled { return; }
                    let _ = gesture::press_vertical(&evt, &mut drag, &handle, SENSITIVITY);
                }
            },
            ondoubleclick: {
                let handle = handle.clone();
                move |_| if !disabled { gesture::double_click(&mut drag, &handle) }
            },
            onwheel: {
                let handle = handle.clone();
                move |evt: WheelEvent| if !disabled { gesture::wheel(&evt, &handle) }
            },

            svg {
                width: "{width}", height: "{height}", view_box: "0 0 {w} {h}",
                polyline {
                    points: "{points}",
                    fill: "none",
                    stroke: "{accent}",
                    stroke_width: "2",
                }
            }
        }
    }
}
