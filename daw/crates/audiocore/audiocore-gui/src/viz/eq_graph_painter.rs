//! GPU-accelerated EQ graph painter using vello scene overlay.
//!
//! Renders the EQ graph visual elements (grid, curves, spectrum, nodes)
//! directly into the vello scene each frame, giving proper anti-aliasing,
//! smooth curves, and glow effects.

use std::sync::Arc;

use nice_plug_dioxus::prelude::vello::kurbo::{Affine, BezPath, Circle, Line, Rect, Stroke};
use nice_plug_dioxus::prelude::vello::peniko::{Color, Fill};
use nice_plug_dioxus::prelude::vello::Scene;
use nice_plug_dioxus::prelude::SceneOverlay;
use parking_lot::RwLock;

use super::eq_graph::{
    calculate_band_response, calculate_combined_response, get_band_color, EqBand,
};

// ── Shared state ────────────────────────────────────────────────────

/// Shared state between the Dioxus component (interaction) and the painter (rendering).
pub struct EqGraphRenderState {
    pub bands: RwLock<Vec<EqBand>>,
    pub spectrum_db: RwLock<Vec<f32>>,
    pub config: RwLock<GraphConfig>,
    pub interaction: RwLock<InteractionState>,
}

impl EqGraphRenderState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            bands: RwLock::new(Vec::new()),
            spectrum_db: RwLock::new(Vec::new()),
            config: RwLock::new(GraphConfig::default()),
            interaction: RwLock::new(InteractionState::default()),
        })
    }
}

#[derive(Clone)]
pub struct GraphConfig {
    pub db_range: f64,
    pub min_freq: f64,
    pub max_freq: f64,
    pub sample_rate: f64,
    pub show_grid: bool,
    pub show_freq_labels: bool,
    pub show_db_labels: bool,
    pub fill_curve: bool,
    /// Position of the graph area in the window (physical pixels).
    /// Set by the Dioxus component so the overlay paints in the right place.
    pub rect_x: f64,
    pub rect_y: f64,
    pub rect_w: f64,
    pub rect_h: f64,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            db_range: 24.0,
            min_freq: 20.0,
            max_freq: 20000.0,
            sample_rate: 48000.0,
            show_grid: true,
            show_freq_labels: true,
            show_db_labels: true,
            fill_curve: true,
            rect_x: 0.0,
            rect_y: 0.0,
            rect_w: 800.0,
            rect_h: 350.0,
        }
    }
}

#[derive(Clone, Default)]
pub struct InteractionState {
    pub hovered_band: Option<usize>,
    pub dragging_band: Option<usize>,
    pub focused_band: Option<usize>,
    pub selected_bands: Vec<usize>,
}

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
    log_min: f64,
    log_max: f64,
    padding: f64,
    graph_w: f64,
    graph_h: f64,
    db_range: f64,
}

impl CoordMapper {
    fn new(cfg: &GraphConfig, padding: f64) -> Self {
        Self {
            log_min: cfg.min_freq.log10(),
            log_max: cfg.max_freq.log10(),
            padding,
            graph_w: cfg.rect_w - padding * 2.0,
            graph_h: cfg.rect_h - padding * 2.0,
            db_range: cfg.db_range,
        }
    }

    fn freq_to_x(&self, freq: f64) -> f64 {
        let norm = (freq.log10() - self.log_min) / (self.log_max - self.log_min);
        self.padding + norm * self.graph_w
    }

    fn db_to_y(&self, db: f64) -> f64 {
        let clamped = db.clamp(-self.db_range, self.db_range);
        let norm = 0.5 - clamped / (2.0 * self.db_range);
        self.padding + norm * self.graph_h
    }
}

// ── Painter ─────────────────────────────────────────────────────────

/// Scene overlay that paints the EQ graph into the main vello scene.
pub struct EqGraphPainter {
    state: Arc<EqGraphRenderState>,
}

impl EqGraphPainter {
    pub fn new(state: Arc<EqGraphRenderState>) -> Self {
        Self { state }
    }
}

impl SceneOverlay for EqGraphPainter {
    fn paint(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        width: u32,
        height: u32,
        _scale: f64,
    ) {
        let elem_w = width as f64;
        let elem_h = height as f64;
        if elem_w < 1.0 || elem_h < 1.0 {
            return;
        }

        let cfg = self.state.config.read().clone();
        let bands = self.state.bands.read().clone();
        let spectrum = self.state.spectrum_db.read().clone();
        let interaction = self.state.interaction.read().clone();

        // Use CSS pixel coordinates directly — same as PeakWaveformPainter.
        // CoordMapper uses elem_w x elem_h so all coordinates are in CSS pixels.
        // `transform` handles positioning (translate + uniform scale), no aspect-ratio distortion.
        let cfg = GraphConfig {
            rect_w: elem_w,
            rect_h: elem_h,
            ..cfg
        };

        let padding = 0.0;
        let cm = CoordMapper::new(&cfg, padding);

        let area = Rect::new(0.0, 0.0, elem_w, elem_h);

        // Background
        let bg = Color::from_rgb8(10, 10, 10);
        scene.fill(Fill::NonZero, transform, bg, None, &area);

        // Grid
        if cfg.show_grid {
            paint_grid(scene, &cm, &cfg, transform);
        }

        // Spectrum analyzer
        if spectrum.len() >= 2 {
            paint_spectrum(scene, &cm, &cfg, &spectrum, transform);
        }

        // Per-band curves
        let num_points = 400;
        let frequencies = generate_frequencies(&cfg, num_points);

        for band in &bands {
            if !band.used || !band.enabled {
                continue;
            }
            paint_band_curve(scene, &cm, &cfg, band, &frequencies, transform);
        }

        // Connecting lines
        paint_connecting_lines(scene, &cm, &cfg, &bands, transform);

        // Combined curve
        paint_combined_curve(scene, &cm, &cfg, &bands, &frequencies, transform);

        // Band nodes
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
}

// ── Painting functions ──────────────────────────────────────────────

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
    let band_color = hex_to_color(get_band_color(band.index));
    let fill_color = hex_to_color_alpha(get_band_color(band.index), 0.25);
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
        let color = hex_to_color_alpha(get_band_color(band.index), 0.5);
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
    let band_color = hex_to_color(get_band_color(band.index));
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
