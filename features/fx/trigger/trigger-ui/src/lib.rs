//! Trigger GUI — Dioxus components for the FTS Trigger plugin.
//!
//! Hosts the trigger-specific Dioxus root component, the nice_plug parameter
//! tree, and the bridging glue from `nice_plug` parameters to
//! [`fts_audio_ui`] widgets. General-purpose widgets (knobs, toggles,
//! segmented controls, drag provider) come from `fts_audio_ui`; layout
//! primitives from `architect_ui`.
//!
//! - [`control_view`]: the plugin editor — analysis waveform + param surface
//! - [`trigger_waveform`]: the scrolling-peaks display with the draggable
//!   threshold line and per-hit markers (the port of the legacy
//!   FTS-Trigger `TriggerWaveform`)
//! - [`trigger_waveform_svg`]: the portable path/coordinate math
//! - [`params`]: nice_plug parameter tree + shared UI state
//! - [`param_adapter`]: nice_plug `ParamPtr` → `fts_audio_ui::ParamHandle`

// ── Portable core (no plugin framework; compiles for wasm) ──
pub mod trigger_waveform_svg;

// ── The Blitz/vello plugin editor ──
#[cfg(feature = "native")]
pub mod control_view;
#[cfg(feature = "native")]
pub use fts_plug_ui::param_adapter;
#[cfg(feature = "native")]
pub mod params;
#[cfg(feature = "native")]
pub mod trigger_waveform;
