//! XY pad — two-parameter 2D control surface (iced_audio `XYPad` parity).
//!
//! Press jumps the dot to the pointer, then the drag is captured by the shared
//! [`crate::drag::DragProvider`] (two-axis mode) so leaving the pad does not
//! drop it. Y axis is flipped so up = increase. Bound to two [`ParamHandle`]s.
//! Gestures per `fx.control.*`: double-click / Alt-click reset both axes, wheel
//! nudges Y.

use crate::drag::{begin_drag_xy, DragState};
use crate::gesture;
use crate::param::ParamHandle;
use crate::theme::*;
use dioxus::prelude::*;
use dioxus_elements::input_data::MouseButton;

/// Plain-value variant — emits normalized `(x, y)` via callback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XYValue {
    pub x: f32,
    pub y: f32,
}

#[component]
pub fn XYPad(
    x_handle: ParamHandle,
    y_handle: ParamHandle,
    #[props(default = 120)] size: u32,
    #[props(default)] x_label: Option<String>,
    #[props(default)] y_label: Option<String>,
    #[props(default)] color: Option<String>,
    #[props(default)] disabled: bool,
) -> Element {
    let mut drag: Signal<DragState> = use_context();
    let _ = drag.read().move_count;

    let s = size as f64;
    let xn = x_handle.normalized().clamp(0.0, 1.0) as f64;
    let yn = y_handle.normalized().clamp(0.0, 1.0) as f64;
    let px = (xn * s) as u32;
    let py = ((1.0 - yn) * s) as u32;

    let accent = color.as_deref().unwrap_or(ACCENT);
    let opacity = if disabled { "0.5" } else { "1.0" };
    let cursor = if disabled { "not-allowed" } else { "crosshair" };

    rsx! {
        div {
            style: format!(
                "display:inline-flex; flex-direction:column; align-items:center; gap:4px; \
                 opacity:{opacity};"
            ),

            div {
                style: format!(
                    "position:relative; border-radius:4px; border:1px solid {BORDER}; \
                     background:rgba(34,34,64,0.3); overflow:hidden; cursor:{cursor}; \
                     width:{size}px; height:{size}px; user-select:none;"
                ),
                // Press: jump the dot to the pointer, then drag relatively from
                // there through the shared capture (`fx.control.capture`);
                // Alt-click resets both axes; right-click does nothing here.
                onmousedown: {
                    let xh = x_handle.clone();
                    let yh = y_handle.clone();
                    move |evt: MouseEvent| {
                        if disabled { return; }
                        if evt.trigger_button() == Some(MouseButton::Secondary) {
                            return;
                        }
                        if evt.modifiers().alt() {
                            xh.reset_to_default();
                            yh.reset_to_default();
                            return;
                        }
                        let p = evt.element_coordinates();
                        let c = evt.client_coordinates();
                        xh.set_normalized((p.x / s).clamp(0.0, 1.0) as f32);
                        yh.set_normalized((1.0 - p.y / s).clamp(0.0, 1.0) as f32);
                        // One pad pixel = one normalized unit of pad, so the
                        // dot stays under the cursor.
                        begin_drag_xy(&mut drag, xh.clone(), yh.clone(), c.x, c.y, s);
                    }
                },
                ondoubleclick: {
                    let xh = x_handle.clone();
                    let yh = y_handle.clone();
                    move |_| {
                        if disabled { return; }
                        if drag.read().active { drag.set(DragState::default()); }
                        xh.reset_to_default();
                        yh.reset_to_default();
                    }
                },
                onwheel: {
                    let yh = y_handle.clone();
                    move |evt: WheelEvent| if !disabled { gesture::wheel(&evt, &yh) }
                },

                div {
                    style: format!(
                        "position:absolute; left:{px}px; top:0; width:1px; height:100%; \
                         background:rgba(136,136,136,0.3); pointer-events:none;"
                    ),
                }
                div {
                    style: format!(
                        "position:absolute; left:0; top:{py}px; width:100%; height:1px; \
                         background:rgba(136,136,136,0.3); pointer-events:none;"
                    ),
                }
                div {
                    style: format!(
                        "position:absolute; left:{px}px; top:{py}px; \
                         width:12px; height:12px; border-radius:6px; \
                         background:{accent}; border:2px solid {SURFACE}; \
                         transform:translate(-50%,-50%); pointer-events:none;"
                    ),
                }
            }

            div {
                style: format!(
                    "display:flex; justify-content:space-between; width:{size}px; \
                     font-size:10px; color:{TEXT_DIM};"
                ),
                if let Some(x_label) = &x_label { span { "X: {x_label}" } }
                if let Some(y_label) = &y_label { span { "Y: {y_label}" } }
            }
        }
    }
}
