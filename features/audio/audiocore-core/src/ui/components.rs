//! Legacy UI components — most have moved to the `audiocore-gui` crate.
//!
//! This module retains `CompSlider` as a deprecated alias for
//! `audiocore_gui::controls::ParamSlider`. All other components
//! (Toggle, Section, SegmentButton, etc.) are now in `audiocore-gui`.

use audiocore_gui::theme::use_theme;
use nice_plug::prelude::ParamPtr;
use nice_plug_dioxus::prelude::*;

/// Inline-styled parameter slider for Blitz compatibility.
///
/// **Deprecated**: use `audiocore_gui::controls::ParamSlider` instead.
/// This is kept for backward compatibility with existing `comp-ui` code.
#[component]
pub fn CompSlider(param_ptr: ParamPtr, #[props(default = "")] label: &'static str) -> Element {
    let t = use_theme();
    let t = *t.read();
    let ctx = use_param_context();
    let mut revision = use_signal(|| 0u32);
    let mut is_dragging = use_signal(|| false);
    let mut drag_start_value = use_signal(|| 0.0f32);
    let mut drag_start_y = use_signal(|| 0.0f64);
    let _ = *revision.read();

    let normalized = unsafe { param_ptr.modulated_normalized_value() };
    let display_value = unsafe { param_ptr.normalized_value_to_string(normalized, true) };
    let name = if label.is_empty() {
        unsafe { param_ptr.name() }.to_string()
    } else {
        label.to_string()
    };

    let fill_width = format!("{}%", normalized * 100.0);

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:2px; min-width:80px; flex:1;",

            div {
                style: format!(
                    "font-size:10px; color:{}; text-transform:uppercase; \
                     letter-spacing:0.3px;",
                    t.text_dim,
                ),
                "{name}"
            }

            div {
                style: format!(
                    "height:24px; background:{}; border-radius:4px; position:relative; \
                     overflow:hidden; cursor:ns-resize; border:1px solid {}; \
                     user-select:none;",
                    t.surface, t.border,
                ),
                onmousedown: {
                    let ctx = ctx.clone();
                    move |evt: MouseEvent| {
                        is_dragging.set(true);
                        drag_start_value.set(normalized);
                        drag_start_y.set(evt.client_coordinates().y);
                        ctx.begin_set_raw(param_ptr);
                        revision += 1;
                    }
                },
                onmousemove: {
                    let ctx = ctx.clone();
                    move |evt: MouseEvent| {
                        if *is_dragging.read() {
                            let delta =
                                (drag_start_y() - evt.client_coordinates().y) as f32 / 150.0;
                            let new_val = (drag_start_value() + delta).clamp(0.0, 1.0);
                            ctx.set_normalized_raw(param_ptr, new_val);
                            revision += 1;
                        }
                    }
                },
                onmouseup: {
                    let ctx = ctx.clone();
                    move |_| {
                        if *is_dragging.read() {
                            is_dragging.set(false);
                            ctx.end_set_raw(param_ptr);
                            revision += 1;
                        }
                    }
                },
                onmouseleave: {
                    let ctx = ctx.clone();
                    move |_| {
                        if *is_dragging.read() {
                            is_dragging.set(false);
                            ctx.end_set_raw(param_ptr);
                            revision += 1;
                        }
                    }
                },
                ondoubleclick: {
                    let ctx = ctx.clone();
                    move |_| {
                        let default = unsafe { param_ptr.default_normalized_value() };
                        ctx.begin_set_raw(param_ptr);
                        ctx.set_normalized_raw(param_ptr, default);
                        ctx.end_set_raw(param_ptr);
                        revision += 1;
                    }
                },

                div {
                    style: format!(
                        "position:absolute; left:0; top:0; bottom:0; width:{fill_width}; \
                         background:{}; opacity:0.6; pointer-events:none;",
                        t.accent,
                    ),
                }

                div {
                    style: format!(
                        "position:absolute; left:0; right:0; top:0; bottom:0; \
                         display:flex; align-items:center; justify-content:center; \
                         font-size:11px; color:{}; pointer-events:none; \
                         font-variant-numeric:tabular-nums;",
                        t.text,
                    ),
                    "{display_value}"
                }
            }
        }
    }
}
