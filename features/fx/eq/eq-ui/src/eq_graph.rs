//! Parametric EQ graph widget.
//!
//! A Dioxus component that renders a parametric EQ graph with Pro-Q style interactions:
//! - Frequency response curve visualization
//! - Draggable band control points (drag to adjust freq/gain)
//! - Mouse wheel to adjust Q while hovering/dragging
//! - Double-click on empty area to add new band
//! - Double-click on band to reset gain to 0 dB
//! - Drag band outside graph area to remove it
//! - Smart filter type selection based on click position
//!
//! Uses SVG rendering for cross-platform compatibility.
//! Ported from the legacy `audio-controls` crate for the nice_plug_dioxus Blitz renderer.

use std::rc::Rc;
use std::sync::Arc;

use dioxus_elements::input_data::MouseButton;
use nice_plug_dioxus::prelude::*;
use nice_plug_dioxus::widget::CustomWidgetAttr;

use super::eq_graph_interaction::{
    GraphMapper, bands_in_rect, drag_gain_for_shape, filter_type_for_position, nearest_band,
    wheel_q_for_shape,
};
pub use super::eq_graph_model::{
    BAND_COLORS, EqBand, EqBandShape, EqGraphRenderState, GraphConfig, InteractionState, MAX_BANDS,
    StereoMode, get_band_color, get_band_fill_color, q_to_slope_db, slope_db_to_q,
};
use super::eq_graph_painter::EqGraphWidget;
use super::eq_graph_popup::{BandContextMenu, BandPopup, EmptyGraphContextMenu};
pub use super::eq_graph_response::{calculate_band_response, calculate_combined_response};
use spectrum_analyzer::dsp::AnalyzerSnapshot;

/// Get current timestamp in milliseconds.
fn now_ms() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

/// How the cheat-sheet overlay profile is chosen.
#[derive(Clone, Copy)]
pub enum OverlayChoice {
    /// Resolve from the host track name via the injected [`TrackInfoProvider`].
    Auto,
    /// Hidden.
    Off,
    /// A specific profile, regardless of the track.
    Pick(&'static crate::cheatsheet::InstrumentProfile),
}

/// Parametric EQ graph component with Pro-Q style interactions.
///
/// # Interactions
///
/// - **Drag band node**: Adjust frequency (X) and gain (Y)
/// - **Shift+Drag**: Fine adjustment mode
/// - **Mouse wheel on band**: Adjust Q factor
/// - **Double-click empty area**: Add new band (filter type based on position)
/// - **Double-click band**: Reset gain to 0 dB
/// - **Drag band outside graph**: Remove band
///
/// # Filter Type Selection (on double-click)
///
/// - Left edge (< 30 Hz): High-pass filter
/// - Right edge (> 15 kHz): Low-pass filter
/// - Low frequencies (30-80 Hz): Low shelf
/// - High frequencies (8-15 kHz): High shelf
/// - Center area: Bell/Peak filter
///
/// # Example
///
/// ```ignore
/// use eq_ui::eq_graph::{EqGraph, EqBand, EqBandShape};
///
/// #[component]
/// fn MyEQ() -> Element {
///     let mut bands = use_signal(|| vec![
///         EqBand { used: true, enabled: true, frequency: 100.0, gain: 3.0, q: 1.0, shape: EqBandShape::Bell, ..Default::default() },
///     ]);
///
///     rsx! {
///         EqGraph {
///             bands: bands,
///             on_band_change: move |(idx, band): (usize, EqBand)| {
///                 bands.write()[idx] = band;
///             },
///             on_band_add: move |band: EqBand| {
///                 bands.write().push(band);
///             },
///             on_band_remove: move |idx: usize| {
///                 bands.write().remove(idx);
///             },
///         }
///     }
/// }
/// ```
#[component]
pub fn EqGraph(
    /// Signal containing the EQ bands.
    bands: Signal<Vec<EqBand>>,
    /// dB range (symmetric around 0).
    #[props(default = 24.0)]
    db_range: f64,
    /// Minimum frequency in Hz.
    #[props(default = 20.0)]
    min_freq: f64,
    /// Maximum frequency in Hz.
    #[props(default = 20000.0)]
    max_freq: f64,
    /// Sample rate for filter calculations.
    #[props(default = 48000.0)]
    sample_rate: f64,
    /// Show grid lines.
    #[props(default = true)]
    show_grid: bool,
    /// Show frequency labels.
    #[props(default = true)]
    show_freq_labels: bool,
    /// Show dB labels.
    #[props(default = true)]
    show_db_labels: bool,
    /// Fill under the curve.
    #[props(default = true)]
    fill_curve: bool,
    /// Callback when a band is changed via drag.
    #[props(default)]
    on_band_change: Option<EventHandler<(usize, EqBand)>>,
    /// Callback when a new band is added (double-click on empty area).
    #[props(default)]
    on_band_add: Option<EventHandler<EqBand>>,
    /// Callback when a band is removed (dragged outside graph area).
    #[props(default)]
    on_band_remove: Option<EventHandler<usize>>,
    /// Callback when band editing begins.
    #[props(default)]
    on_begin: Option<EventHandler<usize>>,
    /// Callback when band editing ends.
    #[props(default)]
    on_end: Option<EventHandler<usize>>,
    /// Additional CSS class (kept for API compat, not functional in Blitz).
    #[props(default)]
    class: String,
    /// Optional external signal to sync focused band state (for detail panels).
    /// EqGraph writes the focused band index to this signal from event handlers.
    #[props(default)]
    focused_band_out: Option<Signal<Option<usize>>>,
    /// Optional external signal for the cheat-sheet overlay selection, so a
    /// parent (e.g. the inspector) can drive it too. If omitted, the graph owns
    /// its own internal selection (defaulting to `Auto`).
    #[props(default)]
    overlay_sel: Option<Signal<OverlayChoice>>,
    /// Optional spectrum analyzer data (dB values for logarithmically-spaced bins).
    #[props(default)]
    spectrum_db: Option<Vec<f32>>,
    /// Optional full analyzer snapshot (pre/post/external/collision). Takes
    /// precedence over `spectrum_db` when it carries data.
    #[props(default)]
    analyzer_snapshot: Option<AnalyzerSnapshot>,
    /// Optional model response data (dB values for logarithmically-spaced bins).
    #[props(default)]
    model_response_db: Option<Vec<f32>>,
    /// Actual rendered width of the SVG container in pixels.
    /// Required for accurate mouse coordinate mapping.
    #[props(default = 0.0)]
    rendered_width: f64,
    /// Actual rendered height of the SVG container in pixels.
    #[props(default = 0.0)]
    rendered_height: f64,
    /// X offset of the SVG element from the window's left edge (in pixels).
    /// Needed because Blitz's element_coordinates() returns window-relative coords.
    #[props(default = 0.0)]
    offset_x: f64,
    /// Y offset of the SVG element from the window's top edge (in pixels).
    #[props(default = 0.0)]
    offset_y: f64,
    /// Whether the control is disabled.
    #[props(default = false)]
    disabled: bool,
) -> Element {
    // Fixed viewBox dimensions for the painter (always 800x350).
    let vb_width: f64 = 800.0;
    let vb_height: f64 = 350.0;
    let padding = 0.0;
    // SVG interaction layer uses CSS pixel coordinates (no viewBox scaling).
    // Falls back to vb_width/vb_height before layout is measured.
    // graph_width/graph_height are the actual CSS pixel dimensions of the element.

    // Internal state
    let mut dragging_band = use_signal(|| None::<usize>);
    let mut hovered_band = use_signal(|| None::<usize>);
    // Focused band shows the info popup (only one at a time)
    let mut focused_band: Signal<Option<usize>> = use_signal(|| None);
    // Helper: set focused_band and sync to external signal
    let mut set_focused = move |val: Option<usize>| {
        focused_band.set(val);
        if let Some(mut ext) = focused_band_out {
            ext.set(val);
        }
    };
    // Selected bands for multi-selection (can be multiple)
    let mut selected_bands: Signal<Vec<usize>> = use_signal(Vec::new);
    // Selection rectangle state: (start_x, start_y, current_x, current_y)
    let mut selection_rect: Signal<Option<(f64, f64, f64, f64)>> = use_signal(|| None);
    // Track last click for double-click detection on mousedown
    // (timestamp_ms, x, y) - allows creating node on second mousedown so user can drag immediately
    let mut last_click: Signal<Option<(f64, f64, f64)>> = use_signal(|| None);
    // Track drag start position for multi-selection movement
    let mut drag_start: Signal<Option<(f64, f64)>> = use_signal(|| None);
    // Track original band positions for proportional scaling during multi-drag
    // Stored as (idx, freq, gain).
    let mut drag_start_bands: Signal<Vec<(usize, f32, f32)>> = use_signal(Vec::new);
    // Dropdown states for the popup
    // Right-click context menu state: (band_idx, viewBox_x, viewBox_y)
    let mut context_menu: Signal<Option<(Option<usize>, f64, f64)>> = use_signal(|| None);
    // Track when mouse left the focused band area (for fade timeout)
    // Stores (timestamp_ms, band_idx) when mouse leaves focus area
    let mut focus_leave_time: Signal<Option<(f64, usize)>> = use_signal(|| None);
    // Popup fade timeout in milliseconds
    let popup_fade_timeout_ms = 300.0;
    // Double-click threshold in milliseconds
    let double_click_threshold_ms = 400.0;
    // Distance threshold for double-click (in viewBox coords)
    let double_click_distance = 20.0;
    // Focus detection radius (larger so popup is easier to interact with)
    let focus_radius = 50.0;

    // ── GPU-accelerated graph rendering ─────────────────────────────────
    // Two paths share the same EqGraphRenderState; only one renders at a
    // time depending on which host context is present.
    //
    //   * Plugin-embedded (DAW host): `OverlayRegistry` is in context — the
    //     `SceneOverlay` background path paints into the main vello scene.
    //   * Standalone / `dx serve` (winit + blitz-shell): the
    //     `DioxusNativeWindowRenderer` is in context — the `CustomPaintSource`
    //     path renders into a wgpu texture composited as a `<canvas>`.
    let render_state = use_hook(EqGraphRenderState::new);

    // EQ cheat-sheet overlay selection. `Auto` resolves the profile from the host
    // track name via the injected `TrackInfoProvider` (a CLAP track-info backend
    // in the plugin, a static/env name in the standalone + tests); `Off` hides it;
    // `Pick` forces a profile. The resolved profile (`active_overlay`, computed
    // each render below) is synced into `render_state.overlay` for the widget and
    // used to position the DOM labels.
    // Use the parent-provided selection signal if given, else an internal one.
    let overlay_sel_internal = use_signal(|| OverlayChoice::Auto);
    let mut overlay_sel: Signal<OverlayChoice> = overlay_sel.unwrap_or(overlay_sel_internal);
    // Track-name source. Optional: standalone/plugin inject it via context; if
    // absent, `Auto` simply resolves to off.
    let track_provider =
        use_hook(|| try_consume_context::<Arc<dyn crate::cheatsheet::TrackInfoProvider>>());

    // Empirically-discovered wrapper bounds. The canvas's content_box
    // (cfg.rect_w/rect_h) and the wrapper's element_coordinate space
    // are in principle the same size, but in blitz they sometimes
    // differ by a handful of pixels (border interactions, layout
    // rounding) — small enough to throw off precise band hit-tests.
    // We pin them down by recording the maximum element_coordinate
    // ever observed inside the wrapper, on every mouse event.
    let mut observed_max: Signal<(f64, f64)> = use_signal(|| (0.0, 0.0));

    // EQ graph rendering: a blitz native custom widget. Its `paint()` records the
    // graph into blitz's own scene at the `<object>` node's box (see the rsx
    // below) — works identically in the plugin editor and standalone, with no
    // SceneOverlay / wgpu-`<canvas>` side-channels. The widget holds an `Arc` to
    // `render_state`, so it always paints the latest bands/curve; blitz repaints
    // it on the frame tick driven below.
    let graph_widget = {
        let state = render_state.clone();
        use_memo(move || CustomWidgetAttr::new(EqGraphWidget::new(state.clone())))
    };

    // Drive continuous repaints. Blitz only re-runs the canvas paint source
    // when it repaints, and it only repaints when the DOM mutates. We need a
    // 60Hz tick from outside any winit input event.
    //
    // `dioxus_core::schedule_update` returns an `Arc<dyn Fn() + Send + Sync>`
    // that — per its docs — explicitly works outside the dioxus runtime, so
    // we can hand it to an OS thread that pings it every 16ms. The call
    // marks the EqGraph scope dirty, which makes `vdom.wait_for_work()`
    // return Ready, which fires the blitz Poll waker, which polls the vdom,
    // which mutates `data-tick` on the outer div, which triggers a
    // request_redraw, which calls our paint source's `render()`. The chain
    // we tried via `use_future + futures_timer` does the same thing in
    // theory but in practice the task waker doesn't propagate up to blitz's
    // event-loop waker reliably here — schedule_update bypasses that.
    let frame_tick: Signal<u64> = use_signal(|| 0);
    use_hook(|| {
        let updater = dioxus_core::schedule_update();
        std::thread::spawn(move || {
            loop {
                // 120 Hz (~8.33 ms) to match the App tick — smoother on
                // high-refresh displays. Presentation stays vsync-bounded.
                std::thread::sleep(std::time::Duration::from_micros(8_333));
                updater();
            }
        });
    });
    // Subscribe so the component re-renders when the tick fires; we no
    // longer write the value into a `data-tick` attribute (which made
    // blitz re-layout every frame and confused content_box reporting).
    // The redraw_pump in the standalone host drives actual repaints
    // independently.
    let _ = *frame_tick.read();

    let mut mounted: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    // `layout_rect` is kept only as a zero fallback for the hit-test sizing below.
    //
    // We deliberately do NOT measure it via `el.get_client_rect()` anymore: in
    // blitz 9ebd23a that call goes through `doc_mut()` and panics ("RefCell
    // already borrowed") when polled while the document is borrowed (a
    // dioxus-native regression). It's also redundant now — the `EqGraphWidget`
    // publishes the live canvas size + DPR into `render_state.config` during
    // `paint()`, which `canvas_w/h/scale` below read directly.
    let layout_rect: Signal<(f64, f64, f64, f64)> = use_signal(|| (0.0, 0.0, 0.0, 0.0));

    // Read measured layout rect for hit-testing dimensions/offsets.
    let (act_ox, act_oy, act_rw, act_rh) = *layout_rect.read();
    // Hit-test dimensions: prefer the *paint source*'s most recent
    // canvas content_box (it gets called every frame with blitz's actual
    // layout), then fall back to the async-measured layout_rect, then
    // the viewBox default. The paint source delivers physical pixels
    // (DPR-scaled), so divide by `scale` to convert to CSS pixels —
    // `evt.element_coordinates()` is CSS too, and they have to match.
    let (canvas_w, canvas_h, canvas_scale) = {
        let cfg = render_state.config.read();
        (cfg.rect_w, cfg.rect_h, cfg.scale.max(1.0))
    };
    let canvas_w_css = canvas_w / canvas_scale;
    let canvas_h_css = canvas_h / canvas_scale;
    let (obs_x, obs_y) = *observed_max.read();
    // Take the max of (paint source's reported size) and (max element
    // coord we've actually seen). The latter is authoritative: if the
    // user successfully clicked at element_y=N, the wrapper is at
    // least N tall regardless of what content_box reports.
    // Graph size for hit-testing comes straight from the widget-published canvas
    // box (`cfg.rect_w/h` set in `EqGraphWidget::paint` from blitz's actual node
    // box every frame), so it tracks window resizes exactly. The old
    // `observed_max` one-way ratchet was removed: it never shrank, so after
    // making the window smaller it kept the stale (larger) size and hit-tests
    // drifted. Fall back to the viewBox default only before the first paint.
    let _ = (obs_x, obs_y, act_rw, act_rh);
    let graph_width = if canvas_w_css > 1.0 {
        canvas_w_css
    } else {
        vb_width
    };
    let graph_height = if canvas_h_css > 1.0 {
        canvas_h_css
    } else {
        vb_height
    };

    // Resolve the cheat-sheet overlay each render: `Auto` reads the track name
    // fresh from the provider (so host renames update live), `Pick` forces a
    // profile, `Off` hides it. Reused below for the painter sync + DOM labels.
    let active_overlay: Option<&'static crate::cheatsheet::InstrumentProfile> =
        match *overlay_sel.read() {
            OverlayChoice::Off => None,
            OverlayChoice::Pick(p) => Some(p),
            OverlayChoice::Auto => track_provider
                .as_ref()
                .and_then(|p| crate::cheatsheet::overlay_for(p.as_ref())),
        };

    // Sync component state → painter each render. Note: we deliberately
    // do NOT touch `rect_w/rect_h/scale` here — those are owned by the
    // paint source's `render()` callback (which gets the authoritative
    // canvas content_box from blitz every frame). Writing them from
    // the component would race with the painter, and the
    // `layout_rect` fallback we used to use is async-measured + often
    // stale.
    {
        *render_state.bands.write() = bands.read().clone();
        let mut cfg = render_state.config.write();
        cfg.db_range = db_range;
        cfg.min_freq = min_freq;
        cfg.max_freq = max_freq;
        cfg.sample_rate = sample_rate;
        cfg.show_grid = show_grid;
        cfg.fill_curve = fill_curve;
        // Only seed rect dims when the paint source hasn't run yet
        // (cfg.rect_w/h still at default), so the embedded plugin path
        // (no paint source) still gets reasonable values from
        // layout_rect.
        if cfg.rect_w <= 1.0 && act_rw > 0.0 {
            cfg.rect_w = act_rw;
        }
        if cfg.rect_h <= 1.0 && act_rh > 0.0 {
            cfg.rect_h = act_rh;
        }
        drop(cfg);

        let mut interaction = render_state.interaction.write();
        interaction.hovered_band = *hovered_band.read();
        interaction.dragging_band = *dragging_band.read();
        interaction.focused_band = *focused_band.read();
        interaction.selected_bands = selected_bands.read().clone();
        drop(interaction);

        *render_state.overlay.write() = active_overlay;

        if let Some(spectrum) = &spectrum_db {
            *render_state.spectrum_db.write() = spectrum.clone();
        } else {
            render_state.spectrum_db.write().clear();
        }
        if let Some(snap) = &analyzer_snapshot {
            *render_state.analyzer.write() = snap.clone();
        } else {
            *render_state.analyzer.write() = AnalyzerSnapshot::default();
        }
        if let Some(response) = &model_response_db {
            *render_state.model_response_db.write() = response.clone();
        } else {
            render_state.model_response_db.write().clear();
        }
    }

    // Coordinate conversions (CSS pixels, matching the vello painter).
    let mapper = GraphMapper::new(
        min_freq,
        max_freq,
        db_range,
        graph_width,
        graph_height,
        padding,
    );

    // `evt.element_coordinates()` is already wrapper-local (0,0 at the
    // top-left of the listener element), so no further translation is
    // needed. Previously this subtracted the layout_rect origin —
    // correct for window-relative input but wrong for element-relative,
    // and `act_ox/act_oy` were often stale (`get_client_rect` is async)
    // which made hit-tests miss entirely.
    let _ = (act_ox, act_oy);
    let transform_coords = move |elt_x: f64, elt_y: f64| -> (f64, f64) { (elt_x, elt_y) };

    rsx! {
        div {
            style: "position:absolute; top:0; left:0; right:0; bottom:0; user-select:none;",
            onmounted: move |event: MountedEvent| {
                mounted.set(Some(event.data()));
            },

            // Wheel: adjust Q for focused/hovered/dragging band
            onwheel: move |evt: WheelEvent| {
                evt.prevent_default();
                if disabled { return; }
                let target_band = dragging_band.read().or(*focused_band.read()).or(*hovered_band.read());
                if let Some(band_idx) = target_band {
                    let delta = evt.delta().strip_units().y;
                    let slope_mode = evt.modifiers().ctrl() || evt.modifiers().meta();
                    let updated = {
                        let mut bv = bands.write();
                        if band_idx < bv.len() {
                            let band = &mut bv[band_idx];
                            band.q = wheel_q_for_shape(band.shape, band.q, delta, slope_mode);
                            Some(bv[band_idx].clone())
                        } else { None }
                    };
                    if let (Some(b), Some(cb)) = (updated, &on_band_change) { cb.call((band_idx, b)); }
                }
            },

            // Mouse leave: end any active drag
            onmouseleave: move |_| {
                // Don't end an in-progress drag when the cursor leaves the
                // window — the user may still be holding the button. The drag
                // resumes when the cursor re-enters (onmousemove). If the button
                // was actually released outside the window, onmousemove detects
                // the missing held button and ends the drag then.
                hovered_band.set(None);
            },

            // Mouse move: drag, hover hit-test, focus detection
            onmousemove: move |evt: MouseEvent| {
                if disabled { return; }
                let coords = evt.element_coordinates();
                {
                    let (mx, my) = *observed_max.peek();
                    let new_x = mx.max(coords.x);
                    let new_y = my.max(coords.y);
                    if new_x != mx || new_y != my {
                        observed_max.set((new_x, new_y));
                    }
                }
                let (x, y) = transform_coords(coords.x, coords.y);

                // Update selection rectangle
                let sel = { *selection_rect.read() };
                if let Some((sx, sy, _, _)) = sel {
                    selection_rect.set(Some((sx, sy, x, y)));
                    return;
                }

                let drag_idx = { *dragging_band.read() };

                // If a drag is active but the primary button is no longer held,
                // the button was released outside the window while we weren't
                // getting events. End the drag instead of resuming it on re-entry.
                if let Some(band_idx) = drag_idx {
                    if !evt.held_buttons().contains(MouseButton::Primary) {
                        dragging_band.set(None);
                        drag_start.set(None);
                        drag_start_bands.set(Vec::new());
                        if let Some(cb) = &on_end {
                            cb.call(band_idx);
                        }
                        return;
                    }
                }

                // Drag band(s)
                if let Some(band_idx) = drag_idx {
                    let selected = { selected_bands.read().clone() };
                    let is_multi = selected.len() > 1 && selected.contains(&band_idx);

                    if is_multi {
                        if let Some((_sx, _sy)) = { *drag_start.read() } {
                            let start_bands = { drag_start_bands.read().clone() };
                            if let Some(&(_, orig_freq, orig_gain)) = start_bands.iter().find(|(i, _, _)| *i == band_idx) {
                                let new_gain = mapper.y_to_db(y).clamp(-30.0, 30.0) as f32;
                                let gain_delta = new_gain - orig_gain;
                                let scale = if orig_gain.abs() > 0.01 { new_gain / orig_gain } else { 1.0 + gain_delta / 10.0 };
                                let new_freq = mapper.x_to_freq(x).clamp(10.0, 30000.0) as f32;
                                let freq_ratio = new_freq / orig_freq;
                                let mut bv = bands.write();
                                for &(idx, _, og) in &start_bands {
                                    if idx < bv.len() {
                                        let scaled_gain = (og * scale).clamp(-30.0, 30.0) as f64;
                                        bv[idx].gain = drag_gain_for_shape(bv[idx].shape, og, scaled_gain);
                                        if let Some(&(_, of_, _)) = start_bands.iter().find(|(i,_,_)| *i==idx) {
                                            bv[idx].frequency = (of_ * freq_ratio).clamp(10.0, 30000.0);
                                        }
                                    }
                                }
                                let updates: Vec<_> = start_bands.iter()
                                    .filter_map(|&(i,_,_)| if i < bv.len() { Some((i, bv[i].clone())) } else { None })
                                    .collect();
                                drop(bv);
                                if let Some(cb) = &on_band_change {
                                    for (i, b) in updates { cb.call((i, b)); }
                                }
                            }
                        }
                    } else {
                        let nf = mapper.x_to_freq(x).clamp(10.0, 30000.0) as f32;
                        let pointer_gain = mapper.y_to_db(y).clamp(-30.0, 30.0);
                        let updated = {
                            let mut bv = bands.write();
                            if band_idx < bv.len() {
                                bv[band_idx].frequency = nf;
                                bv[band_idx].gain = drag_gain_for_shape(
                                    bv[band_idx].shape,
                                    bv[band_idx].gain,
                                    pointer_gain,
                                );
                                Some((band_idx, bv[band_idx].clone()))
                            } else { None }
                        };
                        if let (Some((i, b)), Some(cb)) = (updated, &on_band_change) { cb.call((i, b)); }
                    }
                    return;
                }

                // Hover hit-test (replaces invisible SVG circles)
                let new_hover = {
                    let bv = bands.read();
                    nearest_band(&bv, mapper, x, y, 15.0).map(|(i, _)| i)
                };
                if *hovered_band.peek() != new_hover { hovered_band.set(new_hover); }

                // Focus detection (drives popup visibility)
                let closest_for_focus = {
                    let bv = bands.read();
                    nearest_band(&bv, mapper, x, y, focus_radius)
                };
                let new_focus = closest_for_focus.map(|(i,_)| i);
                let cur_focus = { *focused_band.read() };
                let leave_time = { *focus_leave_time.read() };
                let now = now_ms();
                match (cur_focus, new_focus) {
                    (Some(old), Some(new)) if old != new => { set_focused(Some(new)); focus_leave_time.set(None); }
                    (Some(_), Some(_)) => { focus_leave_time.set(None); }
                    (None, Some(new)) => { set_focused(Some(new)); focus_leave_time.set(None); }
                    (Some(old), None) => {
                        match leave_time {
                            None => { focus_leave_time.set(Some((now, old))); }
                            Some((ts, idx)) if idx == old => {
                                if now - ts > popup_fade_timeout_ms {
                                    set_focused(None);
                                    focus_leave_time.set(None);
                                }
                            }
                            _ => { focus_leave_time.set(Some((now, old))); }
                        }
                    }
                    (None, None) => { focus_leave_time.set(None); }
                }
            },

            // Mouse up: complete selection rect or end drag
            onmouseup: move |evt: MouseEvent| {
                let coords = evt.element_coordinates();
                let (x, y) = transform_coords(coords.x, coords.y);

                let sel = { *selection_rect.read() };
                if let Some((sx, sy, _, _)) = sel {
                    let newly: Vec<usize> = {
                        let bv = bands.read();
                        bands_in_rect(&bv, mapper, sx, sy, x, y)
                    };
                    selected_bands.set(newly);
                    selection_rect.set(None);
                    return;
                }

                let band_idx_opt = { *dragging_band.read() };
                if let Some(band_idx) = band_idx_opt {
                    if !mapper.is_inside(x, y) {
                        if let Some(cb) = &on_band_remove { cb.call(band_idx); }
                    }
                    dragging_band.set(None);
                    drag_start.set(None);
                    drag_start_bands.set(Vec::new());
                    if let Some(cb) = &on_end { cb.call(band_idx); }
                }
            },

            // Mouse down: click/drag bands, double-click to add, right-click menu
            onmousedown: move |evt: MouseEvent| {
                if disabled { return; }
                let coords = evt.element_coordinates();
                // Interaction trace (FTS_EQ_TRACE=1): one line per press with
                // every input the hit-test depends on — the tool for
                // diagnosing dead clicks in embedded hosts (issue #31).
                let trace = std::env::var_os("FTS_EQ_TRACE").is_some();
                if trace {
                    let (tx, ty) = transform_coords(coords.x, coords.y);
                    let near = {
                        let bv = bands.read();
                        crate::eq_graph_interaction::nearest_band(&bv, mapper, tx, ty, 15.0)
                    };
                    eprintln!(
                        "[EQ-TRACE] mousedown elt=({:.1},{:.1}) xy=({:.1},{:.1}) inside={} graph={}x{} near={:?} btn={:?}",
                        coords.x, coords.y, tx, ty,
                        mapper.is_inside(tx, ty),
                        graph_width as i32, graph_height as i32,
                        near.map(|(i, d)| (i, d as i32)),
                        evt.trigger_button(),
                    );
                }
                // Record the max element_coordinate we see — empirically
                // pins the true wrapper size when blitz's content_box
                // reporting is slightly off.
                {
                    let (mx, my) = *observed_max.peek();
                    let new_x = mx.max(coords.x);
                    let new_y = my.max(coords.y);
                    if new_x != mx || new_y != my {
                        observed_max.set((new_x, new_y));
                    }
                }
                let (x, y) = transform_coords(coords.x, coords.y);
                if !mapper.is_inside(x, y) { last_click.set(None); return; }

                // Hit-test existing bands
                let clicked: Option<usize> = {
                    let bv = bands.read();
                    nearest_band(&bv, mapper, x, y, 15.0).map(|(i, _)| i)
                };

                context_menu.set(None);

                // Right-click: show context menu
                if evt.trigger_button() == Some(MouseButton::Secondary) {
                    if let Some(idx) = clicked {
                        context_menu.set(Some((Some(idx), x, y)));
                        set_focused(Some(idx));
                    } else {
                        context_menu.set(Some((None, x, y)));
                        set_focused(None);
                        selected_bands.set(Vec::new());
                    }
                    evt.stop_propagation();
                    evt.prevent_default();
                    return;
                }

                if let Some(idx) = clicked {
                    let now = now_ms();
                    let is_double = { *last_click.read() }.is_some_and(|(t, lx, ly)| {
                        now - t < double_click_threshold_ms &&
                        ((x-lx).powi(2) + (y-ly).powi(2)).sqrt() < double_click_distance
                    });
                    if is_double {
                        last_click.set(None);
                        let updated = {
                            let mut bv = bands.write();
                            if idx < bv.len() { bv[idx].gain = 0.0; Some(bv[idx].clone()) } else { None }
                        };
                        if let (Some(b), Some(cb)) = (updated, &on_band_change) { cb.call((idx, b)); }
                        evt.stop_propagation();
                        return;
                    }
                    last_click.set(Some((now, x, y)));

                    let is_shift = evt.modifiers().shift();
                    let cur_sel = { selected_bands.read().clone() };
                    let new_sel = if is_shift {
                        let mut s = cur_sel.clone();
                        if s.contains(&idx) { s.retain(|&i| i != idx); } else { s.push(idx); }
                        s
                    } else if !cur_sel.contains(&idx) {
                        vec![idx]
                    } else {
                        cur_sel.clone()
                    };
                    selected_bands.set(new_sel.clone());

                    drag_start.set(Some((x, y)));
                    let start_bands: Vec<_> = {
                        let bv = bands.read();
                        new_sel.iter().filter_map(|&i| bv.get(i).map(|b| (i, b.frequency, b.gain))).collect()
                    };
                    drag_start_bands.set(start_bands);
                    selection_rect.set(None);
                    dragging_band.set(Some(idx));
                    set_focused(Some(idx));
                    if let Some(cb) = &on_begin { cb.call(idx); }
                    evt.stop_propagation();
                    evt.prevent_default();
                    return;
                }

                // Empty area: double-click to add, single to start selection
                let now = now_ms();
                let is_double = { *last_click.read() }.is_some_and(|(t, lx, ly)| {
                    now - t < double_click_threshold_ms &&
                    ((x-lx).powi(2) + (y-ly).powi(2)).sqrt() < double_click_distance
                });

                if is_double {
                    last_click.set(None);
                    let new_idx = {
                        let bv = bands.read();
                        bv.iter().position(|b| !b.used).unwrap_or(bv.len())
                    };
                    if new_idx >= MAX_BANDS { return; }
                    let freq = mapper.x_to_freq(x).clamp(20.0, 20000.0) as f32;
                    let gain = mapper.y_to_db(y).clamp(-db_range, db_range) as f32;
                    let shape = filter_type_for_position(freq as f64, gain as f64, db_range);
                    let final_gain = if shape.uses_gain() { gain } else { 0.0 };
                    let new_band = EqBand { index: new_idx, used: true, enabled: true, frequency: freq,
                        gain: final_gain, q: 1.0, shape, solo: false, stereo_mode: StereoMode::default(),
                        name: String::new() };
                    if let Some(cb) = &on_band_add { cb.call(new_band); }
                    dragging_band.set(Some(new_idx));
                    if let Some(cb) = &on_begin { cb.call(new_idx); }
                    evt.stop_propagation();
                    evt.prevent_default();
                } else {
                    last_click.set(Some((now, x, y)));
                    if !evt.modifiers().shift() { selected_bands.set(Vec::new()); }
                    selection_rect.set(Some((x, y, x, y)));
                    drag_start.set(Some((x, y)));
                }
            },

            // EQ graph — painted by the `EqGraphWidget` blitz custom widget,
            // attached to this `<object>` via its `data` attribute. blitz
            // composites the widget's scene into its own paint pass at this
            // node's box, so it renders identically in the plugin editor and
            // standalone. Stretched to fill the positioned-absolute parent
            // (`inset:0`); pointer-events off so the SVG interaction layer above
            // still receives drags.
            object {
                "data": graph_widget,
                // pointer-events:none so the container div's DOM handlers receive
                // interaction (blitz's element_coordinates() now reports correct
                // element-relative coords via our dioxus-native-dom fix).
                style: "position:absolute; top:0; left:0; right:0; bottom:0; \
                        width:100%; height:100%; \
                        display:block; pointer-events:none;",
            }

            // EQ cheat-sheet overlay selector (top-right corner).
            select {
                style: "position:absolute; top:6px; right:6px; z-index:30; \
                        background:rgba(15,15,18,0.92); color:#ddd; \
                        border:1px solid rgba(80,80,85,0.6); border-radius:4px; \
                        font-size:10px; padding:2px 5px; pointer-events:auto;",
                onchange: move |evt| {
                    let v = evt.value();
                    let sel = match v.as_str() {
                        "auto" => OverlayChoice::Auto,
                        "off" => OverlayChoice::Off,
                        other => other
                            .parse::<usize>()
                            .ok()
                            .and_then(|i| crate::cheatsheet::PROFILES.get(i))
                            .map(OverlayChoice::Pick)
                            .unwrap_or(OverlayChoice::Off),
                    };
                    overlay_sel.set(sel);
                },
                option {
                    value: "auto",
                    selected: matches!(*overlay_sel.read(), OverlayChoice::Auto),
                    // Show what Auto currently resolves to, so it's clear when no
                    // track is detected vs. which profile matched.
                    match active_overlay {
                        Some(p) => format!("Auto · {}", p.name),
                        None => "Auto · (no track)".to_string(),
                    }
                }
                option {
                    value: "off",
                    selected: matches!(*overlay_sel.read(), OverlayChoice::Off),
                    "Cheat sheet: Off"
                }
                for (i, p) in crate::cheatsheet::PROFILES.iter().enumerate() {
                    option {
                        value: "{i}",
                        selected: matches!(*overlay_sel.read(), OverlayChoice::Pick(sel) if std::ptr::eq(sel, p)),
                        "{p.name}"
                    }
                }
            }

            // Cheat-sheet zone labels (DOM, positioned via the mapper). One per
            // zone, tinted by direction, at the zone's center frequency.
            if let Some(profile) = active_overlay {
                for (zi, zone) in profile.zones.iter().enumerate() {
                    {
                        let cx = (mapper.freq_to_x(zone.lo_hz as f64)
                            + mapper.freq_to_x(zone.hi_hz as f64))
                            / 2.0;
                        let (r, g, b, _) = zone.dir.rgba();
                        // Stagger vertically so adjacent labels don't fully overlap.
                        let top = 10.0 + (zi % 3) as f64 * 12.0;
                        rsx! {
                            div {
                                key: "{zi}",
                                style: format!(
                                    "position:absolute; left:{cx}px; top:{top}px; \
                                     transform:translateX(-50%); z-index:25; \
                                     font-size:9px; font-weight:600; \
                                     color:rgb({r},{g},{b}); text-shadow:0 1px 2px #000; \
                                     pointer-events:none; white-space:nowrap;",
                                ),
                                "{zone.role}"
                            }
                        }
                    }
                }
            }

            // ISO-octave ear-training ruler (vowel / haptic / sibilance cues) at
            // each octave center along the TOP, tinted by the same frequency→color
            // map as the band nodes. Always on, faint, as a constant reference.
            {
                rsx! {
                    for eb in crate::cheatsheet::EAR_BANDS.iter() {
                        {
                            let ex = mapper.freq_to_x(eb.center_hz as f64);
                            let color = crate::eq_graph_model::freq_to_color(eb.center_hz as f64);
                            rsx! {
                                div {
                                    key: "ear-{eb.center_hz}",
                                    style: format!(
                                        "position:absolute; left:{ex}px; top:3px; \
                                         transform:translateX(-50%); z-index:8; \
                                         font-size:9px; font-weight:600; opacity:0.6; \
                                         color:{color}; text-shadow:0 1px 2px #000; \
                                         pointer-events:none; white-space:nowrap;",
                                    ),
                                    "{eb.cue}"
                                }
                            }
                        }
                    }
                }
            }

            // Too-much / too-little range descriptors near the 0 dB line:
            // `too_much` just above the line (the boost direction), `too_little`
            // just below (the cut direction), at each range's geometric center.
            {
                let center_y = mapper.db_to_y(0.0);
                let tm_y = center_y - 2.0;
                let tl_y = center_y + 2.0;
                rsx! {
                    for fr in crate::cheatsheet::FREQ_RANGES.iter() {
                        {
                            let cx = mapper.freq_to_x((fr.lo_hz as f64 * fr.hi_hz as f64).sqrt());
                            rsx! {
                                div {
                                    key: "tm-{fr.lo_hz}",
                                    style: format!(
                                        "position:absolute; left:{cx}px; top:{tm_y}px; \
                                         transform:translate(-50%, -100%); z-index:7; \
                                         font-size:8px; font-weight:600; opacity:0.5; \
                                         color:#f0a0a0; text-shadow:0 1px 2px #000; \
                                         pointer-events:none; white-space:nowrap;",
                                    ),
                                    "▲ {fr.too_much}"
                                }
                                div {
                                    key: "tl-{fr.lo_hz}",
                                    style: format!(
                                        "position:absolute; left:{cx}px; top:{tl_y}px; \
                                         transform:translateX(-50%); z-index:7; \
                                         font-size:8px; font-weight:600; opacity:0.5; \
                                         color:#8cc7e6; text-shadow:0 1px 2px #000; \
                                         pointer-events:none; white-space:nowrap;",
                                    ),
                                    "▼ {fr.too_little}"
                                }
                            }
                        }
                    }
                }
            }

            // Band name labels — show the user's annotation above each named
            // node so EQ moves are self-documenting at a glance.
            {
                let bv = bands.read();
                rsx! {
                    for band in bv.iter().filter(|b| b.used && !b.name.trim().is_empty()).cloned() {
                        {
                            let lx = mapper.freq_to_x(band.frequency as f64);
                            let ly = mapper.db_to_y(band.gain as f64);
                            let color = crate::eq_graph_model::freq_to_color(band.frequency as f64);
                            rsx! {
                                div {
                                    key: "name-{band.index}",
                                    style: format!(
                                        "position:absolute; left:{lx}px; top:{ly}px; \
                                         transform:translate(-50%, -150%); z-index:24; \
                                         font-size:10px; font-weight:600; \
                                         color:{color}; text-shadow:0 1px 2px #000; \
                                         pointer-events:none; white-space:nowrap;",
                                    ),
                                    "{band.name}"
                                }
                            }
                        }
                    }
                }
            }

            // Selection rectangle overlay
            {
                let sel = *selection_rect.read();
                if let Some((sx, sy, cx, cy)) = sel {
                    let mnx = sx.min(cx); let mny = sy.min(cy);
                    let w = (cx - sx).abs(); let h = (cy - sy).abs();
                    if w > 5.0 || h > 5.0 {
                        rsx! {
                            div {
                                style: format!("position:absolute; left:{mnx}px; top:{mny}px;                                     width:{w}px; height:{h}px;                                     border:1px dashed rgba(100,150,255,0.6);                                     background:rgba(100,150,255,0.15);                                     pointer-events:none; box-sizing:border-box;"),
                            }
                        }
                    } else { rsx! {} }
                } else { rsx! {} }
            }

            // Band info popup
            {
                let dragging = *dragging_band.read();
                let focused  = *focused_band.read();
                let hovered  = *hovered_band.read();
                let overlay_idx = dragging.or(focused).or(hovered);
                if let Some(band_idx) = overlay_idx {
                    let band_opt = bands.read().get(band_idx).cloned();
                    if let Some(band) = band_opt {
                        let bx = mapper.freq_to_x(band.frequency as f64);
                        let by = mapper.db_to_y(band.gain as f64);
                        rsx! {
                            BandPopup {
                                key: "{band_idx}",
                                band_idx,
                                bx,
                                by,
                                graph_w: graph_width,
                                graph_h: graph_height,
                                is_dragging: dragging.is_some(),
                                bands,
                                on_band_change: on_band_change,
                                on_band_remove: on_band_remove,
                                on_dismiss: move |_| { set_focused(None); },
                            }
                        }
                    } else { rsx! {} }
                } else { rsx! {} }
            }

            // Right-click context menu
            {
                let ctx = *context_menu.read();
                if let Some((ctx_idx, ctx_x, ctx_y)) = ctx {
                    if let Some(ctx_idx) = ctx_idx {
                        rsx! {
                            BandContextMenu {
                                band_idx: ctx_idx,
                                x: ctx_x,
                                y: ctx_y,
                                graph_w: graph_width,
                                graph_h: graph_height,
                                bands,
                                on_band_change: on_band_change,
                                on_band_remove: on_band_remove,
                                on_dismiss: move |_| { context_menu.set(None); },
                            }
                        }
                    } else {
                        let freq = mapper.x_to_freq(ctx_x).clamp(20.0, 20000.0) as f32;
                        let gain = mapper.y_to_db(ctx_y).clamp(-db_range, db_range) as f32;
                        let shape = filter_type_for_position(freq as f64, gain as f64, db_range);
                        let final_gain = if shape.uses_gain() { gain } else { 0.0 };
                        rsx! {
                            EmptyGraphContextMenu {
                                x: ctx_x,
                                y: ctx_y,
                                graph_w: graph_width,
                                graph_h: graph_height,
                                next_index: {
                                    let bv = bands.read();
                                    bv.iter().position(|b| !b.used).unwrap_or(bv.len())
                                },
                                frequency: freq,
                                gain: final_gain,
                                shape,
                                on_band_add: on_band_add,
                                on_dismiss: move |_| { context_menu.set(None); },
                            }
                        }
                    }
                } else { rsx! {} }
            }
        }
    }
}
