//! Phase correlation bar painter.
//!
//! Renders a horizontal bar from −1 to +1 showing the stereo phase correlation.
//!
//! Color coding:
//! - Green (right half, > 0): in-phase / mono-compatible.
//! - Yellow (near 0): uncorrelated.
//! - Red (left half, < 0): out-of-phase / mono-incompatible.

use std::sync::Arc;

use meter_dsp::phase::PhaseState;
use nice_plug_dioxus::prelude::vello::kurbo::{Affine, Line, Rect, Stroke};
use nice_plug_dioxus::prelude::vello::peniko::{Color, Fill};
use nice_plug_dioxus::prelude::vello::Scene;
use nice_plug_dioxus::prelude::SceneOverlay;

// ── Colors ────────────────────────────────────────────────────────────────────

const COLOR_BG: Color = Color::from_rgb8(20, 20, 22);
const COLOR_TRACK: Color = Color::from_rgba8(55, 55, 60, 200);
const COLOR_GREEN: Color = Color::from_rgb8(60, 200, 80);
const COLOR_YELLOW: Color = Color::from_rgb8(240, 200, 40);
const COLOR_RED: Color = Color::from_rgb8(220, 50, 50);
const COLOR_NEEDLE: Color = Color::from_rgb8(255, 255, 255);
const COLOR_CENTER: Color = Color::from_rgba8(150, 150, 155, 180);

// ── Config ────────────────────────────────────────────────────────────────────

/// Configuration for [`PhasePainter`].
#[derive(Clone)]
pub struct PhaseConfig {
    pub rect_x: f64,
    pub rect_y: f64,
    pub rect_w: f64,
    pub rect_h: f64,
}

impl Default for PhaseConfig {
    fn default() -> Self {
        Self {
            rect_x: 0.0,
            rect_y: 0.0,
            rect_w: 200.0,
            rect_h: 20.0,
        }
    }
}

// ── Painter ───────────────────────────────────────────────────────────────────

/// Phase correlation bar painter.
pub struct PhasePainter {
    state: Arc<PhaseState>,
    #[allow(dead_code)]
    config: PhaseConfig,
}

impl PhasePainter {
    pub fn new(state: Arc<PhaseState>, config: PhaseConfig) -> Self {
        Self { state, config }
    }
}

impl SceneOverlay for PhasePainter {
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
        if w < 4.0 || h < 2.0 {
            return;
        }

        let correlation = *self.state.correlation.read() as f64;

        // Background
        scene.fill(
            Fill::NonZero,
            transform,
            COLOR_BG,
            None,
            &Rect::new(0.0, 0.0, w, h),
        );
        scene.fill(
            Fill::NonZero,
            transform,
            COLOR_TRACK,
            None,
            &Rect::new(0.0, 0.0, w, h),
        );

        // Map correlation [−1, +1] → x pixel [0, w]
        let corr_to_x = |c: f64| -> f64 { (c + 1.0) * 0.5 * w };
        let center_x = w * 0.5;
        let needle_x = corr_to_x(correlation);

        // Fill from center toward the needle, colored by zone
        if correlation > 0.0 {
            // Green fill: center → needle
            let fill = Rect::new(center_x, 0.0, needle_x, h);
            scene.fill(
                Fill::NonZero,
                transform,
                COLOR_GREEN.with_alpha(0.6),
                None,
                &fill,
            );
        } else if correlation > -0.3 {
            // Yellow fill
            let fill = Rect::new(needle_x, 0.0, center_x, h);
            scene.fill(
                Fill::NonZero,
                transform,
                COLOR_YELLOW.with_alpha(0.6),
                None,
                &fill,
            );
        } else {
            // Red fill
            let fill = Rect::new(needle_x, 0.0, center_x, h);
            scene.fill(
                Fill::NonZero,
                transform,
                COLOR_RED.with_alpha(0.6),
                None,
                &fill,
            );
        }

        // Center tick
        let center_line = Line::new((center_x, 0.0), (center_x, h));
        scene.stroke(
            &Stroke::new(1.0),
            transform,
            COLOR_CENTER,
            None,
            &center_line,
        );

        // Needle
        let needle_line = Line::new((needle_x, 0.0), (needle_x, h));
        scene.stroke(
            &Stroke::new(2.0),
            transform,
            COLOR_NEEDLE,
            None,
            &needle_line,
        );
    }
}
