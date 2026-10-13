//! Spectrum analyzer UI: the painters, and a Dioxus settings panel.
//!
//! The painters are portable (`anyrender` command lists, no plugin host);
//! the settings panel is a plugin editor's panel and needs nice-plug, so it
//! lives behind the `panel` feature. Anything that only draws the analyzer —
//! the EQ graph, in a plugin, on the desktop or in a browser — takes the
//! painters alone.

pub mod paint;
#[cfg(feature = "panel")]
pub mod settings_panel;

pub use paint::{paint_collisions, paint_spectrum_fill, paint_spectrum_line};
#[cfg(feature = "panel")]
pub use settings_panel::AnalyzerSettingsPanel;
