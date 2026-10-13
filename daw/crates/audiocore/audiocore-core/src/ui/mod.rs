//! UI module for FTS plugins — Blitz-compatible components and theme.
//!
//! All components use inline styles for reliable rendering in the
//! nice_plug_dioxus Blitz renderer. Import everything via the prelude:
//!
//! ```ignore
//! use audiocore_core::ui::prelude::*;
//! ```
//!
//! The shared component library lives in the `audiocore-gui` crate.
//! This module re-exports it alongside legacy components for convenience.

pub mod components;

/// Theme re-exported from `audiocore-gui`.
pub use audiocore_gui::theme;

/// UI prelude — import this for all FTS plugin UI building blocks.
///
/// Includes everything from `audiocore_gui::prelude` (Knob, ParamSlider,
/// LevelMeter, TransferCurve, etc.) plus legacy components (Toggle,
/// Section, etc.) that haven't been moved yet.
pub mod prelude {
    pub use super::components::*;
    pub use audiocore_gui::prelude::*;
}
