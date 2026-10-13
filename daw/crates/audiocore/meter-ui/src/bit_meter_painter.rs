//! Bit depth utilization painter.
//!
//! Renders a horizontal segmented bar showing the estimated effective bit depth
//! of the audio stream (1–24 bits).
//!
//! Segments representing bits that are actively used are bright; unused
//! LSB segments are dim. This is useful for verifying that a 24-bit source
//! is actually using the full bit depth and not just 16 bits with zero-padding.

use std::sync::Arc;

use meter_dsp::bit_depth::BitDepthState;
use nice_plug_dioxus::prelude::vello::kurbo::{Affine, Rect};
use nice_plug_dioxus::prelude::vello::peniko::{Color, Fill};
use nice_plug_dioxus::prelude::vello::Scene;
use nice_plug_dioxus::prelude::SceneOverlay;

// ── Colors ────────────────────────────────────────────────────────────────────

const COLOR_BG: Color = Color::from_rgb8(20, 20, 22);
const COLOR_USED: Color = Color::from_rgb8(80, 200, 120);
const COLOR_UNUSED: Color = Color::from_rgba8(50, 55, 55, 160);

// ── Config ────────────────────────────────────────────────────────────────────

/// Configuration for [`BitMeterPainter`].
#[derive(Clone)]
pub struct BitMeterConfig {
    pub rect_x: f64,
    pub rect_y: f64,
    pub rect_w: f64,
    pub rect_h: f64,
    /// Number of bit segments to display. Default: 24.
    pub num_bits: u8,
}

impl Default for BitMeterConfig {
    fn default() -> Self {
        Self {
            rect_x: 0.0,
            rect_y: 0.0,
            rect_w: 200.0,
            rect_h: 16.0,
            num_bits: 24,
        }
    }
}

// ── Painter ───────────────────────────────────────────────────────────────────

/// Bit depth utilization painter.
pub struct BitMeterPainter {
    state: Arc<BitDepthState>,
    config: BitMeterConfig,
}

impl BitMeterPainter {
    pub fn new(state: Arc<BitDepthState>, config: BitMeterConfig) -> Self {
        Self { state, config }
    }
}

impl SceneOverlay for BitMeterPainter {
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

        let bits_used = *self.state.bits_used.read() as u32;
        let num_bits = self.config.num_bits as u32;

        // Background
        scene.fill(
            Fill::NonZero,
            transform,
            COLOR_BG,
            None,
            &Rect::new(0.0, 0.0, w, h),
        );

        let gap = 1.5_f64;
        let total_gaps = gap * (num_bits as f64 + 1.0);
        let seg_w = (w - total_gaps) / num_bits as f64;

        for bit in 0..num_bits {
            let x = gap + bit as f64 * (seg_w + gap);
            let rect = Rect::new(x, gap, x + seg_w, h - gap);

            // A segment is "used" if its bit position is within bits_used.
            // bits_used = effective depth, counted from MSB.
            // bit 0 from MSB = always used if bits_used >= 1, etc.
            let is_used = (bit + 1) <= bits_used;

            let color = if is_used { COLOR_USED } else { COLOR_UNUSED };
            scene.fill(Fill::NonZero, transform, color, None, &rect);
        }
    }
}
