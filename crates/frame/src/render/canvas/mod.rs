//! frame's paint primitives, painted into whatever a host draws with.
//!
//! - [`canvas`]: the [`Canvas`] seam — an anyrender 0.11 `PaintScene`
//!   (feature `anyrender-0-11`, session's mixer), frame's replayable
//!   [`Recording`], or anything else with the same handful of calls.
//! - [`paint`]: `render`'s [`PaintPrimitive`](crate::render::PaintPrimitive)s →
//!   a canvas.
//! - [`text`]: the fonts text is painted (as outlines) with.
//!
//! Shared by `frame-embed` (surfaces in a host's strip) and the theme
//! runtime (a host's own views drawn from documents).

pub mod canvas;
pub mod paint;
pub mod text;

#[cfg(feature = "anyrender-0-11")]
pub use canvas::AnyRender;
pub use canvas::{Canvas, CanvasExt, Cmd, Recording, Tally, TextAlign, TextRun};
pub use paint::{node_of, paint_filtered, paint_primitives};
pub use text::Font;
