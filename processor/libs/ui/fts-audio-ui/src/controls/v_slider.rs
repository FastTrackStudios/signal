//! Vertical slider (iced_audio `VSlider` parity). Drag vertically; up =
//! increase. All gestures per [`crate::gesture`] (`fx.control.*`).

use crate::controls::readout::ValueReadout;
use crate::drag::DragState;
use crate::gesture::{self, Press, SLIDER_SENSITIVITY};
use crate::marks::{TextMarkGroup, TickMarkGroup};
use crate::param::ParamHandle;
use crate::theme::*;
use dioxus::prelude::*;

const DEFAULT_WIDTH: f64 = 24.0;
const DEFAULT_HEIGHT: f64 = 120.0;
const DEFAULT_SENSITIVITY: f64 = SLIDER_SENSITIVITY;

#[component]
pub fn VSlider(
    handle: ParamHandle,
    #[props(default)] label: Option<String>,
    #[props(default = DEFAULT_WIDTH)] width: f64,
    #[props(default = DEFAULT_HEIGHT)] height: f64,
    #[props(default = DEFAULT_SENSITIVITY)] sensitivity: f64,
    /// Deprecated: the reset target is the handle's default; ignored.
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
    let cursor = if disabled { "not-allowed" } else { "ns-resize" };

    rsx! {
        div {
            style: format!(
                "display:inline-flex; flex-direction:column; align-items:center; gap:4px; \
                 opacity:{opacity};"
            ),

            div {
                style: format!(
                    "font-size:10px; color:{TEXT_DIM}; text-transform:uppercase;"
                ),
                "{name}"
            }

            div {
                style: format!("display:flex; align-items:stretch; gap:6px;"),

                div {
                    style: format!(
                        "width:{width}px; height:{height}px; background:{SURFACE}; \
                         border-radius:4px; position:relative; overflow:hidden; \
                         cursor:{cursor}; border:1px solid {BORDER}; user-select:none;"
                    ),
                    tabindex: "0",
                    onmousedown: {
                        let handle = handle.clone();
                        move |evt: MouseEvent| {
                            if disabled { return; }
                            if gesture::press_vertical(&evt, &mut drag, &handle, sensitivity)
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
                            "position:absolute; left:0; right:0; bottom:0; height:{fill_pct}%; \
                             background:{accent}; opacity:0.6; pointer-events:none;"
                        ),
                    }

                    if let Some(marks) = tick_marks {
                        for tick in marks.0.iter() {
                            {
                                let pos = ((1.0 - tick.position.clamp(0.0, 1.0)) * 100.0) as f64;
                                rsx! {
                                    div {
                                        style: format!(
                                            "position:absolute; left:0; right:0; top:{pos}%; \
                                             height:1px; background:{TEXT_DIM}; opacity:0.4; \
                                             pointer-events:none;"
                                        ),
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(marks) = text_marks {
                    div {
                        style: format!(
                            "position:relative; width:32px; font-size:9px; color:{TEXT_DIM};"
                        ),
                        for tm in marks.0.iter() {
                            {
                                let pos = ((1.0 - tm.position.clamp(0.0, 1.0)) * 100.0) as f64;
                                let label = tm.label.clone();
                                rsx! {
                                    span {
                                        style: format!(
                                            "position:absolute; top:{pos}%; transform:translateY(-50%);"
                                        ),
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            ValueReadout {
                handle: handle.clone(),
                disabled,
                open: readout_open,
                style: format!("font-size:10px; color:{TEXT_DIM}; font-variant-numeric:tabular-nums;"),
                testid: "vslider-{name}",
            }
        }
    }
}
