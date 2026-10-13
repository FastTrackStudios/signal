//! Horizontal slider control widget.

use crate::prelude::*;

use crate::core::accessibility::{
    AriaAttributes, FocusState, KeyAction, KeyboardSteps, Orientation,
};
use crate::core::gesture::{GestureState, ScrollSensitivity};
use crate::core::modulation::ModulationRange;
use crate::core::normal::Normal;
use crate::core::sensitivity::ModifierKeys;
use crate::core::value::{ValueFormatter, ValueStepping};
use crate::theming::context::use_theme;
use crate::theming::style::{ControlState, SliderStyle};

/// Slider visual style variants.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum SliderVariant {
    /// Standard slider with filled track from left.
    #[default]
    Default,
    /// Bipolar slider with fill from center.
    Bipolar,
    /// Stepped slider with discrete notches.
    Stepped(usize),
    /// Custom style.
    Custom(SliderStyle),
}

/// Horizontal slider for parameter control.
///
/// # Features
///
/// - **Mouse drag**: Horizontal drag to change value
/// - **Scroll wheel**: Fine value adjustment
/// - **Keyboard**: Arrow keys, Page Up/Down, Home/End
/// - **Touch**: Single finger drag
/// - **Double-click**: Reset to default value
/// - **Shift+drag**: Fine control mode (10x precision)
/// - **Modulation**: Visual indicator of modulation range
///
/// # Accessibility
///
/// - ARIA slider role with proper value attributes
/// - Horizontal orientation for correct arrow key behavior
/// - Keyboard navigation with customizable steps
///
/// # Example
///
/// ```ignore
/// use audio_controls::prelude::*;
///
/// #[component]
/// fn MyUI() -> Element {
///     let mut value = use_signal(|| 0.5f32);
///
///     rsx! {
///         HSlider {
///             value: value,
///             label: Some("Volume".to_string()),
///         }
///     }
/// }
/// ```
#[component]
pub fn HSlider(
    value: Signal<f32>,
    #[props(default = 0.0)] min: f32,
    #[props(default = 1.0)] max: f32,
    #[props(default)] default: Option<f32>,
    #[props(default)] modulation: Option<ModulationRange>,
    #[props(default)] variant: SliderVariant,
    #[props(default = 120)] width: u32,
    #[props(default = 24)] height: u32,
    #[props(default)] label: Option<String>,
    #[props(default)] value_display: Option<String>,
    #[props(default)] formatter: Option<ValueFormatter>,
    #[props(default)] stepping: Option<ValueStepping>,
    #[props(default)] keyboard_steps: Option<KeyboardSteps>,
    #[props(default)] on_begin: Option<Callback<()>>,
    #[props(default)] on_change: Option<Callback<f32>>,
    #[props(default)] on_end: Option<Callback<()>>,
    #[props(default)] class: String,
    #[props(default = false)] disabled: bool,
    #[props(default = true)] scroll_enabled: bool,
    #[props(default = true)] keyboard_enabled: bool,
) -> Element {
    // State
    let mut gesture = use_signal(GestureState::new);
    let mut focus_state = use_signal(|| FocusState::Unfocused);
    let mut is_hovered = use_signal(|| false);
    let mut drag_start_x = use_signal(|| 0.0f32);
    let mut drag_start_normalized = use_signal(|| 0.0f32);

    // Get theme
    let theme = use_theme();
    let scaled_width = theme.scale(width);
    let scaled_height = theme.scale(height);

    // Configuration
    let scroll_sens = ScrollSensitivity::DEFAULT;
    let kb_steps = keyboard_steps.unwrap_or_default();
    let value_step = stepping.unwrap_or_default();

    // Handle stepped variant
    let value_step = match &variant {
        SliderVariant::Stepped(steps) => ValueStepping::discrete(*steps),
        _ => value_step,
    };

    let state = if disabled {
        ControlState::Disabled
    } else if gesture.read().is_active() {
        ControlState::Dragging
    } else if *is_hovered.read() || focus_state() == FocusState::Focused {
        ControlState::Hovered
    } else {
        ControlState::Idle
    };

    let is_bipolar = matches!(variant, SliderVariant::Bipolar);
    let style = match &variant {
        SliderVariant::Custom(s) => s.clone(),
        _ => theme.slider(state, is_bipolar),
    };

    // Calculate normalized value
    let current = value();
    let range = max - min;
    let normalized = if range.abs() > f32::EPSILON {
        ((current - min) / range).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // Apply stepping for display
    let display_normalized = if value_step.step > 0.0 {
        value_step.apply(Normal::new(normalized)).value()
    } else {
        normalized
    };

    let fill_percent = display_normalized * 100.0;
    let default_normalized =
        default
            .map(|d| (d - min) / range)
            .unwrap_or(if is_bipolar { 0.5 } else { 0.0 });

    // Format display value
    let display_text = value_display.clone().unwrap_or_else(|| {
        formatter
            .as_ref()
            .map(|f| f.format(current))
            .unwrap_or_else(|| format!("{current:.2}"))
    });

    // Modulation visualization
    let mod_style = modulation.filter(|m| m.active).map(|m| {
        let left = m.start * 100.0;
        let width = (m.end - m.start) * 100.0;
        format!("left: {left}%; width: {width}%;")
    });

    // ARIA attributes
    let aria = AriaAttributes::slider(label.as_deref().unwrap_or("Slider"), current, min, max)
        .with_value_text(&display_text)
        .with_orientation(Orientation::Horizontal)
        .disabled(disabled);

    // Focus ring style
    let focus_ring_class = if focus_state() == FocusState::Focused {
        "ring-2 ring-offset-2 ring-primary"
    } else {
        ""
    };

    // Step indicators for stepped variant
    let step_indicators = if let SliderVariant::Stepped(_steps) = &variant {
        Some(
            value_step
                .all_steps()
                .into_iter()
                .map(|s| s * 100.0)
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };

    // Helper to update value
    let mut update_value = move |new_normalized: f32| {
        let stepped = if value_step.step > 0.0 {
            value_step.apply(Normal::new(new_normalized)).value()
        } else {
            new_normalized
        };
        let new_value = min + stepped * range;
        value.set(new_value);
        if let Some(cb) = on_change {
            cb.call(new_value);
        }
    };

    rsx! {
        div {
            class: "hslider-container flex flex-col gap-1 {class}",

            // Label row
            if label.is_some() || value_display.is_some() {
                div {
                    class: "flex justify-between items-center text-xs",
                    if let Some(ref label_text) = label {
                        label {
                            class: "text-muted-foreground select-none",
                            "{label_text}"
                        }
                    }
                    if let Some(ref display) = value_display {
                        span {
                            class: "font-mono text-muted-foreground select-none",
                            aria_hidden: "true",
                            "{display}"
                        }
                    }
                }
            }

            // Track container
            div {
                class: "relative rounded-full outline-none {focus_ring_class}",
                style: "width: {scaled_width}px; height: {scaled_height}px; background: {style.track_color};",

                // Accessibility attributes
                role: "{aria.role.as_str()}",
                aria_label: "{aria.label.as_deref().unwrap_or(\"\")}",
                aria_valuemin: "{min}",
                aria_valuemax: "{max}",
                aria_valuenow: "{current}",
                aria_valuetext: "{display_text}",
                aria_orientation: "horizontal",
                aria_disabled: if disabled { "true" } else { "false" },
                tabindex: if disabled { "-1" } else { "0" },

                // Focus events
                onfocus: move |_| {
                    if !disabled {
                        focus_state.set(FocusState::Focused);
                    }
                },
                onblur: move |_| {
                    focus_state.set(FocusState::Unfocused);
                },

                // Keyboard events
                onkeydown: move |evt: KeyboardEvent| {
                    if disabled || !keyboard_enabled {
                        return;
                    }

                    let key = evt.key();
                    let modifiers = ModifierKeys::new(
                        evt.modifiers().shift(),
                        evt.modifiers().ctrl(),
                        evt.modifiers().alt(),
                    );

                    let action = KeyAction::from_key(&key.to_string(), modifiers.shift, modifiers.ctrl);

                    if action != KeyAction::None {
                        evt.prevent_default();

                        let current_normal = Normal::new(normalized);
                        let default_normal = Normal::new(default_normalized);
                        let new_normal = kb_steps.apply(action, current_normal, default_normal);

                        update_value(new_normal.value());
                    }
                },

                // Scroll wheel
                onwheel: move |evt: WheelEvent| {
                    if disabled || !scroll_enabled {
                        return;
                    }
                    evt.prevent_default();

                    let modifiers = ModifierKeys::new(
                        evt.modifiers().shift(),
                        evt.modifiers().ctrl(),
                        evt.modifiers().alt(),
                    );

                    let delta_y = evt.delta().strip_units().y as f32;
                    let new_normalized = gesture.write().process_scroll(
                        delta_y,
                        normalized,
                        &scroll_sens,
                        modifiers,
                    );
                    update_value(new_normalized);
                },

                // Step indicators
                if let Some(ref steps) = step_indicators {
                    for step_pos in steps.iter() {
                        div {
                            class: "absolute top-1/2 -translate-y-1/2 w-0.5 h-2/3 bg-muted-foreground/30 pointer-events-none",
                            style: "left: {step_pos}%;",
                        }
                    }
                }

                // Modulation range
                if let Some(ref m_style) = mod_style {
                    div {
                        class: "absolute top-0 h-full rounded-full opacity-50 pointer-events-none",
                        style: "background: {style.modulation_color}; {m_style}",
                    }
                }

                // Fill
                if is_bipolar {
                    // Bipolar: fill from center
                    if display_normalized >= 0.5 {
                        div {
                            class: "absolute top-0 h-full rounded-r-full pointer-events-none",
                            style: "background: {style.fill_color}; left: 50%; width: {(display_normalized - 0.5) * 100.0}%;",
                        }
                    } else {
                        div {
                            class: "absolute top-0 h-full rounded-l-full pointer-events-none",
                            style: "background: {style.fill_color}; left: {fill_percent}%; width: {(0.5 - display_normalized) * 100.0}%;",
                        }
                    }
                    // Center line
                    div {
                        class: "absolute top-0 h-full pointer-events-none",
                        style: "left: 50%; width: 2px; background: {style.track_color}; transform: translateX(-50%);",
                    }
                } else {
                    // Standard: fill from left
                    div {
                        class: "absolute top-0 left-0 h-full rounded-full pointer-events-none",
                        style: "background: {style.fill_color}; width: {fill_percent}%;",
                    }
                }

                // Thumb
                div {
                    class: "absolute top-1/2 -translate-y-1/2 rounded-full border-2 border-background shadow-sm pointer-events-none",
                    style: "width: {style.thumb_size}px; height: {style.thumb_size}px; background: {style.thumb_color}; left: calc({fill_percent}% - {style.thumb_size / 2.0}px);",
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
                        if gesture.read().is_active() && !disabled {
                            gesture.write().end();
                            if let Some(cb) = on_end {
                                cb.call(());
                            }
                        }
                    },
                    onmousedown: move |evt: MouseEvent| {
                        if disabled {
                            return;
                        }
                        gesture.write().begin_mouse(evt.client_coordinates().y as f32, normalized);
                        drag_start_x.set(evt.client_coordinates().x as f32);
                        drag_start_normalized.set(normalized);
                        if let Some(cb) = on_begin {
                            cb.call(());
                        }
                    },
                    onmousemove: move |evt: MouseEvent| {
                        if !gesture.read().is_active() || disabled {
                            return;
                        }
                        let delta_x = evt.client_coordinates().x as f32 - drag_start_x();
                        let modifiers = ModifierKeys::new(
                            evt.modifiers().shift(),
                            evt.modifiers().ctrl(),
                            evt.modifiers().alt(),
                        );

                        let sensitivity_mult = if modifiers.fine_control() { 0.1 } else { 1.0 };
                        let delta = (delta_x / scaled_width as f32) * sensitivity_mult;
                        let new_normalized = (drag_start_normalized() + delta).clamp(0.0, 1.0);

                        update_value(new_normalized);
                    },
                    onmouseup: move |_| {
                        if gesture.read().is_active() && !disabled {
                            gesture.write().end();
                            if let Some(cb) = on_end {
                                cb.call(());
                            }
                        }
                    },
                    ondoubleclick: move |_| {
                        if disabled {
                            return;
                        }
                        update_value(default_normalized);
                    },
                }
            }
        }
    }
}
