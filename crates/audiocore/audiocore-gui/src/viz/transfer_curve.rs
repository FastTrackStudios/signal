//! Transfer curve visualization — shows input vs output dB.
//!
//! Used by compressor, limiter, gate, and expander plugins to
//! display their gain transfer function.
//!
//! GPU-rendered via Vello scene overlay — produces only 1 DOM node
//! (the placeholder div) instead of ~130 positioned divs.

use std::cell::RefCell;
use std::rc::Rc;

use crate::theme::use_theme;
use crate::viz::waveform::{CanvasPainter, VelloCanvas};
use nice_plug_dioxus::prelude::vello::kurbo::{Affine, BezPath, Circle, Line, Rect, Stroke};
use nice_plug_dioxus::prelude::vello::peniko::{Color, Fill};
use nice_plug_dioxus::prelude::*;

/// Compute gain reduction for a given input level using soft-knee compression.
///
/// Returns output_db for a given input_db.
fn compress_transfer(input_db: f32, threshold_db: f32, ratio: f32, knee_db: f32) -> f32 {
    if ratio <= 1.0 {
        return input_db;
    }

    let slope = 1.0 - 1.0 / ratio;
    let half_knee = knee_db * 0.5;

    if knee_db > 0.001 && (input_db - threshold_db).abs() < half_knee {
        // Soft knee region: quadratic interpolation
        let x = input_db - threshold_db + half_knee;
        let gr = slope * x * x / (2.0 * knee_db);
        input_db - gr
    } else if input_db > threshold_db {
        // Above knee: standard compression
        let gr = slope * (input_db - threshold_db);
        input_db - gr
    } else {
        // Below threshold: no compression
        input_db
    }
}

/// Painter for the transfer curve visualization.
pub struct TransferCurvePainter {
    threshold_db: f32,
    ratio: f32,
    knee_db: f32,
    input_level_db: Option<f32>,
    range_db: f32,
}

impl Default for TransferCurvePainter {
    fn default() -> Self {
        Self::new()
    }
}

impl TransferCurvePainter {
    pub fn new() -> Self {
        Self {
            threshold_db: -18.0,
            ratio: 4.0,
            knee_db: 6.0,
            input_level_db: None,
            range_db: 60.0,
        }
    }

    pub fn update(
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
    }
}

impl CanvasPainter for TransferCurvePainter {
    fn paint(
        &self,
        scene: &mut nice_plug_dioxus::prelude::vello::Scene,
        transform: Affine,
        w: f64,
        h: f64,
    ) {
        let min_db = -self.range_db;
        let range = self.range_db as f64;

        let db_to_x = |db: f64| -> f64 { ((db - min_db as f64) / range) * w };
        let db_to_y = |db: f64| -> f64 { h - ((db - min_db as f64) / range) * h };

        // Background
        scene.fill(
            Fill::NonZero,
            transform,
            Color::from_rgba8(14, 14, 18, 255),
            None,
            &Rect::new(0.0, 0.0, w, h),
        );

        // Grid lines (every 12 dB)
        let grid_color = Color::from_rgba8(255, 255, 255, 13); // rgba(255,255,255,0.05)
        let grid_stroke = Stroke::new(1.0);
        for db_val in [-48, -36, -24, -12] {
            let x = db_to_x(db_val as f64);
            let y = db_to_y(db_val as f64);
            // Vertical grid line
            scene.stroke(
                &grid_stroke,
                transform,
                grid_color,
                None,
                &Line::new((x, 0.0), (x, h)),
            );
            // Horizontal grid line
            scene.stroke(
                &grid_stroke,
                transform,
                grid_color,
                None,
                &Line::new((0.0, y), (w, y)),
            );
        }

        // 1:1 reference line (dashed diagonal)
        let ref_color = Color::from_rgba8(255, 255, 255, 26); // rgba(255,255,255,0.10)
        let dash_len = w / 60.0;
        let num_dashes = 40;
        for i in 0..num_dashes {
            let frac0 = i as f64 / num_dashes as f64;
            let frac1 = (i as f64 + 0.5) / num_dashes as f64;
            let db0 = min_db as f64 + frac0 * range;
            let db1 = min_db as f64 + frac1 * range;
            let _ = dash_len;
            scene.stroke(
                &Stroke::new(1.0),
                transform,
                ref_color,
                None,
                &Line::new((db_to_x(db0), db_to_y(db0)), (db_to_x(db1), db_to_y(db1))),
            );
        }

        // Transfer curve
        let accent_color = Color::from_rgba8(143, 168, 200, 255); // #8fa8c8
        let num_points = 80;
        let mut path = BezPath::new();
        for i in 0..=num_points {
            let input = min_db as f64 + (i as f64 / num_points as f64) * range;
            let output =
                compress_transfer(input as f32, self.threshold_db, self.ratio, self.knee_db) as f64;
            let x = db_to_x(input);
            let y = db_to_y(output);
            if i == 0 {
                path.move_to((x, y));
            } else {
                path.line_to((x, y));
            }
        }
        scene.stroke(&Stroke::new(2.0), transform, accent_color, None, &path);

        // Threshold crosshair
        let crosshair_color = Color::from_rgba8(255, 100, 100, 77); // rgba(255,100,100,0.3)
        let thresh_x = db_to_x(self.threshold_db as f64);
        let thresh_y = db_to_y(self.threshold_db as f64);
        scene.stroke(
            &Stroke::new(1.0),
            transform,
            crosshair_color,
            None,
            &Line::new((thresh_x, 0.0), (thresh_x, h)),
        );
        scene.stroke(
            &Stroke::new(1.0),
            transform,
            crosshair_color,
            None,
            &Line::new((0.0, thresh_y), (w, thresh_y)),
        );

        // Input level indicator (ball on curve)
        if let Some(level) = self.input_level_db {
            let out = compress_transfer(level, self.threshold_db, self.ratio, self.knee_db) as f64;
            let bx = db_to_x(level as f64);
            let by = db_to_y(out);
            scene.fill(
                Fill::NonZero,
                transform,
                Color::from_rgba8(255, 255, 255, 230),
                None,
                &Circle::new((bx, by), 3.0),
            );
        }
    }
}

/// Transfer curve visualization — shows input vs output dB.
///
/// GPU-rendered via Vello scene overlay — produces only 1 DOM node
/// (the placeholder div) instead of ~130 positioned divs.
#[component]
pub fn TransferCurve(
    /// Threshold in dBFS.
    threshold_db: f32,
    /// Compression ratio (e.g. 4.0 = 4:1).
    ratio: f32,
    /// Soft knee width in dB (0 = hard knee, 6 = gentle).
    #[props(default = 6.0)]
    knee_db: f32,
    /// Current input level (for the "ball" indicator), or None.
    #[props(default)]
    input_level_db: Option<f32>,
    /// Widget width in pixels.
    #[props(default = 200.0)]
    width: f32,
    /// Widget height in pixels.
    #[props(default = 200.0)]
    height: f32,
    /// dB range (e.g. 60.0 means -60 to 0).
    #[props(default = 60.0)]
    range_db: f32,
    /// Fill parent container instead of using fixed dimensions.
    #[props(default = false)]
    fill: bool,
    /// Optional inline style to merge onto the outer element.
    #[props(default)]
    style: Option<String>,
) -> Element {
    let t = use_theme();
    let t = *t.read();

    // Create shared painter (persists across renders)
    let painter: &Rc<RefCell<TransferCurvePainter>> =
        &use_hook(|| Rc::new(RefCell::new(TransferCurvePainter::new())));

    // Update data each render
    painter
        .borrow_mut()
        .update(threshold_db, ratio, knee_db, input_level_db);

    // Type-erase to dyn CanvasPainter for VelloCanvas
    let dyn_painter: Rc<RefCell<dyn CanvasPainter>> = painter.clone();

    let outer_style = style.as_deref().unwrap_or("");
    rsx! {
        VelloCanvas {
            painter: dyn_painter,
            width: width,
            height: height,
            fill: fill,
            style: format!(
                "border-radius:6px; overflow:hidden; border:1px solid {border}; {outer_style}",
                border = t.border,
            ),
        }
    }
}
