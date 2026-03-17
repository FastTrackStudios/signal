//! Chart Editor Component
//!
//! Interactive split-view editor with live chart preview.

use dioxus::dioxus_core::Task;
use dioxus::prelude::*;
use fts_ui::prelude::dropdown::*;
use keyflow::highlighting::{HighlightKind, Highlighter};

// =============================================================================
// Example Charts
// =============================================================================

/// Empty chart template
const EMPTY_CHART: &str = r#"New Song
120bpm 4/4 #A

VS
A D F#m // E // D


"#;

/// Thriller - Dirty Loops, Cory Wong cover arrangement
/// Demonstrates push/pull triplets and complex rhythm notation
const EXAMPLE_THRILLER: &str = r#"Thriller - Dirty Loops, Cory Wong
Transcribed By: Cody Wright
120bpm 4/4 #Ab
/push = triplet

COUNT 2

HITS
r8t >Ab9_8t r8t r8t r8t >F9_8t r2 | s1

IN
>'Cm . . . 

VS
>'F/C . Cm . 'F/C . Cm . 'F/C . Cm . 'F/C . Cm Cm9


CH
>Cm/Eb / 'Eb /// | 'Eb / 'F/C / 'Cm // | 'F/A //// | 'Fm9  ////
>Cm/Eb / 'Eb /// | 'Eb / 'F/C / 'Cm // | 'F/A | r8t >Ab9_8t r8t r8t >'F9_8t r8t r4 >Fm/Ab_4

INST 4
Cm . F6 // Abdim7 'Csus2 // 'C5 //

VS
F/C . Cm . 'F/C . Cm . 'F/C . 'Cm . 'F/C . Cm // Gm7 // 'Abmaj7 / Abmaj7#5 / 'Db7#11/G //

CH
>Cm/Eb / 'Eb /// | 'Eb / 'F/C / 'Cm // | 'F/A //// | 'Fm9  ////
>Cm/Eb / 'Eb /// | 'Eb / 'F/C / 'Cm // | 'F/A | r8t >Ab9_8t r8t r8t >'F9_8t r8t r4 >Fm/Ab_4 | s1


BR
>'_4F7 | . |  Abmaj9 //// | // r8t >Abmaj9_8t r8t >Bb_8t r8t >Cm7_8t | Cm7 | Ebmaj7/Bb | Am7b5 | Abmaj7 | G7sus4 | 'G7

VS
>'F/C . Cm . 'F/C . Cm Db7#11 // 'Cmaj9 'F7 //// F7 // 'Fmaj7/C 'Cm7 . 
'F 'Am // 'Dbmaj7 // 'Gmaj7 // 'Fm11 // 'Eb9 / Bbm/F / 'Gb7b9 //

CH
>'Cm/Eb / 'Eb /// | 'Eb / 'F/C / 'Cm // | 'F/A //// | 'Fm9  ////
>Cm/Eb / 'Eb /// | 'Eb / 'F/C / 'Cm // | 'F/A | 
r8t >Ab9_8t r8t r8t >'F9_8t r8t r4 r8t >Bb9_8t r8t 
r8t r8t C7#11_8t r4 r8t Dbmaj7_8t r8t r4 
'Bb11

CH
>Cm/Eb / 'Eb /// | 'Eb / 'F/C / 'Cm // | 'F/A //// | 'Fm9  ////
>Cm/Eb / 'Eb /// | 'Eb / 'F/C / 'Cm // | 'F/A | r8t >Ab9_8t r8t r8t >'F9_8t r8t r4 >Cm

Interlude 8
Cm . . . . . . .

Interlude 8 "HORNS"
/push 4
'Cm . 'Cm7b5 . 'Cm Cm/maj7  'B/C . 

Interlude 8 "WINDS"
C C+ // C // Cm7b5 Cmaj7 
'Cmaj7 . Fm/C Cdim7 

Interlude 8 "TRUMPETS"
Fm6 . 'Dbmaj7/F .  D/F . B7/F . 

Outro 8
Em7b5/D 'Dmaj9 x3
Gm7/D 'D11 

Outro 8
'Gm7/D 'Dadd9 'Em7b5 'Dadd9 
'Em7b5 'Dadd9 'Gm9/Bb 'Fmaj9/C 

Hits 4
'C#/G . . .  

"#;

/// Default chart content - start with an example
const DEFAULT_CHART: &str = EXAMPLE_THRILLER;

/// Preview mode - Snippet (content-sized) or Page (A4)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewMode {
    Snippet,
    Page,
}

/// Chart editor with live preview.
#[component]
pub fn ChartEditor() -> Element {
    // Source text state
    let mut source = use_signal(|| DEFAULT_CHART.to_string());
    // Preview mode state - default to Page (A4) mode
    let mut preview_mode = use_signal(|| PreviewMode::Page);

    let is_snippet = *preview_mode.read() == PreviewMode::Snippet;

    rsx! {
        div {
            class: "grid grid-cols-2 h-[calc(100vh-4rem)]",

            // Left side - Text editor
            div {
                class: "flex flex-col border-r border-border overflow-hidden",

                // Editor header
                div {
                    class: "px-4 py-3 border-b border-border bg-card flex items-center justify-between shrink-0",

                    div {
                        class: "flex items-center gap-2",
                        lucide_dioxus::FileText { class: "w-4 h-4 text-primary" }
                        h2 {
                            class: "text-sm font-semibold text-foreground",
                            "Keyflow Source"
                        }
                    }

                    div {
                        class: "flex items-center gap-2",

                        // Examples dropdown
                        Dropdown {
                            DropdownTrigger {
                                class: "text-xs text-muted-foreground bg-background px-3 py-1.5 rounded-md border border-border hover:border-primary/50 focus:outline-none focus:ring-1 focus:ring-primary transition-colors cursor-pointer inline-flex items-center gap-1",
                                "Examples"
                                fts_ui::lucide_dioxus::ChevronDown { class: "w-3 h-3" }
                            }
                            DropdownContent { width: "w-72".to_string(),
                                DropdownItem::<String> {
                                    value: "empty".to_string(),
                                    on_select: move |_: String| source.set(EMPTY_CHART.to_string()),
                                    "New (Empty)"
                                }
                                DropdownItem::<String> {
                                    value: "thriller".to_string(),
                                    on_select: move |_: String| source.set(EXAMPLE_THRILLER.to_string()),
                                    "Thriller - Dirty Loops, Cory Wong"
                                }
                            }
                        }

                        button {
                            class: "text-xs text-muted-foreground hover:text-foreground px-3 py-1.5 rounded-md hover:bg-accent transition-colors border border-border",
                            onclick: move |_| source.set(DEFAULT_CHART.to_string()),
                            "Reset"
                        }
                    }
                }

                // Code editor with syntax highlighting
                div {
                    class: "flex-1 overflow-hidden",

                    HighlightedEditor {
                        value: source(),
                        on_change: move |new_value: String| source.set(new_value),
                        placeholder: "Enter keyflow chart notation..."
                    }
                }
            }

            // Right side - Preview
            div {
                class: "flex flex-col bg-muted/30 overflow-hidden",

                // Preview header with mode toggle
                div {
                    class: "px-4 py-3 border-b border-border bg-card flex items-center justify-between shrink-0",

                    // Left side - title
                    div {
                        class: "flex items-center gap-2",
                        lucide_dioxus::Eye { class: "w-4 h-4 text-primary" }
                        h2 {
                            class: "text-sm font-semibold text-foreground",
                            "Live Preview"
                        }
                    }

                    // Right side - mode toggle and export
                    div {
                        class: "flex items-center gap-3",

                        // Mode toggle
                        div {
                            class: "flex items-center gap-1 bg-muted rounded-lg p-1",

                            button {
                                class: if is_snippet {
                                    "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium bg-background text-foreground shadow-sm"
                                } else {
                                    "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium text-muted-foreground hover:text-foreground transition-colors"
                                },
                                onclick: move |_| preview_mode.set(PreviewMode::Snippet),
                                lucide_dioxus::Scissors { class: "w-3.5 h-3.5" }
                                "Snippet"
                            }

                            button {
                                class: if !is_snippet {
                                    "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium bg-background text-foreground shadow-sm"
                                } else {
                                    "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium text-muted-foreground hover:text-foreground transition-colors"
                                },
                                onclick: move |_| preview_mode.set(PreviewMode::Page),
                                lucide_dioxus::FileText { class: "w-3.5 h-3.5" }
                                "Page (A4)"
                            }
                        }

                        // Export button
                        ExportButton { source: source }
                    }
                }

                // Chart preview - canvas always fills panel, mode affects internal rendering
                div {
                    class: "flex-1 overflow-hidden",

                    // Pass signals directly so child effects can track them
                    DynamicChartRenderer {
                        source: source,
                        mode: preview_mode
                    }
                }
            }
        }
    }
}

/// Static chart renderer for non-interactive display.
/// Renders the chart as a clean page without pan/zoom controls or gray background.
/// Ideal for showcases and embedded previews.
///
/// Gracefully handles invalid/incomplete source by keeping the last valid render
/// or showing a blank white page. This allows smooth typewriter animations without
/// flashing error states.
#[component]
pub fn StaticChartRenderer(
    source: Signal<String>,
    mode: Signal<PreviewMode>,
    canvas_id: Option<String>,
    /// Fixed width for layout calculations (ignores container width from CSS).
    /// Useful when container is transformed (skewed) and bounding rect is distorted.
    fixed_layout_width: Option<f64>,
) -> Element {
    // Use provided canvas_id or default
    let canvas_id = canvas_id.unwrap_or_else(|| "static-chart-canvas".to_string());

    // Always render the canvas - StaticChartCanvas handles invalid source gracefully
    rsx! {
        div {
            class: "relative w-full h-full",

            // Canvas for WebGPU rendering - fills container, chart scales to fit
            // White background ensures blank page when content is invalid
            canvas {
                id: "{canvas_id}",
                class: "w-full h-full",
                style: "touch-action: none; background: white;",
            }

            // WebGPU rendering - static (no pan/zoom)
            // Handles parse errors gracefully without reinitializing
            StaticChartCanvas {
                source: source,
                mode: mode,
                canvas_id: canvas_id.clone(),
                fixed_layout_width: fixed_layout_width
            }
        }
    }
}

/// Static canvas component - renders chart without pan/zoom interactivity.
#[component]
fn StaticChartCanvas(
    source: Signal<String>,
    mode: Signal<PreviewMode>,
    canvas_id: String,
    /// Fixed width for layout calculations (ignores container width from CSS).
    fixed_layout_width: Option<f64>,
) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        use crate::renderer::ChartLayoutManager;
        use wasm_bindgen::JsCast;
        use wasm_bindgen::prelude::*;

        // Create layout manager signal
        let mut layout_manager = use_signal(|| None::<ChartLayoutManager>);
        let mut error_state = use_signal(|| None::<String>);
        let mut is_initialized = use_signal(|| false);

        // Track the current render task so we can cancel it when a new one starts
        let current_render_task = use_hook(|| std::cell::RefCell::new(None::<Task>));

        // Trigger re-render for resize events
        let mut render_trigger = use_signal(|| 0_u32);

        // Initialize WebGPU on mount
        use_effect(move || {
            let mut render_trigger_clone = render_trigger.clone();
            spawn(async move {
                match ChartLayoutManager::new() {
                    Ok(manager) => {
                        layout_manager.set(Some(manager));
                        is_initialized.set(true);
                        tracing::info!("Static chart layout manager initialized");

                        // Force initial render after a short delay to ensure canvas has proper size
                        #[cfg(target_arch = "wasm32")]
                        {
                            let promise = js_sys::Promise::new(&mut |resolve, _reject| {
                                if let Some(window) = web_sys::window() {
                                    let _ = window
                                        .set_timeout_with_callback_and_timeout_and_arguments_0(
                                            &resolve, 50,
                                        );
                                }
                            });
                            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                        }

                        // Trigger initial render (safely handle if signal was dropped)
                        if let Some(trigger) = render_trigger_clone.try_peek().ok().map(|r| *r) {
                            render_trigger_clone.set(trigger.wrapping_add(1));
                        }
                    }
                    Err(e) => {
                        error_state.set(Some(e));
                        tracing::error!("Failed to initialize static chart layout manager");
                    }
                }
            });
        });

        // Setup ResizeObserver (no mouse events for static renderer)
        let events_setup = use_hook(|| std::cell::Cell::new(false));
        let canvas_id_for_events = canvas_id.clone();
        use_effect(move || {
            if events_setup.get() {
                return;
            }

            let window = match web_sys::window() {
                Some(w) => w,
                None => return,
            };
            let document = match window.document() {
                Some(d) => d,
                None => return,
            };
            let canvas = match document.get_element_by_id(&canvas_id_for_events) {
                Some(c) => c,
                None => return,
            };

            events_setup.set(true);

            // Setup ResizeObserver to detect when canvas gets its proper size
            let mut render_trigger_clone = render_trigger.clone();
            let resize_callback = Closure::wrap(Box::new(
                move |_entries: js_sys::Array, _observer: web_sys::ResizeObserver| {
                    // Use try_peek() to safely handle dropped signals
                    // (can happen if component unmounts while ResizeObserver is still active)
                    if let Some(trigger) = render_trigger_clone.try_peek().ok().map(|r| *r) {
                        render_trigger_clone.set(trigger.wrapping_add(1));
                    }
                },
            )
                as Box<dyn FnMut(js_sys::Array, web_sys::ResizeObserver)>);

            if let Ok(observer) =
                web_sys::ResizeObserver::new(resize_callback.as_ref().unchecked_ref())
            {
                observer.observe(&canvas);
            }
            resize_callback.forget();
        });

        // Layout and render when source changes or mode changes
        let canvas_id_for_render = canvas_id.clone();
        use_effect(move || {
            let canvas_id_inner = canvas_id_for_render.clone();

            // Read ALL reactive values FIRST
            let initialized = *is_initialized.read();
            let trigger = *render_trigger.read();
            let source_text = source.read().clone();
            let current_mode = *mode.read();
            let is_snippet = current_mode == PreviewMode::Snippet;

            // Silence unused variable warning
            let _ = trigger;

            if !initialized {
                return;
            }

            // Cancel any previous render task
            if let Some(previous_task) = current_render_task.borrow_mut().take() {
                previous_task.cancel();
            }

            let task = spawn(async move {
                let mut manager_guard = layout_manager.write();

                let Some(ref mut manager) = *manager_guard else {
                    return;
                };

                #[cfg(target_arch = "wasm32")]
                {
                    use wasm_bindgen::JsCast;

                    let Some(window) = web_sys::window() else {
                        return;
                    };
                    let dpr = window.device_pixel_ratio();

                    let Some(document) = window.document() else {
                        return;
                    };
                    let Some(canvas) = document.get_element_by_id(&canvas_id_inner) else {
                        return;
                    };
                    let Ok(html_canvas) = canvas.dyn_into::<web_sys::HtmlCanvasElement>() else {
                        return;
                    };

                    // Use offsetWidth/offsetHeight instead of getBoundingClientRect
                    // because offset dimensions are NOT affected by CSS transforms (like skew).
                    // This gives us the true CSS layout size regardless of parent transforms.
                    let css_width = html_canvas.offset_width() as f64;
                    let css_height = html_canvas.offset_height() as f64;

                    let buffer_width = (css_width * dpr) as u32;
                    let buffer_height = (css_height * dpr) as u32;
                    html_canvas.set_width(buffer_width);
                    html_canvas.set_height(buffer_height);

                    // Use fixed layout width if provided (controls chart "density"/wrapping),
                    // otherwise use the actual CSS width
                    let layout_width = fixed_layout_width.unwrap_or(css_width);

                    // Try to parse the chart
                    let chart_result = if source_text.trim().is_empty() {
                        None
                    } else {
                        keyflow::parse(source_text.as_str()).ok()
                    };

                    // Check if we have a valid chart with actual content (sections)
                    let has_content = chart_result
                        .as_ref()
                        .map(|c| !c.sections.is_empty())
                        .unwrap_or(false);

                    if has_content {
                        // Parse succeeded and has sections - layout and render
                        let chart = chart_result.unwrap();

                        // Layout chart for export (no page offsets, positioned at origin)
                        // Use layout_width which controls chart "density" (how much content per line)
                        manager.layout_chart_for_export(&chart, layout_width, is_snippet);

                        // Render with fit-to-width scaling - the chart will scale to fill
                        // the container width. Since we're using offsetWidth (unaffected by
                        // transforms), this works correctly even with skewed containers.
                        if let Err(e) = manager.render_to_canvas_white(&html_canvas, dpr).await {
                            tracing::error!("Failed to render static chart: {}", e);
                        }
                    } else {
                        // Empty, invalid, or no sections - render blank white page
                        if let Err(e) = manager.render_blank_white(&html_canvas, dpr).await {
                            tracing::error!("Failed to render blank page: {}", e);
                        }
                    }
                }
            });

            *current_render_task.borrow_mut() = Some(task);
        });

        if let Some(error) = error_state.read().as_ref() {
            return rsx! {
                div {
                    class: "absolute inset-0 flex items-center justify-center text-yellow-400 text-sm",
                    "WebGPU not available: {error}"
                }
            };
        }

        if !*is_initialized.read() {
            return rsx! {
                div {
                    class: "absolute inset-0 flex items-center justify-center text-muted-foreground",
                    "Initializing..."
                }
            };
        }

        rsx! {}
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        rsx! {
            div {
                class: "absolute inset-0 flex items-center justify-center text-muted-foreground",
                "Chart rendering requires WebGPU (browser only)"
            }
        }
    }
}

/// Dynamic chart renderer that accepts signals for reactive updates.
/// The canvas always fills the container; mode affects the paper rendering inside.
#[component]
pub fn DynamicChartRenderer(
    source: Signal<String>,
    mode: Signal<PreviewMode>,
    canvas_id: Option<String>,
) -> Element {
    // Read source to trigger re-render on changes and for validation
    let source_value = source.read();
    let mode_value = *mode.read();

    // Debug: log when this component renders
    let first_line = source_value.lines().next().unwrap_or("(empty)");
    tracing::debug!(
        "[ChartRenderer] Rendering with mode={:?}, source_first_line='{}'",
        mode_value,
        first_line
    );

    // Use provided canvas_id or default
    let canvas_id = canvas_id.unwrap_or_else(|| "editor-chart-canvas".to_string());

    // For empty input, render canvas (DynamicChartCanvas handles empty gracefully)
    // For valid charts, render canvas
    // Only show error UI for non-empty invalid input
    let is_empty = source_value.trim().is_empty();
    let parse_result = if is_empty {
        Ok(keyflow::Chart::default())
    } else {
        keyflow::parse(source_value.as_str())
    };

    match parse_result {
        Ok(_chart) => {
            rsx! {
                div {
                    class: "w-full h-full relative",

                    // Canvas for WebGPU rendering - fills entire container
                    // Gray background shows around the white "paper"
                    canvas {
                        id: "{canvas_id}",
                        class: "w-full h-full cursor-grab active:cursor-grabbing",
                        style: "touch-action: none; background: #374151;",
                    }

                    // WebGPU rendering - pass signals for reactive tracking
                    DynamicChartCanvas { source: source, mode: mode, canvas_id: canvas_id.clone() }
                }
            }
        }
        Err(e) => {
            let error_msg = e.to_string();
            rsx! {
                div {
                    class: "w-full h-full flex items-center justify-center",
                    style: "background: #374151;",

                    div {
                        class: "text-center p-6 max-w-md bg-white rounded-lg shadow-xl",

                        lucide_dioxus::CircleAlert { class: "w-12 h-12 mx-auto mb-3 text-red-400" }

                        div {
                            class: "text-lg font-semibold text-red-600 mb-2",
                            "Parse Error"
                        }

                        div {
                            class: "text-sm text-red-500 font-mono whitespace-pre-wrap text-left bg-red-50 p-3 rounded-lg",
                            "{error_msg}"
                        }
                    }
                }
            }
        }
    }
}

/// Canvas component with WebGPU rendering for dynamic content.
/// Accepts signals directly to enable reactive effect tracking.
#[component]
fn DynamicChartCanvas(
    source: Signal<String>,
    mode: Signal<PreviewMode>,
    canvas_id: String,
) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        use crate::renderer::ChartLayoutManager;
        use wasm_bindgen::JsCast;
        use wasm_bindgen::prelude::*;

        // Create layout manager signal
        let mut layout_manager = use_signal(|| None::<ChartLayoutManager>);
        let mut error_state = use_signal(|| None::<String>);
        let mut is_initialized = use_signal(|| false);

        // Track the current render task so we can cancel it when a new one starts
        // Using RefCell instead of Signal to avoid triggering reactive updates when we store the task
        let current_render_task = use_hook(|| std::cell::RefCell::new(None::<Task>));

        // Trigger re-render for non-signal state changes (mouse, resize, etc.)
        let mut render_trigger = use_signal(|| 0_u32);

        // Transform state for pan/zoom
        let mut transform_x = use_signal(|| 20.0_f64);
        let mut transform_y = use_signal(|| 20.0_f64);
        let mut scale = use_signal(|| 1.0_f64);

        // Mouse interaction state
        let mut is_dragging = use_signal(|| false);
        let mut last_mouse_x = use_signal(|| 0.0_f64);
        let mut last_mouse_y = use_signal(|| 0.0_f64);

        // Initialize WebGPU on mount
        // Using Dioxus's spawn() instead of wasm_bindgen_futures::spawn_local()
        // for proper integration with Dioxus's runtime
        use_effect(move || {
            let mut render_trigger_clone = render_trigger.clone();
            spawn(async move {
                match ChartLayoutManager::new() {
                    Ok(manager) => {
                        layout_manager.set(Some(manager));
                        is_initialized.set(true);
                        tracing::info!("Editor chart layout manager initialized");

                        // Force initial render after a short delay to ensure canvas has proper size
                        // This helps with the initial weird scaling issue
                        #[cfg(target_arch = "wasm32")]
                        {
                            let promise = js_sys::Promise::new(&mut |resolve, _reject| {
                                if let Some(window) = web_sys::window() {
                                    let _ = window
                                        .set_timeout_with_callback_and_timeout_and_arguments_0(
                                            &resolve, 50, // Small delay for DOM to stabilize
                                        );
                                }
                            });
                            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                        }

                        // Trigger initial render (safely handle if signal was dropped)
                        if let Some(trigger) = render_trigger_clone.try_peek().ok().map(|r| *r) {
                            render_trigger_clone.set(trigger.wrapping_add(1));
                        }
                    }
                    Err(e) => {
                        error_state.set(Some(e));
                        tracing::error!("Failed to initialize editor chart layout manager");
                    }
                }
            });
        });

        // Setup mouse event listeners (only once)
        // Use a hook to track if we've already set up the listeners to avoid
        // duplicate registrations and stale signal references
        let events_setup = use_hook(|| std::cell::Cell::new(false));
        let canvas_id_for_events = canvas_id.clone();
        use_effect(move || {
            // Only set up events once - the closures capture signal clones that
            // would become stale if we set up multiple times
            if events_setup.get() {
                return;
            }

            let window = match web_sys::window() {
                Some(w) => w,
                None => return,
            };
            let document = match window.document() {
                Some(d) => d,
                None => return,
            };
            let canvas = match document.get_element_by_id(&canvas_id_for_events) {
                Some(c) => c,
                None => return,
            };

            // Mark as set up before adding listeners
            events_setup.set(true);

            // Mouse down - start dragging
            let mut is_dragging_clone = is_dragging.clone();
            let mut last_mouse_x_clone = last_mouse_x.clone();
            let mut last_mouse_y_clone = last_mouse_y.clone();
            let mousedown_closure = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
                // Safely handle dropped signals (component may have unmounted)
                if is_dragging_clone.try_peek().is_ok() {
                    is_dragging_clone.set(true);
                    last_mouse_x_clone.set(event.client_x() as f64);
                    last_mouse_y_clone.set(event.client_y() as f64);
                }
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
                // Safely handle dropped signals (component may have unmounted)
                let Some(is_dragging) = is_dragging_clone.try_read().ok().map(|r| *r) else {
                    return;
                };
                if is_dragging {
                    let Some(last_x) = last_mouse_x_clone.try_read().ok().map(|r| *r) else {
                        return;
                    };
                    let Some(last_y) = last_mouse_y_clone.try_read().ok().map(|r| *r) else {
                        return;
                    };
                    let dx = event.client_x() as f64 - last_x;
                    let dy = event.client_y() as f64 - last_y;
                    let Some(cur_tx) = transform_x_clone.try_read().ok().map(|r| *r) else {
                        return;
                    };
                    let Some(cur_ty) = transform_y_clone.try_read().ok().map(|r| *r) else {
                        return;
                    };
                    transform_x_clone.set(cur_tx + dx);
                    transform_y_clone.set(cur_ty + dy);
                    last_mouse_x_clone.set(event.client_x() as f64);
                    last_mouse_y_clone.set(event.client_y() as f64);
                    if let Some(trigger) = render_trigger_clone.try_read().ok().map(|r| *r) {
                        render_trigger_clone.set(trigger.wrapping_add(1));
                    }
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
                // Safely handle dropped signals (component may have unmounted)
                if is_dragging_clone.try_peek().is_ok() {
                    is_dragging_clone.set(false);
                }
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

                // Safely handle dropped signals (component may have unmounted)
                let Some(old_scale) = scale_clone.try_read().ok().map(|r| *r) else {
                    return;
                };

                let delta = -event.delta_y() / 500.0;
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
                    let Some(cur_tx) = transform_x_clone.try_read().ok().map(|r| *r) else {
                        return;
                    };
                    let Some(cur_ty) = transform_y_clone.try_read().ok().map(|r| *r) else {
                        return;
                    };
                    let new_tx = mouse_x - (mouse_x - cur_tx) * scale_change;
                    let new_ty = mouse_y - (mouse_y - cur_ty) * scale_change;

                    transform_x_clone.set(new_tx);
                    transform_y_clone.set(new_ty);
                }

                scale_clone.set(new_scale);
                if let Some(trigger) = render_trigger_clone.try_read().ok().map(|r| *r) {
                    render_trigger_clone.set(trigger.wrapping_add(1));
                }
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

            // Setup ResizeObserver to detect when canvas gets its proper size
            let mut render_trigger_clone = render_trigger.clone();
            let resize_callback = Closure::wrap(Box::new(
                move |_entries: js_sys::Array, _observer: web_sys::ResizeObserver| {
                    // Safely handle dropped signals (component may have unmounted)
                    if let Some(trigger) = render_trigger_clone.try_read().ok().map(|r| *r) {
                        render_trigger_clone.set(trigger.wrapping_add(1));
                    }
                },
            )
                as Box<dyn FnMut(js_sys::Array, web_sys::ResizeObserver)>);

            if let Ok(observer) =
                web_sys::ResizeObserver::new(resize_callback.as_ref().unchecked_ref())
            {
                if let Some(canvas) = document.get_element_by_id(&canvas_id_for_events) {
                    observer.observe(&canvas);
                }
            }
            resize_callback.forget();
        });

        // Layout and render when source changes, mode changes, or transform changes
        // IMPORTANT: We MUST read ALL signals BEFORE any early returns to establish
        // reactive dependencies. If we return early without reading a signal,
        // it won't be tracked as a dependency and changes to it won't trigger re-runs.
        let canvas_id_for_render = canvas_id.clone();
        use_effect(move || {
            // Clone canvas_id for async block
            let canvas_id_inner = canvas_id_for_render.clone();

            // Read ALL reactive values FIRST - this establishes dependencies
            let initialized = *is_initialized.read();
            let tx = *transform_x.read();
            let ty = *transform_y.read();
            let s = *scale.read();
            let trigger = *render_trigger.read();
            let source_text = source.read().clone();
            let current_mode = *mode.read();
            let is_snippet = current_mode == PreviewMode::Snippet;

            // Debug: log what triggered the effect
            let first_line = source_text.lines().next().unwrap_or("(empty)");
            tracing::debug!(
                "[ChartCanvas] Render effect: initialized={}, trigger={}, mode={:?}, first_line='{}'",
                initialized,
                trigger,
                current_mode,
                first_line
            );

            // NOW check if we should skip rendering
            if !initialized {
                tracing::debug!("[ChartCanvas] Skipping render - not initialized yet");
                return;
            }

            // Cancel any previous render task to avoid borrow conflicts
            if let Some(previous_task) = current_render_task.borrow_mut().take() {
                previous_task.cancel();
            }

            // Spawn the render task and store it so we can cancel it if needed
            let task = spawn(async move {
                // Get mutable access to the layout manager
                let mut manager_guard = layout_manager.write();

                let Some(ref mut manager) = *manager_guard else {
                    tracing::debug!("[ChartCanvas] Skipping render - manager not initialized");
                    return;
                };

                #[cfg(target_arch = "wasm32")]
                {
                    use wasm_bindgen::JsCast;

                    let Some(window) = web_sys::window() else {
                        return;
                    };
                    let dpr = window.device_pixel_ratio();

                    let Some(document) = window.document() else {
                        return;
                    };
                    let Some(canvas) = document.get_element_by_id(&canvas_id_inner) else {
                        return;
                    };
                    let Ok(html_canvas) = canvas.dyn_into::<web_sys::HtmlCanvasElement>() else {
                        return;
                    };

                    let rect = html_canvas.get_bounding_client_rect();
                    let css_width = rect.width();

                    let buffer_width = (css_width * dpr) as u32;
                    let buffer_height = (rect.height() * dpr) as u32;
                    html_canvas.set_width(buffer_width);
                    html_canvas.set_height(buffer_height);

                    // Try to parse the chart
                    let chart_result = if source_text.trim().is_empty() {
                        None
                    } else {
                        keyflow::parse(source_text.as_str()).ok()
                    };

                    // Check if we have a valid chart with actual content (sections)
                    let has_content = chart_result
                        .as_ref()
                        .map(|c| !c.sections.is_empty())
                        .unwrap_or(false);

                    if has_content {
                        // Parse succeeded and has sections - render normally
                        let chart = chart_result.unwrap();

                        // Use appropriate layout mode based on preview setting
                        manager.layout_chart_with_mode(&chart, css_width, is_snippet);

                        if let Err(e) = manager
                            .render_to_canvas_with_transform(&html_canvas, tx * dpr, ty * dpr, s * dpr)
                            .await
                        {
                            tracing::error!("Failed to render chart: {}", e);
                        }
                    } else {
                        // Empty, invalid, or no sections - render blank page
                        if let Err(e) = manager
                            .render_blank_page_with_transform(
                                &html_canvas,
                                tx * dpr,
                                ty * dpr,
                                s * dpr,
                                is_snippet,
                            )
                            .await
                        {
                            tracing::error!("Failed to render blank page: {}", e);
                        }
                    }
                }
            });

            // Store the task so we can cancel it on the next effect run (non-reactive storage)
            *current_render_task.borrow_mut() = Some(task);
        });

        if let Some(error) = error_state.read().as_ref() {
            return rsx! {
                div {
                    class: "absolute inset-0 flex items-center justify-center text-yellow-400 text-sm",
                    "WebGPU not available: {error}"
                }
            };
        }

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

/// Syntax-highlighted code editor for keyflow notation.
///
/// Uses a layered approach with a transparent textarea for input and
/// a highlighted display layer behind it. Both layers must have
/// identical text styling for proper alignment.
#[component]
pub fn HighlightedEditor(
    value: String,
    on_change: EventHandler<String>,
    placeholder: &'static str,
    /// Optional unique ID for the textarea element. Defaults to "keyflow-editor-textarea".
    textarea_id: Option<String>,
) -> Element {
    // Track scroll position to sync layers
    let mut scroll_top = use_signal(|| 0.0_f64);
    let mut scroll_left = use_signal(|| 0.0_f64);

    // Unique ID for the textarea to query scroll position
    let textarea_id = textarea_id.unwrap_or_else(|| "keyflow-editor-textarea".to_string());

    // Common text styling - MUST match exactly between textarea and highlight layer
    // Using explicit line-height to ensure pixel-perfect alignment
    let text_style = "font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 14px; line-height: 21px; white-space: pre; tab-size: 4;";

    rsx! {
        div {
            class: "relative w-full h-full",

            // Highlighted display layer (behind textarea)
            div {
                id: "keyflow-editor-highlight",
                class: "absolute inset-0 p-4 overflow-hidden pointer-events-none bg-background",
                style: "{text_style}",

                // Inner container with scroll offset
                div {
                    style: "transform: translate(-{scroll_left}px, -{scroll_top}px);",

                    HighlightedCode { source: value.clone() }
                }
            }

            // Transparent textarea for actual input (on top)
            textarea {
                id: "{textarea_id}",
                class: "absolute inset-0 w-full h-full p-4 resize-none focus:outline-none bg-transparent text-transparent caret-foreground z-10 overflow-auto",
                style: "{text_style}",
                value: "{value}",
                spellcheck: false,
                placeholder: "{placeholder}",
                oninput: {
                    let textarea_id = textarea_id.clone();
                    move |evt| {
                        on_change.call(evt.value());
                        // Update scroll position after input
                        #[cfg(target_arch = "wasm32")]
                        {
                            use wasm_bindgen::JsCast;
                            if let Some(window) = web_sys::window() {
                                if let Some(document) = window.document() {
                                    if let Some(elem) = document.get_element_by_id(&textarea_id) {
                                        if let Ok(html_elem) = elem.dyn_into::<web_sys::HtmlElement>() {
                                            scroll_top.set(html_elem.scroll_top() as f64);
                                            scroll_left.set(html_elem.scroll_left() as f64);
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                onscroll: {
                    let textarea_id = textarea_id.clone();
                    move |_evt| {
                        // Sync scroll with highlighted layer
                        #[cfg(target_arch = "wasm32")]
                        {
                            use wasm_bindgen::JsCast;
                            if let Some(window) = web_sys::window() {
                                if let Some(document) = window.document() {
                                    if let Some(elem) = document.get_element_by_id(&textarea_id) {
                                        if let Ok(html_elem) = elem.dyn_into::<web_sys::HtmlElement>() {
                                            scroll_top.set(html_elem.scroll_top() as f64);
                                            scroll_left.set(html_elem.scroll_left() as f64);
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
}

/// Renders highlighted keyflow source code.
///
/// Renders each line as plain text with inline spans for highlighting.
/// Uses newline characters to separate lines (matching textarea behavior).
#[component]
fn HighlightedCode(source: String) -> Element {
    // Split into lines but preserve the structure
    let lines: Vec<&str> = source.split('\n').collect();

    rsx! {
        // Use a single pre-like container to match textarea rendering
        div {
            class: "text-foreground",

            for (idx, line) in lines.iter().enumerate() {
                // Render each line
                HighlightedLine { line: line.to_string() }
                // Add newline between lines (except after last)
                if idx < lines.len() - 1 {
                    "\n"
                }
            }
        }
    }
}

/// Renders a single highlighted line of keyflow notation.
#[component]
fn HighlightedLine(line: String) -> Element {
    let spans = Highlighter::highlight_line(&line);

    // Empty line - render empty span to preserve height
    if line.is_empty() {
        return rsx! { span { "" } };
    }

    if spans.is_empty() {
        // No highlighting - render as plain text
        return rsx! { span { "{line}" } };
    }

    // Build the highlighted segments
    let mut segments: Vec<Element> = Vec::new();
    let mut last_end = 0;

    for span in &spans {
        let start = span.span.start;
        let end = span.span.start + span.span.len;

        // Add any unhighlighted text before this span
        if start > last_end {
            let text = &line[last_end..start];
            segments.push(rsx! { span { "{text}" } });
        }

        // Add the highlighted span
        let text = &line[start..end.min(line.len())];
        let class = highlight_class(span.kind);
        segments.push(rsx! {
            span { class: "{class}", "{text}" }
        });

        last_end = end;
    }

    // Add any remaining text
    if last_end < line.len() {
        let text = &line[last_end..];
        segments.push(rsx! { span { "{text}" } });
    }

    // Return inline spans (no block element wrapper)
    rsx! {
        for segment in segments {
            {segment}
        }
    }
}

/// Export PDF using rasterization for pixel-perfect output.
///
/// This function:
/// 1. Gets page information from the layout
/// 2. For each page, renders to an off-screen canvas at high resolution
/// 3. Extracts each page as PNG via data URL
/// 4. Creates a multi-page PDF with all pages
#[cfg(target_arch = "wasm32")]
async fn export_pdf_rasterized(
    manager: &mut crate::renderer::ChartLayoutManager,
) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;

    // Get page information (page_number, x_offset, y_offset, width, height)
    let page_info = manager.get_page_info();

    if page_info.is_empty() {
        return Err("No pages to export".to_string());
    }

    // Scale factor for print quality (216 DPI)
    let scale_factor = 3.0;

    // Create off-screen canvas (will be resized for each page)
    let window = web_sys::window().ok_or("No window")?;
    let document = window.document().ok_or("No document")?;
    let canvas = document
        .create_element("canvas")
        .map_err(|_| "Failed to create canvas")?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| "Failed to cast to canvas")?;

    // Render each page and collect PNG data
    let mut pages_data: Vec<(Vec<u8>, f64, f64)> = Vec::new();

    for (_page_num, page_x, page_y, page_width, page_height) in &page_info {
        // Set canvas size for this page
        let canvas_width = (*page_width * scale_factor).ceil() as u32;
        let canvas_height = (*page_height * scale_factor).ceil() as u32;
        canvas.set_width(canvas_width);
        canvas.set_height(canvas_height);

        // Render this page to the canvas
        // Offset so this specific page fills the canvas
        manager
            .render_page_to_canvas(&canvas, *page_x, *page_y, scale_factor)
            .await?;

        // Extract canvas content as PNG
        let data_url = canvas
            .to_data_url_with_type("image/png")
            .map_err(|_| "Failed to get data URL")?;

        let base64_prefix = "data:image/png;base64,";
        let base64_data = data_url
            .strip_prefix(base64_prefix)
            .ok_or_else(|| "Invalid data URL format".to_string())?;

        let decoded = window
            .atob(base64_data)
            .map_err(|_| "Failed to decode base64")?;

        let png_bytes: Vec<u8> = decoded.chars().map(|c| c as u8).collect();

        pages_data.push((png_bytes, *page_width, *page_height));
    }

    // Create multi-page PDF
    manager.export_multi_page_pdf(pages_data)
}

/// Export button component for downloading chart as SVG or PDF.
#[component]
fn ExportButton(source: Signal<String>) -> Element {
    let mut is_exporting = use_signal(|| false);
    let mut show_dropdown = use_signal(|| false);

    let mut do_export = move |format: &'static str| {
        #[cfg(target_arch = "wasm32")]
        {
            use crate::renderer::ChartLayoutManager;
            use wasm_bindgen::JsCast;

            is_exporting.set(true);
            show_dropdown.set(false);
            let source_text = source.read().clone();

            spawn(async move {
                // Parse the chart
                let chart = match keyflow::parse(source_text.as_str()) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::error!("Failed to parse chart for export: {}", e);
                        is_exporting.set(false);
                        return;
                    }
                };

                // Create layout manager and layout the chart
                let mut manager = match ChartLayoutManager::new() {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!("Failed to create layout manager: {}", e);
                        is_exporting.set(false);
                        return;
                    }
                };

                // Layout in page mode for export (A4, no page offsets for clean export)
                manager.layout_chart_for_export(&chart, 595.0, false);

                // Export based on format
                let title = chart.metadata.title.as_deref().unwrap_or("chart");
                let base_filename = title.replace(' ', "_");

                let (content, mime_type, extension): (Vec<u8>, &str, &str) = match format {
                    "pdf" => {
                        // Use SVG-to-PDF conversion for vector output
                        match manager.export_multi_page_pdf_via_svg() {
                            Ok(bytes) => (bytes, "application/pdf", "pdf"),
                            Err(e) => {
                                tracing::error!("Failed to export PDF: {}", e);
                                is_exporting.set(false);
                                return;
                            }
                        }
                    }
                    _ => {
                        // SVG export - use per-page export for LilyPond-style output
                        match manager.export_pages_to_svg() {
                            Ok(pages) => {
                                if pages.len() == 1 {
                                    // Single page - export directly
                                    (pages.into_iter().next().unwrap().into_bytes(), "image/svg+xml", "svg")
                                } else {
                                    // Multi-page - create a zip file containing all pages
                                    match create_svg_zip(&pages, &base_filename) {
                                        Ok(zip_bytes) => (zip_bytes, "application/zip", "zip"),
                                        Err(e) => {
                                            tracing::error!("Failed to create zip: {}", e);
                                            is_exporting.set(false);
                                            return;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("Failed to export SVG: {}", e);
                                is_exporting.set(false);
                                return;
                            }
                        }
                    }
                };

                // Trigger download
                if let Some(window) = web_sys::window() {
                    if let Some(document) = window.document() {
                        // Create blob from bytes
                        let uint8_array = js_sys::Uint8Array::from(content.as_slice());
                        let array = js_sys::Array::new();
                        array.push(&uint8_array);

                        let mut options = web_sys::BlobPropertyBag::new();
                        options.set_type(mime_type);

                        if let Ok(blob) =
                            web_sys::Blob::new_with_u8_array_sequence_and_options(&array, &options)
                        {
                            if let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) {
                                // Create download link
                                if let Ok(link) = document.create_element("a") {
                                    let link = link.unchecked_into::<web_sys::HtmlAnchorElement>();
                                    link.set_href(&url);

                                    // Use pre-computed filename
                                    let filename = format!("{}.{}", base_filename, extension);
                                    link.set_download(&filename);

                                    // Trigger download
                                    link.click();

                                    // Cleanup
                                    let _ = web_sys::Url::revoke_object_url(&url);
                                }
                            }
                        }
                    }
                }

                is_exporting.set(false);
            });
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = format;
            tracing::warn!("Export is only available in the browser");
        }
    };

    let export_svg = move |_| do_export("svg");
    let export_pdf = move |_| do_export("pdf");

    rsx! {
        div {
            class: "relative",

            // Main button with dropdown
            div {
                class: "flex",

                // Export button
                button {
                    class: "flex items-center gap-1.5 px-3 py-1.5 rounded-l-md text-xs font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50",
                    disabled: *is_exporting.read(),
                    onclick: export_pdf,  // Default to PDF

                    if *is_exporting.read() {
                        lucide_dioxus::LoaderCircle { class: "w-3.5 h-3.5 animate-spin" }
                        "Exporting..."
                    } else {
                        lucide_dioxus::Download { class: "w-3.5 h-3.5" }
                        "Export"
                    }
                }

                // Dropdown toggle
                button {
                    class: "flex items-center px-1.5 py-1.5 rounded-r-md text-xs font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors border-l border-primary-foreground/20 disabled:opacity-50",
                    disabled: *is_exporting.read(),
                    onclick: move |_| {
                        let current = *show_dropdown.read();
                        show_dropdown.set(!current);
                    },

                    lucide_dioxus::ChevronDown { class: "w-3.5 h-3.5" }
                }
            }

            // Dropdown menu
            if *show_dropdown.read() {
                div {
                    class: "absolute top-full right-0 mt-1 bg-popover border border-border rounded-md shadow-lg z-50 min-w-[120px]",

                    button {
                        class: "w-full flex items-center gap-2 px-3 py-2 text-xs text-left hover:bg-accent transition-colors rounded-t-md",
                        onclick: export_pdf,

                        lucide_dioxus::FileText { class: "w-3.5 h-3.5" }
                        "PDF"
                    }

                    button {
                        class: "w-full flex items-center gap-2 px-3 py-2 text-xs text-left hover:bg-accent transition-colors rounded-b-md",
                        onclick: export_svg,

                        lucide_dioxus::FileCode { class: "w-3.5 h-3.5" }
                        "SVG"
                    }
                }
            }
        }
    }
}

/// Map highlight kinds to Tailwind CSS classes.
///
/// Design decisions:
/// - Root + Accidental use the same color (Ab = same color for A and b)
/// - Quality + Extension use the same color (maj9 = same color for maj and 9)
/// - Barlines (MeasureSeparator) are gray/muted
/// - Unknown/unparsed text is muted, not red (avoids visual noise)
fn highlight_class(kind: HighlightKind) -> &'static str {
    match kind {
        // Chord components - Root and Accidental same color
        HighlightKind::Root => "text-sky-400 font-semibold",
        HighlightKind::Accidental => "text-sky-400", // Same as Root
        HighlightKind::ScaleDegree => "text-purple-400 font-semibold",
        HighlightKind::RomanNumeral => "text-purple-400 font-semibold",

        // Quality and Extension same color
        HighlightKind::Quality => "text-amber-400",
        HighlightKind::Extension => "text-amber-400", // Same as Quality
        HighlightKind::Modifier => "text-yellow-300",

        // Bass note - slightly different shade
        HighlightKind::Bass => "text-sky-300",
        HighlightKind::BassSlash => "text-gray-500",

        // Rhythm notation
        HighlightKind::Duration => "text-violet-400",
        HighlightKind::SlashRhythm => "text-gray-400",
        HighlightKind::Push => "text-rose-400",
        HighlightKind::Pull => "text-rose-400",
        HighlightKind::Triplet => "text-rose-300",
        HighlightKind::Dot => "text-violet-400",

        // Structure - Barlines are gray
        HighlightKind::MeasureSeparator => "text-gray-500",
        HighlightKind::Repeat => "text-indigo-400 font-bold",
        HighlightKind::Section => "text-emerald-400 font-semibold",
        HighlightKind::SectionBracket => "text-emerald-400",
        HighlightKind::MeasureCount => "text-emerald-300",
        HighlightKind::SectionComment => "text-emerald-200 italic",

        // Special
        HighlightKind::Rest => "text-gray-400 italic",
        HighlightKind::Space => "text-gray-500",
        HighlightKind::MemoryRecall => "text-gray-400",
        HighlightKind::Dynamic => "text-red-400 italic",

        // Metadata
        HighlightKind::Title => "text-green-400 font-semibold",
        HighlightKind::Artist => "text-green-300",
        HighlightKind::Tempo => "text-orange-400",
        HighlightKind::TempoArrow => "text-orange-300",
        HighlightKind::Key => "text-violet-400",
        HighlightKind::TimeSignature => "text-cyan-400",

        // Comments - muted gray
        HighlightKind::Comment => "text-gray-500 italic",
        HighlightKind::CommentMarker => "text-gray-500",

        // Melody and tracks
        HighlightKind::MelodyBlock => "text-teal-400",
        HighlightKind::TrackMarker => "text-fuchsia-400 font-semibold",

        // Commands and cues - muted (these are config lines like /push = triplet)
        HighlightKind::Command => "text-gray-500",
        HighlightKind::TextCue => "text-gray-400 italic",

        // Whitespace and unknown - muted, not distracting
        HighlightKind::Whitespace => "",
        HighlightKind::Unknown => "text-gray-500", // Muted instead of red
    }
}

/// Create a zip file containing multiple SVG pages.
///
/// Each SVG is stored as `{base_filename}_page{N}.svg` where N is 1-indexed.
#[cfg(target_arch = "wasm32")]
fn create_svg_zip(pages: &[String], base_filename: &str) -> Result<Vec<u8>, String> {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);

    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6));

    for (i, svg_content) in pages.iter().enumerate() {
        let filename = format!("{}_page{}.svg", base_filename, i + 1);
        zip.start_file(&filename, options)
            .map_err(|e| format!("Failed to create zip entry: {e}"))?;
        zip.write_all(svg_content.as_bytes())
            .map_err(|e| format!("Failed to write SVG to zip: {e}"))?;
    }

    zip.finish()
        .map_err(|e| format!("Failed to finalize zip: {e}"))?;

    Ok(buffer.into_inner())
}
