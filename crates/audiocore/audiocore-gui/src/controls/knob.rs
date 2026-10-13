//! Rotary knob widget — 3D-style with arc indicator, drop shadow, and drag interaction.
//!
//! Renders a circular knob body with radial lighting, surrounded by a
//! value arc and optional modulation overlay. The warm hybrid aesthetic
//! uses shadows and highlights to create depth without photorealism.
//!
//! Drag capture is handled by the `DragProvider` wrapper at the editor root.
//! The knob only fires `onmousedown` to start a drag.

use crate::drag::{begin_drag, DragState, TextEditState};
use crate::theme::use_theme;
use nice_plug::prelude::ParamPtr;
use nice_plug_dioxus::prelude::dioxus_elements::geometry::WheelDelta;
use nice_plug_dioxus::prelude::*;
use std::f64::consts::PI;

/// Knob display size.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum KnobSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl KnobSize {
    pub fn diameter(self) -> u32 {
        match self {
            Self::Small => 32,
            Self::Medium => 48,
            Self::Large => 64,
        }
    }

    /// Diameter of the inner knob body circle.
    fn body_diameter(self) -> u32 {
        match self {
            Self::Small => 20,
            Self::Medium => 30,
            Self::Large => 42,
        }
    }

    /// Stroke width for the value arc.
    fn arc_stroke(self) -> f64 {
        match self {
            Self::Small => 3.0,
            Self::Medium => 3.5,
            Self::Large => 4.0,
        }
    }

    /// Stroke width for the track arc.
    fn track_stroke(self) -> f64 {
        match self {
            Self::Small => 2.5,
            Self::Medium => 3.0,
            Self::Large => 3.5,
        }
    }

    /// Length of the indicator line from center outward.
    fn indicator_inner_ratio(self) -> f64 {
        0.25
    }

    fn indicator_outer_ratio(self) -> f64 {
        0.85
    }
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

/// Drag sensitivity: pixels of vertical drag per full 0→1 sweep.
const SENSITIVITY: f64 = 150.0;

/// A rotary knob bound to a nice_plug parameter.
///
/// Requires a `DragProvider` ancestor to handle drag capture.
#[component]
pub fn Knob(
    /// The nice_plug parameter to control.
    param_ptr: ParamPtr,
    /// Display size.
    #[props(default)]
    size: KnobSize,
    /// Label shown below the knob.
    #[props(default)]
    label: Option<String>,
    /// Accent color override (e.g. "#F97316"). Falls back to theme ACCENT.
    #[props(default)]
    color: Option<String>,
    /// Optional modulation range minimum (0.0–1.0).
    #[props(default)]
    mod_min: Option<f64>,
    /// Optional modulation range maximum (0.0–1.0).
    #[props(default)]
    mod_max: Option<f64>,
    /// Whether the knob is disabled.
    #[props(default)]
    disabled: bool,
) -> Element {
    let t = use_theme();
    let t = *t.read();

    let ctx = use_param_context();
    let mut drag: Signal<DragState> = use_context();
    let mut text_edit: Signal<TextEditState> = use_context();
    let mut revision = use_signal(|| 0u32);
    let _ = *revision.read();

    // Also re-render when drag is active (so value display updates)
    let _ = *drag.read();

    let normalized = unsafe { param_ptr.modulated_normalized_value() } as f64;
    let display_value = unsafe { param_ptr.normalized_value_to_string(normalized as f32, true) };
    let param_name = label.unwrap_or_else(|| unsafe { param_ptr.name() }.to_string());

    // Check if THIS knob is being text-edited
    let edit_state = text_edit.read();
    let is_editing = edit_state.active && edit_state.param_ptr == Some(param_ptr);
    let edit_text_display = if is_editing {
        edit_state.text.clone()
    } else {
        String::new()
    };
    drop(edit_state);

    let d = size.diameter();
    let df = d as f64;
    let body_d = size.body_diameter();
    let cx = df / 2.0;
    let cy = df / 2.0;
    let r = df / 2.0 - 3.0; // Arc radius — outer ring
    let val = normalized.clamp(0.0, 1.0);

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
    let mod_path = match (mod_min, mod_max) {
        (Some(lo), Some(hi)) => {
            let lo_angle = angle_for_value(lo.clamp(0.0, 1.0));
            let hi_angle = angle_for_value(hi.clamp(0.0, 1.0));
            svg_arc(cx, cy, r - 2.0, lo_angle, hi_angle)
        }
        _ => String::new(),
    };

    // Indicator line on the knob body — from inner to outer edge
    let body_r = body_d as f64 / 2.0;
    let ind_inner = body_r * size.indicator_inner_ratio();
    let ind_outer = body_r * size.indicator_outer_ratio();
    let (ix1, iy1) = arc_point(cx, cy, ind_inner, end_angle);
    let (ix2, iy2) = arc_point(cx, cy, ind_outer, end_angle);

    let accent = color.as_deref().unwrap_or(t.accent);
    let opacity = if disabled { "0.5" } else { "1.0" };
    let cursor = if disabled { "not-allowed" } else { "pointer" };

    let arc_stroke = size.arc_stroke();
    let track_stroke = size.track_stroke();

    rsx! {
        div {
            style: format!(
                "display:inline-flex; flex-direction:column; align-items:center; \
                 gap:{SPACING_LABEL}; opacity:{opacity}; cursor:{cursor}; position:relative;",
                SPACING_LABEL = t.spacing_label,
            ),

            // Knob area — body + arcs layered
            div {
                style: format!(
                    "position:relative; width:{d}px; height:{d}px; \
                     display:flex; align-items:center; justify-content:center;"
                ),

                // Knob body — 3D circle with lighting
                div {
                    style: format!(
                        "width:{body_d}px; height:{body_d}px; border-radius:50%; \
                         background: linear-gradient(145deg, {LIGHT}, {DARK}); \
                         box-shadow: {SHADOW}, \
                           inset 0 1px 1px rgba(255,255,255,0.07), \
                           inset 0 -1px 1px rgba(0,0,0,0.25); \
                         border: 1px solid rgba(255,255,255,0.04); \
                         position:absolute; z-index:1;",
                        LIGHT = t.knob_body_light,
                        DARK = t.knob_body_dark,
                        SHADOW = t.shadow_knob,
                    ),
                }

                // SVG arcs + indicator — overlaid on top
                svg {
                    width: "{d}",
                    height: "{d}",
                    view_box: "0 0 {df} {df}",
                    style: "position:absolute; z-index:2;",

                    // Track arc (recessed background ring)
                    path {
                        d: "{track_path}",
                        fill: "none",
                        stroke: "{t.knob_track}",
                        stroke_width: "{track_stroke}",
                        stroke_linecap: "round",
                    }

                    // Value arc (lit portion)
                    if !value_path.is_empty() {
                        path {
                            d: "{value_path}",
                            fill: "none",
                            stroke: "{accent}",
                            stroke_width: "{arc_stroke}",
                            stroke_linecap: "round",
                        }
                    }

                    // Modulation overlay
                    if !mod_path.is_empty() {
                        path {
                            d: "{mod_path}",
                            fill: "none",
                            stroke: "{t.signal_mod}",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            opacity: "0.6",
                        }
                    }

                    // Indicator line on the knob body
                    line {
                        x1: "{ix1:.1}",
                        y1: "{iy1:.1}",
                        x2: "{ix2:.1}",
                        y2: "{iy2:.1}",
                        stroke: "{t.knob_indicator}",
                        stroke_width: "2",
                        stroke_linecap: "round",
                    }
                }

                // Invisible drag surface — only handles mousedown, doubleclick, and scroll.
                // mousemove / mouseup are handled by the DragProvider at the root.
                if !disabled {
                    div {
                        style: "position:absolute; inset:0; cursor:ns-resize; \
                                user-select:none; z-index:3;",
                        onwheel: {
                            let ctx = ctx.clone();
                            move |evt: WheelEvent| {
                                evt.stop_propagation();
                                let delta_lines = match evt.delta() {
                                    WheelDelta::Lines(d) => d.y,
                                    WheelDelta::Pixels(d) => d.y / 16.0,
                                    WheelDelta::Pages(d) => d.y * 10.0,
                                };
                                let step = if evt.modifiers().shift() { 0.002 } else { 0.01 };
                                let current = unsafe { param_ptr.modulated_normalized_value() };
                                let new_val = (current + (delta_lines as f32 * step)).clamp(0.0, 1.0);
                                ctx.begin_set_raw(param_ptr);
                                ctx.set_normalized_raw(param_ptr, new_val);
                                ctx.end_set_raw(param_ptr);
                            }
                        },
                        onmousedown: {
                            let ctx = ctx.clone();
                            move |evt: MouseEvent| {
                                if evt.modifiers().ctrl() {
                                    // Ctrl+click → reset to default
                                    let default =
                                        unsafe { param_ptr.default_normalized_value() };
                                    ctx.begin_set_raw(param_ptr);
                                    ctx.set_normalized_raw(param_ptr, default);
                                    ctx.end_set_raw(param_ptr);
                                } else {
                                    begin_drag(
                                        &mut drag,
                                        &ctx,
                                        param_ptr,
                                        evt.client_coordinates().y,
                                        SENSITIVITY,
                                    );
                                }
                                revision += 1;
                            }
                        },
                        ondoubleclick: {
                            move |_| {
                                text_edit.set(TextEditState {
                                    active: true,
                                    param_ptr: Some(param_ptr),
                                    text: String::new(),
                                });
                            }
                        },
                    }
                }
            }

            // Value row — fixed height so editing mode never shifts the label
            if is_editing {
                {
                    let cursor_char = "|";
                    let shown = if edit_text_display.is_empty() {
                        cursor_char.to_string()
                    } else {
                        format!("{edit_text_display}{cursor_char}")
                    };
                    rsx! {
                        div {
                            style: format!(
                                "{VALUE_STYLE} background:{SURFACE}; \
                                 border:1px solid {ACCENT}; border-radius:{RADIUS}; \
                                 box-sizing:border-box; \
                                 min-width:48px; width:56px; height:18px; \
                                 display:flex; align-items:center; justify-content:center; \
                                 box-shadow: 0 0 6px {GLOW}; cursor:text;",
                                VALUE_STYLE = t.style_value(),
                                SURFACE = t.surface,
                                ACCENT = t.accent,
                                RADIUS = t.radius_small,
                                GLOW = t.accent_glow,
                            ),
                            "{shown}"
                        }
                    }
                }
            } else {
                span {
                    style: format!(
                        "{VALUE_STYLE} color:{TEXT_DIM}; \
                         min-width:48px; height:18px; \
                         display:flex; align-items:center; justify-content:center; \
                         cursor:text;",
                        VALUE_STYLE = t.style_value(),
                        TEXT_DIM = t.text_dim,
                    ),
                    ondoubleclick: move |_| {
                        if !disabled {
                            text_edit.set(TextEditState {
                                active: true,
                                param_ptr: Some(param_ptr),
                                text: String::new(),
                            });
                        }
                    },
                    "{display_value}"
                }
            }

            // Label — fixed height so it never moves
            span {
                style: format!(
                    "{LABEL_STYLE} min-width:48px; height:14px; \
                     display:flex; align-items:center; justify-content:center;",
                    LABEL_STYLE = t.style_label(),
                ),
                "{param_name}"
            }
        }
    }
}

/// A rotary knob displaying a raw normalized value (not bound to a parameter).
///
/// Useful for visualizations or custom parameter handling.
#[component]
pub fn RawKnob(
    /// Current normalized value (0.0–1.0).
    #[props(default = 0.5)]
    value: f64,
    /// Display size.
    #[props(default)]
    size: KnobSize,
    /// Label shown below the knob.
    #[props(default)]
    label: Option<String>,
    /// Formatted display value (e.g. "50%", "-12 dB").
    #[props(default)]
    display_value: Option<String>,
    /// Accent color override.
    #[props(default)]
    color: Option<String>,
    /// Optional modulation range minimum (0.0–1.0).
    #[props(default)]
    mod_min: Option<f64>,
    /// Optional modulation range maximum (0.0–1.0).
    #[props(default)]
    mod_max: Option<f64>,
    /// Callback when value changes.
    #[props(default)]
    on_change: Option<Callback<f64>>,
    /// Whether the knob is disabled.
    #[props(default)]
    disabled: bool,
) -> Element {
    let t = use_theme();
    let t = *t.read();

    let d = size.diameter();
    let df = d as f64;
    let body_d = size.body_diameter();
    let cx = df / 2.0;
    let cy = df / 2.0;
    let r = df / 2.0 - 3.0;
    let val = value.clamp(0.0, 1.0);

    let track_path = svg_arc(cx, cy, r, START_ANGLE, START_ANGLE + SWEEP);
    let end_angle = angle_for_value(val);
    let value_path = if val > 0.001 {
        svg_arc(cx, cy, r, START_ANGLE, end_angle)
    } else {
        String::new()
    };

    let mod_path = match (mod_min, mod_max) {
        (Some(lo), Some(hi)) => {
            let lo_angle = angle_for_value(lo.clamp(0.0, 1.0));
            let hi_angle = angle_for_value(hi.clamp(0.0, 1.0));
            svg_arc(cx, cy, r - 2.0, lo_angle, hi_angle)
        }
        _ => String::new(),
    };

    // Indicator line on the knob body
    let body_r = body_d as f64 / 2.0;
    let ind_inner = body_r * size.indicator_inner_ratio();
    let ind_outer = body_r * size.indicator_outer_ratio();
    let (ix1, iy1) = arc_point(cx, cy, ind_inner, end_angle);
    let (ix2, iy2) = arc_point(cx, cy, ind_outer, end_angle);

    let accent = color.as_deref().unwrap_or(t.accent);
    let opacity = if disabled { "0.5" } else { "1.0" };
    let cursor = if disabled { "not-allowed" } else { "pointer" };

    let arc_stroke = size.arc_stroke();
    let track_stroke = size.track_stroke();

    rsx! {
        div {
            style: format!(
                "display:inline-flex; flex-direction:column; align-items:center; \
                 gap:{SPACING_LABEL}; opacity:{opacity}; cursor:{cursor};",
                SPACING_LABEL = t.spacing_label,
            ),

            // Knob area — body + arcs layered
            div {
                style: format!(
                    "position:relative; width:{d}px; height:{d}px; \
                     display:flex; align-items:center; justify-content:center;"
                ),

                // Knob body — 3D circle
                div {
                    style: format!(
                        "width:{body_d}px; height:{body_d}px; border-radius:50%; \
                         background: linear-gradient(145deg, {LIGHT}, {DARK}); \
                         box-shadow: {SHADOW}, \
                           inset 0 1px 1px rgba(255,255,255,0.07), \
                           inset 0 -1px 1px rgba(0,0,0,0.25); \
                         border: 1px solid rgba(255,255,255,0.04); \
                         position:absolute; z-index:1;",
                        LIGHT = t.knob_body_light,
                        DARK = t.knob_body_dark,
                        SHADOW = t.shadow_knob,
                    ),
                }

                // SVG arcs + indicator
                svg {
                    width: "{d}",
                    height: "{d}",
                    view_box: "0 0 {df} {df}",
                    style: "position:absolute; z-index:2;",

                    path {
                        d: "{track_path}",
                        fill: "none",
                        stroke: "{t.knob_track}",
                        stroke_width: "{track_stroke}",
                        stroke_linecap: "round",
                    }

                    if !value_path.is_empty() {
                        path {
                            d: "{value_path}",
                            fill: "none",
                            stroke: "{accent}",
                            stroke_width: "{arc_stroke}",
                            stroke_linecap: "round",
                        }
                    }

                    if !mod_path.is_empty() {
                        path {
                            d: "{mod_path}",
                            fill: "none",
                            stroke: "{t.signal_mod}",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            opacity: "0.6",
                        }
                    }

                    // Indicator line on the knob body
                    line {
                        x1: "{ix1:.1}",
                        y1: "{iy1:.1}",
                        x2: "{ix2:.1}",
                        y2: "{iy2:.1}",
                        stroke: "{t.knob_indicator}",
                        stroke_width: "2",
                        stroke_linecap: "round",
                    }
                }

                // Hidden range input for interaction
                if !disabled {
                    input {
                        r#type: "range",
                        style: "position:absolute; inset:0; opacity:0; \
                                cursor:pointer; z-index:3;",
                        min: "0",
                        max: "1",
                        step: "0.005",
                        value: "{val}",
                        oninput: move |evt: FormEvent| {
                            if let Ok(v) = evt.value().parse::<f64>() {
                                if let Some(cb) = &on_change {
                                    cb.call(v.clamp(0.0, 1.0));
                                }
                            }
                        },
                    }
                }
            }

            if let Some(display) = &display_value {
                span {
                    style: format!(
                        "{VALUE_STYLE} color:{TEXT_DIM};",
                        VALUE_STYLE = t.style_value(),
                        TEXT_DIM = t.text_dim,
                    ),
                    "{display}"
                }
            }

            if let Some(label) = &label {
                span {
                    style: format!("{LABEL_STYLE}", LABEL_STYLE = t.style_label()),
                    "{label}"
                }
            }
        }
    }
}
