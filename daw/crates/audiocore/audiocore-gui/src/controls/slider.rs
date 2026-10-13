//! Parameter slider — inline-styled, Blitz-compatible.
//!
//! Two variants:
//! - `ParamSlider`: bound to a nice_plug `ParamPtr`, displays name + value
//! - `Slider`: raw normalized value with callback
//!
//! Redesigned with recessed track, illuminated fill bar, and 3D thumb.

use crate::drag::{begin_drag, DragState};
use crate::theme::use_theme;
use nice_plug::prelude::ParamPtr;
use nice_plug_dioxus::prelude::*;

/// Slider orientation.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SliderOrientation {
    #[default]
    Horizontal,
    Vertical,
}

/// Parameter slider bound to a nice_plug parameter.
///
/// Displays parameter name, a horizontal fill bar, and current value.
/// Drag vertically to adjust, double-click to reset.
#[component]
pub fn ParamSlider(
    /// The nice_plug parameter to control.
    param_ptr: ParamPtr,
    /// Label override (defaults to param name).
    #[props(default)]
    label: Option<String>,
    /// Slider height in pixels.
    #[props(default = 24.0)]
    height: f32,
) -> Element {
    let t = use_theme();
    let t = *t.read();

    let ctx = use_param_context();
    let mut drag: Signal<DragState> = use_context();
    let mut revision = use_signal(|| 0u32);
    let _ = *revision.read();

    // Re-render when drag is active (so value display updates)
    let _ = *drag.read();

    let normalized = unsafe { param_ptr.modulated_normalized_value() };
    let display_value = unsafe { param_ptr.normalized_value_to_string(normalized, true) };
    let name = label.unwrap_or_else(|| unsafe { param_ptr.name() }.to_string());

    let fill_width = format!("{}%", normalized * 100.0);

    /// Drag sensitivity: pixels of vertical drag per full 0→1 sweep.
    const SENSITIVITY: f64 = 150.0;

    rsx! {
        div {
            style: format!(
                "display:flex; flex-direction:column; gap:{TIGHT}; min-width:80px; flex:1;",
                TIGHT = t.spacing_tight,
            ),

            // Label
            div {
                style: format!("{LABEL}", LABEL = t.style_label()),
                "{name}"
            }

            // Track (recessed trough)
            div {
                style: format!(
                    "height:{height}px; {INSET} \
                     position:relative; overflow:hidden; cursor:ns-resize; \
                     user-select:none;",
                    INSET = t.style_inset(),
                ),
                onmousedown: {
                    let ctx = ctx.clone();
                    move |evt: MouseEvent| {
                        begin_drag(
                            &mut drag,
                            &ctx,
                            param_ptr,
                            evt.client_coordinates().y,
                            SENSITIVITY,
                        );
                        revision += 1;
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

                // Fill bar with accent glow
                div {
                    style: format!(
                        "position:absolute; left:0; top:0; bottom:0; width:{fill_width}; \
                         background:{ACCENT}; opacity:0.7; pointer-events:none;",
                        ACCENT = t.accent,
                    ),
                }

                // Centered value text
                div {
                    style: format!(
                        "position:absolute; left:0; right:0; top:0; bottom:0; \
                         display:flex; align-items:center; justify-content:center; \
                         {VALUE} pointer-events:none;",
                        VALUE = t.style_value(),
                    ),
                    "{display_value}"
                }
            }
        }
    }
}

/// A raw styled slider with normalized value and callback.
///
/// For use outside of nice_plug parameter binding.
#[component]
pub fn Slider(
    /// Current normalized value (0.0–1.0).
    #[props(default = 0.5)]
    value: f64,
    /// Minimum value.
    #[props(default = 0.0)]
    min: f64,
    /// Maximum value.
    #[props(default = 1.0)]
    max: f64,
    /// Step increment. Use 0.0 for continuous.
    #[props(default = 0.01)]
    step: f64,
    /// Orientation.
    #[props(default)]
    orientation: SliderOrientation,
    /// Whether the slider is disabled.
    #[props(default)]
    disabled: bool,
    /// Callback when value changes.
    #[props(default)]
    on_change: Option<Callback<f64>>,
) -> Element {
    let t = use_theme();
    let t = *t.read();

    let range = max - min;
    let pct = if range > 0.0 {
        ((value - min) / range * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    let is_vertical = orientation == SliderOrientation::Vertical;
    let opacity = if disabled { "0.5" } else { "1.0" };

    let container_style = if is_vertical {
        format!(
            "position:relative; width:8px; height:100%; display:flex; \
             align-items:center; justify-content:center; opacity:{opacity};"
        )
    } else {
        format!(
            "position:relative; height:8px; width:100%; display:flex; \
             align-items:center; opacity:{opacity};"
        )
    };

    let track_style = if is_vertical {
        format!(
            "position:relative; width:8px; flex:1; overflow:hidden; \
             border-radius:{RADIUS}; background:{SURFACE}; \
             box-shadow:{INSET_SHADOW}; border:1px solid {BORDER_S};",
            RADIUS = t.radius_button,
            SURFACE = t.surface,
            INSET_SHADOW = t.shadow_inset,
            BORDER_S = t.border_subtle,
        )
    } else {
        format!(
            "position:relative; height:8px; width:100%; flex:1; overflow:hidden; \
             border-radius:{RADIUS}; background:{SURFACE}; \
             box-shadow:{INSET_SHADOW}; border:1px solid {BORDER_S};",
            RADIUS = t.radius_button,
            SURFACE = t.surface,
            INSET_SHADOW = t.shadow_inset,
            BORDER_S = t.border_subtle,
        )
    };

    let fill_style = if is_vertical {
        format!(
            "height:{pct}%; width:100%; position:absolute; bottom:0; \
             background:{ACCENT};",
            ACCENT = t.accent,
        )
    } else {
        format!(
            "width:{pct}%; height:100%; background:{ACCENT};",
            ACCENT = t.accent,
        )
    };

    let thumb_style = if is_vertical {
        format!(
            "position:absolute; left:50%; bottom:calc({pct}% - 7px); \
             transform:translateX(-50%); width:14px; height:14px; \
             border-radius:7px; border:2px solid {ACCENT}; \
             background:{BG}; \
             box-shadow:{SHADOW};",
            ACCENT = t.accent,
            BG = t.bg,
            SHADOW = t.shadow_subtle,
        )
    } else {
        format!(
            "position:absolute; top:50%; left:calc({pct}% - 7px); \
             transform:translateY(-50%); width:14px; height:14px; \
             border-radius:7px; border:2px solid {ACCENT}; \
             background:{BG}; \
             box-shadow:{SHADOW};",
            ACCENT = t.accent,
            BG = t.bg,
            SHADOW = t.shadow_subtle,
        )
    };

    rsx! {
        div {
            style: container_style,

            div {
                style: track_style,
                div { style: fill_style }
                div { style: thumb_style }
            }

            if !disabled {
                input {
                    r#type: "range",
                    style: "position:absolute; inset:0; opacity:0; cursor:pointer;",
                    min: min.to_string(),
                    max: max.to_string(),
                    step: step.to_string(),
                    value: value.to_string(),
                    oninput: move |evt: FormEvent| {
                        if let Ok(val) = evt.value().parse::<f64>() {
                            if let Some(cb) = &on_change {
                                cb.call(val);
                            }
                        }
                    },
                }
            }
        }
    }
}
