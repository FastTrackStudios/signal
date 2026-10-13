//! Theming system for audio controls.
//!
//! Provides customizable styling through the `StyleSheet` trait and preset themes.

/// The arrange-view context — REAPER's palette-driven ruler/grid/item colours.
pub mod arrange;
pub mod context;
/// The envelope-control-panel context — REAPER's `envcp.*` vocabulary.
pub mod envcp;
/// The MCP strip context — REAPER's `mcp.*` element vocabulary as typed layouts.
pub mod mcp;
pub mod presets;
/// REAPER theme import (palette/params → [`Theme`]) — `reaper-theme` feature.
#[cfg(feature = "reaper-theme")]
pub mod reaper_import;
pub mod style;
pub mod svg_texture;
/// Token-based theme model (vector-first; Phase 1 of the themeable-UI plan).
pub mod theme;
/// The transport context — REAPER's `trans.*` element vocabulary.
pub mod trans;
/// WALTER layout primitives (8-value anchor coords, margins, fonts, params).
pub mod walter;

pub use arrange::ArrangeTheme;
pub use context::{ControlConfig, ThemeContext, ThemeProvider, use_theme, use_theme_signal};
pub use envcp::{EnvcpLayout, EnvcpSkin, EnvcpTheme};
pub use mcp::{
    ButtonSkin, ButtonStateSkin, KnobSkin, McpColors, McpCustom, McpLayout, McpSkin, McpTheme,
    NineSlice, SkinImage, StripState,
};
pub use style::{ControlState, ControlVariant, KnobStyle, SliderStyle, StyleSheet, XYPadStyle};
pub use svg_texture::SvgTexture;
pub use theme::{
    Color, ElementRole, FaderStyle, MeterStyle, Metrics, PanelStyle, StripStyle, Theme, ThemeState,
    ToggleKind, ToggleStyle, Tokens,
};
pub use trans::{TransLayout, TransSkin, TransTheme};
pub use walter::{ColorPair, Coord, FaderMode, FontSpec, Margin, Rect, ThemeParam};

#[cfg(feature = "reaper-theme")]
pub use reaper_import::theme_from_reaper;
