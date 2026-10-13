//! Accessibility types and helpers for audio controls.
//!
//! Provides ARIA attributes, keyboard navigation, and screen reader support.

use crate::core::normal::Normal;

/// ARIA role for audio parameter controls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AriaRole {
    /// Standard slider role (most audio controls).
    #[default]
    Slider,
    /// Spinbutton for discrete values.
    SpinButton,
    /// Dial for rotary controls (less common support).
    Dial,
}

impl AriaRole {
    /// Get the ARIA role string.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Slider => "slider",
            Self::SpinButton => "spinbutton",
            Self::Dial => "slider", // Dial isn't widely supported, fallback to slider
        }
    }
}

/// ARIA attributes for an audio control.
#[derive(Debug, Clone, Default)]
pub struct AriaAttributes {
    /// ARIA role (slider, spinbutton, etc.)
    pub role: AriaRole,
    /// Accessible label for the control.
    pub label: Option<String>,
    /// Current value as text (e.g., "-6.0 dB")
    pub value_text: Option<String>,
    /// Minimum value (for slider/spinbutton)
    pub value_min: f32,
    /// Maximum value (for slider/spinbutton)
    pub value_max: f32,
    /// Current value (normalized 0-1 or actual value)
    pub value_now: f32,
    /// Optional description for the control.
    pub description: Option<String>,
    /// Whether the control is disabled.
    pub disabled: bool,
    /// Whether the control is read-only.
    pub readonly: bool,
    /// Orientation (horizontal, vertical, or unspecified)
    pub orientation: Option<Orientation>,
}

impl AriaAttributes {
    /// Create new ARIA attributes for a slider control.
    #[must_use]
    pub fn slider(label: impl Into<String>, value: f32, min: f32, max: f32) -> Self {
        Self {
            role: AriaRole::Slider,
            label: Some(label.into()),
            value_min: min,
            value_max: max,
            value_now: value,
            ..Default::default()
        }
    }

    /// Create new ARIA attributes for a knob (rotary control).
    #[must_use]
    pub fn knob(label: impl Into<String>, value: f32, min: f32, max: f32) -> Self {
        Self {
            role: AriaRole::Dial,
            label: Some(label.into()),
            value_min: min,
            value_max: max,
            value_now: value,
            ..Default::default()
        }
    }

    /// Set the human-readable value text (e.g., "-6.0 dB", "440 Hz").
    #[must_use]
    pub fn with_value_text(mut self, text: impl Into<String>) -> Self {
        self.value_text = Some(text.into());
        self
    }

    /// Set the description for additional context.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Mark as disabled.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Set orientation.
    #[must_use]
    pub fn with_orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = Some(orientation);
        self
    }
}

/// Control orientation for accessibility.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Orientation {
    /// Horizontal control (left = min, right = max).
    #[default]
    Horizontal,
    /// Vertical control (bottom = min, top = max).
    Vertical,
}

impl Orientation {
    /// Get the ARIA orientation string.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

/// Keyboard navigation action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// Increment value by small step.
    IncrementSmall,
    /// Decrement value by small step.
    DecrementSmall,
    /// Increment value by large step.
    IncrementLarge,
    /// Decrement value by large step.
    DecrementLarge,
    /// Go to minimum value.
    GoToMin,
    /// Go to maximum value.
    GoToMax,
    /// Reset to default value.
    ResetToDefault,
    /// No action.
    None,
}

impl KeyAction {
    /// Parse a keyboard event into an action.
    ///
    /// Standard mappings:
    /// - Arrow Up/Right: Increment small
    /// - Arrow Down/Left: Decrement small
    /// - Page Up: Increment large
    /// - Page Down: Decrement large
    /// - Home: Go to minimum
    /// - End: Go to maximum
    /// - Delete/Backspace: Reset to default (when shift held)
    #[must_use]
    pub fn from_key(key: &str, shift: bool, ctrl: bool) -> Self {
        match key {
            "ArrowUp" | "ArrowRight" => {
                if shift {
                    Self::IncrementLarge
                } else {
                    Self::IncrementSmall
                }
            }
            "ArrowDown" | "ArrowLeft" => {
                if shift {
                    Self::DecrementLarge
                } else {
                    Self::DecrementSmall
                }
            }
            "PageUp" => Self::IncrementLarge,
            "PageDown" => Self::DecrementLarge,
            "Home" => Self::GoToMin,
            "End" => Self::GoToMax,
            "Delete" | "Backspace" if ctrl || shift => Self::ResetToDefault,
            _ => Self::None,
        }
    }
}

/// Step configuration for keyboard navigation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyboardSteps {
    /// Small step size (arrow keys).
    pub small: f32,
    /// Large step size (page up/down, shift+arrow).
    pub large: f32,
}

impl KeyboardSteps {
    /// Default steps: 1% small, 10% large.
    pub const DEFAULT: Self = Self {
        small: 0.01,
        large: 0.10,
    };

    /// Fine steps: 0.1% small, 1% large.
    pub const FINE: Self = Self {
        small: 0.001,
        large: 0.01,
    };

    /// Coarse steps: 5% small, 25% large.
    pub const COARSE: Self = Self {
        small: 0.05,
        large: 0.25,
    };

    /// Create stepped values for discrete parameters.
    #[must_use]
    pub fn discrete(steps: usize) -> Self {
        let step = 1.0 / steps as f32;
        Self {
            small: step,
            large: (step * 5.0).min(0.5),
        }
    }

    /// Apply a keyboard action to a normalized value.
    #[must_use]
    pub fn apply(&self, action: KeyAction, value: Normal, default: Normal) -> Normal {
        match action {
            KeyAction::IncrementSmall => Normal::new(value.value() + self.small),
            KeyAction::DecrementSmall => Normal::new(value.value() - self.small),
            KeyAction::IncrementLarge => Normal::new(value.value() + self.large),
            KeyAction::DecrementLarge => Normal::new(value.value() - self.large),
            KeyAction::GoToMin => Normal::MIN,
            KeyAction::GoToMax => Normal::MAX,
            KeyAction::ResetToDefault => default,
            KeyAction::None => value,
        }
    }
}

impl Default for KeyboardSteps {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Focus state for controls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FocusState {
    /// Control is not focused.
    #[default]
    Unfocused,
    /// Control has keyboard focus.
    Focused,
    /// Control has focus and is receiving keyboard input.
    Active,
}

impl FocusState {
    /// Check if the control should show focus ring.
    #[must_use]
    pub const fn show_focus_ring(&self) -> bool {
        matches!(self, Self::Focused | Self::Active)
    }
}

/// Announcement for screen readers.
///
/// Use this to announce value changes to assistive technologies.
#[derive(Debug, Clone)]
pub struct Announcement {
    /// The message to announce.
    pub message: String,
    /// Priority level (polite or assertive).
    pub priority: AnnouncePriority,
}

impl Announcement {
    /// Create a polite announcement (doesn't interrupt current speech).
    #[must_use]
    pub fn polite(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            priority: AnnouncePriority::Polite,
        }
    }

    /// Create an assertive announcement (interrupts current speech).
    #[must_use]
    pub fn assertive(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            priority: AnnouncePriority::Assertive,
        }
    }

    /// Create a value change announcement.
    #[must_use]
    pub fn value_change(label: &str, value_text: &str) -> Self {
        Self::polite(format!("{label}: {value_text}"))
    }
}

/// Announcement priority for screen readers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AnnouncePriority {
    /// Polite announcements wait for a pause in speech.
    #[default]
    Polite,
    /// Assertive announcements interrupt immediately.
    Assertive,
}

impl AnnouncePriority {
    /// Get the aria-live attribute value.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Polite => "polite",
            Self::Assertive => "assertive",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_action_parsing() {
        assert_eq!(
            KeyAction::from_key("ArrowUp", false, false),
            KeyAction::IncrementSmall
        );
        assert_eq!(
            KeyAction::from_key("ArrowUp", true, false),
            KeyAction::IncrementLarge
        );
        assert_eq!(
            KeyAction::from_key("ArrowDown", false, false),
            KeyAction::DecrementSmall
        );
        assert_eq!(
            KeyAction::from_key("Home", false, false),
            KeyAction::GoToMin
        );
        assert_eq!(KeyAction::from_key("End", false, false), KeyAction::GoToMax);
        assert_eq!(
            KeyAction::from_key("Delete", true, false),
            KeyAction::ResetToDefault
        );
        assert_eq!(KeyAction::from_key("a", false, false), KeyAction::None);
    }

    #[test]
    fn keyboard_steps_apply() {
        let steps = KeyboardSteps::DEFAULT;
        let value = Normal::new(0.5);
        let default = Normal::new(0.0);

        let incremented = steps.apply(KeyAction::IncrementSmall, value, default);
        assert!((incremented.value() - 0.51).abs() < f32::EPSILON);

        let decremented = steps.apply(KeyAction::DecrementLarge, value, default);
        assert!((decremented.value() - 0.40).abs() < f32::EPSILON);

        let reset = steps.apply(KeyAction::ResetToDefault, value, default);
        assert!((reset.value() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn aria_attributes_builder() {
        let aria = AriaAttributes::slider("Volume", 0.5, 0.0, 1.0)
            .with_value_text("-6.0 dB")
            .with_description("Controls main output volume")
            .disabled(false);

        assert_eq!(aria.label.as_deref(), Some("Volume"));
        assert_eq!(aria.value_text.as_deref(), Some("-6.0 dB"));
        assert!((aria.value_now - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn discrete_steps() {
        let steps = KeyboardSteps::discrete(10);
        assert!((steps.small - 0.1).abs() < f32::EPSILON);
        assert!((steps.large - 0.5).abs() < f32::EPSILON);
    }
}
