//! Rotary knob widget — arc-style with optional modulation range overlay.
//!
//! Renders an SVG arc knob with drag-to-rotate interaction. The arc spans
//! 270° (from 7 o'clock to 5 o'clock) with the zero point at 7 o'clock.

use dioxus::prelude::*;
use std::f64::consts::PI;

/// Knob display size.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KnobSize {
    Small,
    Medium,
    Large,
}

impl Default for KnobSize {
    fn default() -> Self {
        Self::Medium
    }
}

impl KnobSize {
    fn diameter(self) -> u32 {
        match self {
            Self::Small => 32,
            Self::Medium => 48,
            Self::Large => 64,
        }
    }
}

/// A rotary knob control.
#[derive(Props, Clone, PartialEq)]
pub struct KnobProps {
    /// Current normalized value (0.0–1.0).
    #[props(default = 0.5)]
    value: f64,

    /// Optional modulation range minimum (0.0–1.0).
    #[props(default)]
    mod_min: Option<f64>,

    /// Optional modulation range maximum (0.0–1.0).
    #[props(default)]
    mod_max: Option<f64>,

    /// Display size.
    #[props(default)]
    size: KnobSize,

    /// Whether the knob is disabled.
    #[props(default)]
    disabled: bool,

    /// Label shown below the knob.
    #[props(default)]
    label: Option<String>,

    /// Formatted display value (e.g. "50%", "-12 dB").
    #[props(default)]
    display_value: Option<String>,

    /// Callback when value changes.
    #[props(default)]
    on_change: Option<Callback<f64>>,

    /// Accent color for the value arc and thumb (e.g. "#F97316").
    /// Falls back to `var(--primary)` when not set.
    #[props(default)]
    color: Option<String>,

    /// Extra CSS classes.
    #[props(default)]
    class: String,
}

// Arc geometry: 270° sweep from 135° (7 o'clock) to 405° (5 o'clock)
const START_ANGLE: f64 = 135.0;
const SWEEP: f64 = 270.0;

fn angle_for_value(v: f64) -> f64 {
    START_ANGLE + v.clamp(0.0, 1.0) * SWEEP
}

fn arc_point(cx: f64, cy: f64, r: f64, angle_deg: f64) -> (f64, f64) {
    let rad = angle_deg * PI / 180.0;
    (cx + r * rad.cos(), cy + r * rad.sin())
}

fn svg_arc(cx: f64, cy: f64, r: f64, start_deg: f64, end_deg: f64) -> String {
    let (x1, y1) = arc_point(cx, cy, r, start_deg);
    let (x2, y2) = arc_point(cx, cy, r, end_deg);
    let large = if (end_deg - start_deg).abs() > 180.0 {
        1
    } else {
        0
    };
    format!("M {x1:.1} {y1:.1} A {r:.1} {r:.1} 0 {large} 1 {x2:.1} {y2:.1}")
}

#[component]
pub fn Knob(props: KnobProps) -> Element {
    let d = props.size.diameter();
    let df = d as f64;
    let cx = df / 2.0;
    let cy = df / 2.0;
    let r = df / 2.0 - 4.0; // Inset for stroke width
    let val = props.value.clamp(0.0, 1.0);

    // Track arc (full background)
    let track_path = svg_arc(cx, cy, r, START_ANGLE, START_ANGLE + SWEEP);

    // Value arc (filled portion)
    let end_angle = angle_for_value(val);
    let value_path = if val > 0.001 {
        svg_arc(cx, cy, r, START_ANGLE, end_angle)
    } else {
        String::new()
    };

    // Modulation overlay arc
    let mod_path = match (props.mod_min, props.mod_max) {
        (Some(lo), Some(hi)) => {
            let lo_angle = angle_for_value(lo.clamp(0.0, 1.0));
            let hi_angle = angle_for_value(hi.clamp(0.0, 1.0));
            svg_arc(cx, cy, r - 2.0, lo_angle, hi_angle)
        }
        _ => String::new(),
    };

    // Thumb indicator line
    let (tx, ty) = arc_point(cx, cy, r - 6.0, end_angle);
    let (tx2, ty2) = arc_point(cx, cy, r + 1.0, end_angle);

    let accent = props.color.as_deref().unwrap_or("var(--primary, #3b82f6)");

    let disabled_class = if props.disabled {
        " opacity-50 cursor-not-allowed"
    } else {
        " cursor-pointer"
    };

    rsx! {
        div {
            class: format!("inline-flex flex-col items-center gap-1{disabled_class} {}", props.class),

            svg {
                width: "{d}",
                height: "{d}",
                view_box: "0 0 {df} {df}",

                // Track
                path {
                    d: "{track_path}",
                    fill: "none",
                    stroke: "var(--muted, #374151)",
                    stroke_width: "3.5",
                    stroke_linecap: "round",
                }

                // Value arc
                if !value_path.is_empty() {
                    path {
                        d: "{value_path}",
                        fill: "none",
                        stroke: "{accent}",
                        stroke_width: "4",
                        stroke_linecap: "round",
                    }
                }

                // Modulation overlay
                if !mod_path.is_empty() {
                    path {
                        d: "{mod_path}",
                        fill: "none",
                        stroke: "var(--signal-mod, #8b5cf6)",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        opacity: "0.6",
                    }
                }

                // Thumb indicator
                line {
                    x1: "{tx:.1}",
                    y1: "{ty:.1}",
                    x2: "{tx2:.1}",
                    y2: "{ty2:.1}",
                    stroke: "var(--foreground, #e5e7eb)",
                    stroke_width: "2",
                    stroke_linecap: "round",
                }
            }

            // Hidden range input for interaction
            input {
                r#type: "range",
                class: "absolute inset-0 opacity-0 cursor-pointer",
                min: "0",
                max: "1",
                step: "0.005",
                value: "{val}",
                disabled: props.disabled,
                oninput: move |evt: FormEvent| {
                    if let Ok(v) = evt.value().parse::<f64>() {
                        if let Some(cb) = &props.on_change {
                            cb.call(v.clamp(0.0, 1.0));
                        }
                    }
                },
            }

            // Display value
            if let Some(display) = &props.display_value {
                span {
                    class: "text-xs text-muted-foreground",
                    "{display}"
                }
            }

            // Label
            if let Some(label) = &props.label {
                span {
                    class: "text-xs text-muted-foreground font-medium",
                    "{label}"
                }
            }
        }
    }
}
