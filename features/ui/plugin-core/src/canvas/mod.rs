//! `audiocore-gui` — Shared Dioxus audio GUI components for FTS plugins.
//!
//! All components use inline styles for reliable rendering in the
//! nice_plug_dioxus Blitz renderer. Import everything via the prelude:
//!
//! ```ignore
//! use crate::canvas::prelude::*;
//! ```
//!
//! # Modules
//!
//! - [`controls`] — Knob, Slider, Toggle, SegmentButton, XYPad
//! - [`meters`] — LevelMeter, GrMeter, SpectrumAnalyzer
//! - [`viz`] — WaveformDisplay, PeakWaveform, TransferCurve
//! - [`layout`] — Section, Header, StatusBar, ActionButton
//! - [`theme`] — Color constants and CSS reset

pub mod controls;
pub mod drag;
pub mod layout;
pub mod meters;
pub mod theme;
pub mod viz;

/// Prelude — import this for all audio GUI building blocks.
pub mod prelude {
    pub use crate::canvas::controls::*;
    pub use crate::canvas::drag::{DragProvider, DragState, TextEditState, begin_drag};
    pub use crate::canvas::layout::*;
    pub use crate::canvas::meters::*;
    pub use crate::canvas::theme;
    pub use crate::canvas::theme::{Theme, ThemeProvider, ThemeVariant, use_init_theme, use_theme};
    pub use crate::canvas::viz::*;
}
