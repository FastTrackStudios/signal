//! Vintage hardware style theme.
//!
//! Inspired by classic analog hardware with warm, cream-colored
//! indicators and subtle shadows.

use crate::theming::style::{ControlState, KnobStyle, SliderStyle, StyleSheet, XYPadStyle};

/// Vintage hardware style theme.
///
/// Features warm colors reminiscent of classic analog equipment,
/// with cream indicators and subtle earthy tones.
#[derive(Debug, Clone, Copy, Default)]
pub struct VintageTheme;

impl VintageTheme {
    /// Cream/ivory indicator color.
    const CREAM: &'static str = "#f5e6c8";
    /// Darker cream for hover.
    const CREAM_DARK: &'static str = "#e8d4a8";
    /// Warm brown background.
    const BROWN: &'static str = "#3d3428";
    /// Medium brown.
    const BROWN_MED: &'static str = "#5a4d3a";
    /// Light tan.
    const TAN: &'static str = "#8b7355";
    /// Vintage green modulation.
    const MOD_GREEN: &'static str = "#7cb342";
}

impl StyleSheet for VintageTheme {
    fn knob(&self, state: ControlState, bipolar: bool) -> KnobStyle {
        let fill_color = match state {
            ControlState::Idle => Self::CREAM.to_string(),
            ControlState::Hovered => Self::CREAM_DARK.to_string(),
            ControlState::Dragging => Self::CREAM.to_string(),
            ControlState::Disabled => Self::TAN.to_string(),
        };

        KnobStyle {
            track_color: Self::BROWN.to_string(),
            fill_color,
            pointer_color: Self::CREAM.to_string(),
            modulation_color: Self::MOD_GREEN.to_string(),
            center_color: if bipolar {
                Some(Self::BROWN_MED.to_string())
            } else {
                None
            },
            stroke_width: 4.0,
            pointer_width: 2.5,
            svg_texture: None,
        }
    }

    fn slider(&self, state: ControlState, _bipolar: bool) -> SliderStyle {
        let fill_color = match state {
            ControlState::Idle => Self::CREAM.to_string(),
            ControlState::Hovered => Self::CREAM_DARK.to_string(),
            ControlState::Dragging => Self::CREAM.to_string(),
            ControlState::Disabled => Self::TAN.to_string(),
        };

        SliderStyle {
            track_color: Self::BROWN.to_string(),
            fill_color,
            thumb_color: Self::CREAM.to_string(),
            modulation_color: Self::MOD_GREEN.to_string(),
            track_height: 6.0,
            thumb_size: 16.0,
        }
    }

    fn xy_pad(&self, state: ControlState) -> XYPadStyle {
        let cursor_color = match state {
            ControlState::Disabled => Self::TAN.to_string(),
            _ => Self::CREAM.to_string(),
        };

        XYPadStyle {
            background_color: Self::BROWN.to_string(),
            grid_color: Self::BROWN_MED.to_string(),
            cursor_color,
            modulation_color: Self::MOD_GREEN.to_string(),
            show_grid: true,
            cursor_size: 12.0,
        }
    }

    fn modulation_color(&self) -> String {
        Self::MOD_GREEN.to_string()
    }
}
