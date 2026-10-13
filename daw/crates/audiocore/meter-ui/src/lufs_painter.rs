//! EBU R128 LUFS meter painter.
//!
//! Renders three vertical bar segments for Momentary (M), Short-term (S), and
//! Integrated (I) loudness values, plus a True Peak bar on the right.
//!
//! Color zones follow EBU R128 recommendations:
//! - Green:  −23 LUFS to −9 LUFS  (target programme loudness)
//! - Yellow: −9 LUFS to −6 LUFS   (caution)
//! - Red:    above −6 LUFS         (over-loud)

use std::sync::Arc;

use meter_dsp::lufs::LufsState;
use nice_plug_dioxus::prelude::vello::kurbo::{Affine, Rect};
use nice_plug_dioxus::prelude::vello::peniko::{Color, Fill};
use nice_plug_dioxus::prelude::vello::Scene;
use nice_plug_dioxus::prelude::SceneOverlay;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Bottom of the scale (LUFS).
const SCALE_MIN: f64 = -60.0;
/// Top of the scale (LUFS).
const SCALE_MAX: f64 = 0.0;

const COLOR_GREEN: Color = Color::from_rgb8(60, 200, 80);
const COLOR_YELLOW: Color = Color::from_rgb8(240, 200, 40);
const COLOR_RED: Color = Color::from_rgb8(220, 50, 50);
const COLOR_BG: Color = Color::from_rgb8(20, 20, 22);
const COLOR_DIM: Color = Color::from_rgba8(60, 60, 65, 180);

// ── Config ────────────────────────────────────────────────────────────────────

/// Configuration for [`LufsPainter`].
#[derive(Clone)]
pub struct LufsConfig {
    /// Position / size in physical pixels.
    pub rect_x: f64,
    pub rect_y: f64,
    pub rect_w: f64,
    pub rect_h: f64,
    /// Whether to show the true peak bar.
    pub show_true_peak: bool,
}

impl Default for LufsConfig {
    fn default() -> Self {
        Self {
            rect_x: 0.0,
            rect_y: 0.0,
            rect_w: 80.0,
            rect_h: 300.0,
            show_true_peak: true,
        }
    }
}

// ── Painter ───────────────────────────────────────────────────────────────────

/// LUFS loudness meter painter.
pub struct LufsPainter {
    state: Arc<LufsState>,
    config: LufsConfig,
}

impl LufsPainter {
    pub fn new(state: Arc<LufsState>, config: LufsConfig) -> Self {
        Self { state, config }
    }
}

impl SceneOverlay for LufsPainter {
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

        let momentary = *self.state.momentary_lufs.read() as f64;
        let short_term = *self.state.short_term_lufs.read() as f64;
        let integrated = *self.state.integrated_lufs.read() as f64;
        let tp_l = *self.state.true_peak_l.read() as f64;
        let tp_r = *self.state.true_peak_r.read() as f64;
        let tp = tp_l.max(tp_r);

        // Background
        scene.fill(
            Fill::NonZero,
            transform,
            COLOR_BG,
            None,
            &Rect::new(0.0, 0.0, w, h),
        );

        let show_tp = self.config.show_true_peak;
        let num_bars = if show_tp { 4 } else { 3 };
        let gap = 3.0_f64;
        let total_gap = gap * (num_bars as f64 + 1.0);
        let bar_w = (w - total_gap) / num_bars as f64;

        let bars: &[(f64, &str)] = if show_tp {
            &[
                (momentary, "M"),
                (short_term, "S"),
                (integrated, "I"),
                (tp, "TP"),
            ]
        } else {
            &[(momentary, "M"), (short_term, "S"), (integrated, "I")]
        };

        for (idx, &(val, _label)) in bars.iter().enumerate() {
            let bx = gap + idx as f64 * (bar_w + gap);
            paint_lufs_bar(scene, transform, bx, bar_w, h, val);
        }
    }
}

/// Paint a single LUFS/TP bar with color zones.
fn paint_lufs_bar(
    scene: &mut Scene,
    transform: Affine,
    x: f64,
    bar_w: f64,
    total_h: f64,
    value_lufs: f64,
) {
    let scale_range = SCALE_MAX - SCALE_MIN; // 60 dB

    // Map LUFS value to y pixel (0 = top = SCALE_MAX).
    let lufs_to_y = |lufs: f64| -> f64 {
        let norm = (SCALE_MAX - lufs.clamp(SCALE_MIN, SCALE_MAX)) / scale_range;
        norm * total_h
    };

    // Background track
    let bg_rect = Rect::new(x, 0.0, x + bar_w, total_h);
    scene.fill(Fill::NonZero, transform, COLOR_DIM, None, &bg_rect);

    if value_lufs.is_infinite() || value_lufs < SCALE_MIN {
        return;
    }

    let bottom_y = total_h;
    let top_y = lufs_to_y(value_lufs);

    // Segment boundaries (LUFS)
    // Green: −60 to −9, Yellow: −9 to −6, Red: −6 to 0
    let zones: &[(f64, f64, Color)] = &[
        (SCALE_MIN, -9.0, COLOR_GREEN),
        (-9.0, -6.0, COLOR_YELLOW),
        (-6.0, SCALE_MAX, COLOR_RED),
    ];

    for &(zone_min, zone_max, color) in zones {
        let seg_bottom = lufs_to_y(zone_min).min(bottom_y);
        let seg_top = lufs_to_y(zone_max);

        // Clip the bar to [top_y, bottom_y]
        let draw_top = seg_top.max(top_y);
        let draw_bottom = seg_bottom.min(bottom_y);

        if draw_top >= draw_bottom {
            continue; // value is below this zone
        }

        let rect = Rect::new(x, draw_top, x + bar_w, draw_bottom);
        scene.fill(Fill::NonZero, transform, color, None, &rect);
    }
}
