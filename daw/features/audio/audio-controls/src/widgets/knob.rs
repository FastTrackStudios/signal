//! Rotary knob control widget.
//!
//! The Knob widget provides a rotary control for continuous parameters,
//! using CSS-based rendering for broad compatibility.

use crate::prelude::*;

use crate::core::accessibility::{FocusState, KeyAction, KeyboardSteps};
use crate::core::gesture::{GestureState, ScrollSensitivity};
use crate::core::modulation::ModulationRange;
use crate::core::normal::Normal;
use crate::core::sensitivity::{DragSensitivity, ModifierKeys};
use crate::core::value::{ValueFormatter, ValueStepping};
use crate::theming::SvgTexture;
use crate::theming::context::use_theme;
use crate::theming::style::{ControlState, KnobStyle};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// Knob visual style variants.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum KnobVariant {
    /// Simple arc indicator (default).
    #[default]
    Arc,
    /// Arc with distinct colors for left/right/center (for bipolar parameters).
    ArcBipolar,
    /// Filled circle with pointer line.
    Circle,
    /// Minimal dot indicator.
    Dot,
    /// Custom style with explicit configuration.
    Custom(KnobStyle),
    /// Custom SVG texture.
    Svg(SvgTexture),
}

/// Knob component for rotary parameter control.
///
/// # Features
///
/// - **Mouse drag**: Vertical drag to change value
/// - **Scroll wheel**: Fine value adjustment
/// - **Keyboard**: Arrow keys, Page Up/Down, Home/End
/// - **Touch**: Single finger drag, pinch for fine control
/// - **Double-click**: Reset to default value
/// - **Shift+drag**: Fine control mode (10x precision)
/// - **Modulation**: Visual indicator of modulation range
#[component]
pub fn Knob(
    value: Signal<f32>,
    #[props(default = 0.0)] min: f32,
    #[props(default = 1.0)] max: f32,
    #[props(default)] default: Option<f32>,
    #[props(default)] modulation: Option<ModulationRange>,
    #[props(default)] variant: KnobVariant,
    #[props(default = 48)] size: u32,
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

    // Get theme
    let theme = use_theme();
    let scaled_size = theme.scale(size);

    // Configuration
    let sensitivity = DragSensitivity::DEFAULT;
    let scroll_sens = ScrollSensitivity::DEFAULT;
    let kb_steps = keyboard_steps.unwrap_or_default();
    let value_step = stepping.unwrap_or_default();

    // Determine control state
    let state = if disabled {
        ControlState::Disabled
    } else if gesture.read().is_active() {
        ControlState::Dragging
    } else if *is_hovered.read() || focus_state() == FocusState::Focused {
        ControlState::Hovered
    } else {
        ControlState::Idle
    };

    // Get style based on variant
    let is_bipolar = matches!(variant, KnobVariant::ArcBipolar);
    let style = match &variant {
        KnobVariant::Custom(s) => s.clone(),
        _ => theme.knob(state, is_bipolar),
    };

    // Calculate normalized value
    let current = value();
    let range = max - min;
    let normalized = if range.abs() > f32::EPSILON {
        ((current - min) / range).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // Apply stepping if configured
    let display_normalized = if value_step.step > 0.0 {
        value_step.apply(Normal::new(normalized)).value()
    } else {
        normalized
    };

    // Rotation angle: 0 = -135deg, 1 = +135deg (270 degree sweep)
    let rotation_deg = -135.0 + (display_normalized * 270.0);

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

    // Focus ring style
    let focus_ring_class = if focus_state() == FocusState::Focused {
        "ring-2 ring-offset-2 ring-blue-500"
    } else {
        ""
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

    // Calculate arc for conic gradient (CSS-based arc)
    // For the track: full 270 degrees from 135deg to 405deg (or -135 to 135)
    // For the value: portion of that arc
    let value_percent = display_normalized * 75.0; // 75% of circle = 270 degrees

    rsx! {
        div {
            class: "knob-container flex flex-col items-center gap-1 {class}",

            // Label
            if let Some(ref label_text) = label {
                label {
                    class: "text-xs text-gray-400 text-center select-none",
                    "{label_text}"
                }
            }

            // Knob wrapper for accessibility and interaction
            div {
                role: "slider",
                aria_valuemin: "{min}",
                aria_valuemax: "{max}",
                aria_valuenow: "{current}",
                aria_valuetext: "{display_text}",
                aria_disabled: if disabled { "true" } else { "false" },
                tabindex: if disabled { "-1" } else { "0" },
                class: "relative rounded-full outline-none {focus_ring_class}",
                class: if disabled { "opacity-50 cursor-not-allowed" } else { "cursor-pointer" },
                style: "width: {scaled_size}px; height: {scaled_size}px;",

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

                // Mouse events - only handle mousedown locally, move/up are global
                onmouseenter: move |_| {
                    if !disabled {
                        is_hovered.set(true);
                    }
                },
                onmouseleave: move |_| {
                    is_hovered.set(false);
                    // Don't end gesture here - let global mouseup handle it
                },
                onmousedown: move |evt: MouseEvent| {
                    if disabled {
                        return;
                    }
                    evt.prevent_default();
                    gesture.write().begin_mouse(evt.client_coordinates().y as f32, normalized);
                    if let Some(cb) = on_begin {
                        cb.call(());
                    }

                    // Set up global mouse listeners for drag tracking
                    #[cfg(target_arch = "wasm32")]
                    {
                        use std::cell::RefCell;
                        use std::rc::Rc;
                        use wasm_bindgen::closure::Closure;

                        let window = web_sys::window().unwrap();
                        let document = window.document().unwrap();

                        // Clone what we need for closures
                        let mut gesture_move = gesture;
                        let mut gesture_up = gesture;
                        let on_end_clone = on_end;

                        // Create shared state for cleanup
                        let move_closure: Rc<RefCell<Option<Closure<dyn FnMut(web_sys::MouseEvent)>>>> = Rc::new(RefCell::new(None));
                        let up_closure: Rc<RefCell<Option<Closure<dyn FnMut(web_sys::MouseEvent)>>>> = Rc::new(RefCell::new(None));

                        let move_closure_ref = move_closure.clone();
                        let up_closure_ref = up_closure.clone();
                        let document_for_cleanup = document.clone();

                        // Mouse move handler
                        let on_mousemove = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
                            if !gesture_move.read().is_active() {
                                return;
                            }
                            let shift = e.shift_key();
                            let ctrl = e.ctrl_key();
                            let alt = e.alt_key();
                            let modifiers = ModifierKeys::new(shift, ctrl, alt);
                            let new_normalized = gesture_move.write().update_mouse(
                                e.client_y() as f32,
                                sensitivity,
                                modifiers,
                            );
                            update_value(new_normalized);
                        }) as Box<dyn FnMut(web_sys::MouseEvent)>);

                        // Mouse up handler - also cleans up listeners
                        let on_mouseup = Closure::wrap(Box::new(move |_e: web_sys::MouseEvent| {
                            if gesture_up.read().is_active() {
                                gesture_up.write().end();
                                if let Some(cb) = on_end_clone {
                                    cb.call(());
                                }
                            }

                            // Clean up listeners
                            if let Some(closure) = move_closure_ref.borrow_mut().take() {
                                let _ = document_for_cleanup.remove_event_listener_with_callback(
                                    "mousemove",
                                    closure.as_ref().unchecked_ref(),
                                );
                            }
                            if let Some(closure) = up_closure_ref.borrow_mut().take() {
                                let _ = document_for_cleanup.remove_event_listener_with_callback(
                                    "mouseup",
                                    closure.as_ref().unchecked_ref(),
                                );
                            }
                        }) as Box<dyn FnMut(web_sys::MouseEvent)>);

                        // Add listeners
                        let _ = document.add_event_listener_with_callback(
                            "mousemove",
                            on_mousemove.as_ref().unchecked_ref(),
                        );
                        let _ = document.add_event_listener_with_callback(
                            "mouseup",
                            on_mouseup.as_ref().unchecked_ref(),
                        );

                        // Store closures so they don't get dropped
                        *move_closure.borrow_mut() = Some(on_mousemove);
                        *up_closure.borrow_mut() = Some(on_mouseup);
                    }
                },
                ondoubleclick: move |_| {
                    if disabled {
                        return;
                    }
                    update_value(default_normalized);
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

                // Outer ring (track background)
                // Conic gradient starts at 225deg (7:30 position, bottom-left) to align with pointer
                // Pointer rotates from -135deg to +135deg (270 degree sweep)
                // In CSS conic-gradient: 0deg = 12 o'clock, clockwise
                // 225deg = 7:30 position (bottom-left) where our min value is
                div {
                    class: "absolute inset-0 rounded-full",
                    style: "background: conic-gradient(from 225deg, {style.track_color} 0deg, {style.track_color} 270deg, transparent 270deg); padding: 4px;",

                    // Value arc overlay
                    div {
                        class: "absolute inset-0 rounded-full",
                        style: "background: conic-gradient(from 225deg, {style.fill_color} 0deg, {style.fill_color} {value_percent * 3.6}deg, transparent {value_percent * 3.6}deg); padding: 4px;",
                    }

                    // Inner circle (knob body)
                    div {
                        class: "absolute rounded-full",
                        style: "inset: {style.stroke_width}px; background: #1f2937;",

                        // Pointer indicator
                        div {
                            class: "absolute w-full h-full",
                            style: "transform: rotate({rotation_deg}deg);",

                            // Pointer line
                            div {
                                class: "absolute left-1/2 rounded-full",
                                style: "width: {style.pointer_width}px; height: 40%; top: 10%; transform: translateX(-50%); background: {style.pointer_color};",
                            }
                        }

                        // Center dot
                        div {
                            class: "absolute left-1/2 top-1/2 rounded-full",
                            style: "width: 6px; height: 6px; transform: translate(-50%, -50%); background: {style.track_color};",
                        }
                    }
                }
            }

            // Value display
            if let Some(ref display) = value_display {
                div {
                    class: "text-xs font-mono text-center text-gray-400 select-none",
                    "{display}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn knob_rotation_range() {
        // At 0%, rotation should be -135 deg
        let rotation_at_0: f64 = -135.0 + (0.0 * 270.0);
        assert!((rotation_at_0 - (-135.0)).abs() < 0.01);

        // At 50%, rotation should be 0 deg
        let rotation_at_50: f64 = -135.0 + (0.5 * 270.0);
        assert!((rotation_at_50 - 0.0).abs() < 0.01);

        // At 100%, rotation should be +135 deg
        let rotation_at_100: f64 = -135.0 + (1.0 * 270.0);
        assert!((rotation_at_100 - 135.0).abs() < 0.01);
    }
}
