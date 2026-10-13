//! `expression-editor-paint` — the expression editor as a recorded
//! `anyrender::Scene`, and the pointer and key logic that edits it.
//!
//! No DOM, no renderer, no window, no dioxus. Everything here is a pure
//! map from an [`expression_editor_core::Editor`] to a picture
//! ([`paint`]), or from a pointer event to a change in that editor
//! ([`interaction`]). Whatever anyrender backend a host has replays the
//! scene: Vello on a wgpu surface in the session window, the CPU
//! rasterizer under a WebView, a headless canvas in a test.
//!
//! It was carved out of `expression-editor-ui` so that a host which is
//! not a Dioxus tree — the session window, which drives Vello directly
//! from winit — can draw the roll and the strip at the frame rate the
//! GPU allows, instead of rasterizing every frame to a bitmap and
//! handing it to a web view as an image. The Dioxus crate still owns the
//! chrome around the roll (toolbar, inspector, panels) and re-exports
//! these modules under their old names.

pub mod canvas;
pub mod chrome;
pub mod cursor;
pub mod demo;
pub mod guitar;
pub mod interaction;
pub mod keys;
pub mod num;
pub mod paint;
pub mod scroll;
pub mod stack;
pub mod text;
pub mod theme;

pub use expression_editor_core as core;
pub use guitar::BendFlow;
pub use interaction::Drag;

/// The glyph drawn on a handle, as an SVG path.
///
/// Each mark says what the handle *does* rather than naming it: the
/// slopes are the slope they apply, fine pitch is a tick, formant a
/// bar, amplitude a dot, vibrato a wave. At fourteen pixels there is no
/// room for a word, and a shape is faster to read than one anyway.
/// `hollow` draws the amplitude handle as an empty circle, which is how
/// the manual signals that a drag will hit only the sibilants rather
/// than the whole note.
pub fn handle_mark(
    handle: expression_editor_core::Handle,
    cx: f64,
    cy: f64,
    r: f64,
    hollow: bool,
) -> String {
    use expression_editor_core::Handle as H;
    match handle {
        H::LeftSlope => format!(
            "M {:.1} {:.1} L {:.1} {:.1}",
            cx - r,
            cy + r * 0.5,
            cx + r,
            cy - r * 0.5
        ),
        H::RightSlope => format!(
            "M {:.1} {:.1} L {:.1} {:.1}",
            cx - r,
            cy - r * 0.5,
            cx + r,
            cy + r * 0.5
        ),
        H::FinePitch => format!(
            "M {:.1} {:.1} L {:.1} {:.1}",
            cx - r * 0.6,
            cy,
            cx + r * 0.6,
            cy
        ),
        H::Formant => format!(
            "M {:.1} {:.1} L {:.1} {:.1}",
            cx,
            cy - r * 0.7,
            cx,
            cy + r * 0.7
        ),
        H::Amplitude => {
            // A small circle, drawn as two arcs so it stays one path.
            // Larger when hollow, since an outline reads smaller than a
            // filled dot at this size.
            let d = if hollow { r * 0.58 } else { r * 0.42 };
            format!(
                "M {:.1} {:.1} a {:.1} {:.1} 0 1 0 {:.1} 0 a {:.1} {:.1} 0 1 0 {:.1} 0",
                cx - d,
                cy,
                d,
                d,
                d * 2.0,
                d,
                d,
                -d * 2.0
            )
        }
        H::Vibrato => format!(
            "M {:.1} {:.1} q {:.1} {:.1} {:.1} 0 q {:.1} {:.1} {:.1} 0",
            cx - r,
            cy,
            r * 0.25,
            -r * 0.9,
            r,
            r * 0.25,
            r * 0.9,
            r
        ),
        H::Pitch => String::new(),
    }
}
