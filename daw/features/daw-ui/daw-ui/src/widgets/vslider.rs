//! Vertical slider control widget.

use crate::prelude::*;

use crate::core::modulation::ModulationRange;
use crate::core::sensitivity::{DragSensitivity, ModifierKeys};
use crate::theming::style::{ControlState, SliderStyle};
use crate::widgets::hslider::SliderVariant;

/// Vertical slider for parameter control.
///
/// Particularly useful for fader-style controls like channel volume.
///
/// # Example
///
/// ```ignore
/// use audio_controls::prelude::*;
///
/// #[component]
/// fn MyUI() -> Element {
///     let mut volume = use_signal(|| 0.75f32);
///
///     rsx! {
///         VSlider {
///             value: volume,
///             height: 200,
///             label: Some("Master".to_string()),
///         }
///     }
/// }
/// ```
#[component]
pub fn VSlider(
    value: Signal<f32>,
    #[props(default = 0.0)] min: f32,
    #[props(default = 1.0)] max: f32,
    #[props(default)] default: Option<f32>,
    #[props(default)] modulation: Option<ModulationRange>,
    #[props(default)] variant: SliderVariant,
    #[props(default = 24)] width: u32,
    #[props(default = 120)] height: u32,
    #[props(default)] label: Option<String>,
    #[props(default)] value_display: Option<String>,
    #[props(default)] on_begin: Option<Callback<()>>,
    #[props(default)] on_change: Option<Callback<f32>>,
    #[props(default)] on_end: Option<Callback<()>>,
    #[props(default)] class: String,
    #[props(default = false)] disabled: bool,
) -> Element {
    let mut is_dragging = use_signal(|| false);
    let mut drag_start_y = use_signal(|| 0.0f32);
    let mut drag_start_value = use_signal(|| 0.0f32);
    let mut is_hovered = use_signal(|| false);

    let sensitivity = DragSensitivity::new(height as f32, 0.1);

    let state = if disabled {
        ControlState::Disabled
    } else if *is_dragging.read() {
        ControlState::Dragging
    } else if *is_hovered.read() {
        ControlState::Hovered
    } else {
        ControlState::Idle
    };

    let is_bipolar = matches!(variant, SliderVariant::Bipolar);
    let style = match &variant {
        SliderVariant::Custom(s) => s.clone(),
        _ => SliderStyle::css_vars(state),
    };

    let current = value();
    let normalized = ((current - min) / (max - min)).clamp(0.0, 1.0);
    // Invert for vertical (bottom = 0, top = 1)
    let fill_percent = normalized * 100.0;

    let default_value = default.unwrap_or(if is_bipolar { 0.5 } else { 0.0 });

    // Modulation visualization (inverted for vertical)
    let mod_style = modulation.filter(|m| m.active).map(|m| {
        let bottom = m.start * 100.0;
        let height = (m.end - m.start) * 100.0;
        format!("bottom: {bottom}%; height: {height}%;")
    });

    rsx! {
        div {
            class: "vslider-container flex flex-col items-center gap-1 {class}",

            // Label
            if let Some(ref label_text) = label {
                div {
                    class: "text-xs text-muted-foreground text-center select-none",
                    "{label_text}"
                }
            }

            // Track container
            div {
                class: "relative rounded-full",
                style: "width: {width}px; height: {height}px; background: {style.track_color};",

                // Modulation range
                if let Some(ref m_style) = mod_style {
                    div {
                        class: "absolute left-0 w-full rounded-full opacity-50",
                        style: "background: {style.modulation_color}; {m_style}",
                    }
                }

                // Fill
                if is_bipolar {
                    // Bipolar: fill from center
                    if normalized >= 0.5 {
                        div {
                            class: "absolute left-0 w-full rounded-t-full",
                            style: "background: {style.fill_color}; bottom: 50%; height: {(normalized - 0.5) * 100.0}%;",
                        }
                    } else {
                        div {
                            class: "absolute left-0 w-full rounded-b-full",
                            style: "background: {style.fill_color}; bottom: {fill_percent}%; height: {(0.5 - normalized) * 100.0}%;",
                        }
                    }
                    // Center line
                    div {
                        class: "absolute left-0 w-full",
                        style: "bottom: 50%; height: 2px; background: {style.track_color}; transform: translateY(50%);",
                    }
                } else {
                    // Standard: fill from bottom
                    div {
                        class: "absolute left-0 bottom-0 w-full rounded-full",
                        style: "background: {style.fill_color}; height: {fill_percent}%;",
                    }
                }

                // Thumb
                div {
                    class: "absolute left-1/2 -translate-x-1/2 rounded-full border-2 border-background shadow-sm",
                    style: "width: {style.thumb_size}px; height: {style.thumb_size}px; background: {style.thumb_color}; bottom: calc({fill_percent}% - {style.thumb_size / 2.0}px);",
                }

                // Interaction overlay
                div {
                    class: if disabled { "absolute inset-0 cursor-not-allowed opacity-50" } else { "absolute inset-0 cursor-pointer" },

                    onmouseenter: move |_| {
                        if !disabled {
                            is_hovered.set(true);
                        }
                    },
                    onmouseleave: move |_| {
                        is_hovered.set(false);
                        if *is_dragging.read() && !disabled {
                            is_dragging.set(false);
                            if let Some(cb) = on_end {
                                cb.call(());
                            }
                        }
                    },
                    onmousedown: move |evt: MouseEvent| {
                        if disabled {
                            return;
                        }
                        is_dragging.set(true);
                        drag_start_y.set(evt.client_coordinates().y as f32);
                        drag_start_value.set(normalized);
                        if let Some(cb) = on_begin {
                            cb.call(());
                        }
                    },
                    onmousemove: move |evt: MouseEvent| {
                        if !*is_dragging.read() || disabled {
                            return;
                        }
                        let delta_y = drag_start_y() - evt.client_coordinates().y as f32; // Inverted for vertical
                        let modifiers = ModifierKeys::new(evt.modifiers().shift(), evt.modifiers().ctrl(), evt.modifiers().alt());
                        let delta = sensitivity.calculate_delta(-delta_y, modifiers); // Negate to invert

                        let new_normalized = (drag_start_value() + delta).clamp(0.0, 1.0);
                        let new_value = min + new_normalized * (max - min);

                        value.set(new_value);
                        if let Some(cb) = on_change {
                            cb.call(new_value);
                        }
                    },
                    onmouseup: move |_| {
                        if *is_dragging.read() && !disabled {
                            is_dragging.set(false);
                            if let Some(cb) = on_end {
                                cb.call(());
                            }
                        }
                    },
                    ondoubleclick: move |_| {
                        if disabled {
                            return;
                        }
                        let reset_value = min + default_value * (max - min);
                        value.set(reset_value);
                        if let Some(cb) = on_change {
                            cb.call(reset_value);
                        }
                    },
                }
            }

            // Value display
            if let Some(ref display_text) = value_display {
                div {
                    class: "text-xs font-mono text-muted-foreground text-center select-none",
                    "{display_text}"
                }
            }
        }
    }
}
