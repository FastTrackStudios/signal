//! Control graphics as [`anyrender::Scene`]s (spec `fx.control.painted`).
//!
//! This is *only* the drawing: no DOM, no renderer, no dioxus. A scene is a
//! plain recording of draw commands, so everything here compiles anywhere
//! the geometry does — including wasm — and whatever anyrender backend is
//! present replays it. The native seam that puts a scene on screen is
//! [`crate::widget`].
//!
//! Why not inline `<svg>` (what the kit used to do): Blitz paints an inline
//! svg as a replaced element with a hardcoded `object-fit: contain`, so the
//! drawing is scaled by (element box / declared size) and the pointer
//! mapping with it, and every value change re-parses markup into a usvg tree.
//! A scene has neither problem.
//!
//! Coordinates are element space: `(0, 0)` is the top-left of the control's
//! own box, in CSS pixels.

pub mod knob;

use peniko::Color;

/// Parse a CSS colour (`#rrggbb`, `rgb(...)`, a name) into a paint colour.
///
/// Falls back to transparent rather than panicking: a mistyped theme colour
/// should cost a shape, not the window.
pub fn color(css: &str) -> Color {
    peniko::color::parse_color(css.trim())
        .map(|c| c.to_alpha_color())
        .unwrap_or(Color::TRANSPARENT)
}

/// `color` with its alpha replaced.
pub fn with_alpha(c: Color, alpha: f32) -> Color {
    c.with_alpha(alpha)
}
