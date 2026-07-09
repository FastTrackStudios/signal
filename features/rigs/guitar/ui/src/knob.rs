//! Rotary knob — a 1:1 port of audio-gui's `Knob` (FTS plugin editors) for
//! the detached remotes: same 270° arc geometry (135° start), same 3D body
//! with the arc ring + indicator line, same label/value stack and drag feel
//! (150 px per sweep, wheel steps). The nih-plug `ParamPtr` binding is
//! replaced by plain value/on_change props writing over the vox link.

use std::f64::consts::PI;

use dioxus::prelude::*;

/// Knob display size — audio-gui's diameters.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum KnobSize {
    /// Strip-embedded: 24 px, pairs with hidden value readouts.
    Tiny,
    Small,
    #[default]
    Medium,
    Large,
}

impl KnobSize {
    fn diameter(self) -> f64 {
        match self {
            Self::Tiny => 24.0,
            Self::Small => 32.0,
            Self::Medium => 48.0,
            Self::Large => 64.0,
        }
    }
    fn body_diameter(self) -> f64 {
        match self {
            Self::Tiny => 15.0,
            Self::Small => 20.0,
            Self::Medium => 30.0,
            Self::Large => 42.0,
        }
    }
    fn arc_stroke(self) -> f64 {
        match self {
            Self::Tiny => 2.5,
            Self::Small => 3.0,
            Self::Medium => 3.5,
            Self::Large => 4.0,
        }
    }
    fn track_stroke(self) -> f64 {
        match self {
            Self::Tiny => 2.0,
            Self::Small => 2.5,
            Self::Medium => 3.0,
            Self::Large => 3.5,
        }
    }
}

// Arc geometry: 270° sweep from 135° (7 o'clock) to 405° (5 o'clock).
const START_ANGLE: f64 = 135.0;
const SWEEP: f64 = 270.0;
/// Pixels of vertical drag per full 0→1 sweep.
const SENSITIVITY: f64 = 150.0;

// audio-gui dark theme values.
const ACCENT: &str = "#8fa8c8";
const KNOB_TRACK: &str = "#252528";
const BODY_LIGHT: &str = "#333338";
const BODY_DARK: &str = "#1c1c20";

fn arc_point(cx: f64, cy: f64, r: f64, angle_deg: f64) -> (f64, f64) {
    let rad = angle_deg * PI / 180.0;
    (cx + r * rad.cos(), cy + r * rad.sin())
}

fn svg_arc(cx: f64, cy: f64, r: f64, start_deg: f64, end_deg: f64) -> String {
    let (x1, y1) = arc_point(cx, cy, r, start_deg);
    let (x2, y2) = arc_point(cx, cy, r, end_deg);
    let large = if (end_deg - start_deg).abs() > 180.0 { 1 } else { 0 };
    format!("M {x1:.1} {y1:.1} A {r:.1} {r:.1} 0 {large} 1 {x2:.1} {y2:.1}")
}

fn angle_for_value(v: f64) -> f64 {
    START_ANGLE + v.clamp(0.0, 1.0) * SWEEP
}

/// A rotary knob over a plain value. `value` is in real units; the arc maps
/// through `min..max`. Drag vertically (pointer-captured), wheel for steps.
#[component]
pub fn Knob(
    label: String,
    value: f32,
    min: f32,
    max: f32,
    on_change: Callback<f32>,
    #[props(default)] size: KnobSize,
    /// Hide the numeric readout (tight strips show label only).
    #[props(default)] hide_value: bool,
    /// Value formatter override (default `{:.1}`).
    #[props(default)] fmt: Option<fn(f32) -> String>,
    /// Accent color override.
    #[props(default)] color: Option<String>,
) -> Element {
    // Drag state: (start_y, start_normalized) while a drag is live.
    let mut drag = use_signal(|| None::<(f64, f64)>);

    let range = (max - min).max(1e-6);
    let val = ((value - min) / range).clamp(0.0, 1.0) as f64;

    let d = size.diameter();
    let body_d = size.body_diameter();
    let (cx, cy) = (d / 2.0, d / 2.0);
    let r = d / 2.0 - 3.0;

    let track_path = svg_arc(cx, cy, r, START_ANGLE, START_ANGLE + SWEEP);
    let end_angle = angle_for_value(val);
    let value_path = if val > 0.001 {
        svg_arc(cx, cy, r, START_ANGLE, end_angle)
    } else {
        String::new()
    };

    // Indicator line on the body — inner 25% to outer 85% of the body radius.
    let body_r = body_d / 2.0;
    let (ix1, iy1) = arc_point(cx, cy, body_r * 0.25, end_angle);
    let (ix2, iy2) = arc_point(cx, cy, body_r * 0.85, end_angle);

    let accent = color.unwrap_or_else(|| ACCENT.to_string());
    let display = match fmt {
        Some(f) => f(value),
        // Adaptive precision: fine params (mix, Q) need two decimals; big
        // ones (ms, Hz) don't.
        None => {
            if value.abs() >= 100.0 {
                format!("{value:.0}")
            } else if value.abs() >= 10.0 {
                format!("{value:.1}")
            } else {
                format!("{value:.2}")
            }
        }
    };

    let apply = move |norm: f64| {
        on_change.call(min + (norm.clamp(0.0, 1.0) as f32) * range);
    };

    rsx! {
        div {
            class: "flex flex-col items-center select-none",
            style: "gap: 2px; cursor: ns-resize; touch-action: none;",

            div {
                style: "position: relative; width: {d}px; height: {d}px; \
                        display: flex; align-items: center; justify-content: center;",
                onpointerdown: move |e: PointerEvent| {
                    drag.set(Some((e.client_coordinates().y, val)));
                },
                onwheel: move |e: WheelEvent| {
                    let step = if e.delta().strip_units().y < 0.0 { 0.02 } else { -0.02 };
                    apply(val + step);
                },
                ondoubleclick: move |_| {
                    // Reset to the range midpoint (no default plumbed yet).
                    apply(0.5);
                },

                // Knob body — 3D circle with lighting (audio-gui styling).
                div {
                    style: "width: {body_d}px; height: {body_d}px; border-radius: 50%; \
                            background: linear-gradient(145deg, {BODY_LIGHT}, {BODY_DARK}); \
                            box-shadow: 0 2px 6px rgba(0,0,0,0.5), \
                              inset 0 1px 1px rgba(255,255,255,0.07), \
                              inset 0 -1px 1px rgba(0,0,0,0.25); \
                            border: 1px solid rgba(255,255,255,0.04); \
                            position: absolute; z-index: 1;",
                }

                svg {
                    width: "{d}",
                    height: "{d}",
                    view_box: "0 0 {d} {d}",
                    style: "position: absolute; z-index: 2; pointer-events: none;",
                    path {
                        d: "{track_path}",
                        fill: "none",
                        stroke: "{KNOB_TRACK}",
                        stroke_width: "{size.track_stroke()}",
                        stroke_linecap: "round",
                    }
                    if !value_path.is_empty() {
                        path {
                            d: "{value_path}",
                            fill: "none",
                            stroke: "{accent}",
                            stroke_width: "{size.arc_stroke()}",
                            stroke_linecap: "round",
                        }
                    }
                    line {
                        x1: "{ix1:.1}", y1: "{iy1:.1}", x2: "{ix2:.1}", y2: "{iy2:.1}",
                        stroke: "#e8e8ec",
                        stroke_width: "2",
                        stroke_linecap: "round",
                    }
                }
            }

            span {
                style: "font-size: 8px; font-weight: 600; text-transform: uppercase; \
                        letter-spacing: 0.04em; color: #8a8a92; max-width: 64px; \
                        overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                "{label}"
            }
            // Drag shield: while a drag is live, a fullscreen layer owns the
            // pointer — moving off the knob no longer drops the gesture.
            if drag().is_some() {
                div {
                    class: "fixed inset-0",
                    style: "z-index: 1000; cursor: ns-resize;",
                    onpointermove: move |e: PointerEvent| {
                        if let Some((y0, v0)) = drag() {
                            let dy = y0 - e.client_coordinates().y;
                            apply(v0 + dy / SENSITIVITY);
                        }
                    },
                    onpointerup: move |_| drag.set(None),
                }
            }

            if !hide_value {
                span {
                    style: "font-family: ui-monospace, monospace; font-size: 10px; \
                            font-variant-numeric: tabular-nums; color: #e8e8ec; \
                            min-width: 36px; text-align: center;",
                    "{display}"
                }
            }
        }
    }
}
