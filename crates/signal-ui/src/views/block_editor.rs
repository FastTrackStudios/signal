//! Block editor view -- parameter editing with colored cards and knobs.
//!
//! Smart component that takes a `BlockType` and renders
//! parameter sliders with color-coded block cards. Composes
//! `components::block_color()` for styling.

use std::f64::consts::PI;

use dioxus::prelude::*;
use dioxus::prelude::dioxus_elements::geometry::WheelDelta;
use signal::{Block, BlockType};

use crate::components::block_color;

// region: --- Cursor warp (macOS)

/// CoreGraphics FFI for cursor control on macOS.
///
/// WKWebView doesn't support the Pointer Lock API, so we use native CG
/// calls to grab/release the cursor. `CGAssociateMouseAndMouseCursorPosition`
/// freezes the cursor at its current position while still delivering mouse
/// events with correct deltas to the webview — this is the same function
/// that `tao::Window::set_cursor_grab` calls under the hood.
#[cfg(target_os = "macos")]
mod cg_cursor {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    /// Guard to ensure exactly one hide is matched with one show,
    /// even if grab() is called multiple times (double-click, re-render).
    static GRABBED: AtomicBool = AtomicBool::new(false);

    /// CG cursor position saved at grab time — in the CoreGraphics global
    /// display coordinate space, so it warps back to the correct monitor.
    static SAVED_POS: Mutex<(f64, f64)> = Mutex::new((0.0, 0.0));

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    // Opaque type — we only hold a pointer.
    enum CGEvent {}

    unsafe extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayHideCursor(display: u32) -> i32;
        fn CGDisplayShowCursor(display: u32) -> i32;
        fn CGWarpMouseCursorPosition(point: CGPoint) -> i32;
        fn CGAssociateMouseAndMouseCursorPosition(connected: i32) -> i32;
        fn CGEventCreate(source: *const core::ffi::c_void) -> *mut CGEvent;
        fn CGEventGetLocation(event: *const CGEvent) -> CGPoint;
        fn CFRelease(cf: *const core::ffi::c_void);
    }

    /// Read the current cursor position in CG global display coordinates.
    /// This coordinate space spans all monitors and is the same space that
    /// `CGWarpMouseCursorPosition` expects, so the cursor always warps back
    /// to the correct display regardless of monitor arrangement or scaling.
    fn cg_cursor_position() -> (f64, f64) {
        unsafe {
            let event = CGEventCreate(core::ptr::null());
            let pos = CGEventGetLocation(event);
            CFRelease(event as *const _);
            (pos.x, pos.y)
        }
    }

    /// Freeze the cursor and hide it. Saves the current CG cursor position
    /// so `release()` can warp back to the correct display.
    /// Safe to call multiple times — only the first call hides the cursor.
    pub fn grab() {
        if GRABBED.swap(true, Ordering::SeqCst) {
            return; // already grabbed
        }
        // Save position in CG coordinates before freezing.
        *SAVED_POS.lock().unwrap() = cg_cursor_position();
        unsafe {
            CGAssociateMouseAndMouseCursorPosition(0);
            CGDisplayHideCursor(CGMainDisplayID());
        }
    }

    /// Unfreeze the cursor, warp it to the saved CG position, and show it.
    /// Safe to call multiple times — only the first call shows the cursor.
    pub fn release() {
        if !GRABBED.swap(false, Ordering::SeqCst) {
            return; // wasn't grabbed
        }
        let (x, y) = *SAVED_POS.lock().unwrap();
        unsafe {
            CGAssociateMouseAndMouseCursorPosition(1);
            CGWarpMouseCursorPosition(CGPoint { x, y });
            CGDisplayShowCursor(CGMainDisplayID());
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod cg_cursor {
    pub fn grab() {}
    pub fn release() {}
}

// endregion: --- Cursor warp (macOS)

// region: --- Arc geometry helpers

const START_ANGLE: f64 = 135.0;
const SWEEP: f64 = 270.0;

fn angle_for_value(v: f64) -> f64 {
    START_ANGLE + v.clamp(0.0, 1.0) * SWEEP
}

fn arc_point(cx: f64, cy: f64, r: f64, angle_deg: f64) -> (f64, f64) {
    let rad = angle_deg * PI / 180.0;
    (cx + r * rad.cos(), cy + r * rad.sin())
}

fn svg_arc(cx: f64, cy: f64, r: f64, start_deg: f64, end_deg: f64) -> String {
    let (x1, y1) = arc_point(cx, cy, r, start_deg);
    let (x2, y2) = arc_point(cx, cy, r, end_deg);
    let large = if (end_deg - start_deg).abs() > 180.0 {
        1
    } else {
        0
    };
    format!("M {x1:.1} {y1:.1} A {r:.1} {r:.1} 0 {large} 1 {x2:.1} {y2:.1}")
}

// endregion: --- Arc geometry helpers

// region: --- BlockEditor

/// A block parameter editor.
///
/// Fetches the current block state from the controller and renders
/// an interactive parameter card with colored header and knobs.
#[component]
pub fn BlockEditor(block_type: BlockType) -> Element {
    let signal = crate::use_signal_service();
    let mut block = use_signal(Block::default);

    {
        let signal = signal.clone();
        use_effect(move || {
            let signal = signal.clone();
            spawn(async move {
                block.set(signal.blocks().get(block_type).await.unwrap_or_default());
            });
        });
    }

    let color = block_color(block_type.as_str());
    let b: Block = block();
    let params = b.parameters().to_vec();

    rsx! {
        div { class: "rounded-lg border overflow-hidden",
            style: "border-color: {color.border};",

            // Colored header
            div {
                class: "flex items-center gap-2 px-4 py-2.5",
                style: "background: linear-gradient(180deg, {color.bg}30 0%, {color.bg}15 100%);",
                div {
                    class: "w-3 h-3 rounded-full",
                    style: "background-color: {color.bg};",
                }
                span { class: "font-semibold text-sm", "{block_type.display_name()}" }
            }

            // Parameters
            div { class: "p-4 space-y-3 bg-zinc-900/50",
                if params.is_empty() {
                    div { class: "text-xs text-zinc-500 text-center py-2", "No parameters" }
                } else {
                    div { class: "grid grid-cols-3 gap-4",
                        for (index, parameter) in params.into_iter().enumerate() {
                            {
                                let label = parameter.name().to_string();
                                let value = parameter.value().get();
                                let row_signal = signal.clone();
                                rsx! {
                                    MiniKnobParam {
                                        key: "{parameter.id()}",
                                        label,
                                        value,
                                        on_change: move |new_val: f32| {
                                            let mut current = block();
                                            current.set_parameter_value(index, new_val);
                                            block.set(current.clone());
                                            let signal = row_signal.clone();
                                            spawn(async move {
                                                let _ = signal.blocks().set(block_type, current).await;
                                            });
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// endregion: --- BlockEditor

// region: --- BlockCard

/// A single block detail card with color-coded header and parameter knobs.
///
/// Unlike `BlockEditor`, this doesn't own a controller — it receives block
/// data directly and reports changes via callbacks. Suitable for embedding
/// in module views.
#[component]
pub fn BlockCard(
    /// Block type key for color lookup.
    block_type_key: String,
    /// Display name.
    name: String,
    /// Whether the block is bypassed.
    #[props(default)]
    bypassed: bool,
    /// Parameter names and values (0.0-1.0).
    parameters: Vec<(String, f32)>,
    /// Callback when bypass is toggled.
    on_toggle_bypass: EventHandler<()>,
    /// Callback when a parameter changes: (param_index, new_value).
    on_param_change: EventHandler<(usize, f32)>,
) -> Element {
    let color = block_color(&block_type_key);

    let container_class = if bypassed {
        "border border-zinc-700/50 rounded-lg overflow-hidden opacity-60"
    } else {
        "border border-zinc-700 rounded-lg overflow-hidden"
    };

    rsx! {
        div { class: "{container_class}",
            // Header
            div {
                class: "flex items-center justify-between px-3 py-2 border-b border-zinc-800",
                style: "background: linear-gradient(180deg, {color.bg}20 0%, {color.bg}10 100%);",

                div { class: "flex items-center gap-2",
                    div {
                        class: "w-3 h-3 rounded-full",
                        style: "background-color: {color.bg};",
                    }
                    span { class: "font-medium text-sm text-zinc-200", "{name}" }
                }

                button {
                    class: if bypassed {
                        "px-2 py-1 text-xs rounded bg-red-500/20 text-red-400 hover:bg-red-500/30"
                    } else {
                        "px-2 py-1 text-xs rounded bg-green-500/20 text-green-400 hover:bg-green-500/30"
                    },
                    onclick: move |_| on_toggle_bypass.call(()),
                    if bypassed { "Bypassed" } else { "Active" }
                }
            }

            // Parameters
            div { class: "p-3 bg-zinc-900/30",
                if parameters.is_empty() {
                    div { class: "text-xs text-zinc-500 text-center py-2", "No parameters" }
                } else {
                    div { class: "grid grid-cols-3 gap-3",
                        for (idx, (param_name, param_value)) in parameters.iter().enumerate() {
                            MiniKnobParam {
                                key: "{idx}",
                                label: param_name.clone(),
                                value: *param_value,
                                on_change: move |new_val: f32| {
                                    on_param_change.call((idx, new_val));
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

// endregion: --- BlockCard

// region: --- MiniKnobParam

/// A labeled parameter knob control with click-to-edit value readout.
#[component]
fn MiniKnobParam(
    label: String,
    value: f32,
    on_change: EventHandler<f32>,
    /// When true, display and parse values as bipolar (-100% to +100%).
    #[props(default)]
    bipolar: bool,
    /// Override the display text (e.g. "2.5 kHz" instead of "50%").
    #[props(default)]
    format_value: Option<String>,
) -> Element {
    // Local display value for immediate visual feedback during drag
    let mut display_value = use_signal(|| value);
    let mut editing = use_signal(|| false);
    let mut edit_text = use_signal(String::new);

    // Sync from props when not being actively dragged
    use_effect(move || {
        display_value.set(value);
    });

    let dv = display_value();

    let display_text = if let Some(ref fmt) = format_value {
        fmt.clone()
    } else if bipolar {
        let pct = ((dv - 0.5) * 200.0) as i32;
        if pct > 0 {
            format!("+{pct}%")
        } else {
            format!("{pct}%")
        }
    } else {
        format!("{}%", (dv * 100.0) as i32)
    };

    let tooltip = format!("{label}: {display_text}");

    rsx! {
        div { class: "flex flex-col items-center gap-1",
            MiniKnob {
                value,
                bipolar,
                tooltip: tooltip,
                on_change: move |new_val: f32| {
                    display_value.set(new_val);
                    on_change.call(new_val);
                },
            }
            span { class: "text-xs text-zinc-400 text-center truncate w-14", "{label}" }

            if editing() {
                input {
                    class: "text-xs font-mono text-zinc-300 text-center bg-zinc-800 border border-zinc-600 rounded w-14 px-1 outline-none focus:border-blue-500",
                    r#type: "text",
                    value: "{edit_text}",
                    autofocus: true,
                    oninput: move |e| {
                        edit_text.set(e.value());
                    },
                    onkeydown: {
                        let bipolar = bipolar;
                        move |e: KeyboardEvent| {
                            if e.key() == Key::Enter {
                                let text = edit_text().trim().replace('%', "");
                                if let Ok(v) = text.parse::<f32>() {
                                    let normalized = if bipolar {
                                        (v / 200.0 + 0.5).clamp(0.0, 1.0)
                                    } else {
                                        (v / 100.0).clamp(0.0, 1.0)
                                    };
                                    display_value.set(normalized);
                                    on_change.call(normalized);
                                }
                                editing.set(false);
                            } else if e.key() == Key::Escape {
                                editing.set(false);
                            }
                        }
                    },
                    onblur: {
                        let bipolar = bipolar;
                        move |_| {
                            let text = edit_text().trim().replace('%', "");
                            if let Ok(v) = text.parse::<f32>() {
                                let normalized = if bipolar {
                                    (v / 200.0 + 0.5).clamp(0.0, 1.0)
                                } else {
                                    (v / 100.0).clamp(0.0, 1.0)
                                };
                                display_value.set(normalized);
                                on_change.call(normalized);
                            }
                            editing.set(false);
                        }
                    },
                }
            } else {
                span {
                    class: "text-xs font-mono text-zinc-300 text-center cursor-text hover:text-zinc-100",
                    onclick: move |_| {
                        edit_text.set(display_text.clone());
                        editing.set(true);
                    },
                    "{display_text}"
                }
            }
        }
    }
}

// endregion: --- MiniKnobParam

// region: --- MiniKnob

/// An SVG rotary knob with drag-to-adjust interaction.
///
/// Hides the cursor during drag for DAW-style knob control. Uses a
/// full-viewport overlay to capture all mouse events and prevent text
/// selection or hover-chain breakage while dragging.
#[component]
pub fn MiniKnob(
    value: f32,
    on_change: EventHandler<f32>,
    /// Accent color for the pointer (e.g. "#F97316"). Defaults to blue.
    #[props(default)]
    color: Option<String>,
    /// Value to reset to on double-click (0.0-1.0). Defaults to 0.5.
    #[props(default = 0.5)]
    default_value: f32,
    /// When true, draw the value arc from center (12 o'clock) outward.
    #[props(default)]
    bipolar: bool,
    /// Native tooltip text shown on hover.
    #[props(default)]
    tooltip: Option<String>,
) -> Element {
    let mut dragging = use_signal(|| false);
    // Local value for immediate pointer feedback during drag
    let mut drag_value = use_signal(|| value);
    // Track last mousedown time for manual double-click detection
    // (ondoubleclick won't fire because the drag overlay blocks the 2nd click)
    let mut last_mousedown = use_signal(std::time::Instant::now);

    // Sync from prop when not dragging
    if !dragging() {
        drag_value.set(value);
    }

    let display = drag_value();

    let size: f64 = 36.0;
    let center: f64 = size / 2.0;
    let radius: f64 = 14.0;

    // Arc geometry
    let track_path = svg_arc(center, center, radius, angle_for_value(0.0), angle_for_value(1.0));

    let value_path = if bipolar {
        let center_angle = angle_for_value(0.5);
        let val_angle = angle_for_value(display as f64);
        if display > 0.501 {
            svg_arc(center, center, radius, center_angle, val_angle)
        } else if display < 0.499 {
            svg_arc(center, center, radius, val_angle, center_angle)
        } else {
            String::new()
        }
    } else if display > 0.001 {
        svg_arc(
            center,
            center,
            radius,
            angle_for_value(0.0),
            angle_for_value(display as f64),
        )
    } else {
        String::new()
    };

    // Bipolar center tick at 12 o'clock
    let (tick_x, tick_y) = arc_point(center, center, radius + 2.0, angle_for_value(0.5));
    let (tick_x2, tick_y2) = arc_point(center, center, radius - 1.0, angle_for_value(0.5));

    let value_angle = 135.0 + (display * 270.0);
    let end_angle = (value_angle as f64).to_radians();

    let accent = color.as_deref().unwrap_or("#3B82F6");

    let pointer_length = radius - 3.0;
    let pointer_end_x = center + pointer_length * end_angle.cos();
    let pointer_end_y = center + pointer_length * end_angle.sin();

    let title_attr = tooltip.as_deref().unwrap_or("").to_string();

    rsx! {
        div {
            title: "{title_attr}",
        svg {
            class: "w-9 h-9 cursor-pointer",
            view_box: "0 0 {size} {size}",

            // Scroll wheel adjustment
            onwheel: move |evt: WheelEvent| {
                evt.prevent_default();
                let dy = match evt.delta() {
                    WheelDelta::Pixels(p) => p.y,
                    WheelDelta::Lines(l) => l.y * 16.0,
                    WheelDelta::Pages(p) => p.y * 160.0,
                };
                let step = if evt.modifiers().contains(keyboard_types::Modifiers::SHIFT) {
                    0.002
                } else {
                    0.01
                };
                let delta = if dy < 0.0 { step } else { -step };
                let new_val = (display + delta as f32).clamp(0.0, 1.0);
                drag_value.set(new_val);
                on_change.call(new_val);
            },

            // Background track arc
            path {
                d: "{track_path}",
                fill: "none",
                stroke: "#374151",
                stroke_width: "3",
                stroke_linecap: "round",
            }

            // Value arc
            if !value_path.is_empty() {
                path {
                    d: "{value_path}",
                    fill: "none",
                    stroke: "{accent}",
                    stroke_width: "3",
                    stroke_linecap: "round",
                }
            }

            // Bipolar center tick mark
            if bipolar {
                line {
                    x1: "{tick_x:.1}",
                    y1: "{tick_y:.1}",
                    x2: "{tick_x2:.1}",
                    y2: "{tick_y2:.1}",
                    stroke: "#6B7280",
                    stroke_width: "1.5",
                    stroke_linecap: "round",
                }
            }

            // Center circle
            circle {
                cx: "{center}",
                cy: "{center}",
                r: "{radius - 4.0}",
                fill: "#1F2937",
            }

            // Pointer
            line {
                x1: "{center}",
                y1: "{center}",
                x2: "{pointer_end_x:.1}",
                y2: "{pointer_end_y:.1}",
                stroke: "{accent}",
                stroke_width: "2",
                stroke_linecap: "round",
            }

            // Hit area — drag starts here, then JS captures all movement
            circle {
                cx: "{center}",
                cy: "{center}",
                r: "{radius + 2.0}",
                fill: "transparent",
                onmousedown: move |e| {
                    // Cmd+click reset (Logic Pro convention)
                    if e.modifiers().contains(keyboard_types::Modifiers::META) {
                        on_change.call(default_value);
                        return;
                    }

                    // Double-click detection: if two mousedowns within 300ms, reset
                    let now = std::time::Instant::now();
                    let prev = last_mousedown();
                    last_mousedown.set(now);
                    if now.duration_since(prev).as_millis() < 300 {
                        on_change.call(default_value);
                        return;
                    }

                    let start_val = display;
                    dragging.set(true);
                    drag_value.set(display);

                    // Freeze + hide cursor at the OS level. The cursor is
                    // pinned in place, but JS movementY still reports deltas.
                    cg_cursor::grab();
                    document::eval("document.body.style.userSelect = 'none';");

                    // Drive the entire drag from JS via movementY deltas.
                    spawn(async move {
                        let mut eval = document::eval(
                            r#"
                            const startVal = await dioxus.recv();
                            const sens = 150;
                            let accumulated = 0;

                            const onMove = (e) => {
                                const step = e.shiftKey ? 5 : 1;
                                accumulated -= e.movementY / step;
                                // Clamp accumulator to the valid range so that
                                // reversing direction responds immediately —
                                // no dead zone past 0% or 100%.
                                const lo = -startVal * sens;
                                const hi = (1 - startVal) * sens;
                                accumulated = Math.max(lo, Math.min(hi, accumulated));
                                dioxus.send(accumulated);
                            };

                            document.addEventListener('mousemove', onMove);

                            await new Promise(resolve => {
                                document.addEventListener('mouseup', () => {
                                    document.removeEventListener('mousemove', onMove);
                                    // Send sentinel so Rust breaks the recv loop immediately
                                    // instead of waiting for the eval channel to close.
                                    dioxus.send("done");
                                    resolve();
                                }, { once: true });
                            });
                        "#,
                        );

                        // Send start value so JS can compute clamp bounds
                        let _ = eval.send(start_val as f64);

                        loop {
                            match eval.recv::<f64>().await {
                                Ok(acc) => {
                                    let new_val = start_val + acc as f32 / 150.0;
                                    drag_value.set(new_val);
                                    on_change.call(new_val);
                                }
                                // "done" sentinel (fails f64 parse) or channel closed
                                Err(_) => break,
                            }
                        }

                        // Drag done — unfreeze, warp to saved CG position, show cursor
                        cg_cursor::release();
                        dragging.set(false);
                        document::eval("document.body.style.userSelect = '';");
                    });
                },
            }
        }

        // Drag overlay — blocks all other UI interactions while dragging.
        // Mouseup is handled by the JS document listener in the eval above.
        if dragging() {
            div {
                class: "fixed inset-0 z-[100]",
                style: "user-select: none; -webkit-user-select: none;",
            }
        }
        }
    }
}

// endregion: --- MiniKnob
