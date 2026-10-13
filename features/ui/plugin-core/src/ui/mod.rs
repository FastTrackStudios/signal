//! UI module for FTS plugins — Blitz-compatible components and theme.
//!
//! All components use inline styles for reliable rendering in the
//! nice_plug_dioxus Blitz renderer. Import everything via the prelude:
//!
//! ```ignore
//! use crate::ui::prelude::*;
//! ```
//!
//! The shared component library lives in the `audiocore-gui` crate.
//! This module re-exports it alongside legacy components for convenience.

pub mod components;

/// Theme re-exported from `audiocore-gui`.
pub use crate::canvas::theme;

/// UI prelude — import this for all FTS plugin UI building blocks.
///
/// Includes everything from `crate::canvas::prelude` (Knob, ParamSlider,
/// LevelMeter, TransferCurve, etc.) plus legacy components (Toggle,
/// Section, etc.) that haven't been moved yet.
pub mod prelude {
    pub use super::components::*;
    pub use crate::canvas::prelude::*;
}
