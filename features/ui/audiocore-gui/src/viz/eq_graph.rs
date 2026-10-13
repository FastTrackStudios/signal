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

use dioxus_elements::input_data::MouseButton;
use nice_plug_dioxus::prelude::*;

use super::eq_graph_painter::{EqGraphPainter, EqGraphRenderState};

/// Get current timestamp in milliseconds.
fn now_ms() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

/// Maximum number of EQ bands supported.
pub const MAX_BANDS: usize = 24;

/// Stereo placement mode for EQ bands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StereoMode {
    #[default]
    Stereo,
    Left,
    Right,
    Mid,
    Side,
}

impl StereoMode {
    /// Get display label for the stereo mode.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Stereo => "Stereo",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Mid => "Mid",
            Self::Side => "Side",
        }
    }

    /// Get short label for the stereo mode.
    pub fn short_label(&self) -> &'static str {
        match self {
            Self::Stereo => "ST",
            Self::Left => "L",
            Self::Right => "R",
            Self::Mid => "M",
            Self::Side => "S",
        }
    }
}

/// EQ graph band data for rendering.
///
/// A simplified band representation for the EQ graph when
/// the full audiocore-dsp types aren't needed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EqBand {
    /// Band index (0-23).
    pub index: usize,
    /// Whether this band slot is in use.
    pub used: bool,
    /// Whether the band is enabled (bypassed when false).
    pub enabled: bool,
    /// Center frequency in Hz (10-30000).
    pub frequency: f32,
    /// Gain in dB (-30 to +30).
    pub gain: f32,
    /// Q factor (0.025 to 40). For cut filters, this represents slope order.
    pub q: f32,
    /// Filter shape (bell, shelf, cut, etc.).
    pub shape: EqBandShape,
    /// Whether this band is soloed (only this band audible).
    pub solo: bool,
    /// Stereo placement mode.
    pub stereo_mode: StereoMode,
}

/// EQ band filter shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EqBandShape {
    #[default]
    Bell,
    LowShelf,
    HighShelf,
    LowCut,
    HighCut,
    Notch,
    BandPass,
    TiltShelf,
    FlatTilt,
    AllPass,
}

impl EqBandShape {
    /// Get display label for the filter shape.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Bell => "Bell",
            Self::LowShelf => "Low Shelf",
            Self::HighShelf => "High Shelf",
            Self::LowCut => "Low Cut",
            Self::HighCut => "High Cut",
            Self::Notch => "Notch",
            Self::BandPass => "Band Pass",
            Self::TiltShelf => "Tilt Shelf",
            Self::FlatTilt => "Flat Tilt",
            Self::AllPass => "All Pass",
        }
    }

    /// Whether this filter type uses slope (dB/oct) instead of Q.
    pub fn uses_slope(&self) -> bool {
        matches!(self, Self::LowCut | Self::HighCut)
    }

    /// Whether this filter type uses gain.
    pub fn uses_gain(&self) -> bool {
        !matches!(
            self,
            Self::LowCut | Self::HighCut | Self::Notch | Self::AllPass
        )
    }

    /// All available filter shapes.
    pub fn all() -> &'static [EqBandShape] {
        &[
            Self::Bell,
            Self::LowShelf,
            Self::HighShelf,
            Self::LowCut,
            Self::HighCut,
            Self::Notch,
            Self::BandPass,
            Self::TiltShelf,
            Self::FlatTilt,
            Self::AllPass,
        ]
    }
}

/// Convert Q value to slope in dB/octave for cut filters.
pub fn q_to_slope_db(q: f32) -> f32 {
    // Q represents filter order: 0.5 = 6dB/oct, 1.0 = 12dB/oct, etc.
    (q * 2.0).round().max(1.0) * 6.0
}

/// Convert slope in dB/octave to Q value for cut filters.
pub fn slope_db_to_q(slope_db: f32) -> f32 {
    // 6dB/oct = 0.5, 12dB/oct = 1.0, etc.
    (slope_db / 6.0).round().max(1.0) / 2.0
}

/// Band colors matching Pro-Q / ZL Equalizer style.
/// Colors cycle through for bands 0-23, matching the screenshot reference.
pub const BAND_COLORS: &[&str] = &[
    "#4ade80", // 1: Green
    "#60a5fa", // 2: Blue
    "#c084fc", // 3: Purple
    "#f472b6", // 4: Pink
    "#fb7185", // 5: Red/Rose
    "#fb923c", // 6: Orange
    "#facc15", // 7: Yellow
    "#a3e635", // 8: Lime
    "#34d399", // 9: Emerald
    "#22d3d8", // 10: Cyan
    "#818cf8", // 11: Indigo
    "#e879f9", // 12: Fuchsia
    "#f87171", // 13: Red
    "#fdba74", // 14: Light Orange
    "#fde047", // 15: Light Yellow
    "#bef264", // 16: Light Lime
    "#6ee7b7", // 17: Light Emerald
    "#67e8f9", // 18: Light Cyan
    "#a5b4fc", // 19: Light Indigo
    "#f0abfc", // 20: Light Fuchsia
    "#fca5a5", // 21: Light Red
    "#fed7aa", // 22: Peach
    "#fef08a", // 23: Pale Yellow
    "#d9f99d", // 24: Pale Lime
];

/// Get the color for a band by index.
pub fn get_band_color(index: usize) -> &'static str {
    BAND_COLORS[index % BAND_COLORS.len()]
}

/// Get the fill color (semi-transparent) for a band by index.
pub fn get_band_fill_color(index: usize) -> String {
    let hex = get_band_color(index);
    // Convert hex to rgba with low opacity for fill
    if let (Ok(r), Ok(g), Ok(b)) = (
        u8::from_str_radix(&hex[1..3], 16),
        u8::from_str_radix(&hex[3..5], 16),
        u8::from_str_radix(&hex[5..7], 16),
    ) {
        format!("rgba({r}, {g}, {b}, 0.30)")
    } else {
        "rgba(100, 100, 100, 0.30)".to_string()
    }
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
/// use audiocore_gui::viz::{EqGraph, EqBand, EqBandShape};
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
    /// Optional spectrum analyzer data (dB values for logarithmically-spaced bins).
    #[props(default)]
    spectrum_db: Option<Vec<f32>>,
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
    let vb_width = 800.0;
    let vb_height = 350.0;
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
    let mut drag_start_bands: Signal<Vec<(usize, f32, f32)>> = use_signal(Vec::new); // (idx, freq, gain)
                                                                                     // Dropdown states for the popup
                                                                                     // Right-click context menu state: (band_idx, viewBox_x, viewBox_y)
    let mut context_menu: Signal<Option<(usize, f64, f64)>> = use_signal(|| None);
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

    // ── GPU-accelerated graph rendering via vello SceneOverlay ──────────
    // Create shared render state (persists across renders via use_hook)
    let render_state = use_hook(EqGraphRenderState::new);

    // Register the vello painter overlay as BACKGROUND so it renders behind the DOM.
    // Popup divs and selection rect overlays then appear on top naturally.
    let overlay_handle = {
        let state = render_state.clone();
        use_scene_overlay_background(move || EqGraphPainter::new(state))
    };

    // Self-measure our bounding rect (same pattern as VelloCanvas / PeakWaveform).
    // Dioxus tracks `mounted` as a dependency so this re-runs when the element mounts.
    let mut mounted: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let mut layout_rect: Signal<(f64, f64, f64, f64)> = use_signal(|| (0.0, 0.0, 0.0, 0.0));
    // Window size signal — re-measure on resize (same as VelloCanvas).
    let window_size: Option<Signal<(u32, u32)>> = try_consume_context();
    {
        let overlay = overlay_handle.clone();
        use_effect(move || {
            let el = mounted.read().clone();
            // Subscribe to window size so a resize triggers re-measurement.
            if let Some(sig) = window_size {
                let _ = sig.read();
            }
            let overlay = overlay.clone();
            if let Some(el) = el {
                spawn(async move {
                    if let Ok(rect) = el.get_client_rect().await {
                        let ox = rect.origin.x;
                        let oy = rect.origin.y;
                        let rw = rect.size.width;
                        let rh = rect.size.height;
                        overlay.set_rect(ox, oy, rw, rh);
                        if *layout_rect.peek() != (ox, oy, rw, rh) {
                            layout_rect.set((ox, oy, rw, rh));
                        }
                    }
                });
            }
        });
    }

    // Read measured layout rect first so the sync block can use actual dimensions.
    let (act_ox, act_oy, act_rw, act_rh) = *layout_rect.read();
    // Always call set_rect — if dimensions are unknown, pass zero so the painter
    // skips instead of defaulting to full-window paint (which would show as a
    // black rectangle covering areas outside the EQ graph).
    overlay_handle.set_rect(act_ox, act_oy, act_rw, act_rh);
    let graph_width = if act_rw > 0.0 { act_rw } else { vb_width };
    let graph_height = if act_rh > 0.0 { act_rh } else { vb_height };

    // Sync component state → painter each render
    {
        *render_state.bands.write() = bands.read().clone();
        let mut cfg = render_state.config.write();
        cfg.db_range = db_range;
        cfg.min_freq = min_freq;
        cfg.max_freq = max_freq;
        cfg.sample_rate = sample_rate;
        cfg.show_grid = show_grid;
        cfg.fill_curve = fill_curve;
        cfg.rect_w = act_rw;
        cfg.rect_h = act_rh;
        drop(cfg);

        let mut interaction = render_state.interaction.write();
        interaction.hovered_band = *hovered_band.read();
        interaction.dragging_band = *dragging_band.read();
        interaction.focused_band = *focused_band.read();
        interaction.selected_bands = selected_bands.read().clone();
        drop(interaction);

        if let Some(spectrum) = &spectrum_db {
            *render_state.spectrum_db.write() = spectrum.clone();
        }
    }

    // Coordinate conversions (CSS pixels, matching the vello painter)
    let log_min = min_freq.log10();
    let log_max = max_freq.log10();

    let freq_to_x = move |freq: f64| -> f64 {
        let normalized = (freq.log10() - log_min) / (log_max - log_min);
        padding + normalized * graph_width
    };

    let x_to_freq = move |x: f64| -> f64 {
        let normalized = (x - padding) / graph_width;
        10.0_f64.powf(log_min + normalized * (log_max - log_min))
    };

    let db_to_y = move |db: f64| -> f64 {
        let normalized = 0.5 - db / (2.0 * db_range);
        padding + normalized * graph_height
    };

    let y_to_db = move |y: f64| -> f64 {
        let normalized = (y - padding) / graph_height;
        (0.5 - normalized) * 2.0 * db_range
    };

    // Transform window-relative pixel coordinates to graph CSS pixel coordinates.
    let transform_coords = move |win_x: f64, win_y: f64| -> (f64, f64) {
        let rel_x = win_x - act_ox;
        let rel_y = win_y - act_oy;
        (rel_x, rel_y)
    };

    // Check if position is inside the graph area (in viewBox coords)
    let is_inside_graph = move |x: f64, y: f64| -> bool {
        x >= padding && x <= padding + graph_width && y >= padding && y <= padding + graph_height
    };

    // Determine filter type based on click position
    let get_filter_type_for_position = move |freq: f64, gain: f64| -> EqBandShape {
        let gain_near_zero = gain.abs() < db_range * 0.2;

        if freq < 30.0 && gain_near_zero {
            EqBandShape::LowCut // High-pass on left edge
        } else if freq > 15000.0 && gain_near_zero {
            EqBandShape::HighCut // Low-pass on right edge
        } else if freq < 80.0 {
            EqBandShape::LowShelf
        } else if freq > 8000.0 {
            EqBandShape::HighShelf
        } else {
            EqBandShape::Bell
        }
    };

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
                    let q_mul = if delta < 0.0 { 1.15 } else { 0.87 };
                    let updated = {
                        let mut bv = bands.write();
                        if band_idx < bv.len() {
                            bv[band_idx].q = (bv[band_idx].q * q_mul).clamp(0.1, 18.0);
                            Some(bv[band_idx].clone())
                        } else { None }
                    };
                    if let (Some(b), Some(cb)) = (updated, &on_band_change) { cb.call((band_idx, b)); }
                }
            },

            // Mouse leave: end any active drag
            onmouseleave: move |_| {
                let idx = { *dragging_band.read() };
                if let Some(i) = idx {
                    dragging_band.set(None);
                    if let Some(cb) = &on_end { cb.call(i); }
                }
                hovered_band.set(None);
            },

            // Mouse move: drag, hover hit-test, focus detection
            onmousemove: move |evt: MouseEvent| {
                if disabled { return; }
                let coords = evt.element_coordinates();
                let (x, y) = transform_coords(coords.x, coords.y);

                // Update selection rectangle
                let sel = { *selection_rect.read() };
                if let Some((sx, sy, _, _)) = sel {
                    selection_rect.set(Some((sx, sy, x, y)));
                    return;
                }

                let drag_idx = { *dragging_band.read() };

                // Drag band(s)
                if let Some(band_idx) = drag_idx {
                    let selected = { selected_bands.read().clone() };
                    let is_multi = selected.len() > 1 && selected.contains(&band_idx);

                    if is_multi {
                        if let Some((_sx, _sy)) = { *drag_start.read() } {
                            let start_bands = { drag_start_bands.read().clone() };
                            if let Some(&(_, orig_freq, orig_gain)) = start_bands.iter().find(|(i, _, _)| *i == band_idx) {
                                let new_gain = y_to_db(y).clamp(-30.0, 30.0) as f32;
                                let gain_delta = new_gain - orig_gain;
                                let scale = if orig_gain.abs() > 0.01 { new_gain / orig_gain } else { 1.0 + gain_delta / 10.0 };
                                let new_freq = x_to_freq(x).clamp(10.0, 30000.0) as f32;
                                let freq_ratio = new_freq / orig_freq;
                                let mut bv = bands.write();
                                for &(idx, _, og) in &start_bands {
                                    if idx < bv.len() {
                                        bv[idx].gain = (og * scale).clamp(-30.0, 30.0);
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
                        let nf = x_to_freq(x).clamp(10.0, 30000.0) as f32;
                        let ng = y_to_db(y).clamp(-30.0, 30.0) as f32;
                        let updated = {
                            let mut bv = bands.write();
                            if band_idx < bv.len() {
                                bv[band_idx].frequency = nf;
                                bv[band_idx].gain = ng;
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
                    let mut best: Option<(usize, f64)> = None;
                    for (idx, band) in bv.iter().enumerate() {
                        if band.used {
                            let bx = freq_to_x(band.frequency as f64);
                            let by = db_to_y(band.gain as f64);
                            let d = ((x-bx).powi(2) + (y-by).powi(2)).sqrt();
                            if d < 15.0 && (best.is_none() || d < best.unwrap().1) {
                                best = Some((idx, d));
                            }
                        }
                    }
                    best.map(|(i,_)| i)
                };
                if *hovered_band.peek() != new_hover { hovered_band.set(new_hover); }

                // Focus detection (drives popup visibility)
                let closest_for_focus = {
                    let bv = bands.read();
                    let mut best: Option<(usize, f64)> = None;
                    for (idx, band) in bv.iter().enumerate() {
                        if band.used {
                            let bx = freq_to_x(band.frequency as f64);
                            let by = db_to_y(band.gain as f64);
                            let d = ((x-bx).powi(2) + (y-by).powi(2)).sqrt();
                            if d < focus_radius && (best.is_none() || d < best.unwrap().1) {
                                best = Some((idx, d));
                            }
                        }
                    }
                    best
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
                    let (mnx, mxx) = (sx.min(x), sx.max(x));
                    let (mny, mxy) = (sy.min(y), sy.max(y));
                    let newly: Vec<usize> = {
                        let bv = bands.read();
                        bv.iter().enumerate()
                            .filter(|(_, b)| {
                                if !b.used { return false; }
                                let bx = freq_to_x(b.frequency as f64);
                                let by = db_to_y(b.gain as f64);
                                bx >= mnx && bx <= mxx && by >= mny && by <= mxy
                            })
                            .map(|(i, _)| i).collect()
                    };
                    selected_bands.set(newly);
                    selection_rect.set(None);
                    return;
                }

                let band_idx_opt = { *dragging_band.read() };
                if let Some(band_idx) = band_idx_opt {
                    if !is_inside_graph(x, y) {
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
                let (x, y) = transform_coords(coords.x, coords.y);
                if !is_inside_graph(x, y) { last_click.set(None); return; }

                // Hit-test existing bands
                let clicked: Option<usize> = {
                    let bv = bands.read();
                    let mut best: Option<(usize, f64)> = None;
                    for (idx, band) in bv.iter().enumerate() {
                        if !band.used { continue; }
                        let bx = freq_to_x(band.frequency as f64);
                        let by = db_to_y(band.gain as f64);
                        let d = ((x-bx).powi(2) + (y-by).powi(2)).sqrt();
                        if d < 15.0 && (best.is_none() || d < best.unwrap().1) {
                            best = Some((idx, d));
                        }
                    }
                    best.map(|(i,_)| i)
                };

                context_menu.set(None);

                // Right-click: show context menu
                if evt.trigger_button() == Some(MouseButton::Secondary) {
                    if let Some(idx) = clicked {
                        context_menu.set(Some((idx, x, y)));
                        set_focused(Some(idx));
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
                    let freq = x_to_freq(x).clamp(20.0, 20000.0) as f32;
                    let gain = y_to_db(y).clamp(-db_range, db_range) as f32;
                    let shape = get_filter_type_for_position(freq as f64, gain as f64);
                    let final_gain = match shape { EqBandShape::LowCut | EqBandShape::HighCut => 0.0, _ => gain };
                    let new_band = EqBand { index: new_idx, used: true, enabled: true, frequency: freq,
                        gain: final_gain, q: 1.0, shape, solo: false, stereo_mode: StereoMode::default() };
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
                        let bx = freq_to_x(band.frequency as f64);
                        let by = db_to_y(band.gain as f64);
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
                } else { rsx! {} }
            }
        }
    }
}

// ── Band info popup ──────────────────────────────────────────────────────────

#[component]
fn BandPopup(
    band_idx: usize,
    bx: f64,
    by: f64,
    graph_w: f64,
    graph_h: f64,
    is_dragging: bool,
    bands: Signal<Vec<EqBand>>,
    on_band_change: Option<EventHandler<(usize, EqBand)>>,
    on_band_remove: Option<EventHandler<usize>>,
    on_dismiss: EventHandler<()>,
) -> Element {
    let mut show_shape_dropdown = use_signal(|| false);

    let Some(band) = bands.read().get(band_idx).cloned() else {
        return rsx! {};
    };

    let freq_str = if band.frequency >= 1000.0 {
        format!("{:.1}k", band.frequency / 1000.0)
    } else {
        format!("{:.0}", band.frequency)
    };
    let q_str = if band.shape.uses_slope() {
        format!("{:.0}dB/o", q_to_slope_db(band.q))
    } else {
        format!("Q{:.1}", band.q)
    };
    let band_color = get_band_color(band_idx);
    let band_enabled = band.enabled;
    let band_solo = band.solo;
    let band_gain = band.gain;
    let band_shape = band.shape;

    // Popup dimensions — taller when showing buttons
    let popup_w: f64 = 126.0;
    let popup_h: f64 = if is_dragging { 26.0 } else { 52.0 };

    // Position above band node, clamped to graph area
    let popup_x = (bx - popup_w / 2.0).clamp(0.0, (graph_w - popup_w).max(0.0));
    let popup_y = if by - popup_h - 18.0 >= 0.0 {
        by - popup_h - 18.0
    } else {
        by + 18.0
    }
    .clamp(0.0, (graph_h - popup_h).max(0.0));

    let bypass_color = if band_enabled { "#666" } else { "#f66" };
    let bypass_bg = if band_enabled {
        "transparent"
    } else {
        "rgba(255,80,80,0.3)"
    };
    let solo_color = if band_solo { "#fc0" } else { "#666" };
    let solo_bg = if band_solo {
        "rgba(255,200,50,0.3)"
    } else {
        "transparent"
    };

    rsx! {
        div {
            style: format!(
                "position:absolute; left:{popup_x}px; top:{popup_y}px; width:{popup_w}px;                  background:rgba(10,10,12,0.92); border:1px solid {band_color}66;                  border-radius:4px; font-size:10px; color:#fff;                  pointer-events:auto; z-index:10; box-sizing:border-box;",
            ),
            // Prevent clicks from bubbling to the graph's mousedown
            onmousedown: move |evt| { evt.stop_propagation(); },

            // Top row: freq | gain | Q
            div {
                style: "text-align:center; padding:5px 4px 3px; white-space:nowrap;                         font-size:10px; color:#ddd;",
                "{freq_str}Hz  {band_gain:+.1}dB  {q_str}"
            }

            // Button row — only when not dragging
            if !is_dragging {
                div {
                    style: "display:flex; justify-content:space-around;                             align-items:center; padding:0 6px 5px;",

                    // Bypass
                    div {
                        style: format!(
                            "cursor:pointer; width:18px; height:18px; border-radius:50%;                              border:1px solid {bypass_color}; background:{bypass_bg};                              display:flex; align-items:center; justify-content:center;                              font-size:9px; color:{bypass_color};",
                        ),
                        title: if band_enabled { "Bypass" } else { "Enable" },
                        onclick: {
                            let cb = on_band_change;
                            move |evt: MouseEvent| {
                                evt.stop_propagation();
                                let updated = {
                                    let mut bv = bands.write();
                                    if band_idx < bv.len() { bv[band_idx].enabled = !bv[band_idx].enabled; Some(bv[band_idx].clone()) } else { None }
                                };
                                if let (Some(b), Some(c)) = (updated, &cb) { c.call((band_idx, b)); }
                            }
                        },
                        "⏻"
                    }

                    // Solo
                    div {
                        style: format!(
                            "cursor:pointer; width:18px; height:18px; border-radius:50%;                              border:1px solid {solo_color}; background:{solo_bg};                              display:flex; align-items:center; justify-content:center;                              font-size:8px; font-weight:700; color:{solo_color};",
                        ),
                        title: if band_solo { "Unsolo" } else { "Solo" },
                        onclick: {
                            let cb = on_band_change;
                            move |evt: MouseEvent| {
                                evt.stop_propagation();
                                let updated = {
                                    let mut bv = bands.write();
                                    if band_idx < bv.len() { bv[band_idx].solo = !bv[band_idx].solo; Some(bv[band_idx].clone()) } else { None }
                                };
                                if let (Some(b), Some(c)) = (updated, &cb) { c.call((band_idx, b)); }
                            }
                        },
                        "S"
                    }

                    // Shape
                    div {
                        style: "cursor:pointer; padding:2px 5px;                                 background:rgba(60,60,65,0.9); border-radius:2px;                                 font-size:7px; color:#aaa; white-space:nowrap;",
                        onclick: move |evt: MouseEvent| {
                            evt.stop_propagation();
                            show_shape_dropdown.toggle();
                        },
                        "{band_shape.label()}"
                    }

                    // Delete
                    div {
                        style: "cursor:pointer; width:18px; height:18px; border-radius:50%;                                 border:1px solid #666; display:flex; align-items:center;                                 justify-content:center; font-size:14px; color:#888;                                 line-height:1;",
                        title: "Delete",
                        onclick: {
                            let cb = on_band_remove;
                            move |evt: MouseEvent| {
                                evt.stop_propagation();
                                if let Some(c) = &cb { c.call(band_idx); }
                                on_dismiss.call(());
                            }
                        },
                        "×"
                    }
                }

                // Shape dropdown
                if *show_shape_dropdown.read() {
                    div {
                        style: format!(
                            "position:absolute; left:0; top:{popup_h}px; width:{popup_w}px;                              background:rgba(18,18,22,0.98); border:1px solid rgba(80,80,85,0.5);                              border-radius:3px; z-index:11; box-sizing:border-box;",
                        ),
                        for shape in EqBandShape::all() {
                            {
                                let shape_c = *shape;
                                let is_sel = shape_c == band_shape;
                                rsx! {
                                    div {
                                        style: format!(
                                            "padding:4px 8px; cursor:pointer; font-size:8px;                                              background:{}; color:{};",
                                            if is_sel { "rgba(100,150,255,0.3)" } else { "transparent" },
                                            if is_sel { "#fff" } else { "#ccc" },
                                        ),
                                        onclick: {
                                            let cb = on_band_change;
                                            move |evt: MouseEvent| {
                                                evt.stop_propagation();
                                                let updated = {
                                                    let mut bv = bands.write();
                                                    if band_idx < bv.len() {
                                                        bv[band_idx].shape = shape_c;
                                                        if shape_c.uses_slope() { bv[band_idx].q = 1.0; }
                                                        Some(bv[band_idx].clone())
                                                    } else { None }
                                                };
                                                if let (Some(b), Some(c)) = (updated, &cb) { c.call((band_idx, b)); }
                                                show_shape_dropdown.set(false);
                                            }
                                        },
                                        "{shape.label()}"
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

// ── Right-click context menu ─────────────────────────────────────────────────

#[component]
fn BandContextMenu(
    band_idx: usize,
    x: f64,
    y: f64,
    graph_w: f64,
    graph_h: f64,
    bands: Signal<Vec<EqBand>>,
    on_band_change: Option<EventHandler<(usize, EqBand)>>,
    on_band_remove: Option<EventHandler<usize>>,
    on_dismiss: EventHandler<()>,
) -> Element {
    let Some(band) = bands.read().get(band_idx).cloned() else {
        return rsx! {};
    };

    let shapes = EqBandShape::all();
    let menu_w: f64 = 130.0;
    let item_h: f64 = 20.0;
    let menu_h = (5.0 + shapes.len() as f64) * item_h + 16.0;
    let menu_x = x.min((graph_w - menu_w).max(0.0));
    let menu_y = y.min((graph_h - menu_h).max(0.0));

    let band_color = get_band_color(band_idx);
    let is_enabled = band.enabled;
    let is_solo = band.solo;
    let cur_shape = band.shape;

    rsx! {
        // Click-outside dismiss
        div {
            style: "position:absolute; inset:0; z-index:20;",
            onmousedown: {
                let dismiss = on_dismiss;
                move |evt: MouseEvent| { evt.stop_propagation(); dismiss.call(()); }
            },
        }

        // Menu panel
        div {
            style: format!(
                "position:absolute; left:{menu_x}px; top:{menu_y}px; width:{menu_w}px;                  background:rgba(15,15,18,0.97); border:1px solid rgba(80,80,85,0.6);                  border-radius:4px; z-index:21; font-size:9px; color:#aaa;                  box-sizing:border-box;",
            ),
            onmousedown: move |evt| { evt.stop_propagation(); },

            // Header
            div { style: format!("padding:6px 10px 4px; color:{band_color}; font-weight:700;"), "Band {band_idx + 1}" }

            // Bypass / Enable
            div {
                style: format!("padding:4px 10px; cursor:pointer; color:{};",
                    if is_enabled { "#aaa" } else { "#f66" }),
                onclick: {
                    let cb = on_band_change;
                    let dismiss = on_dismiss;
                    move |evt: MouseEvent| {
                        evt.stop_propagation();
                        let upd = { let mut bv = bands.write(); if band_idx < bv.len() { bv[band_idx].enabled = !bv[band_idx].enabled; Some(bv[band_idx].clone()) } else { None } };
                        if let (Some(b), Some(c)) = (upd, &cb) { c.call((band_idx, b)); }
                        dismiss.call(());
                    }
                },
                if is_enabled { "Bypass" } else { "Enable" }
            }

            // Solo
            div {
                style: format!("padding:4px 10px; cursor:pointer; color:{};",
                    if is_solo { "#fc0" } else { "#aaa" }),
                onclick: {
                    let cb = on_band_change;
                    let dismiss = on_dismiss;
                    move |evt: MouseEvent| {
                        evt.stop_propagation();
                        let upd = { let mut bv = bands.write(); if band_idx < bv.len() { bv[band_idx].solo = !bv[band_idx].solo; Some(bv[band_idx].clone()) } else { None } };
                        if let (Some(b), Some(c)) = (upd, &cb) { c.call((band_idx, b)); }
                        dismiss.call(());
                    }
                },
                if is_solo { "Unsolo" } else { "Solo" }
            }

            div { style: "height:1px; background:rgba(80,80,85,0.5); margin:2px 6px;" }

            // Reset Gain
            div {
                style: "padding:4px 10px; cursor:pointer;",
                onclick: {
                    let cb = on_band_change;
                    let dismiss = on_dismiss;
                    move |evt: MouseEvent| {
                        evt.stop_propagation();
                        let upd = { let mut bv = bands.write(); if band_idx < bv.len() { bv[band_idx].gain = 0.0; Some(bv[band_idx].clone()) } else { None } };
                        if let (Some(b), Some(c)) = (upd, &cb) { c.call((band_idx, b)); }
                        dismiss.call(());
                    }
                },
                "Reset Gain"
            }

            div { style: "height:1px; background:rgba(80,80,85,0.5); margin:2px 6px;" }

            div { style: "padding:3px 10px 2px; font-size:8px; color:#555;", "Filter Type" }

            // Filter type list
            for shape in shapes.iter() {
                {
                    let sc = *shape;
                    let is_cur = sc == cur_shape;
                    rsx! {
                        div {
                            style: format!(
                                "padding:3px 10px 3px {}px; cursor:pointer; color:{}; background:{};",
                                if is_cur { 18 } else { 10 },
                                if is_cur { "#fff" } else { "#bbb" },
                                if is_cur { "rgba(100,150,255,0.2)" } else { "transparent" },
                            ),
                            onclick: {
                                let cb = on_band_change;
                                let dismiss = on_dismiss;
                                move |evt: MouseEvent| {
                                    evt.stop_propagation();
                                    let upd = {
                                        let mut bv = bands.write();
                                        if band_idx < bv.len() {
                                            bv[band_idx].shape = sc;
                                            if sc.uses_slope() { bv[band_idx].q = 1.0; }
                                            Some(bv[band_idx].clone())
                                        } else { None }
                                    };
                                    if let (Some(b), Some(c)) = (upd, &cb) { c.call((band_idx, b)); }
                                    dismiss.call(());
                                }
                            },
                            if is_cur { "● " }
                            "{sc.label()}"
                        }
                    }
                }
            }

            div { style: "height:1px; background:rgba(80,80,85,0.5); margin:2px 6px;" }

            // Delete
            div {
                style: "padding:4px 10px; cursor:pointer; color:rgba(255,80,80,0.8);",
                onclick: {
                    let cb = on_band_remove;
                    let dismiss = on_dismiss;
                    move |evt: MouseEvent| {
                        evt.stop_propagation();
                        if let Some(c) = &cb { c.call(band_idx); }
                        dismiss.call(());
                    }
                },
                "Delete Band"
            }
        }
    }
}
/// All EQ curve paths (combined and per-band).
/// Used by SVG rendering path (kept for tests; vello painter has replaced runtime use).
#[allow(dead_code)]
#[derive(Clone, Default, PartialEq)]
struct AllEqCurves {
    /// Combined curve stroke path.
    combined_stroke: String,
    /// Combined curve fill path.
    combined_fill: String,
    /// Per-band curves: Vec of (band_index, stroke_path, fill_path) for each active band.
    band_curves: Vec<(usize, String, String)>,
}

#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
fn generate_all_eq_curves(
    bands: &[EqBand],
    sample_rate: f64,
    min_freq: f64,
    max_freq: f64,
    db_range: f64,
    padding: f64,
    graph_width: f64,
    graph_height: f64,
    num_points: usize,
) -> AllEqCurves {
    let log_min = min_freq.log10();
    let log_max = max_freq.log10();

    let frequencies: Vec<f64> = (0..num_points)
        .map(|i| {
            let t = i as f64 / (num_points - 1) as f64;
            10.0_f64.powf(log_min + t * (log_max - log_min))
        })
        .collect();

    let freq_to_x = |freq: f64| -> f64 {
        let normalized = (freq.log10() - log_min) / (log_max - log_min);
        padding + normalized * graph_width
    };

    let db_to_y = |db: f64| -> f64 {
        let clamped = db.clamp(-db_range, db_range);
        let normalized = 0.5 - clamped / (2.0 * db_range);
        padding + normalized * graph_height
    };

    let zero_y = db_to_y(0.0);

    // Generate combined response
    let combined_response: Vec<f64> = frequencies
        .iter()
        .map(|&freq| calculate_combined_response(bands, freq, sample_rate))
        .collect();

    let (combined_stroke, combined_fill) =
        build_curve_paths(&frequencies, &combined_response, freq_to_x, db_to_y, zero_y);

    // Generate per-band curves (with band index for color mapping)
    let mut band_curves = Vec::new();
    for (idx, band) in bands.iter().enumerate() {
        if !band.used || !band.enabled {
            continue;
        }

        let band_response: Vec<f64> = frequencies
            .iter()
            .map(|&freq| calculate_band_response(band, freq, sample_rate))
            .collect();

        let (stroke, fill) =
            build_curve_paths(&frequencies, &band_response, freq_to_x, db_to_y, zero_y);
        band_curves.push((idx, stroke, fill));
    }

    AllEqCurves {
        combined_stroke,
        combined_fill,
        band_curves,
    }
}

#[allow(dead_code)]
fn build_curve_paths<F, G>(
    frequencies: &[f64],
    response_db: &[f64],
    freq_to_x: F,
    db_to_y: G,
    zero_y: f64,
) -> (String, String)
where
    F: Fn(f64) -> f64,
    G: Fn(f64) -> f64,
{
    let mut stroke_path = String::new();
    for (i, (&freq, &db)) in frequencies.iter().zip(response_db.iter()).enumerate() {
        let x = freq_to_x(freq);
        let y = db_to_y(db);
        if i == 0 {
            stroke_path.push_str(&format!("M{x:.2} {y:.2}"));
        } else {
            stroke_path.push_str(&format!("L{x:.2} {y:.2}"));
        }
    }

    let mut fill_path = String::new();
    let first_x = freq_to_x(frequencies[0]);
    fill_path.push_str(&format!("M{first_x:.2} {zero_y:.2}"));

    for (&freq, &db) in frequencies.iter().zip(response_db.iter()) {
        let x = freq_to_x(freq);
        let y = db_to_y(db);
        fill_path.push_str(&format!("L{x:.2} {y:.2}"));
    }

    let last_x = freq_to_x(*frequencies.last().unwrap());
    fill_path.push_str(&format!("L{last_x:.2} {zero_y:.2}Z"));

    (stroke_path, fill_path)
}

/// Generate the SVG path for the EQ curve.
///
/// Returns (stroke_path, fill_path)
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
fn generate_eq_curve_path(
    bands: &[EqBand],
    sample_rate: f64,
    min_freq: f64,
    max_freq: f64,
    db_range: f64,
    padding: f64,
    graph_width: f64,
    graph_height: f64,
    num_points: usize,
) -> (String, String) {
    let log_min = min_freq.log10();
    let log_max = max_freq.log10();

    let frequencies: Vec<f64> = (0..num_points)
        .map(|i| {
            let t = i as f64 / (num_points - 1) as f64;
            10.0_f64.powf(log_min + t * (log_max - log_min))
        })
        .collect();

    let response_db: Vec<f64> = frequencies
        .iter()
        .map(|&freq| calculate_combined_response(bands, freq, sample_rate))
        .collect();

    let freq_to_x = |freq: f64| -> f64 {
        let normalized = (freq.log10() - log_min) / (log_max - log_min);
        padding + normalized * graph_width
    };

    let db_to_y = |db: f64| -> f64 {
        let clamped = db.clamp(-db_range, db_range);
        let normalized = 0.5 - clamped / (2.0 * db_range);
        padding + normalized * graph_height
    };

    // Build stroke path
    let mut stroke_path = String::new();
    for (i, (&freq, &db)) in frequencies.iter().zip(response_db.iter()).enumerate() {
        let x = freq_to_x(freq);
        let y = db_to_y(db);
        if i == 0 {
            stroke_path.push_str(&format!("M{x:.2} {y:.2}"));
        } else {
            stroke_path.push_str(&format!("L{x:.2} {y:.2}"));
        }
    }

    // Build fill path (closed area from 0dB line)
    let mut fill_path = String::new();
    let zero_y = db_to_y(0.0);

    let first_x = freq_to_x(frequencies[0]);
    fill_path.push_str(&format!("M{first_x:.2} {zero_y:.2}"));

    for (&freq, &db) in frequencies.iter().zip(response_db.iter()) {
        let x = freq_to_x(freq);
        let y = db_to_y(db);
        fill_path.push_str(&format!("L{x:.2} {y:.2}"));
    }

    let last_x = freq_to_x(*frequencies.last().unwrap());
    fill_path.push_str(&format!("L{last_x:.2} {zero_y:.2}Z"));

    (stroke_path, fill_path)
}

pub fn calculate_combined_response(bands: &[EqBand], freq: f64, sample_rate: f64) -> f64 {
    let mut total_db = 0.0;

    for band in bands {
        if !band.used || !band.enabled {
            continue;
        }
        total_db += calculate_band_response(band, freq, sample_rate);
    }

    total_db
}

/// Calculate analog second-order filter magnitude squared.
///
/// This implements |H(jw)|^2 for a biquad filter with transfer function:
/// H(s) = (b0 + b1*s + b2*s^2) / (a0 + a1*s + a2*s^2)
///
/// Coefficients array: [a0, a1, a2, b0, b1, b2]
fn biquad_magnitude_squared(coeff: &[f64; 6], w: f64) -> f64 {
    let w2 = w * w;
    // Denominator: |a0 + a1*jw + a2*(jw)^2|^2 = |a0 - a2*w^2|^2 + |a1*w|^2
    let denom_real = coeff[0] - coeff[2] * w2;
    let denom_imag = coeff[1] * w;
    let denominator = denom_real * denom_real + denom_imag * denom_imag;

    // Numerator: |b0 + b1*jw + b2*(jw)^2|^2 = |b0 - b2*w^2|^2 + |b1*w|^2
    let numer_real = coeff[3] - coeff[5] * w2;
    let numer_imag = coeff[4] * w;
    let numerator = numer_real * numer_real + numer_imag * numer_imag;

    if denominator > 1e-30 {
        numerator / denominator
    } else {
        1.0
    }
}

/// Get coefficients for a second-order low-pass filter.
/// Returns [a0, a1, a2, b0, b1, b2] for H(s) = w0^2 / (s^2 + (w0/Q)*s + w0^2)
fn lowpass_coeffs(w0: f64, q: f64) -> [f64; 6] {
    let w02 = w0 * w0;
    [1.0, w0 / q, w02, w02, 0.0, 0.0]
}

/// Get coefficients for a second-order high-pass filter.
/// Returns [a0, a1, a2, b0, b1, b2] for H(s) = s^2 / (s^2 + (w0/Q)*s + w0^2)
fn highpass_coeffs(w0: f64, q: f64) -> [f64; 6] {
    let w02 = w0 * w0;
    [1.0, w0 / q, w02, 0.0, 0.0, 1.0]
}

/// Get coefficients for a second-order low-shelf filter.
/// Returns [a0, a1, a2, b0, b1, b2]
///
/// Low shelf boosts/cuts frequencies below w0.
/// H(s) = G * (s^2 + sqrt(G)*w0/Q*s + w0^2) / (s^2 + sqrt(G)*w0/Q*s + G*w0^2)  for boost
/// The response is G at DC (s=0) and 1 at high frequencies.
fn lowshelf_coeffs(w0: f64, gain_linear: f64, q: f64) -> [f64; 6] {
    let w02 = w0 * w0;
    let sqrt_g = gain_linear.sqrt();
    let g4 = gain_linear.sqrt().sqrt(); // Fourth root for smoother transition

    // For low shelf: at DC (w=0), we want gain_linear. At high freq, we want 1.
    // Denominator: a0 + a1*s + a2*s^2
    // Numerator: b0 + b1*s + b2*s^2
    // At s=0: H = b0/a0 = gain_linear
    // At s=inf: H = b2/a2 = 1
    [
        w02,
        w0 * g4 / q,
        1.0,
        gain_linear * w02,
        w0 * sqrt_g * g4 / q,
        1.0,
    ]
}

/// Get coefficients for a second-order high-shelf filter.
/// Returns [a0, a1, a2, b0, b1, b2]
///
/// High shelf boosts/cuts frequencies above w0.
/// The response is 1 at DC and G at high frequencies.
fn highshelf_coeffs(w0: f64, gain_linear: f64, q: f64) -> [f64; 6] {
    let w02 = w0 * w0;
    let sqrt_g = gain_linear.sqrt();
    let g4 = gain_linear.sqrt().sqrt(); // Fourth root for smoother transition

    // For high shelf: at DC (w=0), we want 1. At high freq, we want gain_linear.
    // At s=0: H = b0/a0 = 1
    // At s=inf: H = b2/a2 = gain_linear
    [
        w02,
        w0 * g4 / q,
        1.0,
        w02,
        w0 * sqrt_g * g4 / q,
        gain_linear,
    ]
}

/// Get coefficients for a second-order peaking/bell filter.
/// Returns [a0, a1, a2, b0, b1, b2]
///
/// The analog transfer function for a peak filter is:
/// H(s) = (s^2 + s*(w0/Q)*A + w0^2) / (s^2 + s*(w0/Q)/A + w0^2)
/// where A = sqrt(gain_linear) for boost, A = 1/sqrt(gain_linear) for cut
///
/// At w = w0, for boost: magnitude = A * A = gain_linear
/// At w = w0, for cut: magnitude = 1/A * 1/A = gain_linear
fn peak_coeffs(w0: f64, gain_linear: f64, q: f64) -> [f64; 6] {
    let w02 = w0 * w0;
    let a = gain_linear.sqrt();

    // Coefficients: [a0, a1, a2, b0, b1, b2]
    // For the transfer function H(s) = (b0 + b1*s + b2*s^2) / (a0 + a1*s + a2*s^2)
    // We use: H(s) = (w0^2 + (w0*A/Q)*s + s^2) / (w0^2 + (w0/(A*Q))*s + s^2)
    //
    // At s = jw0: numerator = w0^2 + jw0^2*A/Q - w0^2 = jw0^2*A/Q
    //             denominator = w0^2 + jw0^2/(A*Q) - w0^2 = jw0^2/(A*Q)
    //             |H(jw0)| = |A/Q| / |1/(A*Q)| = A^2  = gain_linear
    [w02, w0 / (a * q), 1.0, w02, w0 * a / q, 1.0]
}

/// Get coefficients for a second-order notch filter.
/// Returns [a0, a1, a2, b0, b1, b2]
fn notch_coeffs(w0: f64, q: f64) -> [f64; 6] {
    let w02 = w0 * w0;
    [1.0, w0 / q, w02, w02, 0.0, 1.0]
}

/// Get coefficients for a second-order band-pass filter.
/// Returns [a0, a1, a2, b0, b1, b2]
fn bandpass_coeffs(w0: f64, q: f64) -> [f64; 6] {
    let w02 = w0 * w0;
    [1.0, w0 / q, w02, 0.0, w0 / q, 0.0]
}

/// Calculate cascaded filter response for higher orders.
/// For a filter of order N, we cascade N/2 second-order sections with Butterworth Q distribution.
fn cascaded_magnitude_db(freq: f64, f0: f64, order: usize, filter_type: &EqBandShape) -> f64 {
    if order == 0 {
        return 0.0;
    }

    // Angular frequency
    let w = 2.0 * std::f64::consts::PI * freq;
    let w0 = 2.0 * std::f64::consts::PI * f0;

    // For first-order (order 1), use a simple one-pole response
    if order == 1 {
        let _ratio = freq / f0;
        let mag_sq = match filter_type {
            EqBandShape::LowCut | EqBandShape::HighCut => {
                // First-order high-pass: H(s) = s / (s + w0)
                // |H(jw)|^2 = w^2 / (w^2 + w0^2)
                if matches!(filter_type, EqBandShape::LowCut) {
                    let w2 = w * w;
                    let w02 = w0 * w0;
                    w2 / (w2 + w02)
                } else {
                    // First-order low-pass: H(s) = w0 / (s + w0)
                    // |H(jw)|^2 = w0^2 / (w^2 + w0^2)
                    let w2 = w * w;
                    let w02 = w0 * w0;
                    w02 / (w2 + w02)
                }
            }
            _ => 1.0,
        };
        return 10.0 * mag_sq.max(1e-30).log10();
    }

    // For higher orders, cascade second-order sections
    let num_sections = order / 2;
    let has_first_order = order % 2 == 1;

    let mut total_mag_sq = 1.0;

    // Add first-order section if odd order
    if has_first_order {
        let first_order_mag = match filter_type {
            EqBandShape::LowCut => {
                let w2 = w * w;
                let w02 = w0 * w0;
                w2 / (w2 + w02)
            }
            EqBandShape::HighCut => {
                let w2 = w * w;
                let w02 = w0 * w0;
                w02 / (w2 + w02)
            }
            _ => 1.0,
        };
        total_mag_sq *= first_order_mag;
    }

    // Add second-order sections with Butterworth pole distribution
    for i in 0..num_sections {
        // Butterworth Q for each section: Q = 1 / (2 * cos(theta))
        // where theta = pi * (2k + 1) / (2n) for k = 0..n/2-1
        let theta = std::f64::consts::PI * (2 * i + 1) as f64 / (2 * order) as f64;
        let section_q = 1.0 / (2.0 * theta.cos());

        let coeffs = match filter_type {
            EqBandShape::LowCut => highpass_coeffs(w0, section_q),
            EqBandShape::HighCut => lowpass_coeffs(w0, section_q),
            _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], // Unity
        };

        total_mag_sq *= biquad_magnitude_squared(&coeffs, w);
    }

    // Convert to dB
    10.0 * total_mag_sq.max(1e-30).log10()
}

pub fn calculate_band_response(band: &EqBand, freq: f64, _sample_rate: f64) -> f64 {
    let f0 = band.frequency as f64;
    let gain = band.gain as f64;
    let q = band.q as f64;

    // Angular frequencies
    let w = 2.0 * std::f64::consts::PI * freq;
    let w0 = 2.0 * std::f64::consts::PI * f0;

    match band.shape {
        EqBandShape::Bell => {
            // Peaking/parametric EQ filter using proper biquad response
            let gain_linear = 10.0_f64.powf(gain / 20.0);
            let coeffs = peak_coeffs(w0, gain_linear, q);
            let mag_sq = biquad_magnitude_squared(&coeffs, w);
            10.0 * mag_sq.max(1e-30).log10()
        }
        EqBandShape::LowShelf => {
            // Low shelf filter
            let gain_linear = 10.0_f64.powf(gain / 20.0);
            let coeffs = lowshelf_coeffs(w0, gain_linear, q.max(0.5));
            let mag_sq = biquad_magnitude_squared(&coeffs, w);
            10.0 * mag_sq.max(1e-30).log10()
        }
        EqBandShape::HighShelf => {
            // High shelf filter
            let gain_linear = 10.0_f64.powf(gain / 20.0);
            let coeffs = highshelf_coeffs(w0, gain_linear, q.max(0.5));
            let mag_sq = biquad_magnitude_squared(&coeffs, w);
            10.0 * mag_sq.max(1e-30).log10()
        }
        EqBandShape::LowCut => {
            // High-pass filter (cuts low frequencies)
            // Q represents slope: 1 = 6dB/oct (1st order), 2 = 12dB/oct (2nd order), etc.
            let order = (q * 2.0).round().max(1.0) as usize;
            cascaded_magnitude_db(freq, f0, order, &EqBandShape::LowCut)
        }
        EqBandShape::HighCut => {
            // Low-pass filter (cuts high frequencies)
            // Q represents slope: 1 = 6dB/oct (1st order), 2 = 12dB/oct (2nd order), etc.
            let order = (q * 2.0).round().max(1.0) as usize;
            cascaded_magnitude_db(freq, f0, order, &EqBandShape::HighCut)
        }
        EqBandShape::Notch => {
            // Notch filter
            let coeffs = notch_coeffs(w0, q.max(0.5));
            let mag_sq = biquad_magnitude_squared(&coeffs, w);
            10.0 * mag_sq.max(1e-30).log10()
        }
        EqBandShape::BandPass => {
            // Band-pass filter
            let coeffs = bandpass_coeffs(w0, q.max(0.5));
            let mag_sq = biquad_magnitude_squared(&coeffs, w);
            // Normalize so peak is at 0dB, then add gain
            let peak_mag_sq = biquad_magnitude_squared(&coeffs, w0);
            let normalized = mag_sq / peak_mag_sq.max(1e-30);
            gain + 10.0 * normalized.max(1e-30).log10()
        }
        EqBandShape::TiltShelf | EqBandShape::FlatTilt => {
            // Tilt filter - gradual slope across spectrum
            // Approximated as a slope in dB per octave
            let octaves = (freq / f0).log2();
            let slope_db_per_oct = gain / 3.0;
            octaves * slope_db_per_oct
        }
        EqBandShape::AllPass => 0.0, // All-pass doesn't change magnitude
    }
}

#[allow(dead_code)]
fn generate_grid_elements(
    padding: f64,
    graph_width: f64,
    graph_height: f64,
    min_freq: f64,
    max_freq: f64,
    db_range: f64,
) -> Vec<(f64, f64, f64, f64, bool)> {
    let mut lines = Vec::new();
    let log_min = min_freq.log10();
    let log_max = max_freq.log10();

    let freq_to_x = |freq: f64| -> f64 {
        let normalized = (freq.log10() - log_min) / (log_max - log_min);
        padding + normalized * graph_width
    };

    let db_to_y = |db: f64| -> f64 {
        let normalized = 0.5 - db / (2.0 * db_range);
        padding + normalized * graph_height
    };

    // Vertical frequency lines
    let major_freqs = [100.0, 1000.0, 10000.0];
    let minor_freqs = [20.0, 50.0, 200.0, 500.0, 2000.0, 5000.0, 20000.0];

    for freq in major_freqs {
        if freq >= min_freq && freq <= max_freq {
            let x = freq_to_x(freq);
            lines.push((x, padding, x, padding + graph_height, true));
        }
    }

    for freq in minor_freqs {
        if freq >= min_freq && freq <= max_freq {
            let x = freq_to_x(freq);
            lines.push((x, padding, x, padding + graph_height, false));
        }
    }

    // Horizontal dB lines
    let y_zero = db_to_y(0.0);
    lines.push((padding, y_zero, padding + graph_width, y_zero, true));

    let db_step = 6.0;
    let mut db = db_step;
    while db <= db_range {
        let y_pos = db_to_y(db);
        let y_neg = db_to_y(-db);
        lines.push((padding, y_pos, padding + graph_width, y_pos, false));
        lines.push((padding, y_neg, padding + graph_width, y_neg, false));
        db += db_step;
    }

    lines
}

#[allow(dead_code)]
fn generate_freq_labels(
    padding: f64,
    graph_width: f64,
    height: f64,
    min_freq: f64,
    max_freq: f64,
) -> Vec<(f64, f64, String)> {
    let mut labels = Vec::new();
    let log_min = min_freq.log10();
    let log_max = max_freq.log10();
    let y = height - padding + 15.0;

    let freq_to_x = |freq: f64| -> f64 {
        let normalized = (freq.log10() - log_min) / (log_max - log_min);
        padding + normalized * graph_width
    };

    let freq_labels = [
        (20.0, "20"),
        (50.0, "50"),
        (100.0, "100"),
        (200.0, "200"),
        (500.0, "500"),
        (1000.0, "1k"),
        (2000.0, "2k"),
        (5000.0, "5k"),
        (10000.0, "10k"),
        (20000.0, "20k"),
    ];

    for (freq, label) in freq_labels {
        if freq >= min_freq && freq <= max_freq {
            labels.push((freq_to_x(freq), y, label.to_string()));
        }
    }

    labels
}

#[allow(dead_code)]
fn generate_db_labels(padding: f64, graph_height: f64, db_range: f64) -> Vec<(f64, f64, String)> {
    let mut labels = Vec::new();
    let x = padding - 10.0;

    let db_to_y = |db: f64| -> f64 {
        let normalized = 0.5 - db / (2.0 * db_range);
        padding + normalized * graph_height
    };

    labels.push((x, db_to_y(0.0), "0".to_string()));

    let db_step = 6.0;
    let mut db = db_step;
    while db <= db_range {
        labels.push((x, db_to_y(db), format!("+{}", db as i32)));
        labels.push((x, db_to_y(-db), format!("{}", -(db as i32))));
        db += db_step;
    }

    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eq_band_default() {
        let band = EqBand::default();
        assert!(!band.used);
        assert!(!band.enabled);
        assert_eq!(band.gain, 0.0);
    }

    #[test]
    fn test_band_response_bell() {
        let band = EqBand {
            used: true,
            enabled: true,
            frequency: 1000.0,
            gain: 6.0,
            q: 1.0,
            shape: EqBandShape::Bell,
            ..Default::default()
        };

        let response_at_center = calculate_band_response(&band, 1000.0, 48000.0);
        // Bell filter should produce gain at center frequency
        // Allow some tolerance for numerical precision
        assert!(
            (response_at_center - 6.0).abs() < 0.5,
            "Expected ~6.0 dB at center, got {response_at_center}"
        );

        let response_far = calculate_band_response(&band, 100.0, 48000.0);
        // Far from center should be close to 0 dB
        assert!(
            response_far.abs() < 2.0,
            "Expected near 0 dB far from center, got {response_far}"
        );
    }

    #[test]
    fn test_combined_response() {
        let bands = vec![
            EqBand {
                used: true,
                enabled: true,
                frequency: 100.0,
                gain: 3.0,
                q: 1.0,
                shape: EqBandShape::Bell,
                ..Default::default()
            },
            EqBand {
                used: true,
                enabled: true,
                frequency: 10000.0,
                gain: -3.0,
                q: 1.0,
                shape: EqBandShape::Bell,
                ..Default::default()
            },
        ];

        let mid_response = calculate_combined_response(&bands, 1000.0, 48000.0);
        assert!(mid_response.abs() < 1.0);
    }

    #[test]
    fn test_grid_generation() {
        let grid = generate_grid_elements(40.0, 720.0, 220.0, 20.0, 20000.0, 24.0);
        assert!(!grid.is_empty());

        let major_count = grid.iter().filter(|l| l.4).count();
        let minor_count = grid.iter().filter(|l| !l.4).count();
        assert!(major_count > 0);
        assert!(minor_count > 0);
    }

    #[test]
    fn test_freq_labels_generation() {
        let labels = generate_freq_labels(40.0, 720.0, 300.0, 20.0, 20000.0);
        assert!(!labels.is_empty());

        let label_texts: Vec<&str> = labels.iter().map(|l| l.2.as_str()).collect();
        assert!(label_texts.contains(&"100"));
        assert!(label_texts.contains(&"1k"));
        assert!(label_texts.contains(&"10k"));
    }

    #[test]
    fn test_band_response_bell_negative_gain() {
        let band = EqBand {
            used: true,
            enabled: true,
            frequency: 1000.0,
            gain: -6.0, // Negative gain (cut)
            q: 1.0,
            shape: EqBandShape::Bell,
            ..Default::default()
        };

        let response_at_center = calculate_band_response(&band, 1000.0, 48000.0);
        // Bell filter with negative gain should produce cut at center frequency
        assert!(
            (response_at_center - (-6.0)).abs() < 0.5,
            "Expected ~-6.0 dB at center for negative gain, got {response_at_center}"
        );

        let response_far = calculate_band_response(&band, 100.0, 48000.0);
        // Far from center should be close to 0 dB
        assert!(
            response_far.abs() < 2.0,
            "Expected near 0 dB far from center, got {response_far}"
        );
    }

    #[test]
    fn test_band_response_low_shelf() {
        let band = EqBand {
            used: true,
            enabled: true,
            frequency: 100.0,
            gain: 6.0,
            q: 0.7,
            shape: EqBandShape::LowShelf,
            ..Default::default()
        };

        // Low shelf should boost below cutoff frequency
        let response_low = calculate_band_response(&band, 20.0, 48000.0);
        assert!(
            response_low > 3.0,
            "Expected boost below cutoff for low shelf, got {response_low}"
        );

        // Should be close to 0 dB well above cutoff
        let response_high = calculate_band_response(&band, 1000.0, 48000.0);
        assert!(
            response_high.abs() < 2.0,
            "Expected ~0 dB above cutoff for low shelf, got {response_high}"
        );
    }

    #[test]
    fn test_band_response_high_shelf() {
        let band = EqBand {
            used: true,
            enabled: true,
            frequency: 8000.0,
            gain: 6.0,
            q: 0.7,
            shape: EqBandShape::HighShelf,
            ..Default::default()
        };

        // High shelf should boost above cutoff frequency
        let response_high = calculate_band_response(&band, 16000.0, 48000.0);
        assert!(
            response_high > 3.0,
            "Expected boost above cutoff for high shelf, got {response_high}"
        );

        // Should be close to 0 dB well below cutoff
        let response_low = calculate_band_response(&band, 1000.0, 48000.0);
        assert!(
            response_low.abs() < 2.0,
            "Expected ~0 dB below cutoff for high shelf, got {response_low}"
        );
    }

    #[test]
    fn test_db_labels_generation() {
        let labels = generate_db_labels(40.0, 220.0, 24.0);
        assert!(!labels.is_empty());

        let label_texts: Vec<&str> = labels.iter().map(|l| l.2.as_str()).collect();
        assert!(label_texts.contains(&"0"));
        assert!(label_texts.contains(&"+6"));
        assert!(label_texts.contains(&"-6"));
    }
}
