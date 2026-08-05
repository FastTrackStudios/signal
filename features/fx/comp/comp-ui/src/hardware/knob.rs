//! A hardware knob — pointer, skirt, and a scale ring printed on the panel.
//!
//! Different from [`fts_ui_audio::Knob`] in the way that matters: an FTS knob
//! draws its value as an arc, because the value is the point. A hardware knob
//! draws a *pointer*, and the value is read off numbers silkscreened on the
//! panel around it — which is why the ring here is drawn even where the
//! pointer is not, and why the numbers are the unit's own (LA-2A GAIN reads
//! 0–10, 1176 INPUT reads -48…+12) rather than the engine's.
//!
//! Geometry lives in [`crate::hardware::knob_svg`]. Dragging goes through the
//! same [`DragProvider`](fts_ui_audio::drag::DragProvider) as every other FTS
//! control, so a hardware face behaves like the rest of the editor.

use audiocore_core::prelude::*;
use fts_ui_audio::drag::{begin_drag, DragState};
use fts_ui_audio::prelude::*;

use crate::hardware::knob_svg::{knob_angle, pointer_polygon, ring_arc_path, ring_point, ScaleMark};

/// Design-space radii inside the knob's own `-55 -55 110 110` viewBox.
const BODY_R: f64 = 30.0;
const RING_R: f64 = 41.0;
const LABEL_R: f64 = 50.0;

/// Pixels of vertical drag per full sweep. Looser than the FTS knob's 150 —
/// these are big knobs with printed scales, and a coarse feel suits them.
const SENSITIVITY: f64 = 190.0;
const WHEEL_STEP: f64 = 0.02;
const WHEEL_STEP_FINE: f64 = 0.005;

/// How the knob body is finished.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KnobStyle {
    /// Black bakelite skirt with a white pointer line — the LA-2A / 1176 knob.
    Bakelite,
    /// Brushed metal with a dark pointer — the SSL-style knob.
    Metal,
}

impl KnobStyle {
    fn body(self) -> &'static str {
        match self {
            Self::Bakelite => "radial-gradient(circle at 34% 26%, #4a4a4e 0%, #17171a 62%, #0b0b0d 100%)",
            Self::Metal => "radial-gradient(circle at 34% 26%, #d8d8d4 0%, #9a9a96 58%, #6d6d69 100%)",
        }
    }
    fn pointer(self) -> &'static str {
        match self {
            Self::Bakelite => "#f2f2f0",
            Self::Metal => "#1c1c1e",
        }
    }
}

/// A knob on a hardware faceplate.
///
/// `marks` is the printed scale ring — build it with
/// [`scale_ring`](crate::hardware::knob_svg::scale_ring).
#[component]
pub fn HardwareKnob(
    handle: ParamHandle,
    /// Stable test id; rendered as `hw-knob-{testid}`.
    testid: String,
    scale: f64,
    /// Knob diameter in design-space px, excluding the printed ring.
    #[props(default = 62.0)]
    diameter: f64,
    #[props(default = KnobStyle::Bakelite)] style: KnobStyle,
    /// Colour of the silkscreened ring numbers.
    #[props(default = "#2b2620".to_string())]
    ink: String,
    #[props(default)] marks: Vec<ScaleMark>,
) -> Element {
    let mut drag: Signal<DragState> = use_context();
    // Re-render while a drag is in flight so the pointer tracks the cursor.
    let _ = drag.read().move_count;

    let normalized = handle.normalized() as f64;
    let angle = knob_angle(normalized);
    let display = handle.display_value();
    let name = handle.name();

    // The printed ring is drawn outside the body, so the box is wider than
    // the knob — the viewBox spans -55..55 with the body at r = 30.
    let box_px = diameter * (110.0 / (BODY_R * 2.0)) * scale;
    let pointer = pointer_polygon(BODY_R - 5.0, 3.4);
    let ring = ring_arc_path(RING_R);

    rsx! {
        div {
            "data-testid": "hw-knob-{testid}",
            "data-normalized": "{normalized:.4}",
            title: format!("{name} — {display}\nDrag · Shift=fine · Wheel · Alt-click=reset"),
            style: format!(
                "position:relative; width:{box_px:.1}px; height:{box_px:.1}px;"
            ),

            svg {
                style: "position:absolute; inset:0; width:100%; height:100%; display:block;",
                view_box: "-55 -55 110 110",

                // The printed scale ring: a faint band with the unit's own
                // numbers around it.
                path {
                    d: "{ring}",
                    fill: "none",
                    stroke: "{ink}",
                    stroke_width: "0.6",
                    opacity: "0.35",
                }
                for mark in marks.iter() {
                    {
                        let (x1, y1) = ring_point(mark.normalized, RING_R - 1.0);
                        let (x2, y2) = ring_point(
                            mark.normalized,
                            if mark.major { RING_R + 5.0 } else { RING_R + 3.0 },
                        );
                        let (lx, ly) = ring_point(mark.normalized, LABEL_R);
                        rsx! {
                            line {
                                x1: "{x1:.2}", y1: "{y1:.2}", x2: "{x2:.2}", y2: "{y2:.2}",
                                stroke: "{ink}",
                                stroke_width: if mark.major { "1.4" } else { "0.8" },
                                opacity: if mark.major { "0.9" } else { "0.55" },
                            }
                            if let Some(label) = &mark.label {
                                text {
                                    x: "{lx:.2}", y: "{ly + 2.6:.2}",
                                    fill: "{ink}", font_size: "7",
                                    font_weight: "700", text_anchor: "middle",
                                    "{label}"
                                }
                            }
                        }
                    }
                }

                // Body shadow — the knob sits proud of the panel.
                circle {
                    cx: "0", cy: "1.5", r: "{BODY_R:.1}",
                    fill: "rgba(0,0,0,0.35)",
                }
            }

            // The knob body itself is a div so it can carry a CSS gradient —
            // the moulded-plastic look does not survive as flat SVG fill.
            div {
                style: format!(
                    "position:absolute; left:50%; top:50%; \
                     width:{:.1}px; height:{:.1}px; \
                     margin-left:{:.1}px; margin-top:{:.1}px; \
                     border-radius:50%; background:{}; \
                     box-shadow:0 {:.1}px {:.1}px rgba(0,0,0,0.45);",
                    diameter * scale,
                    diameter * scale,
                    -(diameter * scale) / 2.0,
                    -(diameter * scale) / 2.0,
                    style.body(),
                    1.5 * scale,
                    4.0 * scale,
                ),
            }

            // Pointer, rotated to the value. Kept in its own SVG layer above
            // the body so the rotation is exact at any panel scale.
            svg {
                style: "position:absolute; inset:0; width:100%; height:100%; \
                        display:block; pointer-events:none;",
                view_box: "-55 -55 110 110",
                g {
                    transform: "rotate({angle:.2})",
                    polygon {
                        points: "{pointer}",
                        fill: "{style.pointer()}",
                    }
                }
                circle { cx: "0", cy: "0", r: "4.5", fill: "rgba(0,0,0,0.35)" }
            }

            // Interaction overlay — same gestures as every FTS control.
            div {
                style: "position:absolute; inset:0; cursor:ns-resize; user-select:none;",
                onmousedown: {
                    let handle = handle.clone();
                    move |evt: MouseEvent| {
                        if evt.modifiers().alt() {
                            evt.prevent_default();
                            handle.reset_to_default();
                            return;
                        }
                        begin_drag(
                            &mut drag,
                            handle.clone(),
                            evt.client_coordinates().y,
                            SENSITIVITY,
                        );
                    }
                },
                onwheel: {
                    let handle = handle.clone();
                    move |evt: WheelEvent| {
                        evt.prevent_default();
                        let delta_y = evt.delta().strip_units().y;
                        if delta_y == 0.0 {
                            return;
                        }
                        let direction = if delta_y < 0.0 { 1.0 } else { -1.0 };
                        let mods = evt.modifiers();
                        let step = if mods.shift() { WHEEL_STEP_FINE } else { WHEEL_STEP };
                        let next = (handle.normalized() as f64 + direction * step)
                            .clamp(0.0, 1.0) as f32;
                        handle.begin_edit();
                        handle.set_normalized(next);
                        handle.end_edit();
                    }
                },
            }
        }
    }
}
