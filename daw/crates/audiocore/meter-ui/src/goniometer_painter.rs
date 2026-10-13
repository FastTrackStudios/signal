//! Stereo goniometer (Lissajous) painter.
//!
//! Renders recent (mid, side) sample pairs as a fading dot plot on a dark
//! circular background with M/S axes. Older samples fade out via decreasing
//! alpha so the current stereo image is always visible.
//!
//! The horizontal axis is Mid (mono sum), vertical is Side (difference).

use std::sync::Arc;

use meter_dsp::phase::PhaseState;
use nice_plug_dioxus::prelude::vello::kurbo::{Affine, Circle, Line, Stroke};
use nice_plug_dioxus::prelude::vello::peniko::{Color, Fill};
use nice_plug_dioxus::prelude::vello::Scene;
use nice_plug_dioxus::prelude::SceneOverlay;

// ── Colors ────────────────────────────────────────────────────────────────────

const COLOR_BG: Color = Color::from_rgb8(12, 12, 14);
const COLOR_AXIS: Color = Color::from_rgba8(80, 80, 85, 140);
const COLOR_RING: Color = Color::from_rgba8(50, 50, 55, 200);

// ── Config ────────────────────────────────────────────────────────────────────

/// Configuration for [`GoniometerPainter`].
#[derive(Clone)]
pub struct GoniometerConfig {
    pub rect_x: f64,
    pub rect_y: f64,
    pub rect_w: f64,
    pub rect_h: f64,
    /// Dot radius in pixels.
    pub dot_radius: f64,
}

impl Default for GoniometerConfig {
    fn default() -> Self {
        Self {
            rect_x: 0.0,
            rect_y: 0.0,
            rect_w: 150.0,
            rect_h: 150.0,
            dot_radius: 1.2,
        }
    }
}

// ── Painter ───────────────────────────────────────────────────────────────────

/// Stereo goniometer dot-plot painter.
pub struct GoniometerPainter {
    state: Arc<PhaseState>,
    config: GoniometerConfig,
}

impl GoniometerPainter {
    pub fn new(state: Arc<PhaseState>, config: GoniometerConfig) -> Self {
        Self { state, config }
    }
}

impl SceneOverlay for GoniometerPainter {
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
        if w < 4.0 || h < 4.0 {
            return;
        }

        let cx = w * 0.5;
        let cy = h * 0.5;
        let radius = cx.min(cy) - 2.0;

        // Background circle
        let bg_circle = Circle::new((cx, cy), radius);
        scene.fill(Fill::NonZero, transform, COLOR_BG, None, &bg_circle);
        scene.stroke(&Stroke::new(1.0), transform, COLOR_RING, None, &bg_circle);

        // Axes: horizontal (M) and vertical (S)
        let axis_len = radius - 2.0;
        let h_axis = Line::new((cx - axis_len, cy), (cx + axis_len, cy));
        let v_axis = Line::new((cx, cy - axis_len), (cx, cy + axis_len));
        let diag1 = Line::new(
            (cx - axis_len * 0.707, cy - axis_len * 0.707),
            (cx + axis_len * 0.707, cy + axis_len * 0.707),
        );
        let diag2 = Line::new(
            (cx - axis_len * 0.707, cy + axis_len * 0.707),
            (cx + axis_len * 0.707, cy - axis_len * 0.707),
        );
        let axis_stroke = Stroke::new(0.5);
        scene.stroke(&axis_stroke, transform, COLOR_AXIS, None, &h_axis);
        scene.stroke(&axis_stroke, transform, COLOR_AXIS, None, &v_axis);
        scene.stroke(
            &axis_stroke,
            transform,
            COLOR_AXIS.with_alpha(0.4),
            None,
            &diag1,
        );
        scene.stroke(
            &axis_stroke,
            transform,
            COLOR_AXIS.with_alpha(0.4),
            None,
            &diag2,
        );

        // Dots
        let samples = self.state.goniometer_samples.read();
        let n = samples.len();
        if n == 0 {
            return;
        }

        let dot_r = self.config.dot_radius;
        let scale = radius; // map ±1 → ±radius

        for (idx, &(mid, side)) in samples.iter().enumerate() {
            // Normalise age: 0 = oldest, 1 = newest
            let age = idx as f64 / n as f64;

            // Interpolate color: old → dim blue (40, 80, 120, 40), new → bright cyan (160, 230, 255, 220)
            let r = lerp(40.0, 160.0, age) as u8;
            let g = lerp(80.0, 230.0, age) as u8;
            let b = lerp(120.0, 255.0, age) as u8;
            let a = lerp(40.0, 220.0, age) as u8;
            let color = Color::from_rgba8(r, g, b, a);

            // Goniometer: X=mid, Y=−side (up = positive side)
            let px = cx + mid as f64 * scale;
            let py = cy - side as f64 * scale;

            // Clip to circle
            let dx = px - cx;
            let dy = py - cy;
            if dx * dx + dy * dy > radius * radius {
                continue;
            }

            scene.fill(
                Fill::NonZero,
                transform,
                color,
                None,
                &Circle::new((px, py), dot_r),
            );
        }
    }
}

#[inline]
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
