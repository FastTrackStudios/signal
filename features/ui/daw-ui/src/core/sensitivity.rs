//! Drag sensitivity and modifier key handling.
//!
//! These types control how mouse movement translates to parameter changes,
//! including support for fine control with modifier keys.

/// Modifier keys state for fine control.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModifierKeys {
    /// Shift key is held (typically enables fine control).
    pub shift: bool,
    /// Control/Command key is held.
    pub ctrl: bool,
    /// Alt/Option key is held.
    pub alt: bool,
}

impl ModifierKeys {
    /// Create from individual key states.
    #[must_use]
    pub const fn new(shift: bool, ctrl: bool, alt: bool) -> Self {
        Self { shift, ctrl, alt }
    }

    /// Check if fine control mode is active (shift held).
    #[must_use]
    pub const fn fine_control(&self) -> bool {
        self.shift
    }

    /// Check if any modifier is held.
    #[must_use]
    pub const fn any(&self) -> bool {
        self.shift || self.ctrl || self.alt
    }
}

/// Drag sensitivity configuration.
///
/// Controls how mouse movement maps to parameter changes.
/// Different sensitivities can be configured for normal and fine modes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragSensitivity {
    /// Pixels of vertical movement for a full 0-1 range change (normal mode).
    pub normal_pixels: f32,
    /// Multiplier applied when fine control is active (shift held).
    pub fine_multiplier: f32,
}

impl DragSensitivity {
    /// Default sensitivity suitable for most controls.
    pub const DEFAULT: Self = Self {
        normal_pixels: 150.0,
        fine_multiplier: 0.1,
    };

    /// High sensitivity for quick adjustments.
    pub const HIGH: Self = Self {
        normal_pixels: 100.0,
        fine_multiplier: 0.1,
    };

    /// Low sensitivity for precise controls.
    pub const LOW: Self = Self {
        normal_pixels: 300.0,
        fine_multiplier: 0.05,
    };

    /// Create custom sensitivity.
    #[must_use]
    pub const fn new(normal_pixels: f32, fine_multiplier: f32) -> Self {
        Self {
            normal_pixels,
            fine_multiplier,
        }
    }

    /// Calculate normalized delta from pixel movement.
    ///
    /// Positive delta_y (downward movement) decreases value.
    /// Negative delta_y (upward movement) increases value.
    #[must_use]
    pub fn calculate_delta(&self, delta_y: f32, modifiers: ModifierKeys) -> f32 {
        let base_delta = -delta_y / self.normal_pixels;

        if modifiers.fine_control() {
            base_delta * self.fine_multiplier
        } else {
            base_delta
        }
    }

    /// Get the effective pixels per unit for the current mode.
    #[must_use]
    pub fn effective_pixels(&self, modifiers: ModifierKeys) -> f32 {
        if modifiers.fine_control() {
            self.normal_pixels / self.fine_multiplier
        } else {
            self.normal_pixels
        }
    }
}

impl Default for DragSensitivity {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Drag state tracking for controls.
///
/// Tracks the state of a drag gesture including starting position
/// and accumulated movement.
#[derive(Debug, Clone, Copy, Default)]
pub struct DragState {
    /// Whether a drag is currently active.
    pub active: bool,
    /// Starting Y coordinate of the drag.
    pub start_y: f32,
    /// Value at the start of the drag.
    pub start_value: f32,
    /// Current accumulated delta.
    pub current_delta: f32,
}

impl DragState {
    /// Begin a new drag gesture.
    #[must_use]
    pub fn begin(y: f32, value: f32) -> Self {
        Self {
            active: true,
            start_y: y,
            start_value: value,
            current_delta: 0.0,
        }
    }

    /// Update the drag with new Y coordinate.
    ///
    /// Returns the new value after applying sensitivity.
    pub fn update(&mut self, y: f32, sensitivity: DragSensitivity, modifiers: ModifierKeys) -> f32 {
        if !self.active {
            return self.start_value;
        }

        let delta_y = y - self.start_y;
        self.current_delta = sensitivity.calculate_delta(delta_y, modifiers);

        (self.start_value + self.current_delta).clamp(0.0, 1.0)
    }

    /// End the drag gesture.
    pub fn end(&mut self) {
        self.active = false;
    }

    /// Cancel the drag and return to start value.
    #[must_use]
    pub fn cancel(&mut self) -> f32 {
        self.active = false;
        self.start_value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitivity_normal_mode() {
        let sens = DragSensitivity::DEFAULT;
        let mods = ModifierKeys::default();

        // Full range movement
        let delta = sens.calculate_delta(-150.0, mods);
        assert!((delta - 1.0).abs() < f32::EPSILON);

        // Half range
        let delta = sens.calculate_delta(-75.0, mods);
        assert!((delta - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn sensitivity_fine_mode() {
        let sens = DragSensitivity::DEFAULT;
        let mods = ModifierKeys::new(true, false, false);

        // Same movement but 10x finer
        let delta = sens.calculate_delta(-150.0, mods);
        assert!((delta - 0.1).abs() < f32::EPSILON);
    }

    #[test]
    fn drag_state_lifecycle() {
        let sens = DragSensitivity::DEFAULT;
        let mods = ModifierKeys::default();

        let mut state = DragState::begin(100.0, 0.5);
        assert!(state.active);

        // Move up 75px (should increase by 0.5)
        let value = state.update(25.0, sens, mods);
        assert!((value - 1.0).abs() < f32::EPSILON);

        state.end();
        assert!(!state.active);
    }
}
