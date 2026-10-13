//! Minimal theme using CSS variables.
//!
//! This is the default theme that works with Tailwind CSS and integrates
//! seamlessly with existing design systems.

use crate::theming::style::{ControlState, KnobStyle, SliderStyle, StyleSheet, XYPadStyle};

/// Minimal theme using CSS variables.
///
/// This theme uses CSS custom properties, making it compatible with
/// Tailwind CSS themes and easy to customize through CSS.
#[derive(Debug, Clone, Copy, Default)]
pub struct MinimalTheme;

impl StyleSheet for MinimalTheme {
    fn knob(&self, state: ControlState, bipolar: bool) -> KnobStyle {
        KnobStyle::css_vars(state, bipolar)
    }

    fn slider(&self, state: ControlState, bipolar: bool) -> SliderStyle {
        let _ = bipolar; // Could use for center-origin styling
        SliderStyle::css_vars(state)
    }

    fn xy_pad(&self, state: ControlState) -> XYPadStyle {
        XYPadStyle::css_vars(state)
    }
}
