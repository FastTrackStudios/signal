//! FTS Limiter GUI — Dioxus components for the `limiter-plugin` editor.
//!
//! Built on [`fts_plug_ui`]'s shared chrome, so this crate carries only what
//! is limiter-specific:
//!
//! - [`params`]: the nice_plug parameter tree + the audio→UI metering state
//! - [`gr_trace_svg`]: portable geometry for the gain-reduction trace
//! - [`gr_trace`]: the Dioxus component that renders it
//! - [`control_view`]: the editor root the plugin shell embeds

pub mod control_view;
pub mod gr_trace;
pub mod gr_trace_svg;
pub mod params;
