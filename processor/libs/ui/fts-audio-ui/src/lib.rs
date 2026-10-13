//! FastTrack Studio audio widget kit.
//!
//! Dioxus port of [`iced_audio`](https://github.com/iced-rs/iced_audio): rotary
//! knobs, horizontal/vertical sliders, XY pads, modulation range inputs, ramps,
//! and audio meters. Provider-agnostic — widgets bind to a [`ParamHandle`] that
//! consumers wire to whatever param system they use (nih_plug, vizia, plain
//! signals, etc).
//!
//! Companion to [`architect-ui`](../architect_ui) which provides the general-purpose
//! shadcn-style design system. Audio plugin UIs typically depend on both.
//!
//! # Quick start
//! ```rust,ignore
//! use fts_audio_ui::prelude::*;
//! ```

/// Whether painted visualisers should move (the host says; see [`animate::Animate`]).
pub mod animate;
pub mod axis;
pub mod color;
pub mod controls;
pub mod drag;
pub mod gesture;
pub mod hardware;
pub mod marks;
pub mod meters;
pub mod paint;
pub mod param;
/// A painted panel on a `<canvas>` — the browser's answer to the custom
/// widget in [`widget`], replaying the same scenes.
#[cfg(target_arch = "wasm32")]
pub mod scene_canvas;
/// A WGSL fragment shader composited into a panel, with the contract for
/// falling back to vectors when a renderer will not take a texture.
#[cfg(feature = "shader")]
pub mod shader;
pub mod shell;
pub mod theme;
#[cfg(not(target_arch = "wasm32"))]
pub mod widget;

pub mod prelude;

pub use param::ParamHandle;
pub use shell::{EditorForm, EDITOR_FORMS};
