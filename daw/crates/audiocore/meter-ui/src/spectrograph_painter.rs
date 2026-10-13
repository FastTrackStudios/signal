//! Spectrograph (waterfall) painter.
//!
//! Renders a time-scrolling waterfall display of FFT spectrum frames.
//! Each column represents one spectrum snapshot; time scrolls left.
//! The color encodes magnitude using a Viridis-like colormap.
//!
//! The ring buffer of frames is stored in the painter itself (not in shared
//! DSP state) since it is purely a display artifact.

use std::sync::Arc;

use meter_dsp::spectrum::SpectrumState;
use nice_plug_dioxus::prelude::vello::kurbo::{Affine, Rect};
use nice_plug_dioxus::prelude::vello::peniko::{Color, Fill};
use nice_plug_dioxus::prelude::vello::Scene;
use nice_plug_dioxus::prelude::SceneOverlay;

// ── Colormap ──────────────────────────────────────────────────────────────────

/// Map a normalized value [0, 1] to a Viridis-like RGBA color.
///
/// 0 = silence (dark purple), 1 = loud (bright yellow).
fn viridis(t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    // Piecewise linear approximation of the Viridis colormap.
    // Anchors: 0→dark purple, 0.25→blue, 0.5→teal, 0.75→green-yellow, 1→yellow
    let stops: &[(f64, u8, u8, u8)] = &[
        (0.00, 68, 1, 84),
        (0.25, 59, 82, 139),
        (0.50, 33, 145, 140),
        (0.75, 94, 201, 98),
        (1.00, 253, 231, 37),
    ];

    let i = stops
        .partition_point(|&(stop_t, _, _, _)| stop_t < t)
        .min(stops.len() - 1);
    let (t0, r0, g0, b0) = stops[i.saturating_sub(1)];
    let (t1, r1, g1, b1) = stops[i];

    let seg = if (t1 - t0).abs() < 1e-9 {
        0.0
    } else {
        (t - t0) / (t1 - t0)
    };
    let r = lerp(r0 as f64, r1 as f64, seg) as u8;
    let g = lerp(g0 as f64, g1 as f64, seg) as u8;
    let b = lerp(b0 as f64, b1 as f64, seg) as u8;

    Color::from_rgb8(r, g, b)
}

#[inline]
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

// ── Config ────────────────────────────────────────────────────────────────────

/// Configuration for [`SpectrographPainter`].
#[derive(Clone)]
pub struct SpectrographConfig {
    /// Minimum displayed frequency (Hz).
    pub min_freq: f64,
    /// Maximum displayed frequency (Hz).
    pub max_freq: f64,
    /// Minimum level mapped to the colormap (dB). Typically −90.
    pub min_db: f64,
    /// Maximum level mapped to the colormap (dB). Typically 0.
    pub max_db: f64,
    /// Maximum number of time frames to keep (columns).
    pub max_frames: usize,
    pub rect_x: f64,
    pub rect_y: f64,
    pub rect_w: f64,
    pub rect_h: f64,
}

impl Default for SpectrographConfig {
    fn default() -> Self {
        Self {
            min_freq: 20.0,
            max_freq: 20_000.0,
            min_db: -90.0,
            max_db: 0.0,
            max_frames: 200,
            rect_x: 0.0,
            rect_y: 0.0,
            rect_w: 400.0,
            rect_h: 150.0,
        }
    }
}

// ── Painter ───────────────────────────────────────────────────────────────────

/// Spectrograph (waterfall) painter.
///
/// Each call to `paint` pulls the latest spectrum from `SpectrumState`,
/// appends it to an internal ring buffer, then renders all frames as
/// color-coded columns scrolling left.
pub struct SpectrographPainter {
    state: Arc<SpectrumState>,
    config: SpectrographConfig,
    /// Ring buffer of spectrum frames (oldest first).
    /// Each frame is a snapshot of `bins_db` at the time of capture.
    frames: std::collections::VecDeque<Vec<f32>>,
    /// Last snapshot we stored (reserved for dedup — currently unused).
    #[allow(dead_code)]
    last_snapshot_len: usize,
}

impl SpectrographPainter {
    pub fn new(state: Arc<SpectrumState>, config: SpectrographConfig) -> Self {
        let max_frames = config.max_frames;
        Self {
            state,
            config,
            frames: std::collections::VecDeque::with_capacity(max_frames + 1),
            last_snapshot_len: 0,
        }
    }
}

impl SceneOverlay for SpectrographPainter {
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

        scene.fill(
            Fill::NonZero,
            transform,
            Color::from_rgb8(14, 14, 16),
            None,
            &Rect::new(0.0, 0.0, w, h),
        );

        // Snapshot the current spectrum bins
        {
            let bins = self.state.bins_db.read();
            if !bins.is_empty() {
                self.frames.push_back(bins.clone());
                while self.frames.len() > self.config.max_frames {
                    self.frames.pop_front();
                }
            }
        }

        let n_frames = self.frames.len();
        if n_frames == 0 {
            return;
        }

        let fs = self
            .state
            .sample_rate
            .load(std::sync::atomic::Ordering::Relaxed) as f64;
        let fft_sz = self
            .state
            .fft_size
            .load(std::sync::atomic::Ordering::Relaxed);
        let n_bins = fft_sz / 2 + 1;
        let cfg = &self.config;
        let db_range = cfg.max_db - cfg.min_db;

        // Column width per frame
        let col_w = w / n_frames as f64;

        // Log-frequency row mapping: bin → y pixel (top = high freq)
        let log_min = cfg.min_freq.log10();
        let log_max = cfg.max_freq.log10();
        let bin_to_y = |bin: usize| -> f64 {
            let freq = (bin as f64 * fs / fft_sz as f64).max(1.0);
            let norm = (freq.log10() - log_min) / (log_max - log_min);
            (1.0 - norm.clamp(0.0, 1.0)) * h
        };

        for (frame_idx, frame) in self.frames.iter().enumerate() {
            let col_x = frame_idx as f64 * col_w;

            // Render each freq row as a colored rect segment
            for (bin, &val) in frame.iter().enumerate().take(n_bins.min(frame.len())) {
                let db_val = val as f64;
                let t = ((db_val - cfg.min_db) / db_range).clamp(0.0, 1.0);
                let color = viridis(t);

                let y_top = bin_to_y(bin + 1);
                let y_bot = bin_to_y(bin);
                if y_bot <= y_top {
                    continue;
                }

                let rect = Rect::new(col_x, y_top, col_x + col_w, y_bot);
                scene.fill(Fill::NonZero, transform, color, None, &rect);
            }
        }
    }
}
