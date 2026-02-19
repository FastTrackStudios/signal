//! Slider component — range input with shadcn styling.
//!
//! Uses CSS custom properties for theming (--primary, --border, etc.)
//! and renders as a styled HTML range input.

use dioxus::prelude::*;

/// Slider orientation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SliderOrientation {
    Horizontal,
    Vertical,
}

impl Default for SliderOrientation {
    fn default() -> Self {
        Self::Horizontal
    }
}

/// A styled range slider.
#[derive(Props, Clone, PartialEq)]
pub struct SliderProps {
    /// Current value.
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

    /// Whether the slider is disabled.
    #[props(default)]
    disabled: bool,

    /// Orientation.
    #[props(default)]
    orientation: SliderOrientation,

    /// Callback when value changes.
    #[props(default)]
    on_change: Option<Callback<f64>>,

    /// Extra CSS classes for the outer container.
    #[props(default)]
    class: String,
}

#[component]
pub fn Slider(props: SliderProps) -> Element {
    let range = props.max - props.min;
    let pct = if range > 0.0 {
        ((props.value - props.min) / range * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    let is_vertical = props.orientation == SliderOrientation::Vertical;

    let container_class = if is_vertical {
        format!(
            "relative flex w-2 touch-none select-none items-center justify-center h-full {}",
            props.class
        )
    } else {
        format!(
            "relative flex w-full touch-none select-none items-center {}",
            props.class
        )
    };

    let track_class = if is_vertical {
        "relative w-2 grow overflow-hidden rounded-full bg-muted"
    } else {
        "relative h-2 w-full grow overflow-hidden rounded-full bg-muted"
    };

    let fill_style = if is_vertical {
        format!("height: {pct}%; width: 100%; position: absolute; bottom: 0;")
    } else {
        format!("width: {pct}%; height: 100%;")
    };

    let thumb_style = if is_vertical {
        format!(
            "position: absolute; left: 50%; bottom: calc({pct}% - 8px); transform: translateX(-50%);"
        )
    } else {
        format!(
            "position: absolute; top: 50%; left: calc({pct}% - 8px); transform: translateY(-50%);"
        )
    };

    let disabled_class = if props.disabled {
        " opacity-50 cursor-not-allowed"
    } else {
        " cursor-pointer"
    };

    rsx! {
        div {
            class: format!("{container_class}{disabled_class}"),
            role: "slider",
            "data-orientation": if is_vertical { "vertical" } else { "horizontal" },
            aria_valuemin: props.min.to_string(),
            aria_valuemax: props.max.to_string(),
            aria_valuenow: props.value.to_string(),
            aria_disabled: props.disabled.to_string(),

            // Track
            div {
                class: track_class,

                // Filled portion
                div {
                    class: "bg-primary",
                    style: fill_style,
                }

                // Thumb
                div {
                    class: "block h-4 w-4 rounded-full border-2 border-primary bg-background ring-offset-background transition-colors hover:scale-110 transition-transform focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
                    style: thumb_style,
                }
            }

            // Invisible native range input for actual interaction
            input {
                r#type: "range",
                class: "absolute inset-0 opacity-0 cursor-pointer",
                min: props.min.to_string(),
                max: props.max.to_string(),
                step: props.step.to_string(),
                value: props.value.to_string(),
                disabled: props.disabled,
                oninput: move |evt: FormEvent| {
                    if let Ok(val) = evt.value().parse::<f64>() {
                        if let Some(cb) = &props.on_change {
                            cb.call(val);
                        }
                    }
                },
            }
        }
    }
}
