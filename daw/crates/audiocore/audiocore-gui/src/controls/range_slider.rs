use crate::theme::use_theme;
use nice_plug::prelude::ParamPtr;
use nice_plug_dioxus::prelude::*;

/// Which thumb of a range slider is being dragged.
#[derive(Clone, Copy, PartialEq)]
enum ActiveThumb {
    Low,
    High,
}

/// Dual-thumb slider for min/max range selection.
#[component]
pub fn RangeSlider(
    low: f64,
    high: f64,
    on_change: EventHandler<(f64, f64)>,
    #[props(default)] label: Option<&'static str>,
    #[props(default)] low_display: Option<String>,
    #[props(default)] high_display: Option<String>,
    #[props(default = 24.0)] height: f32,
) -> Element {
    let t = use_theme();
    let t = *t.read();
    let mut active_thumb = use_signal(|| None::<ActiveThumb>);
    let track_rect = use_signal(|| (0.0f64, 0.0f64)); // (left, width)

    let low_pct = (low * 100.0).clamp(0.0, 100.0);
    let high_pct = (high * 100.0).clamp(0.0, 100.0);

    let low_display_text = low_display.clone().unwrap_or_default();
    let high_display_text = high_display.clone().unwrap_or_default();
    let show_values = low_display.is_some() || high_display.is_some();

    rsx! {
        div {
            style: format!(
                "display:flex; flex-direction:column; gap:{TIGHT}; min-width:80px; flex:1;",
                TIGHT = t.spacing_tight,
            ),

            if let Some(lbl) = label {
                div {
                    style: format!("{LABEL}", LABEL = t.style_label()),
                    "{lbl}"
                }
            }

            // Track
            div {
                style: format!(
                    "height:{height}px; {INSET} position:relative; cursor:pointer;",
                    INSET = t.style_inset(),
                ),

                onmousedown: move |evt: MouseEvent| {
                    let coords = evt.client_coordinates();
                    let x = coords.x;
                    let rect = *track_rect.read();
                    let frac = if rect.1 > 0.0 { ((x - rect.0) / rect.1).clamp(0.0, 1.0) } else { 0.5 };
                    let dist_low = (frac - low).abs();
                    let dist_high = (frac - high).abs();
                    let thumb = if dist_low <= dist_high { ActiveThumb::Low } else { ActiveThumb::High };
                    active_thumb.set(Some(thumb));
                },

                onmousemove: move |evt: MouseEvent| {
                    if let Some(thumb) = *active_thumb.read() {
                        let coords = evt.client_coordinates();
                        let rect = *track_rect.read();
                        let frac = if rect.1 > 0.0 {
                            ((coords.x - rect.0) / rect.1).clamp(0.0, 1.0)
                        } else { 0.5 };
                        match thumb {
                            ActiveThumb::Low => {
                                let new_low = frac.min(high - 0.01);
                                on_change.call((new_low, high));
                            }
                            ActiveThumb::High => {
                                let new_high = frac.max(low + 0.01);
                                on_change.call((low, new_high));
                            }
                        }
                    }
                },

                onmouseup: move |_| {
                    active_thumb.set(None);
                },

                onmouseleave: move |_| {
                    active_thumb.set(None);
                },

                // Fill between thumbs
                div {
                    style: format!(
                        "position:absolute; top:0; bottom:0; \
                         left:{low_pct}%; width:{}%; \
                         background:{ACCENT}; opacity:0.5; pointer-events:none;",
                        high_pct - low_pct,
                        ACCENT = t.accent,
                    ),
                }

                // Low thumb
                div {
                    style: format!(
                        "position:absolute; top:50%; left:{low_pct}%; \
                         transform:translate(-50%, -50%); \
                         width:12px; height:12px; border-radius:6px; \
                         background:{BG}; border:2px solid {ACCENT}; \
                         box-shadow:{SHADOW}; pointer-events:none;",
                        BG = t.surface_raised,
                        ACCENT = t.accent,
                        SHADOW = t.shadow_subtle,
                    ),
                }

                // High thumb
                div {
                    style: format!(
                        "position:absolute; top:50%; left:{high_pct}%; \
                         transform:translate(-50%, -50%); \
                         width:12px; height:12px; border-radius:6px; \
                         background:{BG}; border:2px solid {ACCENT}; \
                         box-shadow:{SHADOW}; pointer-events:none;",
                        BG = t.surface_raised,
                        ACCENT = t.accent,
                        SHADOW = t.shadow_subtle,
                    ),
                }
            }

            // Value display row
            if show_values {
                div {
                    style: format!(
                        "display:flex; justify-content:space-between; {VALUE}",
                        VALUE = t.style_value(),
                    ),

                    span { "{low_display_text}" }
                    span { "{high_display_text}" }
                }
            }
        }
    }
}

/// Dual-thumb range slider bound to two nice_plug parameters.
#[component]
pub fn ParamRangeSlider(
    low_param: ParamPtr,
    high_param: ParamPtr,
    #[props(default)] label: Option<&'static str>,
    #[props(default = 24.0)] height: f32,
) -> Element {
    let ctx = use_param_context();
    let mut revision = use_signal(|| 0u32);
    let _ = *revision.read();

    let low_norm = unsafe { low_param.modulated_normalized_value() } as f64;
    let high_norm = unsafe { high_param.modulated_normalized_value() } as f64;
    let low_display = unsafe { low_param.normalized_value_to_string(low_norm as f32, true) };
    let high_display = unsafe { high_param.normalized_value_to_string(high_norm as f32, true) };

    rsx! {
        RangeSlider {
            low: low_norm,
            high: high_norm,
            label: label,
            low_display: low_display,
            high_display: high_display,
            height: height,
            on_change: {
                let ctx = ctx.clone();
                move |(new_low, new_high): (f64, f64)| {
                    if (new_low - low_norm).abs() > 0.001 {
                        ctx.begin_set_raw(low_param);
                        ctx.set_normalized_raw(low_param, new_low as f32);
                        ctx.end_set_raw(low_param);
                    }
                    if (new_high - high_norm).abs() > 0.001 {
                        ctx.begin_set_raw(high_param);
                        ctx.set_normalized_raw(high_param, new_high as f32);
                        ctx.end_set_raw(high_param);
                    }
                    revision += 1;
                }
            },
        }
    }
}
