//! XY Pad control widget.
//!
//! Two-dimensional control for parameters like filter cutoff/resonance
//! or spatial positioning.

use crate::prelude::*;

use crate::core::modulation::ModulationRange;
use crate::core::sensitivity::ModifierKeys;
use crate::theming::style::{ControlState, XYPadStyle};

/// XY Pad for two-dimensional parameter control.
///
/// # Example
///
/// ```ignore
/// use audio_controls::prelude::*;
///
/// #[component]
/// fn FilterControl() -> Element {
///     let mut cutoff = use_signal(|| 0.5f32);
///     let mut resonance = use_signal(|| 0.3f32);
///
///     rsx! {
///         XYPad {
///             x: cutoff,
///             y: resonance,
///             size: 200,
///             x_label: Some("Cutoff".to_string()),
///             y_label: Some("Resonance".to_string()),
///         }
///     }
/// }
/// ```
#[component]
pub fn XYPad(
    x: Signal<f32>,
    y: Signal<f32>,
    #[props(default)] default_x: Option<f32>,
    #[props(default)] default_y: Option<f32>,
    #[props(default)] x_modulation: Option<ModulationRange>,
    #[props(default)] y_modulation: Option<ModulationRange>,
    #[props(default = 150)] size: u32,
    #[props(default)] x_label: Option<String>,
    #[props(default)] y_label: Option<String>,
    #[props(default = true)] show_grid: bool,
    #[props(default)] on_begin: Option<Callback<()>>,
    #[props(default)] on_change: Option<Callback<(f32, f32)>>,
    #[props(default)] on_end: Option<Callback<()>>,
    #[props(default)] class: String,
    #[props(default = false)] disabled: bool,
) -> Element {
    let mut is_dragging = use_signal(|| false);
    let mut is_hovered = use_signal(|| false);

    let state = if disabled {
        ControlState::Disabled
    } else if *is_dragging.read() {
        ControlState::Dragging
    } else if *is_hovered.read() {
        ControlState::Hovered
    } else {
        ControlState::Idle
    };

    let style = XYPadStyle::css_vars(state);

    let x_val = x().clamp(0.0, 1.0);
    let y_val = y().clamp(0.0, 1.0);

    // Cursor position (Y is inverted: top = 1.0)
    let cursor_x = x_val * 100.0;
    let cursor_y = (1.0 - y_val) * 100.0;

    let default_x = default_x.unwrap_or(0.5);
    let default_y = default_y.unwrap_or(0.5);

    // Modulation indicators
    let x_mod = x_modulation.filter(|m| m.active);
    let y_mod = y_modulation.filter(|m| m.active);

    rsx! {
        div {
            class: "xy-pad-container flex flex-col gap-1 {class}",

            // Labels
            if x_label.is_some() || y_label.is_some() {
                div {
                    class: "flex justify-between text-xs text-muted-foreground",
                    span { "{x_label.as_deref().unwrap_or(\"\")}" }
                    span { "{y_label.as_deref().unwrap_or(\"\")}" }
                }
            }

            // Pad
            div {
                class: "relative rounded-lg overflow-hidden",
                style: "width: {size}px; height: {size}px; background: {style.background_color};",

                // Grid lines
                if show_grid && style.show_grid {
                    // Vertical center
                    div {
                        class: "absolute top-0 h-full",
                        style: "left: 50%; width: 1px; background: {style.grid_color}; opacity: 0.5;",
                    }
                    // Horizontal center
                    div {
                        class: "absolute left-0 w-full",
                        style: "top: 50%; height: 1px; background: {style.grid_color}; opacity: 0.5;",
                    }
                    // Quarters
                    div {
                        class: "absolute top-0 h-full",
                        style: "left: 25%; width: 1px; background: {style.grid_color}; opacity: 0.25;",
                    }
                    div {
                        class: "absolute top-0 h-full",
                        style: "left: 75%; width: 1px; background: {style.grid_color}; opacity: 0.25;",
                    }
                    div {
                        class: "absolute left-0 w-full",
                        style: "top: 25%; height: 1px; background: {style.grid_color}; opacity: 0.25;",
                    }
                    div {
                        class: "absolute left-0 w-full",
                        style: "top: 75%; height: 1px; background: {style.grid_color}; opacity: 0.25;",
                    }
                }

                // X-axis modulation range
                if let Some(ref m) = x_mod {
                    div {
                        class: "absolute bottom-0 h-2 rounded-t",
                        style: "left: {m.start * 100.0}%; width: {(m.end - m.start) * 100.0}%; background: {style.modulation_color}; opacity: 0.5;",
                    }
                }

                // Y-axis modulation range
                if let Some(ref m) = y_mod {
                    div {
                        class: "absolute left-0 w-2 rounded-r",
                        style: "bottom: {m.start * 100.0}%; height: {(m.end - m.start) * 100.0}%; background: {style.modulation_color}; opacity: 0.5;",
                    }
                }

                // Crosshairs
                div {
                    class: "absolute h-full pointer-events-none",
                    style: "left: {cursor_x}%; width: 1px; background: {style.cursor_color}; opacity: 0.3;",
                }
                div {
                    class: "absolute w-full pointer-events-none",
                    style: "top: {cursor_y}%; height: 1px; background: {style.cursor_color}; opacity: 0.3;",
                }

                // Cursor dot
                div {
                    class: "absolute rounded-full pointer-events-none border-2 border-background shadow-lg",
                    style: "width: {style.cursor_size}px; height: {style.cursor_size}px; background: {style.cursor_color}; left: calc({cursor_x}% - {style.cursor_size / 2.0}px); top: calc({cursor_y}% - {style.cursor_size / 2.0}px);",
                }

                // Interaction overlay
                div {
                    class: if disabled { "absolute inset-0 cursor-not-allowed" } else { "absolute inset-0 cursor-crosshair" },

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
                        if let Some(cb) = on_begin {
                            cb.call(());
                        }
                        // Set value immediately on click
                        update_xy_from_event(&evt, size, x, y, on_change);
                    },
                    onmousemove: move |evt: MouseEvent| {
                        if !*is_dragging.read() || disabled {
                            return;
                        }
                        update_xy_from_event(&evt, size, x, y, on_change);
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
                        x.set(default_x);
                        y.set(default_y);
                        if let Some(cb) = on_change {
                            cb.call((default_x, default_y));
                        }
                    },
                }
            }
        }
    }
}

/// Update X and Y values from mouse event.
fn update_xy_from_event(
    evt: &MouseEvent,
    size: u32,
    mut x: Signal<f32>,
    mut y: Signal<f32>,
    on_change: Option<Callback<(f32, f32)>>,
) {
    // Get relative position within the element
    // Note: This is a simplified version. In production, you'd use
    // element.getBoundingClientRect() for accurate positioning.
    let offset_x = evt.element_coordinates().x as f32;
    let offset_y = evt.element_coordinates().y as f32;

    let modifiers = ModifierKeys::new(
        evt.modifiers().shift(),
        evt.modifiers().ctrl(),
        evt.modifiers().alt(),
    );

    let size_f = size as f32;
    let mut new_x = (offset_x / size_f).clamp(0.0, 1.0);
    let mut new_y: f32 = (1.0 - offset_y / size_f).clamp(0.0, 1.0); // Invert Y

    // Fine control snaps to grid
    if modifiers.fine_control() {
        new_x = (new_x * 20.0).round() / 20.0; // Snap to 5% grid
        new_y = (new_y * 20.0).round() / 20.0;
    }

    x.set(new_x);
    y.set(new_y);

    if let Some(cb) = on_change {
        cb.call((new_x, new_y));
    }
}
