//! What frame's painter paints into.
//!
//! [`paint_primitives_into_scene_with`](super::paint_primitives_into_scene_with)
//! is written against [`PaintTarget`], not a renderer: the operations a
//! surface is drawn with — fills, strokes, layers and clips, blurred rects,
//! glyph runs — named and shaped as `vello::Scene`'s, so one painter serves
//! every output.
//!
//! - `vello::Scene` (always): frame's own renderer and the effects stage.
//! - A host's anyrender 0.11 scene (frame-live's `vectors`, feature
//!   `anyrender-0-11`): Blitz's, drawn by whatever renderer the host runs —
//!   vello-hybrid on the iOS simulator, which has no indirect execution for
//!   Vello's compute stages.

use kurbo::{Affine, Rect, Shape, Stroke, Vec2};
use peniko::{BlendMode, BrushRef, Color, Fill, FontData};

/// One run of glyphs, in one face, size and brush.
pub struct GlyphRun<'a> {
    pub font: &'a FontData,
    pub font_size: f32,
    pub normalized_coords: &'a [i16],
    pub brush: BrushRef<'a>,
    pub brush_alpha: f32,
    pub transform: Affine,
    pub glyph_transform: Option<Affine>,
    /// Synthetic bold: how far outlines grow, x and y (zero for none).
    pub embolden: Vec2,
    pub glyphs: &'a [vello::Glyph],
}

/// The drawing operations frame's painter uses (see the module docs).
pub trait PaintTarget {
    /// Called once before a surface is painted (vello's atlas workaround).
    fn begin(&mut self) {}

    fn fill<'b>(
        &mut self,
        style: Fill,
        transform: Affine,
        brush: impl Into<BrushRef<'b>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    );

    fn stroke<'b>(
        &mut self,
        style: &Stroke,
        transform: Affine,
        brush: impl Into<BrushRef<'b>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    );

    fn push_layer(&mut self, clip_style: Fill, blend: impl Into<BlendMode>, alpha: f32, transform: Affine, clip: &impl Shape);

    fn push_clip_layer(&mut self, clip_style: Fill, transform: Affine, clip: &impl Shape);

    fn push_luminance_mask_layer(&mut self, clip_style: Fill, alpha: f32, transform: Affine, clip: &impl Shape);

    fn pop_layer(&mut self);

    fn draw_blurred_rounded_rect(&mut self, transform: Affine, rect: Rect, brush: Color, radius: f64, std_dev: f64);

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_>);

    /// Draw `image` at its own size under `transform`.
    fn draw_image(&mut self, image: &peniko::ImageBrush, transform: Affine);

    /// Draw an SVG document (an image node's vector content) under
    /// `transform`. Whether it parsed and drew.
    fn draw_svg(&mut self, svg_text: &str, transform: Affine) -> bool;
}

impl PaintTarget for vello::Scene {
    fn begin(&mut self) {
        super::paint::keep_image_atlas(self);
    }

    fn fill<'b>(
        &mut self,
        style: Fill,
        transform: Affine,
        brush: impl Into<BrushRef<'b>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        vello::Scene::fill(self, style, transform, brush, brush_transform, shape);
    }

    fn stroke<'b>(
        &mut self,
        style: &Stroke,
        transform: Affine,
        brush: impl Into<BrushRef<'b>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        vello::Scene::stroke(self, style, transform, brush, brush_transform, shape);
    }

    fn push_layer(&mut self, clip_style: Fill, blend: impl Into<BlendMode>, alpha: f32, transform: Affine, clip: &impl Shape) {
        vello::Scene::push_layer(self, clip_style, blend, alpha, transform, clip);
    }

    fn push_clip_layer(&mut self, clip_style: Fill, transform: Affine, clip: &impl Shape) {
        vello::Scene::push_clip_layer(self, clip_style, transform, clip);
    }

    fn push_luminance_mask_layer(&mut self, clip_style: Fill, alpha: f32, transform: Affine, clip: &impl Shape) {
        vello::Scene::push_luminance_mask_layer(self, clip_style, alpha, transform, clip);
    }

    fn pop_layer(&mut self) {
        vello::Scene::pop_layer(self);
    }

    fn draw_blurred_rounded_rect(&mut self, transform: Affine, rect: Rect, brush: Color, radius: f64, std_dev: f64) {
        vello::Scene::draw_blurred_rounded_rect(self, transform, rect, brush, radius, std_dev);
    }

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_>) {
        let mut draw = self
            .draw_glyphs(run.font)
            .font_size(run.font_size)
            .hint(false)
            .normalized_coords(run.normalized_coords)
            .brush(run.brush.clone())
            .brush_alpha(run.brush_alpha)
            .transform(run.transform)
            .glyph_transform(run.glyph_transform);
        if run.embolden != Vec2::ZERO {
            draw = draw.font_embolden(vello::FontEmbolden::new(kurbo::Diagonal2::new(run.embolden.x, run.embolden.y)));
        }
        draw.draw(Fill::NonZero, run.glyphs.iter().copied());
    }

    fn draw_image(&mut self, image: &peniko::ImageBrush, transform: Affine) {
        vello::Scene::draw_image(self, image, transform);
    }

    fn draw_svg(&mut self, svg_text: &str, transform: Affine) -> bool {
        let Some(tree) = super::cache::cached_svg_tree(svg_text) else {
            return false;
        };
        let mut painter = anyrender_vello::VelloScenePainter::new(self);
        anyrender_svg::render_svg_tree(&mut painter, &tree, transform);
        true
    }
}
