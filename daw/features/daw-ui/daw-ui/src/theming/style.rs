//! Style definitions and traits for audio controls.

use super::svg_texture::SvgTexture;

/// Control interaction state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ControlState {
    /// Control is idle (default state).
    #[default]
    Idle,
    /// Mouse is hovering over the control.
    Hovered,
    /// Control is being actively dragged.
    Dragging,
    /// Control is disabled.
    Disabled,
}

/// Style configuration for knob controls.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KnobStyle {
    /// Background track color (CSS color string).
    pub track_color: String,
    /// Filled value indicator color.
    pub fill_color: String,
    /// Pointer/indicator line color.
    pub pointer_color: String,
    /// Modulation range indicator color.
    pub modulation_color: String,
    /// Center indicator color (for bipolar knobs).
    pub center_color: Option<String>,
    /// Arc stroke width.
    pub stroke_width: f32,
    /// Pointer stroke width.
    pub pointer_width: f32,
    /// Optional SVG texture for custom knob appearance.
    pub svg_texture: Option<SvgTexture>,
}

impl KnobStyle {
    /// Create a style using CSS variables (works with Tailwind themes).
    #[must_use]
    pub fn css_vars(state: ControlState, bipolar: bool) -> Self {
        let fill_color = match state {
            ControlState::Idle => "var(--color-primary)".to_string(),
            ControlState::Hovered => "var(--color-primary)".to_string(),
            ControlState::Dragging => "var(--color-primary)".to_string(),
            ControlState::Disabled => "var(--color-muted)".to_string(),
        };

        Self {
            track_color: "var(--color-secondary)".to_string(),
            fill_color,
            pointer_color: "var(--color-foreground)".to_string(),
            modulation_color: "var(--color-accent, #22c55e)".to_string(),
            center_color: if bipolar {
                Some("var(--color-muted-foreground)".to_string())
            } else {
                None
            },
            stroke_width: 4.0,
            pointer_width: 2.0,
            svg_texture: None,
        }
    }

    /// Create a style with hardcoded colors (fallback when CSS vars aren't available).
    #[must_use]
    pub fn default_colors(state: ControlState, bipolar: bool) -> Self {
        let (fill_color, track_color, pointer_color) = match state {
            ControlState::Idle => ("#3b82f6", "#374151", "#e5e7eb"), // blue-500, gray-700, gray-200
            ControlState::Hovered => ("#60a5fa", "#4b5563", "#f3f4f6"), // blue-400, gray-600, gray-100
            ControlState::Dragging => ("#2563eb", "#4b5563", "#ffffff"), // blue-600, gray-600, white
            ControlState::Disabled => ("#6b7280", "#1f2937", "#9ca3af"), // gray-500, gray-800, gray-400
        };

        Self {
            track_color: track_color.to_string(),
            fill_color: fill_color.to_string(),
            pointer_color: pointer_color.to_string(),
            modulation_color: "#22c55e".to_string(), // green-500
            center_color: if bipolar {
                Some("#9ca3af".to_string()) // gray-400
            } else {
                None
            },
            stroke_width: 6.0,
            pointer_width: 3.0,
            svg_texture: None,
        }
    }
}

/// Style configuration for slider controls.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SliderStyle {
    /// Track background color.
    pub track_color: String,
    /// Filled portion color.
    pub fill_color: String,
    /// Thumb/handle color.
    pub thumb_color: String,
    /// Modulation range color.
    pub modulation_color: String,
    /// Track height in pixels.
    pub track_height: f32,
    /// Thumb size in pixels.
    pub thumb_size: f32,
}

impl SliderStyle {
    /// Create a style using CSS variables.
    #[must_use]
    pub fn css_vars(state: ControlState) -> Self {
        let fill_color = match state {
            ControlState::Idle => "var(--color-primary)".to_string(),
            ControlState::Hovered => "var(--color-primary)".to_string(),
            ControlState::Dragging => "var(--color-primary)".to_string(),
            ControlState::Disabled => "var(--color-muted)".to_string(),
        };

        Self {
            track_color: "var(--color-secondary)".to_string(),
            fill_color,
            thumb_color: "var(--color-primary)".to_string(),
            modulation_color: "var(--color-accent, #22c55e)".to_string(),
            track_height: 6.0,
            thumb_size: 16.0,
        }
    }

    /// Create a style with hardcoded colors.
    #[must_use]
    pub fn default_colors(state: ControlState) -> Self {
        let (fill_color, track_color, thumb_color) = match state {
            ControlState::Idle => ("#3b82f6", "#374151", "#3b82f6"),
            ControlState::Hovered => ("#60a5fa", "#4b5563", "#60a5fa"),
            ControlState::Dragging => ("#2563eb", "#4b5563", "#2563eb"),
            ControlState::Disabled => ("#6b7280", "#1f2937", "#6b7280"),
        };

        Self {
            track_color: track_color.to_string(),
            fill_color: fill_color.to_string(),
            thumb_color: thumb_color.to_string(),
            modulation_color: "#22c55e".to_string(),
            track_height: 6.0,
            thumb_size: 16.0,
        }
    }
}

/// Style configuration for XY pad controls.
#[derive(Debug, Clone, Default)]
pub struct XYPadStyle {
    /// Background color.
    pub background_color: String,
    /// Grid line color.
    pub grid_color: String,
    /// Cursor/dot color.
    pub cursor_color: String,
    /// Modulation indicator color.
    pub modulation_color: String,
    /// Whether to show grid lines.
    pub show_grid: bool,
    /// Cursor size in pixels.
    pub cursor_size: f32,
}

impl XYPadStyle {
    /// Create a style using CSS variables.
    #[must_use]
    pub fn css_vars(state: ControlState) -> Self {
        let cursor_color = match state {
            ControlState::Disabled => "var(--color-muted)".to_string(),
            _ => "var(--color-primary)".to_string(),
        };

        Self {
            background_color: "var(--color-secondary)".to_string(),
            grid_color: "var(--color-muted)".to_string(),
            cursor_color,
            modulation_color: "var(--color-accent, #22c55e)".to_string(),
            show_grid: true,
            cursor_size: 12.0,
        }
    }

    /// Create a style with hardcoded colors.
    #[must_use]
    pub fn default_colors(state: ControlState) -> Self {
        let cursor_color = match state {
            ControlState::Disabled => "#6b7280".to_string(),
            _ => "#3b82f6".to_string(),
        };

        Self {
            background_color: "#1f2937".to_string(), // gray-800
            grid_color: "#374151".to_string(),       // gray-700
            cursor_color,
            modulation_color: "#22c55e".to_string(),
            show_grid: true,
            cursor_size: 12.0,
        }
    }
}

/// Trait for providing styles to controls.
///
/// Implement this trait to create custom themes for audio controls.
pub trait StyleSheet: Send + Sync {
    /// Get knob style for the given state.
    fn knob(&self, state: ControlState, bipolar: bool) -> KnobStyle;

    /// Get slider style for the given state.
    fn slider(&self, state: ControlState, bipolar: bool) -> SliderStyle;

    /// Get XY pad style for the given state.
    fn xy_pad(&self, state: ControlState) -> XYPadStyle;

    /// Get the modulation indicator color.
    fn modulation_color(&self) -> String {
        "var(--color-accent, #22c55e)".to_string()
    }
}

/// Control style variant (similar to lumen-blocks ButtonVariant).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ControlVariant {
    /// Primary style using theme primary color.
    #[default]
    Primary,
    /// Secondary style using theme secondary color.
    Secondary,
    /// Accent style using theme accent color.
    Accent,
    /// Muted style for less prominent controls.
    Muted,
}

/// Default implementation using hardcoded colors for visibility.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultStyleSheet;

impl StyleSheet for DefaultStyleSheet {
    fn knob(&self, state: ControlState, bipolar: bool) -> KnobStyle {
        KnobStyle::default_colors(state, bipolar)
    }

    fn slider(&self, state: ControlState, _bipolar: bool) -> SliderStyle {
        SliderStyle::default_colors(state)
    }

    fn xy_pad(&self, state: ControlState) -> XYPadStyle {
        XYPadStyle::default_colors(state)
    }
}
