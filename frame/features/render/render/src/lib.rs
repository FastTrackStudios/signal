//! Frame renderer.
//!
//! One source, two backends:
//! - paint primitives (`anyrender_renderer`) painted into a vello scene
//!   (`anyrender` feature)
//! - SVG export over the same primitives (`svg_export` feature)

pub mod anyrender_renderer;
#[cfg(feature = "svg_export")]
pub mod svg;

pub use anyrender_renderer::{
    build_scene_layout_boxes, build_scene_primitives, build_scene_primitives_and_effects, build_scene_primitives_with,
    collect_render_diagnostics, CollectOptions, EffectsPlan, LayoutBox, PaintPrimitive, RenderDiagnostics, Rgba,
};
#[cfg(feature = "anyrender")]
pub use anyrender_renderer::{paint_primitives_into_scene_with, paint_scene_into};
#[cfg(feature = "anyrender")]
pub use anyrender_renderer::{GlyphRun, PaintTarget};

pub mod fonts;
pub use fonts::{book_for_scene, text_advance, FontBook, FontSpec};

#[cfg(feature = "anyrender")]
pub mod fx;
#[cfg(feature = "anyrender")]
pub mod offscreen;
