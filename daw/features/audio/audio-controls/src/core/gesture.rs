//! Input gesture handling for audio controls.
//!
//! Provides unified handling of mouse, touch, and scroll wheel input
//! with momentum, inertia, and gesture recognition.

use crate::core::sensitivity::{DragSensitivity, DragState, ModifierKeys};

/// Unified input gesture state.
#[derive(Debug, Clone, Default)]
pub struct GestureState {
    /// Current drag state (for mouse/touch).
    pub drag: DragState,
    /// Whether a touch gesture is active.
    pub touch_active: bool,
    /// Number of active touch points.
    pub touch_count: usize,
    /// Last touch Y position.
    pub touch_y: f32,
    /// Last touch X position.
    pub touch_x: f32,
    /// Accumulated scroll delta.
    pub scroll_accumulator: f32,
    /// Last update timestamp (for velocity calculation).
    pub last_update_time: f64,
    /// Current velocity (units per ms).
    pub velocity: f32,
}

impl GestureState {
    /// Create a new gesture state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if any gesture is active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.drag.active || self.touch_active
    }

    /// Begin a mouse drag gesture.
    pub fn begin_mouse(&mut self, y: f32, value: f32) {
        self.drag = DragState::begin(y, value);
        self.velocity = 0.0;
    }

    /// Begin a touch gesture.
    pub fn begin_touch(&mut self, x: f32, y: f32, value: f32, touch_count: usize) {
        self.touch_active = true;
        self.touch_x = x;
        self.touch_y = y;
        self.touch_count = touch_count;
        self.drag = DragState::begin(y, value);
        self.velocity = 0.0;
    }

    /// Update with mouse movement.
    pub fn update_mouse(
        &mut self,
        y: f32,
        sensitivity: DragSensitivity,
        modifiers: ModifierKeys,
    ) -> f32 {
        self.drag.update(y, sensitivity, modifiers)
    }

    /// Update with touch movement.
    pub fn update_touch(
        &mut self,
        x: f32,
        y: f32,
        sensitivity: DragSensitivity,
        modifiers: ModifierKeys,
        timestamp: f64,
    ) -> f32 {
        // Calculate velocity
        let dt = (timestamp - self.last_update_time).max(1.0);
        let dy = y - self.touch_y;
        self.velocity = (dy / dt as f32) * 0.5 + self.velocity * 0.5; // Smooth velocity

        self.touch_x = x;
        self.touch_y = y;
        self.last_update_time = timestamp;

        self.drag.update(y, sensitivity, modifiers)
    }

    /// End the current gesture.
    pub fn end(&mut self) {
        self.drag.end();
        self.touch_active = false;
        self.touch_count = 0;
    }

    /// Cancel the gesture and return to original value.
    pub fn cancel(&mut self) -> f32 {
        self.touch_active = false;
        self.touch_count = 0;
        self.drag.cancel()
    }

    /// Process scroll wheel input.
    ///
    /// Returns the new value after applying scroll delta.
    pub fn process_scroll(
        &mut self,
        delta_y: f32,
        current_value: f32,
        sensitivity: &ScrollSensitivity,
        modifiers: ModifierKeys,
    ) -> f32 {
        // Accumulate scroll for smooth scrolling support
        self.scroll_accumulator += delta_y * sensitivity.scale;

        // Calculate value change
        let value_delta = if modifiers.fine_control() {
            self.scroll_accumulator * sensitivity.fine_multiplier
        } else {
            self.scroll_accumulator
        };

        // Apply and clear accumulator
        self.scroll_accumulator = 0.0;

        (current_value - value_delta).clamp(0.0, 1.0)
    }
}

/// Configuration for scroll wheel behavior.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollSensitivity {
    /// Scale factor for scroll delta.
    /// Typical values: 0.001 (fine) to 0.01 (coarse).
    pub scale: f32,
    /// Multiplier when fine control is active.
    pub fine_multiplier: f32,
    /// Whether to invert scroll direction.
    pub invert: bool,
    /// Whether to enable smooth (momentum) scrolling.
    pub smooth: bool,
}

impl ScrollSensitivity {
    /// Default scroll sensitivity.
    pub const DEFAULT: Self = Self {
        scale: 0.002,
        fine_multiplier: 0.1,
        invert: false,
        smooth: true,
    };

    /// Fine scroll sensitivity.
    pub const FINE: Self = Self {
        scale: 0.001,
        fine_multiplier: 0.1,
        invert: false,
        smooth: true,
    };

    /// Coarse scroll sensitivity.
    pub const COARSE: Self = Self {
        scale: 0.005,
        fine_multiplier: 0.1,
        invert: false,
        smooth: true,
    };

    /// Create custom scroll sensitivity.
    #[must_use]
    pub const fn new(scale: f32) -> Self {
        Self {
            scale,
            fine_multiplier: 0.1,
            invert: false,
            smooth: true,
        }
    }

    /// Invert scroll direction.
    #[must_use]
    pub const fn inverted(mut self) -> Self {
        self.invert = true;
        self
    }

    /// Apply inversion if configured.
    #[must_use]
    pub fn apply_direction(&self, delta: f32) -> f32 {
        if self.invert { -delta } else { delta }
    }
}

impl Default for ScrollSensitivity {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Touch gesture type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TouchGesture {
    /// No active gesture.
    #[default]
    None,
    /// Single finger drag (value change).
    SingleDrag,
    /// Two finger pinch (fine control).
    Pinch,
    /// Two finger rotate (alternative control).
    Rotate,
    /// Long press (shows value editor).
    LongPress,
    /// Double tap (reset to default).
    DoubleTap,
}

/// Touch gesture recognizer.
#[derive(Debug, Clone, Default)]
pub struct TouchRecognizer {
    /// Current recognized gesture.
    pub gesture: TouchGesture,
    /// Start time of current gesture.
    pub start_time: f64,
    /// Start positions of touch points.
    pub start_positions: Vec<(f32, f32)>,
    /// Last tap time (for double-tap detection).
    pub last_tap_time: f64,
    /// Last tap position.
    pub last_tap_position: Option<(f32, f32)>,
}

impl TouchRecognizer {
    /// Time threshold for long press (ms).
    const LONG_PRESS_MS: f64 = 500.0;
    /// Time threshold for double tap (ms).
    const DOUBLE_TAP_MS: f64 = 300.0;
    /// Distance threshold for tap vs drag.
    const TAP_THRESHOLD: f32 = 10.0;
    /// Distance threshold for double tap position.
    const DOUBLE_TAP_DISTANCE: f32 = 30.0;

    /// Create a new touch recognizer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            gesture: TouchGesture::None,
            ..Default::default()
        }
    }

    /// Handle touch start.
    pub fn touch_start(&mut self, positions: &[(f32, f32)], timestamp: f64) {
        self.start_time = timestamp;
        self.start_positions = positions.to_vec();

        // Check for double tap
        if positions.len() == 1
            && let Some(last_pos) = self.last_tap_position
        {
            let dt = timestamp - self.last_tap_time;
            let dx = positions[0].0 - last_pos.0;
            let dy = positions[0].1 - last_pos.1;
            let distance = (dx * dx + dy * dy).sqrt();

            if dt < Self::DOUBLE_TAP_MS && distance < Self::DOUBLE_TAP_DISTANCE {
                self.gesture = TouchGesture::DoubleTap;
                self.last_tap_position = None;
                return;
            }
        }

        self.gesture = match positions.len() {
            1 => TouchGesture::SingleDrag,
            2 => TouchGesture::Pinch,
            _ => TouchGesture::None,
        };
    }

    /// Handle touch move.
    pub fn touch_move(&mut self, positions: &[(f32, f32)], _timestamp: f64) {
        if positions.len() != self.start_positions.len() {
            return;
        }

        // Update gesture type based on movement
        match positions.len() {
            1 => {
                // Single finger - just drag
                self.gesture = TouchGesture::SingleDrag;
            }
            2 => {
                // Two fingers - could be pinch or rotate
                // For now, treat as pinch (fine control)
                self.gesture = TouchGesture::Pinch;
            }
            _ => {}
        }
    }

    /// Handle touch end.
    pub fn touch_end(&mut self, positions: &[(f32, f32)], timestamp: f64) {
        // Check if this was a tap
        if self.start_positions.len() == 1 && positions.is_empty() {
            let dt = timestamp - self.start_time;
            if dt < Self::DOUBLE_TAP_MS {
                // Record tap for potential double-tap
                self.last_tap_time = timestamp;
                self.last_tap_position = Some(self.start_positions[0]);
            }
        }

        if positions.is_empty() {
            self.gesture = TouchGesture::None;
            self.start_positions.clear();
        }
    }

    /// Check for long press.
    pub fn check_long_press(&mut self, timestamp: f64) -> bool {
        if self.gesture == TouchGesture::SingleDrag {
            let dt = timestamp - self.start_time;
            if dt >= Self::LONG_PRESS_MS && self.start_positions.len() == 1 {
                // Check if finger hasn't moved much
                // (This would need current position, simplified here)
                self.gesture = TouchGesture::LongPress;
                return true;
            }
        }
        false
    }

    /// Get pinch scale factor.
    pub fn pinch_scale(&self, current_positions: &[(f32, f32)]) -> f32 {
        if self.start_positions.len() != 2 || current_positions.len() != 2 {
            return 1.0;
        }

        let start_dist = distance(self.start_positions[0], self.start_positions[1]);
        let current_dist = distance(current_positions[0], current_positions[1]);

        if start_dist > 0.0 {
            current_dist / start_dist
        } else {
            1.0
        }
    }

    /// Reset the recognizer.
    pub fn reset(&mut self) {
        self.gesture = TouchGesture::None;
        self.start_positions.clear();
    }
}

/// Calculate distance between two points.
fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    (dx * dx + dy * dy).sqrt()
}

/// Interaction mode for the control.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InteractionMode {
    /// Normal interaction (mouse/touch drag).
    #[default]
    Normal,
    /// Fine control (shift held or pinch gesture).
    Fine,
    /// Text input mode (editing value directly).
    TextInput,
    /// Locked (disabled or read-only).
    Locked,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gesture_state_mouse() {
        let mut state = GestureState::new();
        let sens = DragSensitivity::DEFAULT;
        let mods = ModifierKeys::default();

        state.begin_mouse(100.0, 0.5);
        assert!(state.is_active());

        // Move up (decrease Y) should increase value
        let new_val = state.update_mouse(50.0, sens, mods);
        assert!(new_val > 0.5);

        state.end();
        assert!(!state.is_active());
    }

    #[test]
    fn scroll_sensitivity() {
        let mut state = GestureState::new();
        let sens = ScrollSensitivity::DEFAULT;
        let mods = ModifierKeys::default();

        // Scroll up should increase value
        let new_val = state.process_scroll(-100.0, 0.5, &sens, mods);
        assert!(new_val > 0.5);

        // Scroll down should decrease value
        let new_val = state.process_scroll(100.0, 0.5, &sens, mods);
        assert!(new_val < 0.5);
    }

    #[test]
    fn scroll_fine_control() {
        let mut state = GestureState::new();
        let sens = ScrollSensitivity::DEFAULT;
        let fine_mods = ModifierKeys::new(true, false, false);
        let normal_mods = ModifierKeys::default();

        let normal_change = state.process_scroll(-100.0, 0.5, &sens, normal_mods) - 0.5;
        state = GestureState::new(); // Reset accumulator
        let fine_change = state.process_scroll(-100.0, 0.5, &sens, fine_mods) - 0.5;

        // Fine control should produce smaller change
        assert!(fine_change.abs() < normal_change.abs());
    }

    #[test]
    fn touch_recognizer_double_tap() {
        let mut recognizer = TouchRecognizer::new();

        // First tap
        recognizer.touch_start(&[(50.0, 50.0)], 0.0);
        recognizer.touch_end(&[], 50.0);

        // Second tap within threshold
        recognizer.touch_start(&[(52.0, 52.0)], 150.0);

        assert_eq!(recognizer.gesture, TouchGesture::DoubleTap);
    }

    #[test]
    fn touch_recognizer_pinch() {
        let mut recognizer = TouchRecognizer::new();

        // Start with two fingers
        recognizer.touch_start(&[(50.0, 50.0), (100.0, 100.0)], 0.0);
        assert_eq!(recognizer.gesture, TouchGesture::Pinch);

        // Pinch scale
        let scale = recognizer.pinch_scale(&[(25.0, 50.0), (125.0, 100.0)]);
        assert!(scale > 1.0); // Fingers moved apart
    }
}
