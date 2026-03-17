//! Chart Renderer Component
//!
//! Renders keyflow charts using WebGPU via Vello with pan/zoom interaction.

use dioxus::prelude::*;

/// Layout mode for chart rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutMode {
    /// Content-sized layout (measures don't fill line)
    #[default]
    Snippet,
    /// A4 page layout with Master Rhythm preset (measures fill line)
    Page,
}

/// Chart renderer component that displays a keyflow chart.
///
/// # Props
/// - `source`: The keyflow source code to render
/// - `layout_mode`: Optional layout mode (default: Snippet)
#[component]
pub fn ChartRenderer(source: &'static str, #[props(default)] layout_mode: LayoutMode) -> Element {
    // Parse the chart to validate it
    let parse_result = keyflow::parse(source);

    match parse_result {
        Ok(chart) => {
            // Get chart metadata for display
            let title = chart.metadata.title.as_deref().unwrap_or("Untitled");
            let section_count = chart.sections.len();

            rsx! {
                div {
                    class: "w-full h-full relative",

                    // Canvas - fills entire container
                    canvas {
                        id: "chart-canvas",
                        class: "w-full h-full bg-gray-700 cursor-grab active:cursor-grabbing",
                        style: "touch-action: none;",
                    }

                    // Info overlay
                    div {
                        class: "absolute bottom-2 left-2 text-xs text-gray-400 pointer-events-none",
                        "{title} - {section_count} section(s)"
                    }

                    // WebGPU initialization and interaction
                    ChartCanvas { source: source, layout_mode: layout_mode }
                }
            }
        }
        Err(e) => {
            rsx! {
                div {
                    class: "w-full h-full flex items-center justify-center text-red-400",

                    div {
                        class: "text-center",

                        div {
                            class: "text-lg mb-2",
                            "Parse Error"
                        }

                        div {
                            class: "text-sm text-red-300",
                            "{e}"
                        }
                    }
                }
            }
        }
    }
}

/// Canvas component with WebGPU rendering setup and mouse interaction.
#[component]
fn ChartCanvas(source: &'static str, #[props(default)] layout_mode: LayoutMode) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        use crate::renderer::ChartLayoutManager;
        use wasm_bindgen::JsCast;
        use wasm_bindgen::prelude::*;

        // Create layout manager signal
        let mut layout_manager = use_signal(|| None::<ChartLayoutManager>);
        let mut error_state = use_signal(|| None::<String>);
        let mut is_initialized = use_signal(|| false);

        // Transform state for pan/zoom
        let mut transform_x = use_signal(|| 20.0_f64);
        let mut transform_y = use_signal(|| 20.0_f64);
        let mut scale = use_signal(|| 1.0_f64);

        // Mouse interaction state
        let mut is_dragging = use_signal(|| false);
        let mut last_mouse_x = use_signal(|| 0.0_f64);
        let mut last_mouse_y = use_signal(|| 0.0_f64);

        // Trigger re-render
        let mut render_trigger = use_signal(|| 0_u32);

        // Initialize WebGPU on mount
        use_effect(move || {
            wasm_bindgen_futures::spawn_local(async move {
                // Initialize layout manager
                match ChartLayoutManager::new() {
                    Ok(manager) => {
                        layout_manager.set(Some(manager));
                        is_initialized.set(true);
                        tracing::info!("Chart layout manager initialized");
                    }
                    Err(e) => {
                        error_state.set(Some(e));
                        tracing::error!("Failed to initialize chart layout manager");
                    }
                }
            });
        });

        // Setup mouse event listeners
        use_effect(move || {
            let window = match web_sys::window() {
                Some(w) => w,
                None => return,
            };
            let document = match window.document() {
                Some(d) => d,
                None => return,
            };
            let canvas = match document.get_element_by_id("chart-canvas") {
                Some(c) => c,
                None => return,
            };

            // Mouse down - start dragging
            let mut is_dragging_clone = is_dragging.clone();
            let mut last_mouse_x_clone = last_mouse_x.clone();
            let mut last_mouse_y_clone = last_mouse_y.clone();
            let mousedown_closure = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
                is_dragging_clone.set(true);
                last_mouse_x_clone.set(event.client_x() as f64);
                last_mouse_y_clone.set(event.client_y() as f64);
            }) as Box<dyn FnMut(_)>);
            canvas
                .add_event_listener_with_callback(
                    "mousedown",
                    mousedown_closure.as_ref().unchecked_ref(),
                )
                .ok();
            mousedown_closure.forget();

            // Mouse move - drag to pan
            let is_dragging_clone = is_dragging.clone();
            let mut last_mouse_x_clone = last_mouse_x.clone();
            let mut last_mouse_y_clone = last_mouse_y.clone();
            let mut transform_x_clone = transform_x.clone();
            let mut transform_y_clone = transform_y.clone();
            let mut render_trigger_clone = render_trigger.clone();
            let mousemove_closure = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
                if *is_dragging_clone.read() {
                    let last_x = *last_mouse_x_clone.read();
                    let last_y = *last_mouse_y_clone.read();
                    let dx = event.client_x() as f64 - last_x;
                    let dy = event.client_y() as f64 - last_y;
                    let cur_tx = *transform_x_clone.read();
                    let cur_ty = *transform_y_clone.read();
                    transform_x_clone.set(cur_tx + dx);
                    transform_y_clone.set(cur_ty + dy);
                    last_mouse_x_clone.set(event.client_x() as f64);
                    last_mouse_y_clone.set(event.client_y() as f64);
                    let trigger = *render_trigger_clone.read();
                    render_trigger_clone.set(trigger.wrapping_add(1));
                }
            }) as Box<dyn FnMut(_)>);
            canvas
                .add_event_listener_with_callback(
                    "mousemove",
                    mousemove_closure.as_ref().unchecked_ref(),
                )
                .ok();
            mousemove_closure.forget();

            // Mouse up - stop dragging
            let mut is_dragging_clone = is_dragging.clone();
            let mouseup_closure = Closure::wrap(Box::new(move |_event: web_sys::MouseEvent| {
                is_dragging_clone.set(false);
            }) as Box<dyn FnMut(_)>);
            window
                .add_event_listener_with_callback(
                    "mouseup",
                    mouseup_closure.as_ref().unchecked_ref(),
                )
                .ok();
            mouseup_closure.forget();

            // Mouse wheel - zoom
            let mut scale_clone = scale.clone();
            let mut transform_x_clone = transform_x.clone();
            let mut transform_y_clone = transform_y.clone();
            let mut render_trigger_clone = render_trigger.clone();
            let wheel_closure = Closure::wrap(Box::new(move |event: web_sys::WheelEvent| {
                event.prevent_default();

                let delta = -event.delta_y() / 500.0;
                let old_scale = *scale_clone.read();
                let new_scale = (old_scale * (1.0 + delta)).clamp(0.25, 4.0);

                // Zoom towards mouse position
                let rect = event
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                    .map(|e| e.get_bounding_client_rect());

                if let Some(rect) = rect {
                    let mouse_x = event.client_x() as f64 - rect.left();
                    let mouse_y = event.client_y() as f64 - rect.top();

                    let scale_change = new_scale / old_scale;
                    let cur_tx = *transform_x_clone.read();
                    let cur_ty = *transform_y_clone.read();
                    let new_tx = mouse_x - (mouse_x - cur_tx) * scale_change;
                    let new_ty = mouse_y - (mouse_y - cur_ty) * scale_change;

                    transform_x_clone.set(new_tx);
                    transform_y_clone.set(new_ty);
                }

                scale_clone.set(new_scale);
                let trigger = *render_trigger_clone.read();
                render_trigger_clone.set(trigger.wrapping_add(1));
            }) as Box<dyn FnMut(_)>);
            let mut wheel_options = web_sys::AddEventListenerOptions::new();
            wheel_options.set_passive(false);
            canvas
                .add_event_listener_with_callback_and_add_event_listener_options(
                    "wheel",
                    wheel_closure.as_ref().unchecked_ref(),
                    &wheel_options,
                )
                .ok();
            wheel_closure.forget();
        });

        // Layout and render when manager is ready or transform changes
        use_effect(move || {
            if !*is_initialized.read() {
                return;
            }

            // Read transform values
            let tx = *transform_x.read();
            let ty = *transform_y.read();
            let s = *scale.read();
            let _trigger = *render_trigger.read(); // Dependency for re-render

            let source = source.to_string();

            wasm_bindgen_futures::spawn_local(async move {
                if let Some(ref mut manager) = *layout_manager.write() {
                    // Parse chart
                    if let Ok(chart) = keyflow::parse(source.as_str()) {
                        // Get canvas and its display size
                        if let Some(window) = web_sys::window() {
                            let dpr = window.device_pixel_ratio();

                            if let Some(document) = window.document() {
                                if let Some(canvas) = document.get_element_by_id("chart-canvas") {
                                    if let Ok(html_canvas) =
                                        canvas.dyn_into::<web_sys::HtmlCanvasElement>()
                                    {
                                        // Get CSS size
                                        let rect = html_canvas.get_bounding_client_rect();
                                        let css_width = rect.width();
                                        let css_height = rect.height();

                                        // Set canvas buffer size for high DPI
                                        let buffer_width = (css_width * dpr) as u32;
                                        let buffer_height = (css_height * dpr) as u32;
                                        html_canvas.set_width(buffer_width);
                                        html_canvas.set_height(buffer_height);

                                        // Layout chart at CSS size with appropriate mode
                                        let is_snippet = layout_mode == LayoutMode::Snippet;
                                        manager
                                            .layout_chart_with_mode(&chart, css_width, is_snippet);

                                        // Render with transform (scaled by DPR)
                                        if let Err(e) = manager
                                            .render_to_canvas_with_transform(
                                                &html_canvas,
                                                tx * dpr,
                                                ty * dpr,
                                                s * dpr,
                                            )
                                            .await
                                        {
                                            tracing::error!("Failed to render chart: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            });
        });

        // Show error if initialization failed
        if let Some(error) = error_state.read().as_ref() {
            return rsx! {
                div {
                    class: "absolute inset-0 flex items-center justify-center text-yellow-400 text-sm",
                    "WebGPU not available: {error}"
                }
            };
        }

        // Show loading state
        if !*is_initialized.read() {
            return rsx! {
                div {
                    class: "absolute inset-0 flex items-center justify-center text-gray-400",
                    "Initializing WebGPU..."
                }
            };
        }

        rsx! {}
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        rsx! {
            div {
                class: "absolute inset-0 flex items-center justify-center text-gray-400",
                "Chart rendering requires WebGPU (browser only)"
            }
        }
    }
}
