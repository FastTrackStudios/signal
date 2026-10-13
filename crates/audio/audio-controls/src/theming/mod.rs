//! Theming system for audio controls.
//!
//! Provides customizable styling through the `StyleSheet` trait and preset themes.

pub mod context;
pub mod presets;
pub mod style;
pub mod svg_texture;

pub use context::{ControlConfig, ThemeContext, ThemeProvider, use_theme};
pub use style::{ControlState, ControlVariant, KnobStyle, SliderStyle, StyleSheet, XYPadStyle};
pub use svg_texture::SvgTexture;
