//! Generic read-only pan/zoom viewport wrapper.
//!
//! Wraps arbitrary child content in a pannable, zoomable canvas.
//! Follows the `DynamicGridView` pattern from the legacy crate:
//! - `transform: scale()` with `transform-origin: 0 0` for zoom
//! - `position: absolute; left/top` for pan
//! - `onmounted` + `get_client_rect()` for viewport measurement
//! - Ctrl+scroll = cursor-anchored zoom, scroll = pan

use std::rc::Rc;

use dioxus::prelude::dioxus_elements::geometry::WheelDelta;
use dioxus::prelude::*;

// region: --- Auto-fit

/// Calculate zoom and pan to fit content centered in the viewport with padding.
fn calculate_fit(
    content_w: f64,
    content_h: f64,
    viewport_w: f64,
    viewport_h: f64,
) -> (f64, f64, f64) {
    let padding = 20.0;
    let avail_w = (viewport_w - padding * 2.0).max(1.0);
    let avail_h = (viewport_h - padding * 2.0).max(1.0);
    if content_w <= 0.0 || content_h <= 0.0 {
        return (1.0, 0.0, 0.0);
    }
    let fit_zoom = (avail_w / content_w)
        .min(avail_h / content_h)
        .clamp(0.1, 2.0);
    let scaled_w = content_w * fit_zoom;
    let scaled_h = content_h * fit_zoom;
    let pan_x = (viewport_w - scaled_w) / 2.0;
    let pan_y = (viewport_h - scaled_h) / 2.0;
    (fit_zoom, pan_x, pan_y)
}

// endregion: --- Auto-fit

// region: --- PanZoomCanvas

/// A generic pan/zoom viewport for read-only content.
///
/// Features:
/// - **Pan**: Left-click drag on background
/// - **Zoom**: Ctrl/Meta + scroll (cursor-anchored), factor 1.08, clamped 0.1–3.0
/// - **Scroll pan**: Normal scroll = vertical, Shift+scroll = horizontal
/// - **Auto-fit**: Content centered on mount
/// - **Zoom indicator**: Bottom-right percentage badge
#[component]
pub fn PanZoomCanvas(
    /// Width of the inner content in pixels (at zoom=1.0).
    content_width: f64,
    /// Height of the inner content in pixels (at zoom=1.0).
    content_height: f64,
    /// Child elements to render inside the pannable/zoomable area.
    children: Element,
) -> Element {
    // Viewport state
    let mut pan_x = use_signal(|| 0.0f64);
    let mut pan_y = use_signal(|| 0.0f64);
    let mut zoom = use_signal(|| 1.0f64);
    let mut viewport_w = use_signal(|| 0.0f64);
    let mut viewport_h = use_signal(|| 0.0f64);
    let mut viewport_left = use_signal(|| 0.0f64);
    let mut viewport_top = use_signal(|| 0.0f64);

    // Pan interaction state
    let mut is_panning = use_signal(|| false);
    let mut pan_start_mouse = use_signal(|| (0.0f64, 0.0f64));
    let mut pan_start_offset = use_signal(|| (0.0f64, 0.0f64));

    // Mounted element for viewport measurement
    let mut mounted_el: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    // Auto-fit tracking
    let mut has_fitted = use_signal(|| false);
    let mut last_content = use_signal(|| (0.0f64, 0.0f64));

    // Helper: re-measure container
    let update_viewport = move || {
        if let Some(el) = mounted_el.read().as_ref() {
            let el_clone = el.clone();
            spawn(async move {
                if let Ok(rect) = el_clone.get_client_rect().await {
                    let w = rect.width();
                    let h = rect.height();
                    if w > 0.0 && h > 0.0 {
                        viewport_w.set(w);
                        viewport_h.set(h);
                        viewport_left.set(rect.origin.x);
                        viewport_top.set(rect.origin.y);
                    }
                }
            });
        }
    };

    // Auto-fit when content size changes or on first render
    let content_changed = {
        let (lw, lh) = last_content();
        (lw - content_width).abs() > 0.5 || (lh - content_height).abs() > 0.5
    };
    if (!has_fitted() || content_changed) && viewport_w() > 0.0 && viewport_h() > 0.0 {
        has_fitted.set(true);
        last_content.set((content_width, content_height));
        let (fit_zoom, fit_pan_x, fit_pan_y) =
            calculate_fit(content_width, content_height, viewport_w(), viewport_h());
        zoom.set(fit_zoom);
        pan_x.set(fit_pan_x);
        pan_y.set(fit_pan_y);
    }

    let cursor = if is_panning() { "grabbing" } else { "grab" };
    let zoom_pct = (zoom() * 100.0) as i32;
    let cw = content_width;
    let ch = content_height;

    rsx! {
        div {
            class: "relative w-full h-full overflow-hidden select-none",
            style: "cursor: {cursor}; \
                    background-color: #09090b; \
                    background-image: radial-gradient(circle, #1a1a2e 1px, transparent 1px); \
                    background-size: 20px 20px;",

            onmounted: move |evt: MountedEvent| {
                mounted_el.set(Some(evt.data()));
                update_viewport();
            },

            // Pan start
            onmousedown: move |evt| {
                evt.prevent_default();
                is_panning.set(true);
                pan_start_mouse.set((evt.client_coordinates().x, evt.client_coordinates().y));
                pan_start_offset.set((pan_x(), pan_y()));
            },

            // Pan move
            onmousemove: move |evt| {
                if is_panning() {
                    let (sx, sy) = pan_start_mouse();
                    let (spx, spy) = pan_start_offset();
                    let dx = evt.client_coordinates().x - sx;
                    let dy = evt.client_coordinates().y - sy;
                    pan_x.set(spx + dx);
                    pan_y.set(spy + dy);
                }
            },

            // Pan stop
            onmouseup: move |_| {
                is_panning.set(false);
            },
            onmouseleave: move |_| {
                is_panning.set(false);
            },

            // Scroll: zoom (ctrl) or pan (normal/shift)
            onwheel: move |evt| {
                evt.prevent_default();
                update_viewport();

                let delta = evt.delta();
                let damp = 0.35;
                let (raw_dx, raw_dy) = match delta {
                    WheelDelta::Pixels(p) => (p.x * damp, p.y * damp),
                    WheelDelta::Lines(l) => (l.x * 16.0, l.y * 16.0),
                    WheelDelta::Pages(p) => (p.x * 160.0, p.y * 160.0),
                };

                let modifiers = evt.modifiers();
                let is_ctrl = modifiers.contains(keyboard_types::Modifiers::CONTROL)
                    || modifiers.contains(keyboard_types::Modifiers::META);
                let is_shift = modifiers.contains(keyboard_types::Modifiers::SHIFT);

                if is_ctrl {
                    // Cursor-anchored zoom
                    let old_zoom = zoom();
                    let zoom_factor = if raw_dy < 0.0 { 1.08 } else { 1.0 / 1.08 };
                    let new_zoom = (old_zoom * zoom_factor).clamp(0.1, 3.0);

                    let local_x = evt.client_coordinates().x - viewport_left();
                    let local_y = evt.client_coordinates().y - viewport_top();
                    let canvas_x = (local_x - pan_x()) / old_zoom;
                    let canvas_y = (local_y - pan_y()) / old_zoom;
                    pan_x.set(local_x - canvas_x * new_zoom);
                    pan_y.set(local_y - canvas_y * new_zoom);
                    zoom.set(new_zoom);
                } else if is_shift {
                    // Shift+scroll → horizontal pan
                    pan_x.set(pan_x() - raw_dy);
                    pan_y.set(pan_y() - raw_dx);
                } else {
                    // Normal scroll → pan
                    pan_y.set(pan_y() - raw_dy);
                    pan_x.set(pan_x() - raw_dx);
                }
            },

            // Canvas layer (positioned + zoomed)
            div {
                style: "position: absolute; left: {pan_x()}px; top: {pan_y()}px; \
                        transform: scale({zoom()}); transform-origin: 0 0;",

                div {
                    style: "width: {cw}px; height: {ch}px; position: relative;",
                    {children}
                }
            }

            // Zoom indicator
            div {
                class: "absolute bottom-2 right-2 px-2 py-0.5 rounded text-[10px] \
                        font-mono text-zinc-500 select-none pointer-events-none",
                style: "background-color: rgba(0,0,0,0.5);",
                "{zoom_pct}%"
            }
        }
    }
}

// endregion: --- PanZoomCanvas
