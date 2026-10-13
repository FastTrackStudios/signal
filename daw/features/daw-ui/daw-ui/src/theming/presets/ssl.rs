//! SSL console style theme.
//!
//! Inspired by SSL (Solid State Logic) mixing consoles, featuring
//! orange/amber indicators on dark backgrounds.

use crate::theming::style::{ControlState, KnobStyle, SliderStyle, StyleSheet, XYPadStyle};

/// SSL console style theme.
///
/// Features the iconic SSL orange indicators on dark backgrounds,
/// reminiscent of classic mixing consoles.
#[derive(Debug, Clone, Copy, Default)]
pub struct SSLTheme;

impl SSLTheme {
    /// SSL orange color.
    const ORANGE: &'static str = "#ff6b00";
    /// SSL orange hover.
    const ORANGE_HOVER: &'static str = "#ff8533";
    /// Dark background.
    const DARK_BG: &'static str = "#1a1a1a";
    /// Medium gray.
    const GRAY: &'static str = "#404040";
    /// Light indicator.
    const LIGHT: &'static str = "#ffffff";
    /// Green modulation.
    const MOD_GREEN: &'static str = "#00ff88";
}

impl StyleSheet for SSLTheme {
    fn knob(&self, state: ControlState, bipolar: bool) -> KnobStyle {
        let fill_color = match state {
            ControlState::Idle => Self::ORANGE.to_string(),
            ControlState::Hovered => Self::ORANGE_HOVER.to_string(),
            ControlState::Dragging => Self::ORANGE.to_string(),
            ControlState::Disabled => Self::GRAY.to_string(),
        };

        KnobStyle {
            track_color: Self::DARK_BG.to_string(),
            fill_color,
            pointer_color: Self::LIGHT.to_string(),
            modulation_color: Self::MOD_GREEN.to_string(),
            center_color: if bipolar {
                Some(Self::GRAY.to_string())
            } else {
                None
            },
            stroke_width: 5.0,
            pointer_width: 2.0,
            svg_texture: None,
        }
    }

    fn slider(&self, state: ControlState, _bipolar: bool) -> SliderStyle {
        let fill_color = match state {
            ControlState::Idle => Self::ORANGE.to_string(),
            ControlState::Hovered => Self::ORANGE_HOVER.to_string(),
            ControlState::Dragging => Self::ORANGE.to_string(),
            ControlState::Disabled => Self::GRAY.to_string(),
        };

        SliderStyle {
            track_color: Self::DARK_BG.to_string(),
            fill_color,
            thumb_color: Self::LIGHT.to_string(),
            modulation_color: Self::MOD_GREEN.to_string(),
            track_height: 8.0,
            thumb_size: 18.0,
        }
    }

    fn xy_pad(&self, state: ControlState) -> XYPadStyle {
        let cursor_color = match state {
            ControlState::Disabled => Self::GRAY.to_string(),
            _ => Self::ORANGE.to_string(),
        };

        XYPadStyle {
            background_color: Self::DARK_BG.to_string(),
            grid_color: Self::GRAY.to_string(),
            cursor_color,
            modulation_color: Self::MOD_GREEN.to_string(),
            show_grid: true,
            cursor_size: 14.0,
        }
    }

    fn modulation_color(&self) -> String {
        Self::MOD_GREEN.to_string()
    }
}
