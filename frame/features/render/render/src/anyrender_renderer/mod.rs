mod diagnostics;
pub mod effects_plan;
mod scene_collect;
pub mod transform;
mod types;

#[cfg(feature = "anyrender")]
mod cache;
#[cfg(feature = "anyrender")]
mod geometry;
#[cfg(feature = "anyrender")]
mod paint;
#[cfg(feature = "anyrender")]
mod soft;
#[cfg(feature = "anyrender")]
pub mod target;
#[cfg(feature = "svg_export")]
pub(crate) mod subpaths;

#[cfg(test)]
mod tests;
#[cfg(all(test, feature = "anyrender"))]
mod tests_authored;
#[cfg(all(test, feature = "anyrender"))]
mod tests_dense;
#[cfg(all(test, feature = "anyrender"))]
mod tests_images;

pub use diagnostics::collect_render_diagnostics;
pub use effects_plan::{flicker_level, EffectsPlan, EmissiveDraw, ShaderRegion};
pub use scene_collect::{
    build_scene_layout_boxes, build_scene_primitives, build_scene_primitives_and_effects, build_scene_primitives_with,
    text_font, CollectOptions,
};
#[cfg(feature = "svg_export")]
pub(crate) use scene_collect::fill_paint;
pub use transform::AffineTransform2D;
pub use types::{
    BlendMix, EffectKind, FillPaint, GradientStop, ImageScaleMode, LayoutBox, NodeEffect,
    PaintPrimitive, RenderDiagnostics, Rgba, stroke_gradient_box, StrokeAlign, StrokeCap, StrokeJoin, StrokeStyle,
    TextAlignment, TextCase, TextStyleRun,
};

#[cfg(feature = "anyrender")]
pub use paint::{paint_primitives_into_scene_with, paint_scene_into};
#[cfg(feature = "anyrender")]
pub use target::{GlyphRun, PaintTarget};
