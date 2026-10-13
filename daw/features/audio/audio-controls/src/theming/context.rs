//! Theme context provider for audio controls.
//!
//! Provides a way to set a global theme for all audio controls within a subtree.

use crate::prelude::*;

use super::style::{DefaultStyleSheet, KnobStyle, SliderStyle, StyleSheet, XYPadStyle};

/// Theme context data.
#[derive(Clone)]
pub struct ThemeContext {
    /// The active style sheet.
    sheet: StyleSheetWrapper,
    /// Whether animations are enabled.
    pub animations_enabled: bool,
    /// Whether haptic feedback is enabled (for touch devices).
    pub haptics_enabled: bool,
    /// Global scale factor for control sizes.
    pub scale_factor: f32,
}

/// Wrapper to make StyleSheet cloneable in context.
#[derive(Clone)]
struct StyleSheetWrapper {
    inner: &'static dyn StyleSheet,
}

impl std::fmt::Debug for StyleSheetWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StyleSheetWrapper").finish()
    }
}

impl PartialEq for StyleSheetWrapper {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.inner, other.inner)
    }
}

impl PartialEq for ThemeContext {
    fn eq(&self, other: &Self) -> bool {
        self.sheet == other.sheet
            && self.animations_enabled == other.animations_enabled
            && self.haptics_enabled == other.haptics_enabled
            && (self.scale_factor - other.scale_factor).abs() < f32::EPSILON
    }
}

impl ThemeContext {
    /// Create a new theme context with the default style sheet.
    #[must_use]
    pub fn new() -> Self {
        static DEFAULT: DefaultStyleSheet = DefaultStyleSheet;
        Self {
            sheet: StyleSheetWrapper { inner: &DEFAULT },
            animations_enabled: true,
            haptics_enabled: true,
            scale_factor: 1.0,
        }
    }

    /// Create a theme context with a custom style sheet.
    ///
    /// Note: The style sheet must be a static reference since it needs
    /// to live for the lifetime of the context.
    #[must_use]
    pub fn with_stylesheet(sheet: &'static dyn StyleSheet) -> Self {
        Self {
            sheet: StyleSheetWrapper { inner: sheet },
            animations_enabled: true,
            haptics_enabled: true,
            scale_factor: 1.0,
        }
    }

    /// Enable or disable animations.
    #[must_use]
    pub fn with_animations(mut self, enabled: bool) -> Self {
        self.animations_enabled = enabled;
        self
    }

    /// Enable or disable haptic feedback.
    #[must_use]
    pub fn with_haptics(mut self, enabled: bool) -> Self {
        self.haptics_enabled = enabled;
        self
    }

    /// Set the global scale factor.
    #[must_use]
    pub fn with_scale(mut self, scale: f32) -> Self {
        self.scale_factor = scale.max(0.1);
        self
    }

    /// Get knob style for the given state.
    #[must_use]
    pub fn knob(&self, state: super::style::ControlState, bipolar: bool) -> KnobStyle {
        self.sheet.inner.knob(state, bipolar)
    }

    /// Get slider style for the given state.
    #[must_use]
    pub fn slider(&self, state: super::style::ControlState, bipolar: bool) -> SliderStyle {
        self.sheet.inner.slider(state, bipolar)
    }

    /// Get XY pad style for the given state.
    #[must_use]
    pub fn xy_pad(&self, state: super::style::ControlState) -> XYPadStyle {
        self.sheet.inner.xy_pad(state)
    }

    /// Scale a size value by the global scale factor.
    #[must_use]
    pub fn scale(&self, size: u32) -> u32 {
        ((size as f32) * self.scale_factor).round() as u32
    }

    /// Scale a float size value by the global scale factor.
    #[must_use]
    pub fn scale_f32(&self, size: f32) -> f32 {
        size * self.scale_factor
    }
}

impl Default for ThemeContext {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ThemeContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThemeContext")
            .field("animations_enabled", &self.animations_enabled)
            .field("haptics_enabled", &self.haptics_enabled)
            .field("scale_factor", &self.scale_factor)
            .finish()
    }
}

/// Hook to access the current theme context.
///
/// Returns the theme context from the nearest `ThemeProvider` ancestor,
/// or a default theme if none is found.
///
/// # Example
///
/// ```ignore
/// use audio_controls::theming::use_theme;
///
/// #[component]
/// fn MyControl() -> Element {
///     let theme = use_theme();
///     let style = theme.knob(ControlState::Idle, false);
///     // ...
/// }
/// ```
#[must_use]
pub fn use_theme() -> ThemeContext {
    try_use_context::<ThemeContext>().unwrap_or_default()
}

/// Theme provider component.
///
/// Wraps children with a theme context that can be accessed via `use_theme()`.
///
/// # Example
///
/// ```ignore
/// use audio_controls::theming::{ThemeProvider, ThemeContext};
/// use audio_controls::theming::presets::SSLTheme;
///
/// #[component]
/// fn App() -> Element {
///     rsx! {
///         ThemeProvider {
///             theme: ThemeContext::with_stylesheet(&SSLTheme),
///             // All controls here will use SSL theme
///             Knob { value: gain }
///         }
///     }
/// }
/// ```
#[component]
pub fn ThemeProvider(theme: ThemeContext, children: Element) -> Element {
    use_context_provider(|| theme);
    children
}

/// Configuration options passed to controls.
#[derive(Debug, Clone)]
pub struct ControlConfig {
    /// Whether the control is disabled.
    pub disabled: bool,
    /// Whether to show value on hover.
    pub show_value_on_hover: bool,
    /// Whether to show label.
    pub show_label: bool,
    /// Whether to allow text input on double-click.
    pub text_input_enabled: bool,
    /// Whether scroll wheel changes value.
    pub scroll_enabled: bool,
    /// Whether keyboard navigation is enabled.
    pub keyboard_enabled: bool,
    /// Animation duration in ms (0 to disable).
    pub animation_duration_ms: u32,
}

impl ControlConfig {
    /// Default configuration.
    pub const DEFAULT: Self = Self {
        disabled: false,
        show_value_on_hover: true,
        show_label: true,
        text_input_enabled: true,
        scroll_enabled: true,
        keyboard_enabled: true,
        animation_duration_ms: 150,
    };

    /// Minimal configuration (no extras).
    pub const MINIMAL: Self = Self {
        disabled: false,
        show_value_on_hover: false,
        show_label: false,
        text_input_enabled: false,
        scroll_enabled: true,
        keyboard_enabled: true,
        animation_duration_ms: 0,
    };

    /// Check if animations are enabled.
    #[must_use]
    pub const fn has_animations(&self) -> bool {
        self.animation_duration_ms > 0
    }
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_context_default() {
        let theme = ThemeContext::new();
        assert!(theme.animations_enabled);
        assert!((theme.scale_factor - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn theme_context_scale() {
        let theme = ThemeContext::new().with_scale(2.0);
        assert_eq!(theme.scale(50), 100);
        assert!((theme.scale_f32(25.0) - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn control_config() {
        let config = ControlConfig::DEFAULT;
        assert!(config.has_animations());

        let minimal = ControlConfig::MINIMAL;
        assert!(!minimal.has_animations());
    }
}
