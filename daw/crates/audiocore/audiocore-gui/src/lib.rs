//! `audiocore-gui` — Shared Dioxus audio GUI components for FTS plugins.
//!
//! All components use inline styles for reliable rendering in the
//! nice_plug_dioxus Blitz renderer. Import everything via the prelude:
//!
//! ```ignore
//! use audiocore_gui::prelude::*;
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
    pub use crate::controls::*;
    pub use crate::drag::{begin_drag, DragProvider, DragState, TextEditState};
    pub use crate::layout::*;
    pub use crate::meters::*;
    pub use crate::theme;
    pub use crate::theme::{use_init_theme, use_theme, Theme, ThemeProvider, ThemeVariant};
    pub use crate::viz::*;
}
