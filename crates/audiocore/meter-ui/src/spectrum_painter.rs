//! SPAN-style spectrum analyzer painter.
//!
//! Features:
//! - Visual slope (default 4.5 dB/oct like SPAN — makes pink noise look flat)
//! - All-time maximum spectrum overlay
//! - Fractional-octave visual smoothing (per-pixel max binning)
//! - Frequency + dB grid lines with tick marks
//! - Hold mode (reads hold flag from state)
//! - Configurable color, style (Fill/Curve/Bars)

use std::sync::atomic::Ordering;
use std::sync::Arc;

use meter_dsp::spectrum::SpectrumState;
use nice_plug_dioxus::prelude::vello::kurbo::{Affine, BezPath, Line, Rect, Stroke};
use nice_plug_dioxus::prelude::vello::peniko::{Color, Fill};
use nice_plug_dioxus::prelude::vello::Scene;
use nice_plug_dioxus::prelude::SceneOverlay;
use parking_lot::RwLock;

// ── Config ────────────────────────────────────────────────────────────────────

/// Display style for the spectrum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpectrumStyle {
    /// Filled area under the curve.
    Fill,
    /// Line/curve only, no fill.
    Curve,
    /// Vertical bar per column.
    Bars,
}

/// Configuration for [`SpectrumPainter`].
#[derive(Clone)]
pub struct SpectrumConfig {
    /// Minimum displayed frequency (Hz). Default: 20 Hz.
    pub min_freq: f64,
    /// Maximum displayed frequency (Hz). Default: 20 000 Hz.
    pub max_freq: f64,
    /// Minimum displayed level (dB). Default: -120 dB.
    pub min_db: f64,
    /// Maximum displayed level (dB). Default: 0 dB.
    pub max_db: f64,
    /// Visual slope in dB/octave. Applied as:
    ///   displayed_db = raw_db + slope * log2(freq / min_freq)
    /// Default: 4.5 (SPAN default, makes pink noise appear flat).
    pub slope: f64,
    /// Rendering style.
    pub style: SpectrumStyle,
    /// Primary color for the spectrum curve / bars.
    pub color: Color,
    /// Color for the all-time maximum overlay.
    pub max_color: Color,
    /// Show all-time maximum spectrum overlay. Default: true.
    pub show_max: bool,
    /// Fractional-octave smoothing. 0 = none, 0.33 = 1/3 oct, 1.0 = 1 oct.
    /// Default: 0.5.
    pub smooth_octaves: f64,
    /// Show frequency/dB grid lines. Default: true.
    pub show_grid: bool,
    /// Grid line color.
    pub grid_color: Color,
    /// Label / tick mark color.
    pub label_color: Color,
    /// Paint the solid background rect. Set false for overlays (e.g. R-channel underlay).
    pub draw_background: bool,
}

impl Default for SpectrumConfig {
    fn default() -> Self {
        Self {
            min_freq: 20.0,
            max_freq: 20_000.0,
            min_db: -120.0,
            max_db: 0.0,
            slope: 4.5,
            style: SpectrumStyle::Fill,
            color: Color::from_rgba8(100, 200, 255, 220),
            max_color: Color::from_rgba8(100, 200, 255, 100),
            show_max: true,
            smooth_octaves: 0.5,
            show_grid: true,
            grid_color: Color::from_rgba8(255, 255, 255, 25),
            label_color: Color::from_rgba8(180, 180, 180, 160),
            draw_background: true,
        }
    }
}

// ── Painter ───────────────────────────────────────────────────────────────────

/// SPAN-style spectrum analyzer painter.
///
/// # Example
///
/// ```ignore
/// let painter = SpectrumPainter::new(
///     analyzer.state.clone(),
///     Arc::new(RwLock::new(SpectrumConfig::default())),
/// );
/// ```
pub struct SpectrumPainter {
    state: Arc<SpectrumState>,
    config: Arc<RwLock<SpectrumConfig>>,
}

impl SpectrumPainter {
    /// Create a new SPAN-style spectrum painter.
    pub fn new(state: Arc<SpectrumState>, config: Arc<RwLock<SpectrumConfig>>) -> Self {
        Self { state, config }
    }
}

const COLOR_BG: Color = Color::from_rgb8(14, 14, 16);

/// Standard frequency grid positions (Hz).
const GRID_FREQS: &[f64] = &[
    20.0, 30.0, 50.0, 100.0, 200.0, 300.0, 500.0, 1_000.0, 2_000.0, 3_000.0, 5_000.0, 10_000.0,
    20_000.0,
];

/// Major frequencies get slightly brighter grid lines and ticks.
const MAJOR_FREQS: &[f64] = &[
    20.0, 50.0, 100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0,
];

/// Build the per-column display values from a bin slice.
///
/// Returns a `Vec<f64>` of length `n_cols`. Each entry is the display dB value
/// (with slope applied), or `f64::NEG_INFINITY` if no valid bins exist.
fn build_display_cols(
    bins: &[f32],
    n_cols: usize,
    fft_size: usize,
    sample_rate: f64,
    cfg: &SpectrumConfig,
) -> Vec<f64> {
    let n_bins = bins.len();
    let log_min = cfg.min_freq.max(1.0).log2();

    let mut cols = vec![f64::NEG_INFINITY; n_cols];

    for (i, col) in cols.iter_mut().enumerate().take(n_cols) {
        let t = i as f64 / n_cols as f64;
        let freq = cfg.min_freq * (cfg.max_freq / cfg.min_freq).powf(t);

        let bin_center = freq * fft_size as f64 / sample_rate;

        // Smoothing window in bins.
        let half_bins = if cfg.smooth_octaves > 0.0 {
            (bin_center * (2f64.powf(cfg.smooth_octaves * 0.5) - 1.0)).max(0.5)
        } else {
            0.5
        };

        let bin_lo = ((bin_center - half_bins).max(0.0) as usize).min(n_bins - 1);
        let bin_hi = ((bin_center + half_bins).ceil() as usize).min(n_bins - 1);

        // Take max across the smoothing window.
        let mut raw_db = f64::NEG_INFINITY;
        for &v in &bins[bin_lo..=bin_hi] {
            let v = v as f64;
            if v > raw_db {
                raw_db = v;
            }
        }

        // Apply visual slope.
        let slope_db = cfg.slope * (freq.max(1.0).log2() - log_min);
        *col = raw_db + slope_db;
    }

    cols
}

/// Map a display dB value to a y pixel coordinate (0 = top = max_db).
#[inline]
fn db_to_y(db: f64, cfg: &SpectrumConfig, h: f64) -> f64 {
    let norm = (cfg.max_db - db) / (cfg.max_db - cfg.min_db);
    norm.clamp(0.0, 1.0) * h
}

/// Map a frequency (Hz) to an x pixel coordinate.
#[inline]
fn freq_to_x(freq: f64, cfg: &SpectrumConfig, w: f64) -> f64 {
    let log_min = cfg.min_freq.max(1.0).log10();
    let log_max = cfg.max_freq.max(1.0).log10();
    let norm = (freq.max(1.0).log10() - log_min) / (log_max - log_min);
    norm.clamp(0.0, 1.0) * w
}

fn draw_grid(scene: &mut Scene, transform: Affine, w: f64, h: f64, cfg: &SpectrumConfig) {
    // ── Vertical frequency lines ──────────────────────────────────────────────
    for &freq in GRID_FREQS {
        if freq < cfg.min_freq || freq > cfg.max_freq {
            continue;
        }
        let x = freq_to_x(freq, cfg, w);
        let is_major = MAJOR_FREQS.contains(&freq);

        let line_color = if is_major {
            // Slightly brighter for major frequencies
            Color::from_rgba8(255, 255, 255, 40)
        } else {
            cfg.grid_color
        };

        let line = Line::new((x, 0.0), (x, h));
        scene.stroke(&Stroke::new(1.0), transform, line_color, None, &line);

        // Tick mark at bottom (2px tall rect).
        if is_major {
            let tick = Rect::new(x - 1.0, h - 4.0, x + 1.0, h);
            scene.fill(Fill::NonZero, transform, cfg.label_color, None, &tick);
        }
    }

    // ── Horizontal dB lines ───────────────────────────────────────────────────
    // Lines every 10 dB.
    let mut db = cfg.max_db - 10.0;
    while db > cfg.min_db {
        let y = db_to_y(db, cfg, h);
        // Slightly brighter at multiples of 20 dB.
        let is_major_db = (db as i64).abs() % 20 == 0;
        let line_color = if is_major_db {
            Color::from_rgba8(255, 255, 255, 40)
        } else {
            cfg.grid_color
        };

        let line = Line::new((0.0, y), (w, y));
        scene.stroke(&Stroke::new(1.0), transform, line_color, None, &line);

        db -= 10.0;
    }
}

/// Build a BezPath from a column display-value slice.
fn build_path(cols: &[f64], cfg: &SpectrumConfig, w: f64, h: f64) -> Option<BezPath> {
    let n_cols = cols.len();
    if n_cols == 0 {
        return None;
    }

    let mut path = BezPath::new();
    let mut started = false;

    for (i, &display_db) in cols.iter().enumerate().take(n_cols) {
        let x = (i as f64 / (n_cols - 1).max(1) as f64) * w;
        if display_db.is_infinite() && display_db < 0.0 {
            // Below range — pin to bottom.
            let y = h;
            if !started {
                path.move_to((x, y));
                started = true;
            } else {
                path.line_to((x, y));
            }
        } else {
            let y = db_to_y(display_db, cfg, h);
            if !started {
                path.move_to((x, y));
                started = true;
            } else {
                path.line_to((x, y));
            }
        }
    }

    if started {
        Some(path)
    } else {
        None
    }
}

impl SceneOverlay for SpectrumPainter {
    fn paint(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        width: u32,
        height: u32,
        _scale: f64,
    ) {
        let w = width as f64;
        let h = height as f64;
        if w < 1.0 || h < 1.0 {
            return;
        }

        let cfg = self.config.read().clone();

        // ── Background ────────────────────────────────────────────────────────
        if cfg.draw_background {
            scene.fill(
                Fill::NonZero,
                transform,
                COLOR_BG,
                None,
                &Rect::new(0.0, 0.0, w, h),
            );
        }
        let fs = self.state.sample_rate.load(Ordering::Relaxed) as f64;
        let fft_size = self.state.fft_size.load(Ordering::Relaxed);

        // ── Grid ──────────────────────────────────────────────────────────────
        if cfg.show_grid {
            draw_grid(scene, transform, w, h, &cfg);
        }

        // ── Compute display columns ───────────────────────────────────────────
        let n_cols = (w as usize).clamp(2, 2048);

        let cols = {
            let bins = self.state.bins_db.read();
            let n_bins = bins.len();
            if n_bins < 2 {
                return;
            }
            build_display_cols(&bins, n_cols, fft_size, fs, &cfg)
        };

        let max_cols = if cfg.show_max {
            let max_bins = self.state.max_bins_db.read();
            let n_bins = max_bins.len();
            if n_bins >= 2 {
                Some(build_display_cols(&max_bins, n_cols, fft_size, fs, &cfg))
            } else {
                None
            }
        } else {
            None
        };

        // ── Draw max spectrum overlay (thin stroke, no fill) ──────────────────
        if let Some(ref mc) = max_cols {
            if let Some(max_path) = build_path(mc, &cfg, w, h) {
                scene.stroke(&Stroke::new(1.0), transform, cfg.max_color, None, &max_path);
            }
        }

        // ── Draw main spectrum ────────────────────────────────────────────────
        match cfg.style {
            SpectrumStyle::Bars => {
                for (i, &display_db) in cols.iter().enumerate().take(n_cols) {
                    let x = (i as f64 / (n_cols - 1).max(1) as f64) * w;
                    let y = if display_db.is_infinite() && display_db < 0.0 {
                        h
                    } else {
                        db_to_y(display_db, &cfg, h)
                    };
                    let line = Line::new((x, h), (x, y));
                    scene.stroke(&Stroke::new(1.0), transform, cfg.color, None, &line);
                }
            }

            SpectrumStyle::Curve | SpectrumStyle::Fill => {
                if let Some(path) = build_path(&cols, &cfg, w, h) {
                    // Stroke
                    scene.stroke(&Stroke::new(1.5), transform, cfg.color, None, &path);

                    // Fill
                    if cfg.style == SpectrumStyle::Fill {
                        let mut fill_path = path.clone();
                        fill_path.line_to((w, h));
                        fill_path.line_to((0.0, h));
                        fill_path.close_path();

                        let fill_color = cfg.color.with_alpha(0.15);
                        scene.fill(Fill::NonZero, transform, fill_color, None, &fill_path);
                    }
                }
            }
        }
    }
}
