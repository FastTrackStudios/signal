//! Global drag capture for Blitz-based plugin UIs.
//!
//! Blitz dispatches mouse events to whichever element is under the cursor (no
//! pointer capture). This module provides a shared drag context so that knobs
//! and sliders can start a drag on mousedown, and the root `DragProvider`
//! wrapper handles mousemove / mouseup at the top level regardless of which
//! element the cursor is over.

use std::rc::Rc;

use nice_plug::prelude::ParamPtr;
use nice_plug_dioxus::prelude::*;

/// Fine-adjustment multiplier when Shift is held during drag.
const FINE_MULTIPLIER: f64 = 5.0;

/// Shared state for a parameter drag in progress.
#[derive(Clone, Copy, Default)]
pub struct DragState {
    pub active: bool,
    pub param_ptr: Option<ParamPtr>,
    pub start_value: f64,
    pub start_y: f64,
    pub sensitivity: f64,
    /// Whether shift was held on the last mousemove (for re-anchoring).
    pub last_shift: bool,
    /// Incremented on each mousemove so subscribers (knobs) re-render.
    pub move_count: u64,
}

/// Shared state for text-editing a parameter value (double-click on knob).
///
/// Keyboard events are handled at the DragProvider root level because Blitz
/// doesn't support native `<input>` text editing.
#[derive(Clone, Default)]
pub struct TextEditState {
    pub active: bool,
    pub param_ptr: Option<ParamPtr>,
    pub text: String,
}

/// Begin a drag. Call from a knob/slider's `onmousedown`.
pub fn begin_drag(
    drag: &mut Signal<DragState>,
    ctx: &ParamContext,
    param_ptr: ParamPtr,
    start_y: f64,
    sensitivity: f64,
) {
    let normalized = unsafe { param_ptr.modulated_normalized_value() } as f64;
    ctx.begin_set_raw(param_ptr);
    drag.set(DragState {
        active: true,
        param_ptr: Some(param_ptr),
        start_value: normalized,
        start_y,
        sensitivity,
        last_shift: false,
        move_count: 0,
    });
}

/// Wrapper component that captures mouse events for parameter drags
/// and keyboard events for text editing.
///
/// Place this around your entire editor UI. It provides `Signal<DragState>`
/// and `Signal<TextEditState>` contexts that knobs and sliders use.
#[component]
pub fn DragProvider(children: Element) -> Element {
    let mut drag = use_signal(DragState::default);
    let mut text_edit = use_signal(TextEditState::default);
    let mut revision = use_signal(|| 0u32);
    let ctx = use_param_context();

    // Provide drag and text edit state to child components
    use_context_provider(|| drag);
    use_context_provider(|| text_edit);

    let _ = *revision.read();

    // Store mounted data so we can programmatically focus this div
    // when text editing starts. Blitz only auto-focuses <input> elements
    // on click; we must request focus explicitly for keyboard events to fire.
    let mut self_mounted: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    use_effect(move || {
        let is_editing = text_edit.read().active;
        if is_editing {
            if let Some(mounted) = self_mounted.read().clone() {
                spawn(async move {
                    let _ = mounted.set_focus(true).await;
                });
            }
        }
    });

    rsx! {
        div {
            style: "width:100vw; height:100vh;",
            tabindex: "0",
            onmounted: move |evt| {
                self_mounted.set(Some(evt.data()));
            },

            onkeydown: {
                let ctx = ctx.clone();
                move |evt: KeyboardEvent| {
                    let state = text_edit.read().clone();
                    if !state.active {
                        return;
                    }
                    match evt.key() {
                        Key::Enter => {
                            // Submit: parse and apply value
                            if let Some(param_ptr) = state.param_ptr {
                                if !state.text.is_empty() {
                                    if let Some(normalized) =
                                        unsafe { param_ptr.string_to_normalized_value(&state.text) }
                                    {
                                        ctx.begin_set_raw(param_ptr);
                                        ctx.set_normalized_raw(param_ptr, normalized);
                                        ctx.end_set_raw(param_ptr);
                                    }
                                }
                            }
                            text_edit.set(TextEditState::default());
                            revision += 1;
                        }
                        Key::Escape => {
                            text_edit.set(TextEditState::default());
                        }
                        Key::Backspace => {
                            let mut s = state;
                            s.text.pop();
                            text_edit.set(s);
                        }
                        Key::Character(ref c) => {
                            // Only allow numeric/decimal characters
                            let valid = c.chars().all(|ch| {
                                ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == ' '
                            });
                            if valid {
                                let mut s = state;
                                s.text.push_str(c);
                                text_edit.set(s);
                            }
                        }
                        _ => {}
                    }
                }
            },

            onmousemove: {
                let ctx = ctx.clone();
                move |evt: MouseEvent| {
                    let state = *drag.read();
                    if state.active {
                        if let Some(param_ptr) = state.param_ptr {
                            let shift_held = evt.modifiers().shift();
                            let cur_y = evt.client_coordinates().y;

                            // Re-anchor when shift state changes to avoid jump
                            let (anchor_value, anchor_y) = if shift_held != state.last_shift {
                                // Read current param value as new anchor
                                let current =
                                    unsafe { param_ptr.modulated_normalized_value() } as f64;
                                (current, cur_y)
                            } else {
                                (state.start_value, state.start_y)
                            };

                            let sens = if shift_held {
                                state.sensitivity * FINE_MULTIPLIER
                            } else {
                                state.sensitivity
                            };
                            let delta = (anchor_y - cur_y) / sens;
                            let new_val = (anchor_value + delta).clamp(0.0, 1.0) as f32;
                            ctx.set_normalized_raw(param_ptr, new_val);

                            let mut s = state;
                            s.start_value = anchor_value;
                            s.start_y = anchor_y;
                            s.last_shift = shift_held;
                            s.move_count += 1;
                            drag.set(s);
                        }
                    }
                }
            },

            onmouseup: {
                let ctx = ctx.clone();
                move |_| {
                    let state = *drag.read();
                    if state.active {
                        if let Some(param_ptr) = state.param_ptr {
                            ctx.end_set_raw(param_ptr);
                        }
                        drag.set(DragState::default());
                    }
                }
            },

            {children}
        }
    }
}
