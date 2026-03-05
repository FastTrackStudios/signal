//! Block editor view -- parameter editing with colored cards and knobs.
//!
//! Smart component that takes a `BlockType` and renders
//! parameter sliders with color-coded block cards. Composes
//! `components::block_color()` for styling.

use dioxus::prelude::*;
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

    /// Guard to ensure exactly one hide is matched with one show,
    /// even if grab() is called multiple times (double-click, re-render).
    static GRABBED: AtomicBool = AtomicBool::new(false);

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    unsafe extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayHideCursor(display: u32) -> i32;
        fn CGDisplayShowCursor(display: u32) -> i32;
        fn CGWarpMouseCursorPosition(point: CGPoint) -> i32;
        fn CGAssociateMouseAndMouseCursorPosition(connected: i32) -> i32;
    }

    /// Freeze the cursor and hide it. The mouse can move freely but the
    /// cursor stays pinned. JS `movementY` still reports correct deltas.
    /// Safe to call multiple times — only the first call hides the cursor.
    pub fn grab() {
        if GRABBED.swap(true, Ordering::SeqCst) {
            return; // already grabbed
        }
        unsafe {
            CGAssociateMouseAndMouseCursorPosition(0);
            CGDisplayHideCursor(CGMainDisplayID());
        }
    }

    /// Unfreeze the cursor, warp it to the saved position, and show it.
    /// Safe to call multiple times — only the first call shows the cursor.
    pub fn release(x: f64, y: f64) {
        if !GRABBED.swap(false, Ordering::SeqCst) {
            return; // wasn't grabbed
        }
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
    pub fn release(_x: f64, _y: f64) {}
}

// endregion: --- Cursor warp (macOS)

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

/// A labeled parameter knob control.
#[component]
fn MiniKnobParam(label: String, value: f32, on_change: EventHandler<f32>) -> Element {
    // Local display value for immediate visual feedback during drag
    let mut display_value = use_signal(|| value);

    // Sync from props when not being actively dragged
    use_effect(move || {
        display_value.set(value);
    });

    let dv = display_value();

    rsx! {
        div { class: "flex flex-col items-center gap-1",
            MiniKnob {
                value,
                on_change: move |new_val: f32| {
                    display_value.set(new_val);
                    on_change.call(new_val);
                },
            }
            span { class: "text-xs text-zinc-400 text-center truncate w-14", "{label}" }
            span { class: "text-xs font-mono text-zinc-300 text-center",
                "{(dv * 100.0) as i32}%"
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
) -> Element {
    let mut dragging = use_signal(|| false);
    // Local value for immediate pointer feedback during drag
    let mut drag_value = use_signal(|| value);
    // Saved screen position — used by both eval path and safety fallback
    let mut saved_screen_x = use_signal(|| 0.0f64);
    let mut saved_screen_y = use_signal(|| 0.0f64);

    // Sync from prop when not dragging
    if !dragging() {
        drag_value.set(value);
    }

    let display = drag_value();

    let size = 36.0;
    let center = size / 2.0;
    let radius = 14.0;
    let stroke_width = 3.0;

    let value_angle = 135.0 + (display * 270.0);
    let end_angle: f32 = value_angle.to_radians();

    let accent = color.as_deref().unwrap_or("#3B82F6");

    let pointer_length = radius - 3.0;
    let pointer_end_x = center + pointer_length * end_angle.cos();
    let pointer_end_y = center + pointer_length * end_angle.sin();

    rsx! {
        svg {
            class: "w-9 h-9 cursor-pointer",
            view_box: "0 0 {size} {size}",

            // Background track
            circle {
                cx: "{center}",
                cy: "{center}",
                r: "{radius}",
                fill: "none",
                stroke: "#374151",
                stroke_width: "{stroke_width}",
                stroke_linecap: "round",
                stroke_dasharray: "159 60",
                transform: "rotate(135 {center} {center})",
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
                x2: "{pointer_end_x}",
                y2: "{pointer_end_y}",
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
                    let start_val = display;
                    saved_screen_x.set(e.screen_coordinates().x);
                    saved_screen_y.set(e.screen_coordinates().y);
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
                                accumulated -= e.movementY;
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

                        // Drag done — unfreeze, warp to start position, show cursor
                        cg_cursor::release(saved_screen_x(), saved_screen_y());
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

// endregion: --- MiniKnob
