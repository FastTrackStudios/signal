//! K-system meter painter.
//!
//! Renders a vertical VU bar with K-20/K-14/K-12 reference level lines,
//! green/yellow/red zones, and a held-peak line.
//!
//! The 0 VU reference line is drawn at the K-mode reference level.

use std::sync::Arc;

use meter_dsp::k_meter::KMeterState;
use nice_plug_dioxus::prelude::vello::kurbo::{Affine, Line, Rect, Stroke};
use nice_plug_dioxus::prelude::vello::peniko::{Color, Fill};
use nice_plug_dioxus::prelude::vello::Scene;
use nice_plug_dioxus::prelude::SceneOverlay;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Bottom of the displayed dBFS scale.
const SCALE_MIN_DBFS: f64 = -40.0;
/// Top of the displayed dBFS scale.
const SCALE_MAX_DBFS: f64 = 0.0;

const COLOR_GREEN: Color = Color::from_rgb8(60, 200, 80);
const COLOR_YELLOW: Color = Color::from_rgb8(240, 200, 40);
const COLOR_RED: Color = Color::from_rgb8(220, 50, 50);
const COLOR_BG: Color = Color::from_rgb8(20, 20, 22);
const COLOR_DIM: Color = Color::from_rgba8(55, 55, 60, 200);
const COLOR_HOLD: Color = Color::from_rgb8(255, 255, 255);
const COLOR_REF: Color = Color::from_rgba8(255, 255, 100, 180);

// ── Config ────────────────────────────────────────────────────────────────────

/// Configuration for [`KMeterPainter`].
#[derive(Clone)]
pub struct KMeterConfig {
    pub rect_x: f64,
    pub rect_y: f64,
    pub rect_w: f64,
    pub rect_h: f64,
}

impl Default for KMeterConfig {
    fn default() -> Self {
        Self {
            rect_x: 0.0,
            rect_y: 0.0,
            rect_w: 30.0,
            rect_h: 200.0,
        }
    }
}

// ── Painter ───────────────────────────────────────────────────────────────────

/// K-system VU meter painter.
pub struct KMeterPainter {
    state: Arc<KMeterState>,
    #[allow(dead_code)]
    config: KMeterConfig,
}

impl KMeterPainter {
    pub fn new(state: Arc<KMeterState>, config: KMeterConfig) -> Self {
        Self { state, config }
    }
}

impl SceneOverlay for KMeterPainter {
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
        if w < 2.0 || h < 2.0 {
            return;
        }

        let rms_db = *self.state.rms_db.write() as f64;
        let peak_hold = *self.state.peak_hold_db.write() as f64;
        let mode = *self.state.mode.read();
        let ref_dbfs = mode.reference_dbfs() as f64;

        // Background
        scene.fill(
            Fill::NonZero,
            transform,
            COLOR_BG,
            None,
            &Rect::new(0.0, 0.0, w, h),
        );

        let scale_range = SCALE_MAX_DBFS - SCALE_MIN_DBFS;
        let dbfs_to_y = |db: f64| -> f64 {
            let norm = (SCALE_MAX_DBFS - db.clamp(SCALE_MIN_DBFS, SCALE_MAX_DBFS)) / scale_range;
            norm * h
        };

        // Background track
        scene.fill(
            Fill::NonZero,
            transform,
            COLOR_DIM,
            None,
            &Rect::new(0.0, 0.0, w, h),
        );

        if rms_db.is_finite() {
            // Color zones relative to the K reference:
            // Green: below reference − 2 dB, Yellow: up to ref, Red: above ref
            let green_top_db = SCALE_MIN_DBFS;
            let yellow_top_db = ref_dbfs;
            let red_top_db = SCALE_MAX_DBFS;

            let zones: &[(f64, f64, Color)] = &[
                (green_top_db, yellow_top_db, COLOR_GREEN),
                (yellow_top_db, red_top_db, COLOR_YELLOW),
                (red_top_db, SCALE_MAX_DBFS, COLOR_RED),
            ];

            let bar_top = dbfs_to_y(rms_db);
            let bar_bot = h;

            for &(zone_min_db, zone_max_db, color) in zones {
                let seg_bot = dbfs_to_y(zone_min_db).min(bar_bot);
                let seg_top = dbfs_to_y(zone_max_db);
                let draw_top = seg_top.max(bar_top);
                let draw_bot = seg_bot.min(bar_bot);
                if draw_top >= draw_bot {
                    continue;
                }
                scene.fill(
                    Fill::NonZero,
                    transform,
                    color,
                    None,
                    &Rect::new(0.0, draw_top, w, draw_bot),
                );
            }
        }

        // Peak hold line
        if peak_hold.is_finite() {
            let py = dbfs_to_y(peak_hold);
            let line = Line::new((0.0, py), (w, py));
            scene.stroke(&Stroke::new(2.0), transform, COLOR_HOLD, None, &line);
        }

        // Reference level line (0 VU for the selected K-mode)
        {
            let ry = dbfs_to_y(ref_dbfs);
            let line = Line::new((0.0, ry), (w, ry));
            scene.stroke(&Stroke::new(1.5), transform, COLOR_REF, None, &line);
        }
    }
}
