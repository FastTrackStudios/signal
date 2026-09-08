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
//! Ported from the legacy `audio-controls` crate for the `nice_plug_dioxus` Blitz renderer.

use std::rc::Rc;
use std::sync::Arc;

use dioxus_elements::input_data::MouseButton;
use nice_plug_dioxus::prelude::*;
use nice_plug_dioxus::widget::CustomWidgetAttr;

use super::eq_graph_interaction::{
    GraphMapper, bands_in_rect, drag_gain_for_shape, filter_type_for_position, nearest_band,
    wheel_band,
    CreateMode, DotAction, DragMode, Mods, WheelTarget, create_mode, dot_action, drag_mode, dyn_range_step, fine_scale, gain_step,
    wheel_target,
};
pub use super::eq_graph_model::{
    BAND_COLORS, EqBand, EqBandShape, EqGraphRenderState, GraphConfig, InteractionState, MAX_BANDS,
    StereoMode, get_band_color, get_band_fill_color, slope_db,
};
use super::eq_graph_painter::EqGraphWidget;
use super::eq_graph_popup::{BandContextMenu, BandPopup, BandReadoutChip, EmptyGraphContextMenu};
pub use super::eq_graph_response::{calculate_band_response, calculate_combined_response};
use spectrum_analyzer::dsp::AnalyzerSnapshot;

/// Get current timestamp in milliseconds.
pub(crate) fn now_ms() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

/// How the cheat-sheet overlay profile is chosen.
#[derive(Clone, Copy)]
pub enum OverlayChoice {
    /// Resolve from the host track name via the injected `TrackInfoProvider`.
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
    /// dB range (symmetric around 0). Defaults to the ONE canonical default
    /// (`fx.eq.display.defaults-agree`).
    #[props(default = super::eq_graph_model::DEFAULT_DB_RANGE)]
    db_range: f64,
    /// Expand `db_range` automatically when a band is dragged outside it
    /// (`fx.eq.display.auto-range`). Expansion is emitted through
    /// `on_db_range_change`; contraction is never automatic.
    #[props(default = true)]
    auto_range: bool,
    /// The user picked (or auto-range requests) a new display range, in dB.
    /// The owner maps it back to its `db_range` param.
    #[props(default)]
    on_db_range_change: Option<EventHandler<f64>>,
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
    /// `EqGraph` writes the focused band index to this signal from event handlers.
    #[props(default)]
    focused_band_out: Option<Signal<Option<usize>>>,
    /// Optional external signal for the cheat-sheet overlay selection, so a
    /// parent (e.g. the inspector) can drive it too. If omitted, the graph owns
    /// its own internal selection (defaulting to `Auto`).
    #[props(default)]
    overlay_sel: Option<Signal<OverlayChoice>>,
    /// The mix-EQ teaching furniture — the ear-band ruler, the too-much /
    /// too-little range labels, the overlay selector. On for the EQ plugin;
    /// off when the surface is embedded as some OTHER curve (drive emphasis,
    /// decay rate), where the words would be lies
    /// (`fx.embed-eq.one-surface`).
    #[props(default = true)]
    show_hints: bool,
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
    /// Per-band dynamics handles, indexed by band. Supplied by the editor
    /// (which owns the param tree); the graph only forwards the entry for
    /// whichever band has its popup open. Dynamics are deliberately NOT part
    /// of `EqBand` — that model describes the static curve, and routing a
    /// threshold through it would make every one of its twenty construction
    /// sites carry dynamics state it has no opinion about.
    #[props(default)]
    band_dynamics: Option<Vec<crate::dynamics::DynState>>,
    /// Per-band frequency/gain/Q handles, so the band panel can host real
    /// dials rather than readouts. Same rationale as `band_dynamics`.
    #[props(default)]
    band_handles: Option<Vec<crate::dynamics::BandHandles>>,
    /// Actual rendered width of the SVG container in pixels.
    /// Required for accurate mouse coordinate mapping.
    #[props(default = 0.0)]
    rendered_width: f64,
    /// Actual rendered height of the SVG container in pixels.
    #[props(default = 0.0)]
    rendered_height: f64,
    /// X offset of the SVG element from the window's left edge (in pixels).
    /// Needed because Blitz's `element_coordinates()` returns window-relative coords.
    #[props(default = 0.0)]
    offset_x: f64,
    /// Y offset of the SVG element from the window's top edge (in pixels).
    #[props(default = 0.0)]
    offset_y: f64,
    /// Whether the control is disabled.
    #[props(default = false)]
    disabled: bool,
) -> Element {
    // Own copy for the scroll handler; the prop itself is consumed by the
    // popup and the context menu.
    let dyn_for_wheel = band_dynamics.clone();
    let dyn_for_create = band_dynamics.clone();
    let dyn_for_drag = band_dynamics.clone();

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
    // The band whose name label is currently an open text field, if any, and
    // whether that field has been armed to take focus.
    //
    // The two-step is what makes focus actually land, and both halves are
    // load-bearing:
    //
    //  * `autofocus` is applied by blitz as an ATTRIBUTE MUTATION, and it is
    //    ignored unless the node is already in the document. Dioxus builds an
    //    element and sets its attributes BEFORE inserting it, so a field that
    //    is born with `autofocus` never gets focus. Setting the attribute on
    //    the following render — when the field is mounted — does work.
    //  * That second render has to happen after the pointer-UP, because blitz
    //    focuses the graph widget on pointer-up as well as pointer-down and
    //    would otherwise take focus straight back off the field.
    //
    // So: the double-click's mousedown opens the field, and its mouseup arms
    // the focus. `MountedData::set_focus` is not an option here — calling it
    // from an event handler panics, since the document is still borrowed by
    // the dispatch that created the node.
    let mut editing_label: Signal<Option<usize>> = use_signal(|| None);
    let mut label_autofocus: Signal<bool> = use_signal(|| false);
    // The name the band had when its field was opened, held aside while the
    // field is empty.
    //
    // This stands in for select-all-on-open, which is not reachable: blitz's
    // editor selects all only on Cmd+A, the plugin's key path drops modifiers
    // before blitz sees them (the same reason `latched_mods` exists for the
    // graph), and no DOM-side API touches the editor's selection. Worse, the
    // editor's buffer is authoritative once the field is focused — writing
    // the `value` prop does not replace what the user is typing into. A field
    // that opened pre-filled would therefore not just fail to clear, it would
    // visibly APPEND: type "Boxiness" over "Low Shelf" and the field reads
    // "BoxinessLow Shelf".
    //
    // So the field opens empty, with the old name offered as the placeholder:
    // typing replaces it, which is what a selection would have done. Clicking
    // into the field puts the old name back to edit, and closing without
    // typing restores it — nothing is lost by opening the editor.
    let mut label_original: Signal<String> = use_signal(String::new);
    // Closes the open name field, putting the old name back if the user did
    // not type one. Reached from Enter, Escape, and any press elsewhere on
    // the graph — a double-click that opens a field and goes nowhere must not
    // silently wipe the band's name.
    // Puts the old name back if the field is empty. Reached from Enter,
    // Escape, and any press elsewhere on the graph — opening an editor and
    // walking away must not wipe a band's name.
    let mut restore_label_if_empty = move |idx: usize| {
        let restored = {
            let mut bw = bands.write();
            match bw.get_mut(idx) {
                Some(b) if b.name.trim().is_empty() && !label_original.read().is_empty() => {
                    b.name = label_original.take();
                    Some(b.clone())
                }
                _ => None,
            }
        };
        if let (Some(b), Some(cb)) = (restored, &on_band_change) {
            cb.call((idx, b));
        }
    };
    let mut close_label_editor = move || {
        if let Some(idx) = editing_label.take() {
            restore_label_if_empty(idx);
        }
    };
    // Track drag start position for multi-selection movement
    let mut drag_start: Signal<Option<(f64, f64)>> = use_signal(|| None);
    // Track original band positions for proportional scaling during multi-drag
    // Stored as (idx, freq, gain).
    let mut drag_start_bands: Signal<Vec<(usize, f32, f32)>> = use_signal(Vec::new);
    // The band's Q when a drag began — the anchor a Cmd-drag scales from, so
    // the gesture is absolute against the press rather than accumulating
    // rounding from frame to frame.
    let mut drag_start_q: Signal<f32> = use_signal(|| 1.0);
    // Modifiers latched from the last pointer event.
    //
    // Blitz delivers `WheelEvent` with every modifier flag false — verified
    // with FTS_EQ_TRACE: an Alt+scroll and a Cmd+scroll both arrive as
    // `alt=false shift=false ctrl=false meta=false`, while the same modifiers
    // on a `MouseEvent` are reported correctly (Alt+click toggles bypass).
    // Until that is fixed upstream, the scroll gestures read the modifier
    // state the pointer last reported instead. Reaching a band to scroll it
    // means moving onto it, so in practice the latch is current; pressing Alt
    // without moving the mouse is the case it misses.
    let mut latched_mods: Signal<Mods> = use_signal(Mods::default);
    // A modifier chord on a band dot that has not yet been decided.
    //
    // Alt+click toggles bypass and Alt+drag constrains the axis, and both open
    // with the same Alt+mousedown — so the press cannot act. It arms the drag
    // AND remembers the chord; the first real movement cancels the chord
    // (this is a drag), and a release with no movement fires it (this was a
    // click). Acting on press instead made every Alt+drag impossible, which is
    // what `alt_drag_locks_the_band_to_one_axis` caught.
    let mut pending_chord: Signal<Option<(usize, DotAction)>> = use_signal(|| None);
    // Dropdown states for the popup
    // Right-click context menu state: (band_idx, viewBox_x, viewBox_y)
    let mut context_menu: Signal<Option<(Option<usize>, f64, f64)>> = use_signal(|| None);
    // Track when mouse left the focused band area (for fade timeout)
    // Stores (timestamp_ms, band_idx) when mouse leaves focus area
    let mut focus_leave_time: Signal<Option<(f64, usize)>> = use_signal(|| None);
    // Timestamp of the last pointer event the band popup handled itself. The
    // popup stops those events from bubbling (they carry popup-relative
    // coordinates, which would read as "nowhere near the band"), so this is the
    // only signal the graph gets that the pointer is on the panel. It is a
    // timestamp rather than a bool so it can never latch on and pin focus
    // forever if a leave event is missed.
    let popup_activity: Signal<f64> = use_signal(|| 0.0);
    // How long after the last popup contact focus is still held open.
    let popup_grace_ms = 600.0;
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
        use_hook(try_consume_context::<Arc<dyn crate::cheatsheet::TrackInfoProvider>>);

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
        // Feed the painter the dynamics envelope alongside the curve, so a
        // dynamic band shows how far it may travel rather than only where it
        // currently sits.
        *render_state.band_dynamics.write() = band_dynamics.as_ref().map_or_else(
            Vec::new,
            |v| {
                v.iter()
                    .map(|d| crate::eq_graph_model::BandDyn {
                        range_db: d.range_db(),
                        live_db: d.live_db,
                        spectral: d.spectral.normalized() > 0.5,
                    })
                    .collect()
            },
        );
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
                let Some(band_idx) = target_band else { return };
                let delta = evt.delta().strip_units().y;
                let m = evt.modifiers();
                let from_event = Mods::new(m.alt(), m.shift(), m.ctrl() || m.meta());
                // Prefer what the event says; fall back to the latch when it
                // says "nothing held", which on Blitz is always.
                let mods = if from_event == Mods::default() { latched_mods() } else { from_event };
                if std::env::var_os("FTS_EQ_TRACE").is_some() {
                    eprintln!(
                        "[EQ-TRACE] wheel delta={delta:.2} event={from_event:?} latched={:?} used={mods:?}",
                        latched_mods(),
                    );
                }

                // Unmodified scroll means "width", and which control that is
                // depends on the band — slope for a cut, Q for everything else.
                let uses_slope = bands.read().get(band_idx).is_some_and(|b| b.shape.uses_slope());
                let target = wheel_target(mods, uses_slope);

                // Dynamic range lives on the param tree, not the band model,
                // so it is written straight through its handle.
                if matches!(target, WheelTarget::DynRange | WheelTarget::GainAndRange)
                    && let Some(ds) = dyn_for_wheel.as_ref().and_then(|v: &Vec<crate::dynamics::DynState>| v.get(band_idx))
                {
                    let step = dyn_range_step(delta, mods) as f32;
                    ds.range.set_as_gesture((ds.range.normalized() + step).clamp(0.0, 1.0));
                }

                if matches!(target, WheelTarget::Gain | WheelTarget::GainAndRange) {
                    let step = gain_step(delta, mods) as f32;
                    let updated = {
                        let mut bv = bands.write();
                        if band_idx < bv.len() {
                            bv[band_idx].gain = (bv[band_idx].gain + step).clamp(-30.0, 30.0);
                            Some(bv[band_idx].clone())
                        } else { None }
                    };
                    if let (Some(b), Some(cb)) = (updated, &on_band_change) { cb.call((band_idx, b)); }
                }

                if matches!(target, WheelTarget::Q | WheelTarget::Slope) {
                    let slope_mode = target == WheelTarget::Slope;
                    let fine = fine_scale(mods);
                    let updated = {
                        let mut bv = bands.write();
                        if band_idx < bv.len() {
                            wheel_band(&mut bv[band_idx], delta, slope_mode, fine);
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
                {
                    let m = evt.modifiers();
                    let now = Mods::new(m.alt(), m.shift(), m.ctrl() || m.meta());
                    if *latched_mods.peek() != now { latched_mods.set(now); }
                }
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
                                // r[impl fx.eq.display.auto-range]
                                if auto_range && f64::from(new_gain.abs()) >= db_range * 0.95 {
                                    if let (Some(cb), Some(next)) = (
                                        &on_db_range_change,
                                        super::eq_graph_model::DB_RANGE_STEPS
                                            .iter()
                                            .copied()
                                            .find(|&r| r > db_range),
                                    ) {
                                        cb.call(next);
                                    }
                                }
                                let gain_delta = new_gain - orig_gain;
                                let scale = if orig_gain.abs() > 0.01 { new_gain / orig_gain } else { 1.0 + gain_delta / 10.0 };
                                let new_freq = mapper.x_to_freq(x).clamp(10.0, 30000.0) as f32;
                                let freq_ratio = new_freq / orig_freq;
                                let mut bv = bands.write();
                                for &(idx, _, og) in &start_bands {
                                    if idx < bv.len() {
                                        let scaled_gain = f64::from((og * scale).clamp(-30.0, 30.0));
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
                        let m = evt.modifiers();
                        let mods = Mods::new(m.alt(), m.shift(), m.ctrl() || m.meta());
                        let (sx, sy) = drag_start.read().unwrap_or((x, y));
                        // Past the slop radius this is a drag, so whatever
                        // chord the press armed is no longer a click.
                        if pending_chord.peek().is_some() && (x - sx).hypot(y - sy) > 3.0 {
                            pending_chord.set(None);
                        }
                        let mode = drag_mode(mods, x - sx, y - sy);

                        // Cmd-drag: vertical travel is resonance, not gain.
                        // Scaled from the Q at press so the gesture is
                        // absolute — 60 px is one octave of Q, quartered
                        // under Shift.
                        if mode == DragMode::Resonance {
                            let travel = (sy - y) * fine_scale(mods) / 60.0;
                            let q = (f64::from(drag_start_q()) * travel.exp2()).clamp(0.1, 18.0) as f32;
                            let updated = {
                                let mut bv = bands.write();
                                if band_idx < bv.len() { bv[band_idx].q = q; Some(bv[band_idx].clone()) } else { None }
                            };
                            if let (Some(b), Some(cb)) = (updated, &on_band_change) { cb.call((band_idx, b)); }
                            return;
                        }

                        let nf = mapper.x_to_freq(x).clamp(10.0, 30000.0) as f32;
                        let pointer_gain = mapper.y_to_db(y).clamp(-30.0, 30.0);
                        // Auto-range: dragging the band into the display's
                        // top/bottom edge expands the range to the next step
                        // so the curve is never clipped mid-edit
                        // (`fx.eq.display.auto-range`). Never contracts. The
                        // trigger is the edge (≥95 % of the range), because
                        // the mouse handlers are element-scoped — a move
                        // beyond the box never arrives.
                        // A dynamic band reaches further than its node does:
                        // the envelope runs from the static gain to the range
                        // extreme, and it is the extreme that leaves the
                        // display first. Auto-range follows whichever of the
                        // two is further out, so dragging a dynamic band never
                        // pushes its own envelope off the graph.
                        // r[impl fx.eq.display.auto-range]
                        let reach = dyn_for_drag
                            .as_ref()
                            .and_then(|v: &Vec<crate::dynamics::DynState>| v.get(band_idx))
                            .map_or(pointer_gain, |d| {
                                let extreme = pointer_gain + f64::from(d.range_db());
                                if extreme.abs() > pointer_gain.abs() { extreme } else { pointer_gain }
                            });
                        if auto_range && reach.abs() >= db_range * 0.95 {
                            if let (Some(cb), Some(next)) = (
                                &on_db_range_change,
                                super::eq_graph_model::DB_RANGE_STEPS
                                    .iter()
                                    .copied()
                                    .find(|&r| r > db_range),
                            ) {
                                cb.call(next);
                            }
                        }
                        let updated = {
                            let mut bv = bands.write();
                            if band_idx < bv.len() {
                                // Alt locks the drag to whichever axis the
                                // pointer committed to; the other one keeps
                                // the value it had at press.
                                if mode != DragMode::GainOnly {
                                    bv[band_idx].frequency = nf;
                                }
                                if mode != DragMode::FreqOnly {
                                    bv[band_idx].gain = drag_gain_for_shape(
                                        bv[band_idx].shape,
                                        bv[band_idx].gain,
                                        pointer_gain,
                                    );
                                }
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

                // The detail panel is a real, clickable surface — the pointer
                // has to be able to travel from the band node onto it and
                // operate its controls. Three things keep focus alive for that:
                //
                //   1. the panel just handled a pointer event itself (it stops
                //      them bubbling, so this is the only trace we get),
                //   2. the pointer is inside the union of the panel's box and
                //      the band node — i.e. crossing the gap between them,
                //   3. the band is explicitly selected, in which case the panel
                //      is sticky until the selection is cleared.
                let panel_fresh = now - *popup_activity.read() < popup_grace_ms;
                let in_panel_region = cur_focus
                    .and_then(|old| bands.read().get(old).cloned())
                    .is_some_and(|b| {
                        crate::eq_graph_popup::point_in_popup_region(
                            x, y,
                            mapper.freq_to_x(f64::from(b.frequency)),
                            mapper.db_to_y(f64::from(b.gain)),
                            graph_width, graph_height,
                            false,
                        )
                    });
                let is_selected = cur_focus
                    .is_some_and(|old| selected_bands.read().contains(&old));

                if cur_focus.is_some() && (panel_fresh || in_panel_region || is_selected) {
                    focus_leave_time.set(None);
                } else {
                    match (cur_focus, new_focus) {
                        (Some(old), Some(new)) if old != new => { set_focused(Some(new)); focus_leave_time.set(None); }
                        (Some(_), Some(_)) | (None, None) => { focus_leave_time.set(None); }
                        (None, Some(new)) => { set_focused(Some(new)); focus_leave_time.set(None); }
                        (Some(old), None) => {
                            match leave_time {
                                Some((ts, idx)) if idx == old => {
                                    if now - ts > popup_fade_timeout_ms {
                                        set_focused(None);
                                        focus_leave_time.set(None);
                                    }
                                }
                                _ => { focus_leave_time.set(Some((now, old))); }
                            }
                        }
                    }
                }
            },

            // Mouse up: complete selection rect or end drag
            onmouseup: move |evt: MouseEvent| {
                // The release that completes the double-click opening a name
                // field arrives AFTER the field has mounted and focused
                // itself, and blitz focuses the graph widget on pointer-up as
                // well as pointer-down — so without this the field loses focus
                // in the very gesture that opened it.
                if editing_label.read().is_some() && !label_autofocus() {
                    label_autofocus.set(true);
                }
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

                // A chord that survived the press-to-release trip without the
                // pointer moving was a click, not a drag.
                if let Some((idx, action)) = pending_chord.take() {
                    let updated = {
                        let mut bv = bands.write();
                        if idx < bv.len() {
                            match action {
                                DotAction::ToggleBypass => bv[idx].enabled = !bv[idx].enabled,
                                DotAction::CycleShape => {
                                    let all = EqBandShape::all();
                                    let cur = all.iter().position(|s| *s == bv[idx].shape).unwrap_or(0);
                                    let next = all[(cur + 1) % all.len()];
                                    bv[idx].shape = next;
                                    if next.uses_slope() { bv[idx].q = 0.707; }
                                }
                                DotAction::CycleSlope => {
                                    // Wraps rather than clamps: a cycling
                                    // gesture that sticks at the top can only
                                    // be undone with a different gesture.
                                    let cur = bv[idx].slope.unwrap_or(2.0);
                                    bv[idx].slope = Some(if cur >= 10.0 { 1.0 } else { cur + 1.0 });
                                }
                                _ => {}
                            }
                            Some(bv[idx].clone())
                        } else { None }
                    };
                    if let (Some(b), Some(cb)) = (updated, &on_band_change) { cb.call((idx, b)); }
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
                {
                    let m = evt.modifiers();
                    let now = Mods::new(m.alt(), m.shift(), m.ctrl() || m.meta());
                    if *latched_mods.peek() != now { latched_mods.set(now); }
                }
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
                // Any press on the graph outside the name field closes it —
                // the field stops propagation, so reaching here means the
                // press was somewhere else. Deliberately not `onblur`: blitz
                // moves focus to the graph widget on every pointer-up, so a
                // blur handler would shut the field during its own opening
                // gesture.
                close_label_editor();

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
                        (x - lx).hypot(y - ly) < double_click_distance
                    });
                    if is_double {
                        // Double-click a node to NAME it. Resetting a control
                        // to its default by double-clicking belongs on knobs
                        // and the panel below — on the graph the node is the
                        // band itself, and the useful thing to do to a band
                        // you have just double-clicked is say what it is for.
                        last_click.set(None);
                        set_focused(Some(idx));
                        // Clear through `on_band_change`, not just in the
                        // local vector: `bands` is re-derived from the
                        // plugin's parameters every render, so a name only
                        // cleared here is back by the next frame.
                        let cleared = {
                            let mut bw = bands.write();
                            bw.get_mut(idx).map(|b| {
                                let previous = std::mem::take(&mut b.name);
                                (previous, b.clone())
                            })
                        };
                        if let Some((previous, band)) = cleared {
                            label_original.set(previous);
                            if let Some(cb) = &on_band_change {
                                cb.call((idx, band));
                            }
                        }
                        editing_label.set(Some(idx));
                        label_autofocus.set(false);
                        evt.stop_propagation();
                        return;
                    }
                    last_click.set(Some((now, x, y)));

                    let m = evt.modifiers();
                    let mods = Mods::new(m.alt(), m.shift(), m.ctrl() || m.meta());

                    // Arm the chords that act on the band rather than
                    // selecting it. They fire on release, and only if the
                    // pointer never moved — see `pending_chord`.
                    let action = dot_action(mods);
                    pending_chord.set(
                        matches!(
                            action,
                            DotAction::ToggleBypass | DotAction::CycleShape | DotAction::CycleSlope
                        )
                        .then_some((idx, action)),
                    );

                    let cur_sel = { selected_bands.read().clone() };
                    let new_sel = match dot_action(mods) {
                        DotAction::AddToSelection => {
                            let mut s = cur_sel;
                            if s.contains(&idx) { s.retain(|&i| i != idx); } else { s.push(idx); }
                            s
                        }
                        DotAction::RangeSelect => {
                            // Consecutive by band index, anchored on whatever
                            // was focused. With no anchor there is nothing to
                            // span, so it degrades to a plain select.
                            focused_band.read().map_or_else(
                                || vec![idx],
                                |anchor| {
                                    let (lo, hi) = (anchor.min(idx), anchor.max(idx));
                                    (lo..=hi).collect()
                                },
                            )
                        }
                        _ if !cur_sel.contains(&idx) => vec![idx],
                        _ => cur_sel,
                    };
                    selected_bands.set(new_sel.clone());

                    drag_start.set(Some((x, y)));
                    drag_start_q.set(bands.read().get(idx).map_or(1.0, |b| b.q));
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
                    (x - lx).hypot(y - ly) < double_click_distance
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
                    let shape = filter_type_for_position(f64::from(freq), f64::from(gain), db_range);
                    let final_gain = if shape.uses_gain() { gain } else { 0.0 };
                    let new_band = EqBand { index: new_idx, used: true, enabled: true, frequency: freq,
                        gain: final_gain, q: 1.0, slope: None, shape, solo: false, stereo_mode: StereoMode::default(),
                        name: String::new() };
                    if let Some(cb) = &on_band_add { cb.call(new_band); }

                    // Alt creates a dynamic band, Alt+Shift a spectral one.
                    // Applied after `on_band_add` so the editor has already
                    // marked the slot used — `set_mode` writes the range and
                    // spectral params for a band that now exists.
                    let mode = create_mode(Mods::new(
                        evt.modifiers().alt(),
                        evt.modifiers().shift(),
                        evt.modifiers().ctrl() || evt.modifiers().meta(),
                    ));
                    if mode != CreateMode::Static
                        && let Some(ds) = dyn_for_create.as_ref().and_then(|v: &Vec<crate::dynamics::DynState>| v.get(new_idx))
                    {
                        ds.set_mode(match mode {
                            CreateMode::Spectral => crate::dynamics::DynMode::Spectral,
                            _ => crate::dynamics::DynMode::Dynamic,
                        });
                    }

                    dragging_band.set(Some(new_idx));
                    if let Some(cb) = &on_begin { cb.call(new_idx); }
                    evt.stop_propagation();
                    evt.prevent_default();
                } else {
                    last_click.set(Some((now, x, y)));
                    if !evt.modifiers().shift() {
                        selected_bands.set(Vec::new());
                        // A selected band's panel is sticky (see the focus
                        // logic in onmousemove); clicking empty graph is how the
                        // user dismisses it.
                        set_focused(None);
                        focus_leave_time.set(None);
                    }
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

            // Display range selector — always reachable from the graph
            // surface, at the top of the dB scale (`fx.eq.display.range`).
            // Double-click returns to the default range.
            // r[impl fx.eq.display.range]
            select {
                "data-testid": "eq-db-range",
                style: "position:absolute; top:6px; left:6px; z-index:30; \
                        background:rgba(15,15,18,0.92); color:#ddd; \
                        border:1px solid rgba(80,80,85,0.6); border-radius:4px; \
                        font-size:10px; padding:2px 5px; pointer-events:auto;",
                onchange: move |evt| {
                    if let Ok(db) = evt.value().parse::<f64>() {
                        if let Some(cb) = &on_db_range_change {
                            cb.call(db);
                        }
                    }
                },
                ondoubleclick: move |_| {
                    if let Some(cb) = &on_db_range_change {
                        cb.call(super::eq_graph_model::DEFAULT_DB_RANGE);
                    }
                },
                for r in super::eq_graph_model::DB_RANGE_STEPS {
                    option {
                        value: "{r}",
                        selected: (db_range - r).abs() < 0.5,
                        "± {r:.0} dB"
                    }
                }
            }

            // EQ cheat-sheet overlay selector (top-right corner).
            if show_hints {
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
                            .map_or(OverlayChoice::Off, OverlayChoice::Pick),
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
            }

            // ── Axis labels ──────────────────────────────────────────────
            //
            // The graph had neither: no frequency along the bottom, no dB up
            // the side. The hover panel reports a band's own numbers, but with
            // bare gridlines there was no way to read the curve itself — to
            // see that a dip sits at 300 Hz rather than 500, or how many dB a
            // shelf is worth — which is most of what an EQ display is for.
            //
            // Drawn in the DOM like every other label here (the vello painter
            // does shapes; text is the component's job), positioned through
            // the same mapper the curves use so they cannot disagree.
            {
                // The decade anchors every EQ marks, and the ones that survive
                // being squeezed: at 800 px the full 1-2-5 series collides
                // below 100 Hz, so the sub-100 end keeps only 20 and 50.
                const FREQ_TICKS: [(f64, &str); 10] = [
                    (20.0, "20"),
                    (50.0, "50"),
                    (100.0, "100"),
                    (200.0, "200"),
                    (500.0, "500"),
                    (1_000.0, "1k"),
                    (2_000.0, "2k"),
                    (5_000.0, "5k"),
                    (10_000.0, "10k"),
                    (20_000.0, "20k"),
                ];
                rsx! {
                    for (fi , (hz , label)) in FREQ_TICKS.iter().enumerate() {
                        {
                            let x = mapper.freq_to_x(*hz);
                            // Clamp the end labels inward so they are not half
                            // outside the graph.
                            let shift = if fi == 0 {
                                "translateX(0)"
                            } else if fi == FREQ_TICKS.len() - 1 {
                                "translateX(-100%)"
                            } else {
                                "translateX(-50%)"
                            };
                            rsx! {
                                div {
                                    key: "hz{fi}",
                                    "data-testid": "eq-freq-label",
                                    style: format!(
                                        "position:absolute; left:{x}px; bottom:2px; \
                                         transform:{shift}; z-index:24; font-size:9px; \
                                         color:rgba(160,160,172,0.85); \
                                         text-shadow:0 1px 2px #000; \
                                         pointer-events:none; white-space:nowrap;",
                                    ),
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
            {
                // Four gain gridlines plus zero, at fractions of whatever the
                // display range currently is — so the scale stays correct
                // through an auto-range expansion instead of going stale.
                let steps: [f64; 5] = [1.0, 0.5, 0.0, -0.5, -1.0];
                rsx! {
                    for (di , frac) in steps.iter().enumerate() {
                        {
                            let db = frac * db_range;
                            // Keep the scale clear of the two rows that share
                            // its edges: the frequency labels along the bottom
                            // (which swallowed −6 entirely) and the top rim.
                            // The ±range labels sit at the very edge of the
                            // plot, so without this the outermost one of the
                            // five is always the one you cannot read.
                            let y = mapper
                                .db_to_y(db)
                                .clamp(20.0, (graph_height - 16.0).max(20.0));
                            let text = if db.abs() < 0.05 {
                                "0".to_string()
                            } else {
                                format!("{db:+.0}")
                            };
                            rsx! {
                                div {
                                    key: "db{di}",
                                    "data-testid": "eq-db-label",
                                    style: format!(
                                        "position:absolute; right:3px; top:{y}px; \
                                         transform:translateY(-50%); z-index:26; \
                                         font-size:9px; color:rgba(170,170,182,0.95); \
                                         background:rgba(10,10,14,0.78); \
                                         padding:0 3px; border-radius:2px; \
                                         pointer-events:none; white-space:nowrap;",
                                    ),
                                    "{text}"
                                }
                            }
                        }
                    }
                }
            }

            // Cheat-sheet zone labels (DOM, positioned via the mapper). One per
            // zone, tinted by direction, at the zone's center frequency.
            if let Some(profile) = active_overlay {
                for (zi, zone) in profile.zones.iter().enumerate() {
                    {
                        let cx = f64::midpoint(mapper.freq_to_x(f64::from(zone.lo_hz)), mapper.freq_to_x(f64::from(zone.hi_hz)));
                        let (r, g, b, _) = zone.dir.rgba();
                        // Stagger vertically so adjacent labels don't fully overlap.
                        let top = ((zi % 3) as f64).mul_add(12.0, 10.0);
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
            // map as the band nodes. A constant reference for a MIX EQ; an
            // embedded curve (drive emphasis, decay rate) turns it off with
            // `show_hints: false`.
            {
                if show_hints { rsx! {
                    for eb in crate::cheatsheet::EAR_BANDS.iter() {
                        {
                            let ex = mapper.freq_to_x(f64::from(eb.center_hz));
                            let color = crate::eq_graph_model::freq_to_color(f64::from(eb.center_hz));
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
                } } else { rsx! {} }
            }

            // Too-much / too-little range descriptors near the 0 dB line:
            // `too_much` just above the line (the boost direction), `too_little`
            // just below (the cut direction), at each range's geometric center.
            {
                let center_y = mapper.db_to_y(0.0);
                let tm_y = center_y - 2.0;
                let tl_y = center_y + 2.0;
                if show_hints { rsx! {
                    for fr in crate::cheatsheet::FREQ_RANGES.iter() {
                        {
                            let cx = mapper.freq_to_x((f64::from(fr.lo_hz) * f64::from(fr.hi_hz)).sqrt());
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
                                    // Keys are only allowed on the first node of a
                                    // multi-node rsx block (dioxus deprecation lint);
                                    // the `tm-{fr.lo_hz}` key above already uniquely
                                    // identifies this loop iteration's fragment.
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
                } } else { rsx! {} }
            }

            // Band name labels — the user's annotation above each node, so
            // EQ moves are self-documenting at a glance. Double-clicking the
            // NODE swaps its label for a text field in the same spot, which is
            // why the editing form is rendered here alongside the label it
            // replaces.
            {
                let bv = bands.read();
                let editing = *editing_label.read();
                rsx! {
                    for band in bv.iter().filter(|b| b.used
                        && (!b.name.trim().is_empty() || editing == Some(b.index))).cloned()
                    {
                        {
                            let nx = mapper.freq_to_x(f64::from(band.frequency));
                            let ny = mapper.db_to_y(f64::from(band.gain));
                            let (lx, ly) = crate::eq_graph_popup::band_label_anchor(
                                nx, ny, graph_width, graph_height,
                            );
                            let color = crate::eq_graph_model::freq_to_color(f64::from(band.frequency));
                            let idx = band.index;
                            // Anchor both forms identically so the label does
                            // not jump when it turns into a field. z-index is
                            // above the readout chip's so the open field is
                            // never buried under it.
                            let anchor = format!(
                                "position:absolute; left:{lx}px; top:{ly}px; \
                                 transform:translate(-50%, -100%); z-index:28; \
                                 font-size:10px; font-weight:600; white-space:nowrap;"
                            );
                            if editing == Some(idx) {
                                // Write through on every keystroke rather than
                                // on commit: the label is the same string, so
                                // the band gets its name as the user types it.
                                let mut commit = move |value: String| {
                                    let name = value.trim().to_string();
                                    let updated = {
                                        let mut bw = bands.write();
                                        bw.get_mut(idx).map(|b| {
                                            b.name = name;
                                            b.clone()
                                        })
                                    };
                                    if let (Some(b), Some(cb)) = (updated, &on_band_change) {
                                        cb.call((idx, b));
                                    }
                                };
                                let held = label_original.read().clone();
                                let show_held = band.name.is_empty() && !held.is_empty();
                                rsx! {
                                    if show_held {
                                        div {
                                            "data-testid": "eq-band-label-held",
                                            // Stands in for `placeholder`,
                                            // which blitz does not implement.
                                            // Inert, and drawn over the empty
                                            // field, so the name the field is
                                            // holding is visible and the click
                                            // that brings it back is aimed at
                                            // the field itself.
                                            style: "{anchor} width:110px; text-align:center; \
                                                    color:#6a6a76; padding:2px 4px; \
                                                    border:1px solid transparent; \
                                                    pointer-events:none; z-index:29;",
                                            "{held}"
                                        }
                                    }
                                    input {
                                        "data-testid": "eq-band-label-input",
                                        r#type: "text",
                                        value: "{band.name}",
                                        // Flipped to true one render later —
                                        // see `label_autofocus`. Needs
                                        // blitz-dom's `autofocus` feature,
                                        // which eq-ui turns on; see Cargo.toml.
                                        autofocus: "{label_autofocus}",
                                        style: "{anchor} width:110px; text-align:center; \
                                                color:{color}; background:rgba(12,12,16,0.96); \
                                                border:1px solid {color}; border-radius:4px; \
                                                padding:2px 4px; outline:none;",
                                        // Keep the graph's gestures out of the
                                        // field: a click here places a caret,
                                        // it does not grab the band underneath.
                                        // It also drops the modelled
                                        // selection — clicking into
                                        // highlighted text is how you say
                                        // "keep this, I want to edit it".
                                        onmousedown: move |evt: MouseEvent| {
                                            evt.stop_propagation();
                                            // Clicking into an empty field is
                                            // how you say "keep what was
                                            // there, I only want to edit it".
                                            restore_label_if_empty(idx);
                                        },
                                        oninput: move |evt| commit(evt.value()),
                                        onkeydown: move |evt: KeyboardEvent| {
                                            match evt.key() {
                                                Key::Enter | Key::Escape => {
                                                    close_label_editor();
                                                }
                                                // Everything else is text and
                                                // must not reach the graph's
                                                // keyboard shortcuts.
                                                _ => {}
                                            }
                                            evt.stop_propagation();
                                        },
                                    }
                                }
                            } else {
                                rsx! {
                                    div {
                                        key: "name-{idx}",
                                        "data-testid": "eq-band-label",
                                        // Display only. The gesture that edits
                                        // it lives on the node: a label that
                                        // takes pointer events sits right on
                                        // top of the node it names and shadows
                                        // it, because the transform that lifts
                                        // it clear is not applied to hit-tests.
                                        style: "{anchor} color:{color}; \
                                                text-shadow:0 1px 2px #000; \
                                                pointer-events:none;",
                                        "{band.name}"
                                    }
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
                                "data-testid": "eq-selection-rect",
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
                        let bx = mapper.freq_to_x(f64::from(band.frequency));
                        let by = mapper.db_to_y(f64::from(band.gain));
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
                                popup_activity,
                                on_band_change: on_band_change,
                                on_band_remove: on_band_remove,
                                dyn_state: band_dynamics.as_ref().and_then(|v| v.get(band_idx).cloned()),
                                handles: band_handles.as_ref().and_then(|v| v.get(band_idx).cloned()),
                                on_dismiss: move |()| { set_focused(None); },
                            }
                            // The three changing numbers, under the cursor.
                            // The docked panel has everything else, but it is
                            // at the bottom of the display and the band is
                            // wherever you are dragging it.
                            BandReadoutChip {
                                band_idx,
                                bx,
                                by,
                                graph_w: graph_width,
                                graph_h: graph_height,
                                bands,
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
                                dyn_state: band_dynamics.as_ref().and_then(|v| v.get(ctx_idx).cloned()),
                                on_dismiss: move |()| { context_menu.set(None); },
                            }
                        }
                    } else {
                        let freq = mapper.x_to_freq(ctx_x).clamp(20.0, 20000.0) as f32;
                        let gain = mapper.y_to_db(ctx_y).clamp(-db_range, db_range) as f32;
                        let shape = filter_type_for_position(f64::from(freq), f64::from(gain), db_range);
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
                                on_dismiss: move |()| { context_menu.set(None); },
                            }
                        }
                    }
                } else { rsx! {} }
            }
        }
    }
}
