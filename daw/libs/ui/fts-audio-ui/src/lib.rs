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
pub mod shell;
pub mod theme;
#[cfg(not(target_arch = "wasm32"))]
pub mod widget;

pub mod prelude;

pub use param::ParamHandle;
pub use shell::{EditorForm, EDITOR_FORMS};
