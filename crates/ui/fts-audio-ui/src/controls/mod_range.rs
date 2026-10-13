//! Modulation range input (iced_audio `ModRangeInput` parity).
//!
//! Visualises a base parameter value with a separate modulation-range
//! `(min, max)` overlay. The base handle is the primary param; the min/max
//! handles drive the modulation range. Drag the base track horizontally; drag
//! the small min/max marker thumbs to adjust the range.

use crate::drag::{DragAxis, DragState};
use crate::gesture::{self, SLIDER_SENSITIVITY};
use crate::param::ParamHandle;
use crate::theme::*;
use dioxus::prelude::*;

const SENSITIVITY: f64 = SLIDER_SENSITIVITY;

#[component]
pub fn ModRangeInput(
    /// Base parameter value (0–1).
    base_handle: ParamHandle,
    /// Modulation range minimum (0–1).
    min_handle: ParamHandle,
    /// Modulation range maximum (0–1).
    max_handle: ParamHandle,
    #[props(default)] label: Option<String>,
    #[props(default = 24.0)] height: f64,
    #[props(default)] color: Option<String>,
    #[props(default)] disabled: bool,
) -> Element {
    let mut drag: Signal<DragState> = use_context();
    let _ = drag.read().move_count;

    let base = (base_handle.normalized().clamp(0.0, 1.0) * 100.0) as f64;
    let lo = (min_handle.normalized().clamp(0.0, 1.0) * 100.0) as f64;
    let hi = (max_handle.normalized().clamp(0.0, 1.0) * 100.0) as f64;
    let lo_p = lo.min(hi);
    let hi_p = lo.max(hi);

    let accent = color.as_deref().unwrap_or(ACCENT);
    let mod_color = SIGNAL_MOD;
    let opacity = if disabled { "0.5" } else { "1.0" };
    let label_str = label.unwrap_or_else(|| base_handle.name());

    rsx! {
        div {
            style: format!(
                "display:flex; flex-direction:column; gap:2px; min-width:120px; \
                 opacity:{opacity};"
            ),

            div {
                style: format!("font-size:10px; color:{TEXT_DIM}; text-transform:uppercase;"),
                "{label_str}"
            }

            div {
                style: format!(
                    "height:{height}px; background:{SURFACE}; border-radius:4px; \
                     position:relative; overflow:hidden; border:1px solid {BORDER}; \
                     user-select:none;"
                ),

                // Modulation range band
                div {
                    style: format!(
                        "position:absolute; top:0; bottom:0; left:{lo_p}%; width:{w}%; \
                         background:{mod_color}; opacity:0.35; pointer-events:none;",
                        w = (hi_p - lo_p).max(0.0)
                    ),
                }

                // Base value fill
                div {
                    style: format!(
                        "position:absolute; left:0; top:0; bottom:0; width:{base}%; \
                         background:{accent}; opacity:0.5; pointer-events:none;"
                    ),
                }

                // Base drag surface (full track)
                div {
                    style: "position:absolute; inset:0; cursor:ew-resize;",
                    onmousedown: {
                        let h = base_handle.clone();
                        move |evt: MouseEvent| {
                            if disabled { return; }
                            let _ = gesture::press(&evt, &mut drag, &h, DragAxis::Horizontal, SENSITIVITY);
                        }
                    },
                    ondoubleclick: {
                        let h = base_handle.clone();
                        move |_| if !disabled { gesture::double_click(&mut drag, &h) }
                    },
                    onwheel: {
                        let h = base_handle.clone();
                        move |evt: WheelEvent| if !disabled { gesture::wheel(&evt, &h) }
                    },
                }

                // Min marker thumb
                div {
                    style: format!(
                        "position:absolute; top:0; bottom:0; left:{lo_p}%; width:6px; \
                         transform:translateX(-50%); background:{mod_color}; cursor:ew-resize;"
                    ),
                    onmousedown: {
                        let h = min_handle.clone();
                        move |evt: MouseEvent| {
                            if disabled { return; }
                            evt.stop_propagation();
                            let _ = gesture::press(&evt, &mut drag, &h, DragAxis::Horizontal, SENSITIVITY);
                        }
                    },
                }

                // Max marker thumb
                div {
                    style: format!(
                        "position:absolute; top:0; bottom:0; left:{hi_p}%; width:6px; \
                         transform:translateX(-50%); background:{mod_color}; cursor:ew-resize;"
                    ),
                    onmousedown: {
                        let h = max_handle.clone();
                        move |evt: MouseEvent| {
                            if disabled { return; }
                            evt.stop_propagation();
                            let _ = gesture::press(&evt, &mut drag, &h, DragAxis::Horizontal, SENSITIVITY);
                        }
                    },
                }
            }
        }
    }
}
