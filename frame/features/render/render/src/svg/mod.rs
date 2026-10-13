#[cfg(feature = "svg_export")]
mod cli;
#[cfg(feature = "svg_export")]
mod context;
#[cfg(feature = "svg_export")]
mod effects;
#[cfg(feature = "svg_export")]
mod normalize;
#[cfg(feature = "svg_export")]
mod render;
#[cfg(feature = "svg_export")]
mod scene_source;
#[cfg(feature = "svg_export")]
mod text;

#[cfg(feature = "svg_export")]
pub use cli::run_cli;
#[cfg(feature = "svg_export")]
pub use text::text_element;
