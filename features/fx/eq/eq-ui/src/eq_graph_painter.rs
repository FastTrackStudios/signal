//! GPU-accelerated EQ graph painter using vello scene overlay.
//!
//! Renders the EQ graph visual elements (grid, curves, spectrum, nodes)
//! directly into the vello scene each frame, giving proper anti-aliasing,
//! smooth curves, and glow effects.

use std::sync::Arc;

use nice_plug_dioxus::prelude::vello::kurbo::{Affine, BezPath, Circle, Line, Rect, Stroke};
use nice_plug_dioxus::prelude::vello::peniko::{Color, Fill};
// Paint the EQ graph as a blitz native custom widget: `paint()` records into an
// anyrender `Scene` that blitz composites into its own paint pass at the node's
// box. `PaintScene` brings the `fill`/`stroke` methods into scope.
use nice_plug_dioxus::widget::{
    ComputedStyles, PaintScene as _, RenderContext, Scene, UiEvent, Widget,
};

use super::eq_graph_model::{EqBand, EqGraphRenderState, GraphConfig, freq_to_color};
use super::eq_graph_response::{calculate_band_response, calculate_combined_response};
use spectrum_analyzer::ui::{paint_collisions, paint_spectrum_fill, paint_spectrum_line};

// ── Color helpers ───────────────────────────────────────────────────

fn hex_to_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(128);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(128);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(128);
    Color::from_rgb8(r, g, b)
}

fn hex_to_color_alpha(hex: &str, alpha: f32) -> Color {
    let c = hex_to_color(hex);
    c.with_alpha(alpha)
}

// ── Coordinate helpers ──────────────────────────────────────────────

struct CoordMapper {
    freq_axis: fts_audio_ui::axis::FreqAxis,
    db_axis: fts_audio_ui::axis::DbAxis,
    padding: f64,
    graph_w: f64,
    graph_h: f64,
}

impl CoordMapper {
    fn new(cfg: &GraphConfig, padding: f64) -> Self {
        Self {
            freq_axis: fts_audio_ui::axis::FreqAxis::new(cfg.min_freq, cfg.max_freq),
            db_axis: fts_audio_ui::axis::DbAxis::symmetric(cfg.db_range),
            padding,
            graph_w: cfg.rect_w - padding * 2.0,
            graph_h: cfg.rect_h - padding * 2.0,
        }
    }

    fn freq_to_x(&self, freq: f64) -> f64 {
        self.freq_axis
            .freq_to_x(freq, self.padding, self.padding + self.graph_w)
    }

    fn db_to_y(&self, db: f64) -> f64 {
        self.db_axis
            .db_to_y(db, self.padding, self.padding + self.graph_h)
    }
}

// ── Painter ─────────────────────────────────────────────────────────

/// Blitz custom widget that paints the EQ graph directly into blitz's scene.
///
/// Holds an `Arc` to the shared [`EqGraphRenderState`] the `EqGraph` component
/// writes to, so it always paints the latest bands/curve. Attach it to an
/// `object` element via `CustomWidgetAttr::new(EqGraphWidget::new(state))`.
pub struct EqGraphWidget {
    state: Arc<EqGraphRenderState>,
}

impl EqGraphWidget {
    pub fn new(state: Arc<EqGraphRenderState>) -> Self {
        Self { state }
    }
}

impl Widget for EqGraphWidget {
    fn paint(
        &mut self,
        _render_ctx: &mut dyn RenderContext,
        _styles: &ComputedStyles,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Scene {
        let mut scene = Scene::new();
        // Publish the live canvas size + DPR so the component's hit-testing stays
        // in sync (it derives CSS px as `rect_w / scale`). blitz hands us the
        // node's box in physical pixels, so we draw 1:1 with an identity transform.
        {
            let mut cfg = self.state.config.write();
            cfg.rect_w = width as f64;
            cfg.rect_h = height as f64;
            cfg.scale = scale.max(1.0);
        }
        paint_eq_graph_scene(&mut scene, &self.state, Affine::IDENTITY, width, height);
        scene
    }

    fn handle_event(&mut self, _event: &UiEvent) {
        // Paint-only widget: pointer interaction is handled by the DOM container's
        // event handlers (now that blitz's `element_coordinates()` is fixed), so
        // the `<object>` keeps `pointer-events: none` and this is never called.
    }
}

/// Paint the EQ graph into the given anyrender scene.
///
/// Called by [`EqGraphWidget::paint`] (the blitz custom widget) each frame.
/// `transform` is normally `Affine::IDENTITY` since blitz hands the widget its
/// node box in physical pixels.
pub fn paint_eq_graph_scene(
    scene: &mut Scene,
    state: &EqGraphRenderState,
    transform: Affine,
    width: u32,
    height: u32,
) {
    let elem_w = width as f64;
    let elem_h = height as f64;
    if elem_w < 1.0 || elem_h < 1.0 {
        return;
    }

    let cfg = state.config.read().clone();
    let bands = state.bands.read().clone();
    let spectrum = state.spectrum_db.read().clone();
    let analyzer = state.analyzer.read().clone();
    let model_response = state.model_response_db.read().clone();
    let interaction = state.interaction.read().clone();
    let overlay = *state.overlay.read();

    let cfg = GraphConfig {
        rect_w: elem_w,
        rect_h: elem_h,
        ..cfg
    };

    let padding = 0.0;
    let cm = CoordMapper::new(&cfg, padding);
    let area = Rect::new(0.0, 0.0, elem_w, elem_h);

    let bg = Color::from_rgb8(10, 10, 10);
    scene.fill(Fill::NonZero, transform, bg, None, &area);

    if cfg.show_grid {
        paint_grid(scene, &cm, &cfg, transform);
    }

    // EQ cheat-sheet zone overlay, behind the spectrum + curves.
    if let Some(profile) = overlay {
        paint_cheatsheet_overlay(scene, &cm, &cfg, profile, transform);
    }

    // Prefer the full analyzer snapshot (pre/post/external/collision); fall back
    // to the legacy single `spectrum_db` curve when the analyzer has no data.
    let has_analyzer = analyzer.freq_hz.len() >= 2
        && (analyzer.pre_db.len() == analyzer.freq_hz.len()
            || analyzer.post_db.len() == analyzer.freq_hz.len());
    if has_analyzer {
        paint_analyzer(scene, &cm, &cfg, &analyzer, transform, elem_h);
    } else if spectrum.len() >= 2 {
        paint_spectrum(scene, &cm, &cfg, &spectrum, transform);
    }

    let num_points = 400;
    let frequencies = generate_frequencies(&cfg, num_points);

    for band in &bands {
        if !band.used || !band.enabled {
            continue;
        }
        paint_band_curve(scene, &cm, &cfg, band, &frequencies, transform);
    }

    paint_connecting_lines(scene, &cm, &cfg, &bands, transform);
    paint_combined_curve(scene, &cm, &cfg, &bands, &frequencies, transform);
    if model_response.len() >= 2 {
        paint_model_response_curve(scene, &cm, &cfg, &model_response, transform);
    }

    for band in &bands {
        if !band.used {
            continue;
        }
        let is_hovered = interaction.hovered_band == Some(band.index);
        let is_dragging = interaction.dragging_band == Some(band.index);
        let is_focused = interaction.focused_band == Some(band.index);
        paint_band_node(
            scene,
            &cm,
            band,
            is_hovered,
            is_dragging,
            is_focused,
            transform,
        );
    }
}

// ── Painting functions ──────────────────────────────────────────────

/// Shade the EQ cheat-sheet zones for the active profile: a translucent band per
/// zone (tinted red=cut / green=boost / blue=sweet / amber=sweep), a bright top
/// accent bar, faint edges, and a white highpass marker. Labels are drawn in the
/// DOM by the component (positioned via `freq_to_x`).
fn paint_cheatsheet_overlay(
    scene: &mut Scene,
    cm: &CoordMapper,
    cfg: &GraphConfig,
    profile: &crate::cheatsheet::InstrumentProfile,
    transform: Affine,
) {
    let w = cfg.rect_w;
    let h = cfg.rect_h;
    for zone in profile.zones {
        let x0 = cm.freq_to_x(zone.lo_hz as f64).clamp(0.0, w);
        let x1 = cm.freq_to_x(zone.hi_hz as f64).clamp(0.0, w);
        if x1 - x0 < 0.5 {
            continue;
        }
        let (r, g, b, _) = zone.dir.rgba();
        // Translucent fill (zones overlap; alpha accumulates where they stack).
        scene.fill(
            Fill::NonZero,
            transform,
            Color::from_rgba8(r, g, b, 24),
            None,
            &Rect::new(x0, 0.0, x1, h),
        );
        // Top accent bar.
        scene.fill(
            Fill::NonZero,
            transform,
            Color::from_rgba8(r, g, b, 200),
            None,
            &Rect::new(x0, 0.0, x1, 3.0),
        );
        // Faint edges.
        let edge = Color::from_rgba8(r, g, b, 90);
        scene.stroke(
            &Stroke::new(1.0),
            transform,
            edge,
            None,
            &Line::new((x0, 0.0), (x0, h)),
        );
        scene.stroke(
            &Stroke::new(1.0),
            transform,
            edge,
            None,
            &Line::new((x1, 0.0), (x1, h)),
        );
    }
    // Suggested highpass marker.
    if let Some(hp) = profile.highpass_hz {
        let x = cm.freq_to_x(hp as f64).clamp(0.0, w);
        scene.stroke(
            &Stroke::new(1.5),
            transform,
            Color::from_rgba8(255, 255, 255, 120),
            None,
            &Line::new((x, 0.0), (x, h)),
        );
    }
}

fn generate_frequencies(cfg: &GraphConfig, num_points: usize) -> Vec<f64> {
    let log_min = cfg.min_freq.log10();
    let log_max = cfg.max_freq.log10();
    (0..num_points)
        .map(|i| {
            let t = i as f64 / (num_points - 1) as f64;
            10.0_f64.powf(log_min + t * (log_max - log_min))
        })
        .collect()
}

fn paint_grid(scene: &mut Scene, cm: &CoordMapper, cfg: &GraphConfig, transform: Affine) {
    let grid_minor = Color::from_rgba8(55, 55, 60, 90);
    let grid_mid = Color::from_rgba8(70, 70, 75, 110);
    let grid_major = Color::from_rgba8(90, 90, 95, 140);
    let thin = Stroke::new(0.5);
    let mid = Stroke::new(0.75);
    let thick = Stroke::new(1.25);

    // Full logarithmic frequency grid.
    // tier 2 = decade boundary (100, 1k, 10k) — thickest
    // tier 1 = mid-decade anchor (20, 50, 200, 500, …) — medium
    // tier 0 = subdivision — thin
    #[rustfmt::skip]
    let freq_lines: &[(f64, u8)] = &[
        (15.0,    0), (20.0,    1),
        (30.0,    0), (40.0,    0), (50.0,    1),
        (60.0,    0), (70.0,    0), (80.0,    0), (90.0,    0),
        (100.0,   2),
        (200.0,   1), (300.0,   0), (400.0,   0), (500.0,   1),
        (600.0,   0), (700.0,   0), (800.0,   0), (900.0,   0),
        (1000.0,  2),
        (2000.0,  1), (3000.0,  0), (4000.0,  0), (5000.0,  1),
        (6000.0,  0), (7000.0,  0), (8000.0,  0), (9000.0,  0),
        (10000.0, 2),
        (20000.0, 1),
    ];

    let top_y = cm.padding;
    let bot_y = cm.padding + cm.graph_h;

    for &(freq, tier) in freq_lines {
        if freq < cfg.min_freq || freq > cfg.max_freq {
            continue;
        }
        let x = cm.freq_to_x(freq);
        let line = Line::new((x, top_y), (x, bot_y));
        match tier {
            2 => scene.stroke(&thick, transform, grid_major, None, &line),
            1 => scene.stroke(&mid, transform, grid_mid, None, &line),
            _ => scene.stroke(&thin, transform, grid_minor, None, &line),
        }
    }

    // dB grid lines
    let db_step = if cfg.db_range <= 6.0 {
        2.0
    } else if cfg.db_range <= 12.0 {
        3.0
    } else {
        6.0
    };

    let mut db = -cfg.db_range;
    while db <= cfg.db_range {
        let y = cm.db_to_y(db);
        let line = Line::new((cm.padding, y), (cm.padding + cm.graph_w, y));
        let is_zero = db.abs() < 0.01;
        if is_zero {
            scene.stroke(&thick, transform, grid_major, None, &line);
        } else {
            scene.stroke(&thin, transform, grid_minor, None, &line);
        }
        db += db_step;
    }
}

/// Paint the full analyzer snapshot: collisions (behind), pre-EQ, post-EQ and
/// external spectra. Spectrum dB maps with 0 dB at the top down to `-range_db`
/// at the bottom of the graph area.
fn paint_analyzer(
    scene: &mut Scene,
    cm: &CoordMapper,
    cfg: &GraphConfig,
    snap: &spectrum_analyzer::dsp::AnalyzerSnapshot,
    transform: Affine,
    elem_h: f64,
) {
    let min_freq = cfg.min_freq;
    let max_freq = cfg.max_freq;
    let range = if snap.range_db > 0.0 {
        snap.range_db as f64
    } else {
        90.0
    };
    let ceiling = 0.0f64; // dB at the top of the graph
    let freq_to_x = |f: f64| cm.freq_to_x(f);
    let db_to_y = |db: f64| ((ceiling - db) / range).clamp(0.0, 1.0) * elem_h;

    // Collisions first, behind the curves.
    if snap.collision.len() == snap.freq_hz.len() && !snap.collision.is_empty() {
        paint_collisions(
            scene,
            &snap.freq_hz,
            &snap.collision,
            min_freq,
            max_freq,
            0.0,
            elem_h,
            freq_to_x,
            transform,
        );
    }

    // Pre-EQ spectrum (dim, behind post).
    if snap.pre_db.len() == snap.freq_hz.len() && !snap.pre_db.is_empty() {
        paint_spectrum_fill(
            scene,
            &snap.freq_hz,
            &snap.pre_db,
            min_freq,
            max_freq,
            elem_h,
            Color::from_rgba8(140, 140, 150, 70),
            Color::from_rgba8(140, 140, 150, 16),
            freq_to_x,
            db_to_y,
            transform,
        );
    }

    // Post-EQ spectrum (bright blue).
    if snap.post_db.len() == snap.freq_hz.len() && !snap.post_db.is_empty() {
        paint_spectrum_fill(
            scene,
            &snap.freq_hz,
            &snap.post_db,
            min_freq,
            max_freq,
            elem_h,
            Color::from_rgba8(100, 180, 255, 120),
            Color::from_rgba8(100, 180, 255, 28),
            freq_to_x,
            db_to_y,
            transform,
        );
    }

    // External / sidechain spectrum (orange outline only).
    if !snap.ext_db.is_empty() && snap.ext_db.len() == snap.ext_freq_hz.len() {
        paint_spectrum_line(
            scene,
            &snap.ext_freq_hz,
            &snap.ext_db,
            min_freq,
            max_freq,
            Color::from_rgba8(255, 170, 80, 150),
            1.0,
            freq_to_x,
            db_to_y,
            transform,
        );
    }
}

fn paint_spectrum(
    scene: &mut Scene,
    cm: &CoordMapper,
    cfg: &GraphConfig,
    spectrum: &[f32],
    transform: Affine,
) {
    let num_bins = spectrum.len();
    let log_min = cfg.min_freq.log10();
    let log_max = cfg.max_freq.log10();

    let mut path = BezPath::new();
    for (i, &db_val) in spectrum.iter().enumerate() {
        let t = i as f64 / (num_bins - 1) as f64;
        let freq = 10.0_f64.powf(log_min + t * (log_max - log_min));
        let x = cm.freq_to_x(freq);
        let clamped = (db_val as f64).clamp(-cfg.db_range, cfg.db_range);
        let y = cm.db_to_y(clamped);
        if i == 0 {
            path.move_to((x, y));
        } else {
            path.line_to((x, y));
        }
    }

    // Stroke
    let stroke_color = Color::from_rgba8(100, 180, 255, 90);
    scene.stroke(&Stroke::new(1.0), transform, stroke_color, None, &path);

    // Fill down to bottom
    let mut fill_path = path.clone();
    let last_freq = 10.0_f64.powf(log_max);
    let first_freq = 10.0_f64.powf(log_min);
    let bottom_y = cm.db_to_y(-cfg.db_range);
    fill_path.line_to((cm.freq_to_x(last_freq), bottom_y));
    fill_path.line_to((cm.freq_to_x(first_freq), bottom_y));
    fill_path.close_path();

    let fill_color = Color::from_rgba8(100, 180, 255, 20);
    scene.fill(Fill::NonZero, transform, fill_color, None, &fill_path);
}

fn paint_band_curve(
    scene: &mut Scene,
    cm: &CoordMapper,
    cfg: &GraphConfig,
    band: &EqBand,
    frequencies: &[f64],
    transform: Affine,
) {
    let band_hex = freq_to_color(band.frequency as f64);
    let band_color = hex_to_color(&band_hex);
    let fill_color = hex_to_color_alpha(&band_hex, 0.25);
    let zero_y = cm.db_to_y(0.0);

    let mut stroke_path = BezPath::new();
    let mut fill_path = BezPath::new();

    // Start fill at zero line
    fill_path.move_to((cm.freq_to_x(frequencies[0]), zero_y));

    for (i, &freq) in frequencies.iter().enumerate() {
        let db = calculate_band_response(band, freq, cfg.sample_rate);
        let x = cm.freq_to_x(freq);
        let y = cm.db_to_y(db);

        if i == 0 {
            stroke_path.move_to((x, y));
        } else {
            stroke_path.line_to((x, y));
        }
        fill_path.line_to((x, y));
    }

    // Close fill back to zero
    fill_path.line_to((cm.freq_to_x(*frequencies.last().unwrap()), zero_y));
    fill_path.close_path();

    // Fill
    if cfg.fill_curve {
        scene.fill(Fill::NonZero, transform, fill_color, None, &fill_path);
    }

    // Stroke
    scene.stroke(
        &Stroke::new(1.5),
        transform,
        band_color.with_alpha(0.6),
        None,
        &stroke_path,
    );
}

fn paint_connecting_lines(
    scene: &mut Scene,
    cm: &CoordMapper,
    cfg: &GraphConfig,
    bands: &[EqBand],
    transform: Affine,
) {
    let zero_y = cm.db_to_y(0.0);
    let node_r = 7.0;

    for band in bands {
        if !band.used || !band.enabled {
            continue;
        }
        let db = calculate_band_response(band, band.frequency as f64, cfg.sample_rate);
        if db.abs() <= 0.1 {
            continue;
        }
        let bx = cm.freq_to_x(band.frequency as f64);
        let node_y = cm.db_to_y(band.gain as f64);
        let start_y = if node_y < zero_y {
            node_y + node_r
        } else {
            node_y - node_r
        };

        let line = Line::new((bx, start_y), (bx, zero_y));
        let color = hex_to_color_alpha(&freq_to_color(band.frequency as f64), 0.5);
        scene.stroke(&Stroke::new(1.5), transform, color, None, &line);
    }
}

fn paint_combined_curve(
    scene: &mut Scene,
    cm: &CoordMapper,
    cfg: &GraphConfig,
    bands: &[EqBand],
    frequencies: &[f64],
    transform: Affine,
) {
    let golden = Color::from_rgb8(212, 169, 50);
    let fill_color = Color::from_rgba8(212, 169, 50, 20);
    let zero_y = cm.db_to_y(0.0);

    let mut stroke_path = BezPath::new();
    let mut fill_path = BezPath::new();
    fill_path.move_to((cm.freq_to_x(frequencies[0]), zero_y));

    for (i, &freq) in frequencies.iter().enumerate() {
        let db = calculate_combined_response(bands, freq, cfg.sample_rate);
        let x = cm.freq_to_x(freq);
        let y = cm.db_to_y(db);
        if i == 0 {
            stroke_path.move_to((x, y));
        } else {
            stroke_path.line_to((x, y));
        }
        fill_path.line_to((x, y));
    }

    fill_path.line_to((cm.freq_to_x(*frequencies.last().unwrap()), zero_y));
    fill_path.close_path();

    if cfg.fill_curve {
        scene.fill(Fill::NonZero, transform, fill_color, None, &fill_path);
    }

    scene.stroke(&Stroke::new(2.0), transform, golden, None, &stroke_path);
}

fn paint_model_response_curve(
    scene: &mut Scene,
    cm: &CoordMapper,
    cfg: &GraphConfig,
    response_db: &[f32],
    transform: Affine,
) {
    let color = Color::from_rgb8(92, 214, 179);
    let fill_color = Color::from_rgba8(92, 214, 179, 18);
    let zero_y = cm.db_to_y(0.0);
    let log_min = cfg.min_freq.log10();
    let log_max = cfg.max_freq.log10();

    let mut stroke_path = BezPath::new();
    let mut fill_path = BezPath::new();
    fill_path.move_to((cm.freq_to_x(cfg.min_freq), zero_y));

    for (i, &db) in response_db.iter().enumerate() {
        let t = i as f64 / (response_db.len() - 1) as f64;
        let freq = 10.0_f64.powf(log_min + t * (log_max - log_min));
        let x = cm.freq_to_x(freq);
        let y = cm.db_to_y(db as f64);
        if i == 0 {
            stroke_path.move_to((x, y));
        } else {
            stroke_path.line_to((x, y));
        }
        fill_path.line_to((x, y));
    }

    fill_path.line_to((cm.freq_to_x(cfg.max_freq), zero_y));
    fill_path.close_path();

    if cfg.fill_curve {
        scene.fill(Fill::NonZero, transform, fill_color, None, &fill_path);
    }
    scene.stroke(&Stroke::new(2.4), transform, color, None, &stroke_path);
}

fn paint_band_node(
    scene: &mut Scene,
    cm: &CoordMapper,
    band: &EqBand,
    is_hovered: bool,
    is_dragging: bool,
    is_focused: bool,
    transform: Affine,
) {
    let x = cm.freq_to_x(band.frequency as f64);
    let y = cm.db_to_y(band.gain as f64);
    let band_color = hex_to_color(&freq_to_color(band.frequency as f64));
    let inactive_color = Color::from_rgb8(85, 85, 85);

    let radius = if is_dragging {
        10.0
    } else if is_hovered {
        9.0
    } else {
        7.0
    };

    let fill = if band.enabled {
        band_color
    } else {
        inactive_color
    };

    // Glow ring (subtle white outer ring with blur simulation)
    if band.enabled {
        let glow_alpha = if is_dragging {
            0.25
        } else if is_hovered || is_focused {
            0.18
        } else {
            0.08
        };
        let glow_color = Color::from_rgba8(255, 255, 255, (glow_alpha * 255.0) as u8);
        let glow_circle = Circle::new((x, y), radius + 4.0);
        scene.stroke(&Stroke::new(3.0), transform, glow_color, None, &glow_circle);
    }

    // White outline
    let outline_alpha = if !band.enabled {
        0.0
    } else if is_dragging {
        0.9
    } else if is_hovered {
        0.7
    } else {
        0.4
    };
    let outline_color = Color::from_rgba8(255, 255, 255, (outline_alpha * 255.0) as u8);

    let node = Circle::new((x, y), radius);
    scene.fill(Fill::NonZero, transform, fill, None, &node);
    scene.stroke(&Stroke::new(1.5), transform, outline_color, None, &node);
}
