//! A host's anyrender 0.11 scene as frame's paint target — Blitz's, drawn
//! by whatever renderer the host runs: vello-hybrid on the iOS simulator,
//! whose Metal has no indirect execution for Vello's compute stages. What
//! that API has no word for is approximated: a luminance mask draws its
//! content unmasked, and clips are non-zero.
//!
//! Here rather than in `render`, under the crates' own names: `render` also
//! links frame's anyrender (git main), and a renamed dependency that a host
//! redirects with `[patch]` (signal's, to the blitz fork) breaks dx's
//! dependency scan.

use kurbo::{Affine, Rect, Shape, Stroke};
use peniko::{BlendMode, BrushRef, Color, Fill};

use crate::render::{GlyphRun, PaintTarget};

/// An anyrender 0.11 `PaintScene` as a [`PaintTarget`].
pub struct AnyRender011<'s, S: anyrender_0_11::PaintScene>(pub &'s mut S);

impl<S: anyrender_0_11::PaintScene> PaintTarget for AnyRender011<'_, S> {
    fn fill<'b>(
        &mut self,
        style: Fill,
        transform: Affine,
        brush: impl Into<BrushRef<'b>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        let brush: BrushRef<'b> = brush.into();
        // An image's own opacity goes on a layer: vello-hybrid cannot apply
        // opacity to an image command (vello_common panics).
        if let BrushRef::Image(image) = &brush
            && image.sampler.alpha < 1.0
        {
            let mut opaque = image.clone();
            opaque.sampler.alpha = 1.0;
            self.0.push_layer(
                BlendMode::default(),
                image.sampler.alpha,
                transform,
                shape,
                None,
                None,
            );
            self.0.fill(
                style,
                transform,
                anyrender_0_11::PaintRef::Image(opaque),
                brush_transform,
                shape,
            );
            self.0.pop_layer();
            return;
        }
        self.0.fill(
            style,
            transform,
            anyrender_0_11::PaintRef::from(brush),
            brush_transform,
            shape,
        );
    }

    fn stroke<'b>(
        &mut self,
        style: &Stroke,
        transform: Affine,
        brush: impl Into<BrushRef<'b>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        let brush: BrushRef<'b> = brush.into();
        self.0.stroke(
            style,
            transform,
            anyrender_0_11::PaintRef::from(brush),
            brush_transform,
            shape,
        );
    }

    fn push_layer(
        &mut self,
        _clip_style: Fill,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        self.0.push_layer(blend, alpha, transform, clip, None, None);
    }

    fn push_clip_layer(&mut self, _clip_style: Fill, transform: Affine, clip: &impl Shape) {
        self.0.push_clip_layer(transform, clip);
    }

    fn push_luminance_mask_layer(
        &mut self,
        _clip_style: Fill,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        // No luminance masks here: the mask's content draws unmasked.
        self.0
            .push_layer(BlendMode::default(), alpha, transform, clip, None, None);
    }

    fn pop_layer(&mut self) {
        self.0.pop_layer();
    }

    fn draw_blurred_rounded_rect(
        &mut self,
        transform: Affine,
        rect: Rect,
        brush: Color,
        radius: f64,
        std_dev: f64,
    ) {
        self.0
            .draw_box_shadow(transform, rect, brush, radius, std_dev);
    }

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_>) {
        let glyphs = run.glyphs.iter().map(|g| anyrender_0_11::Glyph {
            id: g.id,
            x: g.x,
            y: g.y,
        });
        self.0.draw_glyphs(
            run.font,
            run.font_size,
            false,
            run.normalized_coords,
            run.embolden,
            Fill::NonZero,
            anyrender_0_11::PaintRef::from(run.brush.clone()),
            run.brush_alpha,
            run.transform,
            run.glyph_transform,
            glyphs,
        );
    }

    fn draw_image(&mut self, image: &peniko::ImageBrush, transform: Affine) {
        let size = Rect::new(
            0.0,
            0.0,
            f64::from(image.image.width),
            f64::from(image.image.height),
        );
        PaintTarget::fill(self, Fill::NonZero, transform, image, None, &size);
    }

    fn draw_svg(&mut self, svg_text: &str, transform: Affine) -> bool {
        let Some(tree) = svg_tree(svg_text) else {
            return false;
        };
        anyrender_svg_0_12::render_svg_tree(&mut *self.0, &tree, transform);
        true
    }
}

/// SVG trees parsed by anyrender_svg 0.12's usvg, by text — frame's own
/// cache holds the other usvg's.
fn svg_tree(svg_text: &str) -> Option<std::sync::Arc<anyrender_svg_0_12::usvg::Tree>> {
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::{Arc, LazyLock, Mutex};
    static TREES: LazyLock<Mutex<HashMap<u64, Option<Arc<anyrender_svg_0_12::usvg::Tree>>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    svg_text.hash(&mut hasher);
    let key = hasher.finish();
    let mut trees = TREES.lock().ok()?;
    trees
        .entry(key)
        .or_insert_with(|| {
            anyrender_svg_0_12::usvg::Tree::from_str(
                svg_text,
                &anyrender_svg_0_12::usvg::Options::default(),
            )
            .ok()
            .map(Arc::new)
        })
        .clone()
}
