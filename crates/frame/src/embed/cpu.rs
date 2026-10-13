//! The CPU path: rasterise an embed with `vello_cpu`.
//!
//! What REAPER's LICE bitmap and headless tests get. No device, no
//! readback: a strip-sized embed rasterises in well under a millisecond
//! (see `tests/harness.rs` for numbers), which is cheaper than a GPU round
//! trip at these sizes before the first byte is copied back.

use kurbo::{Affine, BezPath, Rect, Stroke};
use peniko::{Brush, Color, Fill};
use vello_cpu::{RenderContext, RenderMode, Resources};

use crate::embed::canvas::Canvas;

/// A [`Canvas`] over a `vello_cpu` render context.
pub struct CpuCanvas {
    ctx: RenderContext,
}

impl CpuCanvas {
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            ctx: RenderContext::new(width.max(1), height.max(1)),
        }
    }

    fn brush(&mut self, brush: &Brush, brush_transform: Option<Affine>) {
        match brush {
            Brush::Solid(c) => self.ctx.set_paint(*c),
            Brush::Gradient(g) => self.ctx.set_paint(g.clone()),
            Brush::Image(_) => self.ctx.set_paint(Color::TRANSPARENT),
        }
        match brush_transform {
            Some(t) => self.ctx.set_paint_transform(t),
            None => self.ctx.reset_paint_transform(),
        }
    }
}

impl Canvas for CpuCanvas {
    fn fill(
        &mut self,
        transform: Affine,
        rule: Fill,
        brush: &Brush,
        brush_transform: Option<Affine>,
        shape: &BezPath,
    ) {
        self.ctx.set_transform(transform);
        self.ctx.set_fill_rule(rule);
        self.brush(brush, brush_transform);
        self.ctx.fill_path(shape);
    }

    fn stroke(&mut self, transform: Affine, style: &Stroke, brush: &Brush, shape: &BezPath) {
        self.ctx.set_transform(transform);
        self.brush(brush, None);
        self.ctx.set_stroke(style.clone());
        self.ctx.stroke_path(shape);
    }

    fn push_clip(&mut self, transform: Affine, shape: &BezPath) {
        self.ctx.set_transform(transform);
        self.ctx.set_fill_rule(Fill::NonZero);
        self.ctx.push_clip_layer(shape);
    }

    fn push_opacity(&mut self, transform: Affine, alpha: f32, shape: &BezPath) {
        self.ctx.set_transform(transform);
        self.ctx.set_fill_rule(Fill::NonZero);
        self.ctx
            .push_layer(Some(shape), None, Some(alpha), None, None);
    }

    fn pop(&mut self) {
        self.ctx.pop_layer();
    }
}

/// Rasterised pixels: premultiplied RGBA8, row-major, tightly packed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Pixels {
    /// Pixel `(x, y)` as `[r, g, b, a]`.
    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    /// Copy into a LICE-style bitmap: `0xAARRGGBB` words (blue in the low
    /// byte), `row_span` words per row, bottom-up when `flipped`. The
    /// pixels are opaque when painted over an opaque background, where
    /// premultiplied and straight alpha agree.
    pub fn write_bgra(&self, bits: &mut [u32], row_span: usize, flipped: bool) {
        let (w, h) = (self.width as usize, self.height as usize);
        for y in 0..h {
            let dy = if flipped { h - 1 - y } else { y };
            let Some(row) = bits.get_mut(dy * row_span..dy * row_span + w) else {
                continue;
            };
            let src = &self.rgba[y * w * 4..(y + 1) * w * 4];
            for (d, s) in row.iter_mut().zip(src.as_chunks::<4>().0.iter()) {
                *d = u32::from(s[2])
                    | (u32::from(s[1]) << 8)
                    | (u32::from(s[0]) << 16)
                    | (u32::from(s[3]) << 24);
            }
        }
    }

    /// Straight-alpha RGBA8 (for PNG).
    #[must_use]
    pub fn unpremultiplied(&self) -> Vec<u8> {
        self.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                let a = u32::from(p[3]);
                if a == 0 || a == 255 {
                    [p[0], p[1], p[2], p[3]]
                } else {
                    let un = |c: u8| ((u32::from(c) * 255 + a / 2) / a).min(255) as u8;
                    [un(p[0]), un(p[1]), un(p[2]), p[3]]
                }
            })
            .collect()
    }

    /// Write a PNG.
    ///
    /// # Errors
    /// When the file cannot be written.
    pub fn save_png(&self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        image::save_buffer(
            path,
            &self.unpremultiplied(),
            self.width,
            self.height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())
    }

    /// Several images side by side, top-aligned, `gap` px apart on `bg`.
    #[must_use]
    pub fn row(images: &[Self], gap: u32, bg: [u8; 4]) -> Self {
        let width = images.iter().map(|i| i.width).sum::<u32>()
            + gap * images.len().saturating_sub(1) as u32;
        let height = images.iter().map(|i| i.height).max().unwrap_or(0);
        let mut rgba: Vec<u8> = bg
            .iter()
            .copied()
            .cycle()
            .take((width * height * 4) as usize)
            .collect();
        let mut x0 = 0;
        for img in images {
            for y in 0..img.height {
                let src =
                    &img.rgba[(y * img.width * 4) as usize..((y + 1) * img.width * 4) as usize];
                let at = ((y * width + x0) * 4) as usize;
                rgba[at..at + src.len()].copy_from_slice(src);
            }
            x0 += img.width + gap;
        }
        Self {
            width,
            height,
            rgba,
        }
    }

    /// Stack images top to bottom, left-aligned.
    #[must_use]
    pub fn column(images: &[Self], gap: u32, bg: [u8; 4]) -> Self {
        let width = images.iter().map(|i| i.width).max().unwrap_or(0);
        let height = images.iter().map(|i| i.height).sum::<u32>()
            + gap * images.len().saturating_sub(1) as u32;
        let mut rgba: Vec<u8> = bg
            .iter()
            .copied()
            .cycle()
            .take((width * height * 4) as usize)
            .collect();
        let mut y0 = 0;
        for img in images {
            for y in 0..img.height {
                let src =
                    &img.rgba[(y * img.width * 4) as usize..((y + 1) * img.width * 4) as usize];
                let at = (((y0 + y) * width) * 4) as usize;
                rgba[at..at + src.len()].copy_from_slice(src);
            }
            y0 += img.height + gap;
        }
        Self {
            width,
            height,
            rgba,
        }
    }
}

/// The strip ground behind an embed in the harness (session's strip).
pub const STRIP_GROUND: Color = Color::from_rgb8(0x18, 0x18, 0x1b);

/// Render `embed` (both passes) at its size × `scale`, at `t` ms.
pub fn snapshot(
    raster: &mut Rasterizer,
    embed: &mut crate::embed::Embed,
    scale: f64,
    t: f64,
) -> Pixels {
    let (w, h) = embed.size();
    raster.render(
        (w * scale).round() as u32,
        (h * scale).round() as u32,
        scale,
        STRIP_GROUND,
        |c, at| embed.paint(c, at, t),
    )
}

/// A reusable CPU rasteriser (keeps its resources between frames).
#[derive(Default)]
pub struct Rasterizer {
    resources: Resources,
}

impl std::fmt::Debug for Rasterizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Rasterizer")
    }
}

impl Rasterizer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rasterise `draw` into a `width × height` device-pixel image, the
    /// drawing scaled by `scale` (HiDPI: logical → device px), over `bg`.
    pub fn render(
        &mut self,
        width: u32,
        height: u32,
        scale: f64,
        bg: Color,
        draw: impl FnOnce(&mut CpuCanvas, Affine),
    ) -> Pixels {
        let (w, h) = (
            width.clamp(1, u32::from(u16::MAX)) as u16,
            height.clamp(1, u32::from(u16::MAX)) as u16,
        );
        let mut canvas = CpuCanvas::new(w, h);
        if bg.components[3] > 0.0 {
            canvas.ctx.set_transform(Affine::IDENTITY);
            canvas.ctx.set_paint(bg);
            canvas
                .ctx
                .fill_rect(&Rect::new(0.0, 0.0, f64::from(w), f64::from(h)));
        }
        draw(&mut canvas, Affine::scale(scale));
        canvas.ctx.flush();
        let mut rgba = vec![0u8; usize::from(w) * usize::from(h) * 4];
        canvas.ctx.render_to_buffer(
            &mut self.resources,
            &mut rgba,
            w,
            h,
            RenderMode::OptimizeSpeed,
        );
        Pixels {
            width: u32::from(w),
            height: u32::from(h),
            rgba,
        }
    }
}
