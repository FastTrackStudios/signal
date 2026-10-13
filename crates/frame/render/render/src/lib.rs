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
    CollectOptions, EffectsPlan, LayoutBox, PaintPrimitive, RenderDiagnostics, Rgba,
    build_scene_layout_boxes, build_scene_primitives, build_scene_primitives_and_effects,
    build_scene_primitives_with, collect_render_diagnostics,
};
#[cfg(feature = "anyrender")]
pub use anyrender_renderer::{GlyphRun, PaintTarget};
#[cfg(feature = "anyrender")]
pub use anyrender_renderer::{paint_primitives_into_scene_with, paint_scene_into};

pub mod fonts;
pub use fonts::{FontBook, FontSpec, book_for_scene, text_advance};

#[cfg(feature = "anyrender")]
pub mod fx;
#[cfg(feature = "anyrender")]
pub mod offscreen;
