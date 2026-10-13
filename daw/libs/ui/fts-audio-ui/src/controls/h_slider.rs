//! Horizontal slider (iced_audio `HSlider` parity).
//!
//! Drag horizontally to adjust; all gestures per [`crate::gesture`]
//! (`fx.control.*`): double-click / Alt-click reset, wheel, keyboard,
//! right-click to type. Optional tick-mark and text-mark groups render along
//! the track.

use crate::controls::readout::ValueReadout;
use crate::drag::{DragAxis, DragState};
use crate::gesture::{self, Press, SLIDER_SENSITIVITY};
use crate::marks::{TextMarkGroup, TickMarkGroup};
use crate::param::ParamHandle;
use crate::theme::*;
use dioxus::prelude::*;

const DEFAULT_HEIGHT: f64 = 24.0;
const DEFAULT_SENSITIVITY: f64 = SLIDER_SENSITIVITY;

#[component]
pub fn HSlider(
    handle: ParamHandle,
    #[props(default)] label: Option<String>,
    #[props(default = DEFAULT_HEIGHT)] height: f64,
    #[props(default = DEFAULT_SENSITIVITY)] sensitivity: f64,
    /// Deprecated: the reset target is the handle's default
    /// (`ParamHandle::with_default`); this prop is ignored.
    #[props(default)]
    default_value: Option<f32>,
    #[props(default)] tick_marks: Option<TickMarkGroup>,
    #[props(default)] text_marks: Option<TextMarkGroup>,
    #[props(default)] color: Option<String>,
    #[props(default)] disabled: bool,
) -> Element {
    let mut drag: Signal<DragState> = use_context();
    let _ = drag.read().move_count;
    let _ = default_value;
    let mut readout_open = use_signal(|| false);

    let normalized = handle.normalized();
    let name = label.unwrap_or_else(|| handle.name());
    let fill_pct = (normalized.clamp(0.0, 1.0) * 100.0) as f64;
    let accent = color.as_deref().unwrap_or(ACCENT);
    let opacity = if disabled { "0.5" } else { "1.0" };
    let cursor = if disabled { "not-allowed" } else { "ew-resize" };

    rsx! {
        div {
            style: format!(
                "display:flex; flex-direction:column; gap:2px; min-width:80px; \
                 flex:1; opacity:{opacity};"
            ),

            div {
                style: format!(
                    "font-size:10px; color:{TEXT_DIM}; text-transform:uppercase; \
                     letter-spacing:0.3px;"
                ),
                "{name}"
            }

            div {
                style: format!(
                    "height:{height}px; background:{SURFACE}; border-radius:4px; \
                     position:relative; overflow:hidden; cursor:{cursor}; \
                     border:1px solid {BORDER}; user-select:none;"
                ),
                tabindex: "0",
                onmousedown: {
                    let handle = handle.clone();
                    move |evt: MouseEvent| {
                        if disabled { return; }
                        if gesture::press(&evt, &mut drag, &handle, DragAxis::Horizontal, sensitivity)
                            == Press::Menu
                        {
                            readout_open.set(true);
                        }
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
                onkeydown: {
                    let handle = handle.clone();
                    move |evt: KeyboardEvent| {
                        if !disabled && gesture::key(&evt, &handle) == gesture::KeyOutcome::OpenTextEntry {
                            readout_open.set(true);
                        }
                    }
                },

                div {
                    style: format!(
                        "position:absolute; left:0; top:0; bottom:0; width:{fill_pct}%; \
                         background:{accent}; opacity:0.6; pointer-events:none;"
                    ),
                }

                if let Some(marks) = tick_marks {
                    for tick in marks.0.iter() {
                        {
                            let pos = (tick.position.clamp(0.0, 1.0) * 100.0) as f64;
                            rsx! {
                                div {
                                    style: format!(
                                        "position:absolute; left:{pos}%; top:0; bottom:0; \
                                         width:1px; background:{TEXT_DIM}; opacity:0.4; \
                                         pointer-events:none;"
                                    ),
                                }
                            }
                        }
                    }
                }

                div {
                    style: format!(
                        "position:absolute; inset:0; display:flex; align-items:center; \
                         justify-content:center; font-size:11px; color:{TEXT}; \
                         pointer-events:none; font-variant-numeric:tabular-nums;"
                    ),
                    div {
                        style: "pointer-events:auto;",
                        ValueReadout {
                            handle: handle.clone(),
                            disabled,
                            open: readout_open,
                            style: format!("font-size:11px; color:{TEXT}; font-variant-numeric:tabular-nums;"),
                            testid: "hslider-{name}",
                        }
                    }
                }
            }

            if let Some(marks) = text_marks {
                div {
                    style: format!(
                        "position:relative; height:12px; font-size:9px; color:{TEXT_DIM};"
                    ),
                    for tm in marks.0.iter() {
                        {
                            let pos = (tm.position.clamp(0.0, 1.0) * 100.0) as f64;
                            let label = tm.label.clone();
                            rsx! {
                                span {
                                    style: format!(
                                        "position:absolute; left:{pos}%; transform:translateX(-50%);"
                                    ),
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
