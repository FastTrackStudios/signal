//! Where a theme paints: the mixer's own scene, with its text in the
//! mixer's own font.
//!
//! A theme's text comes here as a run ([`TextRun`]) rather than as frame's
//! outlines of it, and is drawn the way `tcp::glyphs` draws every other
//! label in the window — the same face, the same advances, the same
//! glyph rendering — so a name in a themed strip is the name the painters
//! drew.

use anyrender::PaintScene;
use theme_runtime::{AnyRender, Canvas, TextAlign, TextRun};
use vello::kurbo::{Affine, BezPath, Stroke, Vec2};
use vello::peniko::{Brush, Fill};

use crate::text::Font;

/// How far below the top of a theme's text box the baseline sits, in
/// font sizes. A theme that wants a baseline at `b` puts the box's top
/// at `b - BASELINE * size`.
pub const BASELINE: f64 = 0.8;

/// The mixer's scene, as a theme canvas.
pub struct Host<'a, S: PaintScene> {
    pub scene: &'a mut S,
    pub font: &'a Font,
}

impl<S: PaintScene> Canvas for Host<'_, S> {
    fn fill(
        &mut self,
        transform: Affine,
        rule: Fill,
        brush: &Brush,
        brush_transform: Option<Affine>,
        shape: &BezPath,
    ) {
        AnyRender(&mut *self.scene).fill(transform, rule, brush, brush_transform, shape);
    }

    fn stroke(&mut self, transform: Affine, style: &Stroke, brush: &Brush, shape: &BezPath) {
        AnyRender(&mut *self.scene).stroke(transform, style, brush, shape);
    }

    fn push_clip(&mut self, transform: Affine, shape: &BezPath) {
        AnyRender(&mut *self.scene).push_clip(transform, shape);
    }

    fn push_opacity(&mut self, transform: Affine, alpha: f32, shape: &BezPath) {
        AnyRender(&mut *self.scene).push_opacity(transform, alpha, shape);
    }

    fn pop(&mut self) {
        AnyRender(&mut *self.scene).pop();
    }

    fn handles_text(&self) -> bool {
        true
    }

    fn text(&mut self, transform: Affine, run: &TextRun) -> bool {
        let size = crate::mcp::f64_to_f32(run.size);
        let glyphs = self.font.layout(&run.text, size);
        if glyphs.is_empty() {
            return true;
        }
        let width = self.font.width(&run.text, size);
        let left = match run.align {
            TextAlign::Left => run.left,
            TextAlign::Center => run.left + (run.width - width) / 2.0,
            TextAlign::Right => run.left + run.width - width,
        };
        self.scene.draw_glyphs(
            self.font.data(),
            size,
            true,
            &[],
            Vec2::ZERO,
            Fill::NonZero,
            run.color,
            1.0,
            transform * Affine::translate((left, BASELINE.mul_add(run.size, run.top))),
            None,
            glyphs.into_iter(),
        );
        true
    }
}
