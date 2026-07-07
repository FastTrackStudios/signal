//! Spectrum analyzer UI: vello painters and a Dioxus settings panel.

pub mod paint;
pub mod settings_panel;

pub use paint::{paint_collisions, paint_spectrum_fill, paint_spectrum_line};
pub use settings_panel::AnalyzerSettingsPanel;
