//! Waveform display — renders audio sample data as a centered waveform
//! or a scrolling peak level history.
//!
//! `PeakWaveform` uses a Vello scene overlay for GPU-accelerated rendering,
//! producing zero DOM nodes for the actual bars. The component emits a single
//! placeholder `<div>` for layout and positions the overlay on top of it.

use std::cell::RefCell;
use std::rc::Rc;

use crate::theme::use_theme;
use nice_plug_dioxus::prelude::*;

/// Centered bipolar waveform display.
///
/// Each entry in `samples` renders as one column, centered around the
/// horizontal midline. Values range from -1.0 to 1.0.
#[component]
pub fn WaveformDisplay(
    /// Normalized sample peaks (-1.0 to 1.0). Each entry renders as one column.
    #[props(default)]
    samples: Vec<f64>,
    /// Width in pixels.
    #[props(default = 200)]
    width: u32,
    /// Height in pixels.
    #[props(default = 64)]
    height: u32,
    /// Bar color.
    #[props(default)]
    color: Option<String>,
) -> Element {
    let t = use_theme();
    let t = *t.read();
    let color = color.as_deref().unwrap_or(t.accent);

    let w = width;
    let h = height;
    let hf = h as f64;
    let mid = hf / 2.0;
    let count = samples.len().max(1);
    let bar_w = (w as f64 / count as f64).max(1.0);

    rsx! {
        div {
            style: format!(
                "position:relative; overflow:hidden; border-radius:4px; \
                 background:rgba(34,34,64,0.3); width:{w}px; height:{h}px;"
            ),

            // Center line
            div {
                style: format!(
                    "position:absolute; left:0; top:50%; width:100%; height:1px; \
                     background:rgba(136,136,136,0.2);"
                ),
            }

            // Waveform bars
            for (i, sample) in samples.iter().enumerate() {
                {
                    let amp = sample.abs().clamp(0.0, 1.0);
                    let bar_h = (amp * mid).max(1.0);
                    let bar_top = mid - bar_h;
                    let bar_full_h = bar_h * 2.0;
                    let left = i as f64 * bar_w;
                    rsx! {
                        div {
                            style: format!(
                                "position:absolute; left:{left:.1}px; top:{bar_top:.1}px; \
                                 width:{bar_w:.1}px; height:{bar_full_h:.1}px; \
                                 border-radius:1px; background:{color}; opacity:0.8;"
                            ),
                        }
                    }
                }
            }
        }
    }
}

// ── Reusable VelloCanvas component ──────────────────────────────────────

use nice_plug_dioxus::prelude::vello::kurbo::{Affine, Rect};
use nice_plug_dioxus::prelude::vello::peniko::{Color, Fill};

/// Trait for painting into a Vello scene. Implement this for your custom
/// GPU-rendered content.
pub trait CanvasPainter: 'static {
    /// Paint content into the scene. Coordinates are in CSS pixels,
    /// origin (0,0) at the top-left of the canvas element.
    ///
    /// `transform` maps element-local CSS coords to window physical coords —
    /// pass it to all `scene.fill()` / `scene.stroke()` calls.
    fn paint(
        &self,
        scene: &mut nice_plug_dioxus::prelude::vello::Scene,
        transform: Affine,
        width: f64,
        height: f64,
    );
}

/// Internal overlay adapter: wraps a shared `CanvasPainter` as a `SceneOverlay`.
struct CanvasOverlay {
    painter: Rc<RefCell<dyn CanvasPainter>>,
}

impl SceneOverlay for CanvasOverlay {
    fn paint(
        &mut self,
        scene: &mut nice_plug_dioxus::prelude::vello::Scene,
        transform: Affine,
        width: u32,
        height: u32,
        _scale: f64,
    ) {
        self.painter
            .borrow()
            .paint(scene, transform, width as f64, height as f64);
    }
}

/// Props for [`VelloCanvas`].
#[derive(Props, Clone)]
pub struct VelloCanvasProps {
    /// Shared painter (type-erased).
    painter: Rc<RefCell<dyn CanvasPainter>>,
    /// Width in CSS pixels (ignored when `fill` is true).
    #[props(default = 400.0)]
    width: f32,
    /// Height in CSS pixels (ignored when `fill` is true).
    #[props(default = 200.0)]
    height: f32,
    /// When true, the canvas fills its parent container (flex:1 + 100% size)
    /// and reads its actual dimensions from the bounding rect.
    #[props(default = false)]
    fill: bool,
    /// When true, paint behind the DOM (background layer).
    #[props(default = false)]
    background: bool,
    /// Optional inline style to merge onto the placeholder div.
    #[props(default)]
    style: Option<String>,
}

// Rc<RefCell<dyn CanvasPainter>> has no PartialEq, so always re-render.
impl PartialEq for VelloCanvasProps {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

/// A reusable Dioxus component that renders custom Vello content, automatically
/// positioned to match its DOM element's layout.
///
/// Renders a single placeholder `<div>` for Taffy layout. A Vello scene overlay
/// is registered and repositioned each render cycle to match the element's
/// bounding rect.
#[allow(non_snake_case)]
pub fn VelloCanvas(props: VelloCanvasProps) -> Element {
    let VelloCanvasProps {
        painter,
        width,
        height,
        fill,
        background,
        style,
    } = props;

    let painter_clone = painter.clone();
    let layer = if background {
        OverlayLayer::Background
    } else {
        OverlayLayer::Foreground
    };
    let overlay = use_scene_overlay_on_layer(
        move || CanvasOverlay {
            painter: painter_clone,
        },
        layer,
    );

    // Store MountedData so we can re-query position on mount and resize
    let mut mounted: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    // Window size signal — provided by the window handler, updated on resize.
    // Optional: falls back gracefully if not in context (e.g. screenshot path).
    let window_size: Option<Signal<(u32, u32)>> = try_consume_context();

    // Re-query the element rect when the element mounts OR the window is resized.
    // Reading both signals synchronously registers them as reactive dependencies.
    let w = width;
    let h = height;
    {
        let overlay = overlay.clone();
        use_effect(move || {
            let el = mounted.read().clone();
            // Track window size as a dependency so resize triggers re-query
            if let Some(sig) = window_size {
                let _ = sig.read();
            }
            let overlay = overlay.clone();
            if let Some(el) = el {
                spawn(async move {
                    if let Ok(rect) = el.get_client_rect().await {
                        let origin = rect.origin;
                        let size = rect.size;
                        if fill {
                            overlay.set_rect(origin.x, origin.y, size.width, size.height);
                        } else {
                            overlay.set_rect(origin.x, origin.y, w as f64, h as f64);
                        }
                    }
                });
            }
        });
    }

    let extra_style = style.as_deref().unwrap_or("");

    // In fill mode, stretch to fill the parent container.
    // In fixed mode, set explicit width/height.
    let size_style = if fill {
        "width:100%; height:100%;".to_string()
    } else {
        format!("width:{width}px; height:{height}px;")
    };

    rsx! {
        div {
            style: format!("{size_style} {extra_style}"),
            onmounted: move |event: MountedEvent| {
                mounted.set(Some(event.data()));
            },
        }
    }
}

// ── Vello-rendered PeakWaveform ─────────────────────────────────────────

use nice_plug_dioxus::prelude::vello::kurbo::{BezPath, Circle, Line, Stroke};

/// Compute gain reduction for a given input level using soft-knee compression.
fn compress_transfer(input_db: f32, threshold_db: f32, ratio: f32, knee_db: f32) -> f32 {
    if ratio <= 1.0 {
        return input_db;
    }
    let slope = 1.0 - 1.0 / ratio;
    let half_knee = knee_db * 0.5;
    if knee_db > 0.001 && (input_db - threshold_db).abs() < half_knee {
        let x = input_db - threshold_db + half_knee;
        input_db - slope * x * x / (2.0 * knee_db)
    } else if input_db > threshold_db {
        input_db - slope * (input_db - threshold_db)
    } else {
        input_db
    }
}

/// Painter for the peak waveform visualization with embedded transfer curve.
pub struct PeakWaveformPainter {
    levels: Vec<f32>,
    gr_levels: Vec<f32>,
    /// Fractional scroll phase (0.0–1.0): how far into the current data interval
    /// we are. Used to offset x-positions sub-pixel for smooth scrolling at any
    /// refresh rate — the waveform glides continuously rather than jumping.
    scroll_phase: f32,
    // Transfer curve params (drawn as transparent overlay in left region)
    threshold_db: f32,
    ratio: f32,
    knee_db: f32,
    input_level_db: Option<f32>,
    range_db: f32,
    show_transfer: bool,
}

impl Default for PeakWaveformPainter {
    fn default() -> Self {
        Self::new()
    }
}

impl PeakWaveformPainter {
    pub fn new() -> Self {
        Self {
            levels: Vec::new(),
            gr_levels: Vec::new(),
            scroll_phase: 0.0,
            threshold_db: -18.0,
            ratio: 4.0,
            knee_db: 6.0,
            input_level_db: None,
            range_db: 60.0,
            show_transfer: false,
        }
    }

    pub fn update(&mut self, levels: &[f32], gr_levels: &[f32], scroll_phase: f32) {
        self.scroll_phase = scroll_phase;
        self.levels.clear();
        self.levels.extend_from_slice(levels);
        self.gr_levels.clear();
        self.gr_levels.extend_from_slice(gr_levels);
    }

    pub fn update_transfer(
        &mut self,
        threshold_db: f32,
        ratio: f32,
        knee_db: f32,
        input_level_db: Option<f32>,
    ) {
        self.threshold_db = threshold_db;
        self.ratio = ratio;
        self.knee_db = knee_db;
        self.input_level_db = input_level_db;
        self.show_transfer = true;
    }

    /// Convert a dB value to y position in the waveform display.
    /// 0 dB = top of display, -range_db = bottom.
    fn db_to_waveform_y(&self, db: f64, h: f64) -> f64 {
        let range = self.range_db as f64;
        // 0 dB at top, -range at bottom
        (-db / range) * h
    }

    /// Paint the transfer curve as a transparent overlay in the left portion,
    /// and the threshold line + dB scale across the full width.
    fn paint_transfer_overlay(
        &self,
        scene: &mut nice_plug_dioxus::prelude::vello::Scene,
        transform: Affine,
        w: f64,
        h: f64,
    ) {
        let range = self.range_db as f64;
        let min_db = -self.range_db;
        let scale_margin = 28.0; // right margin for dB scale labels

        // ── Threshold line — dotted, spans the FULL width ──────────────
        let thresh_y = self.db_to_waveform_y(self.threshold_db as f64, h);
        let thresh_color = Color::from_rgba8(255, 100, 100, 80);
        let dash_gap = 6.0;
        let mut x = 0.0;
        while x < w {
            let x_end = (x + dash_gap * 0.6).min(w);
            scene.stroke(
                &Stroke::new(1.0),
                transform,
                thresh_color,
                None,
                &Line::new((x, thresh_y), (x_end, thresh_y)),
            );
            x += dash_gap;
        }

        // ── dB scale ticks + horizontal guide lines on the right ──────
        let tick_color = Color::from_rgba8(255, 255, 255, 30);
        let guide_color = Color::from_rgba8(255, 255, 255, 10);
        let tick_x_start = w - scale_margin;
        let tick_x_end = w - scale_margin + 4.0;
        for db_val in [0, -6, -12, -18, -24, -30, -36] {
            let db = db_val as f64;
            if db < min_db as f64 {
                continue;
            }
            let y = self.db_to_waveform_y(db, h);
            // Horizontal guide line
            scene.stroke(
                &Stroke::new(0.5),
                transform,
                guide_color,
                None,
                &Line::new((0.0, y), (w - scale_margin, y)),
            );
            // Tick mark
            scene.stroke(
                &Stroke::new(1.0),
                transform,
                tick_color,
                None,
                &Line::new((tick_x_start, y), (tick_x_end, y)),
            );
        }

        // ── Transfer curve (left ~35% of display) ─────────────────────
        let curve_w = (w - scale_margin) * 0.35;

        // Subtle background tint for the transfer curve panel — neutral dark
        scene.fill(
            Fill::NonZero,
            transform,
            Color::from_rgba8(255, 255, 255, 10),
            None,
            &Rect::new(0.0, 0.0, curve_w, h),
        );

        let tc_db_to_x = |db: f64| -> f64 { ((db - min_db as f64) / range) * curve_w };
        let tc_db_to_y = |db: f64| -> f64 { h - ((db - min_db as f64) / range) * h };

        // Subtle grid lines within transfer curve area
        let grid_color = Color::from_rgba8(255, 255, 255, 8);
        for db_val in [-48, -36, -24, -12] {
            let x = tc_db_to_x(db_val as f64);
            let y = tc_db_to_y(db_val as f64);
            scene.stroke(
                &Stroke::new(0.5),
                transform,
                grid_color,
                None,
                &Line::new((x, 0.0), (x, h)),
            );
            scene.stroke(
                &Stroke::new(0.5),
                transform,
                grid_color,
                None,
                &Line::new((0.0, y), (curve_w, y)),
            );
        }

        // 1:1 reference line (dashed diagonal)
        let ref_color = Color::from_rgba8(255, 255, 255, 18);
        let num_dashes = 30;
        for i in 0..num_dashes {
            let frac0 = i as f64 / num_dashes as f64;
            let frac1 = (i as f64 + 0.5) / num_dashes as f64;
            let db0 = min_db as f64 + frac0 * range;
            let db1 = min_db as f64 + frac1 * range;
            scene.stroke(
                &Stroke::new(1.0),
                transform,
                ref_color,
                None,
                &Line::new(
                    (tc_db_to_x(db0), tc_db_to_y(db0)),
                    (tc_db_to_x(db1), tc_db_to_y(db1)),
                ),
            );
        }

        // Transfer curve path — yellow-green like Pro-C 3
        let curve_color = Color::from_rgba8(180, 210, 140, 180);
        let num_points = 60;
        let mut path = BezPath::new();
        for i in 0..=num_points {
            let input = min_db as f64 + (i as f64 / num_points as f64) * range;
            let output =
                compress_transfer(input as f32, self.threshold_db, self.ratio, self.knee_db) as f64;
            let x = tc_db_to_x(input);
            let y = tc_db_to_y(output);
            if i == 0 {
                path.move_to((x, y));
            } else {
                path.line_to((x, y));
            }
        }
        scene.stroke(&Stroke::new(2.0), transform, curve_color, None, &path);

        // Input level indicator (ball on curve)
        if let Some(level) = self.input_level_db {
            let out = compress_transfer(level, self.threshold_db, self.ratio, self.knee_db) as f64;
            let bx = tc_db_to_x(level as f64);
            let by = tc_db_to_y(out);
            if bx >= 0.0 && bx <= curve_w {
                scene.fill(
                    Fill::NonZero,
                    transform,
                    Color::from_rgba8(255, 255, 255, 200),
                    None,
                    &Circle::new((bx, by), 3.0),
                );
            }
        }
    }
}

impl PeakWaveformPainter {
    /// Build a smooth filled BezPath from sample data.
    /// Uses Catmull-Rom → cubic Bézier conversion for smooth interpolation.
    ///
    /// `x_offset`: shift all x-positions left by this amount (for phase scrolling).
    /// The caller should clip the resulting path to [0, w] to hide the overshoot.
    fn build_smooth_path(
        samples: &[f32],
        w: f64,
        h: f64,
        from_bottom: bool,
        x_offset: f64,
    ) -> BezPath {
        let n = samples.len();
        if n == 0 {
            return BezPath::new();
        }

        // Step is one sample wider than the display so the scroll has room to slide
        let step = w / n as f64;

        // Convert samples to y-coordinates
        let ys: Vec<f64> = samples
            .iter()
            .map(|&s| {
                let amp = s.clamp(0.0, 1.0) as f64;
                if from_bottom {
                    h - amp * h
                } else {
                    amp * h
                }
            })
            .collect();

        let baseline = if from_bottom { h } else { 0.0 };
        let mut path = BezPath::new();
        path.move_to((-x_offset, baseline));
        path.line_to((-x_offset, ys[0]));

        for i in 0..n - 1 {
            let x0 = i as f64 * step - x_offset;
            let x1 = (i + 1) as f64 * step - x_offset;

            let y_prev = if i > 0 { ys[i - 1] } else { ys[0] };
            let y_curr = ys[i];
            let y_next = ys[i + 1];
            let y_next2 = if i + 2 < n { ys[i + 2] } else { ys[n - 1] };

            let t1_y = (y_next - y_prev) / 2.0;
            let t2_y = (y_next2 - y_curr) / 2.0;

            path.curve_to(
                (x0 + step / 3.0, y_curr + t1_y / 3.0),
                (x1 - step / 3.0, y_next - t2_y / 3.0),
                (x1, y_next),
            );
        }

        path.line_to((n as f64 * step - x_offset, baseline));
        path.close_path();
        path
    }

    /// Build just the top edge as a BezPath (no fill close), with x_offset.
    fn build_edge_path(
        samples: &[f32],
        w: f64,
        h: f64,
        from_bottom: bool,
        x_offset: f64,
    ) -> BezPath {
        let n = samples.len();
        if n == 0 {
            return BezPath::new();
        }
        let step = w / n as f64;
        let ys: Vec<f64> = samples
            .iter()
            .map(|&s| {
                let amp = s.clamp(0.0, 1.0) as f64;
                if from_bottom {
                    h - amp * h
                } else {
                    amp * h
                }
            })
            .collect();

        let mut path = BezPath::new();
        path.move_to((-x_offset, ys[0]));
        for i in 0..n - 1 {
            let x0 = i as f64 * step - x_offset;
            let x1 = (i + 1) as f64 * step - x_offset;
            let y_prev = if i > 0 { ys[i - 1] } else { ys[0] };
            let y_curr = ys[i];
            let y_next = ys[i + 1];
            let y_next2 = if i + 2 < n { ys[i + 2] } else { ys[n - 1] };
            let t1_y = (y_next - y_prev) / 2.0;
            let t2_y = (y_next2 - y_curr) / 2.0;
            path.curve_to(
                (x0 + step / 3.0, y_curr + t1_y / 3.0),
                (x1 - step / 3.0, y_next - t2_y / 3.0),
                (x1, y_next),
            );
        }
        path
    }
}

impl CanvasPainter for PeakWaveformPainter {
    fn paint(
        &self,
        scene: &mut nice_plug_dioxus::prelude::vello::Scene,
        transform: Affine,
        w: f64,
        h: f64,
    ) {
        use nice_plug_dioxus::prelude::vello::peniko::Gradient;

        // Background — pure near-black, neutral (no color cast)
        scene.fill(
            Fill::NonZero,
            transform,
            Color::from_rgba8(8, 8, 8, 255),
            None,
            &Rect::new(0.0, 0.0, w, h),
        );

        // Subtle vertical vignette: slightly brighter mid band, darker edges
        let vignette = Gradient::new_linear((0.0, 0.0), (0.0, h)).with_stops([
            (0.0f32, Color::from_rgba8(0, 0, 0, 40)),
            (0.25f32, Color::from_rgba8(0, 0, 0, 0)),
            (0.75f32, Color::from_rgba8(0, 0, 0, 0)),
            (1.0f32, Color::from_rgba8(0, 0, 0, 50)),
        ]);
        scene.fill(
            Fill::NonZero,
            transform,
            &vignette,
            None,
            &Rect::new(0.0, 0.0, w, h),
        );

        // Phase offset: shift waveform left by `phase * step` pixels so it
        // glides continuously rather than jumping one step at a time.
        let x_offset = if !self.levels.is_empty() {
            self.scroll_phase as f64 * (w / self.levels.len() as f64)
        } else {
            0.0
        };

        // Clip to [0, w] so the phase-shifted overshoot doesn't bleed out.
        // Vello 0.8 added a `clip_style` arg in front of the existing pair.
        scene.push_clip_layer(Fill::NonZero, transform, &Rect::new(0.0, 0.0, w, h));

        // ── Input level — teal/cyan gradient fill ──
        if !self.levels.is_empty() {
            let path = Self::build_smooth_path(&self.levels, w, h, true, x_offset);
            // Dark teal fill: readable but not overwhelming
            let level_gradient = Gradient::new_linear((0.0, 0.0), (0.0, h)).with_stops([
                (0.0f32, Color::from_rgba8(20, 110, 115, 210)),
                (0.45f32, Color::from_rgba8(14, 80, 85, 130)),
                (0.80f32, Color::from_rgba8(8, 50, 55, 55)),
                (1.0f32, Color::from_rgba8(4, 25, 28, 15)),
            ]);
            scene.fill(Fill::NonZero, transform, &level_gradient, None, &path);

            // Bright cyan edge with soft glow halo
            let edge = Self::build_edge_path(&self.levels, w, h, true, x_offset);
            scene.stroke(
                &Stroke::new(4.0),
                transform,
                Color::from_rgba8(0, 200, 210, 30),
                None,
                &edge,
            );
            scene.stroke(
                &Stroke::new(1.5),
                transform,
                Color::from_rgba8(60, 210, 220, 200),
                None,
                &edge,
            );
        }

        // ── Gain reduction — red fill from top (Pro-C 3 style) ──
        if !self.gr_levels.is_empty() {
            let path = Self::build_smooth_path(&self.gr_levels, w, h, false, x_offset);
            let gr_gradient = Gradient::new_linear((0.0, 0.0), (0.0, h)).with_stops([
                (0.0f32, Color::from_rgba8(220, 40, 40, 210)),
                (0.35f32, Color::from_rgba8(200, 30, 30, 130)),
                (0.70f32, Color::from_rgba8(175, 25, 25, 55)),
                (1.0f32, Color::from_rgba8(150, 20, 20, 15)),
            ]);
            scene.fill(Fill::NonZero, transform, &gr_gradient, None, &path);

            // Bright red edge with soft glow
            let edge = Self::build_edge_path(&self.gr_levels, w, h, false, x_offset);
            scene.stroke(
                &Stroke::new(4.0),
                transform,
                Color::from_rgba8(255, 60, 60, 30),
                None,
                &edge,
            );
            scene.stroke(
                &Stroke::new(1.5),
                transform,
                Color::from_rgba8(255, 80, 80, 210),
                None,
                &edge,
            );
        }

        scene.pop_layer();

        // Transfer curve + threshold line + dB scale overlay
        if self.show_transfer {
            self.paint_transfer_overlay(scene, transform, w, h);
        }
    }
}

/// Scrolling peak level waveform with optional gain reduction overlay.
///
/// GPU-rendered via Vello scene overlay — produces only 1 DOM node (the
/// placeholder div) instead of hundreds of positioned divs.
///
/// Used by dynamics plugins (compressor, gate, rider) to show
/// amplitude history over time with GR envelope overlay.
#[component]
pub fn PeakWaveform(
    /// Peak levels (0.0–1.0, newest at end).
    levels: Vec<f32>,
    /// Gain reduction levels (0.0–1.0, same length as `levels`).
    #[props(default = Vec::new())]
    gr_levels: Vec<f32>,
    /// Threshold in dBFS (enables transfer curve overlay when set with `ratio`).
    #[props(default)]
    threshold_db: Option<f32>,
    /// Compression ratio (enables transfer curve overlay when set with `threshold_db`).
    #[props(default)]
    ratio: Option<f32>,
    /// Soft knee width in dB (for transfer curve overlay).
    #[props(default = 6.0)]
    knee_db: f32,
    /// Current input level in dB (for transfer curve ball indicator).
    #[props(default)]
    input_level_db: Option<f32>,
    /// Width in CSS pixels (ignored when `fill` is true).
    #[props(default = 400.0)]
    width: f32,
    /// Height in CSS pixels (ignored when `fill` is true).
    #[props(default = 80.0)]
    height: f32,
    /// Fill parent container instead of using fixed dimensions.
    #[props(default = false)]
    fill: bool,
    /// Optional inline style to merge onto the outer element.
    #[props(default)]
    style: Option<String>,
    /// Scroll phase (0.0–1.0) for sub-pixel smooth scrolling at high refresh rates.
    #[props(default = 0.0)]
    scroll_phase: f32,
) -> Element {
    let t = use_theme();
    let t = *t.read();

    // Create shared painter (persists across renders)
    let painter: &Rc<RefCell<PeakWaveformPainter>> =
        &use_hook(|| Rc::new(RefCell::new(PeakWaveformPainter::new())));

    // Update data each render
    {
        let mut p = painter.borrow_mut();
        p.update(&levels, &gr_levels, scroll_phase);
        if let (Some(thresh), Some(rat)) = (threshold_db, ratio) {
            p.update_transfer(thresh, rat, knee_db, input_level_db);
        }
    }

    // Type-erase to dyn CanvasPainter for VelloCanvas
    let dyn_painter: Rc<RefCell<dyn CanvasPainter>> = painter.clone();

    let outer_style = style.as_deref().unwrap_or("");
    let show_scale = threshold_db.is_some();
    let range_db_val = 36.0_f32; // matches painter's range_db

    let fill_style = if fill { "width:100%; height:100%;" } else { "" };

    rsx! {
        div {
            style: format!(
                "position:relative; border-radius:4px; overflow:hidden; \
                 border:1px solid {border}; {fill_style} {outer_style}",
                border = t.border,
            ),

            VelloCanvas {
                painter: dyn_painter,
                width: width,
                height: height,
                fill: fill,
                background: true,
            }

            // dB scale labels on the right side (positioned over the canvas)
            if show_scale {
                for db_val in [0, -6, -12, -18, -24, -30, -36] {
                    {
                        let db = db_val as f32;
                        let pct = (-db / range_db_val) * 100.0;
                        let label = if db_val == 0 {
                            "0".to_string()
                        } else {
                            format!("{db_val}")
                        };
                        rsx! {
                            span {
                                style: format!(
                                    "position:absolute; right:4px; top:{pct:.1}%; \
                                     transform:translateY(-50%); \
                                     font-family:{font}; font-size:8px; \
                                     color:{dim}; pointer-events:none;",
                                    font = t.font_mono,
                                    dim = t.text_dim,
                                ),
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}
