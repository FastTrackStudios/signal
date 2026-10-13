//! Input event types for keyboard, mouse, scroll, and focus events.

use crate::key::{KeyCode, Modifiers};

/// Top-level input event.
#[derive(Debug, Clone)]
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Scroll(ScrollEvent),
    FocusGained,
    FocusLost,
}

/// A keyboard event.
#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub key: KeyCode,
    pub modifiers: Modifiers,
}

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// What happened with the mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Click,
    DoubleClick,
}

/// A mouse event.
#[derive(Debug, Clone)]
pub struct MouseEvent {
    pub button: MouseButton,
    pub action: MouseAction,
    pub x: f64,
    pub y: f64,
    pub modifiers: Modifiers,
}

/// A scroll event.
#[derive(Debug, Clone)]
pub struct ScrollEvent {
    pub delta_x: f64,
    pub delta_y: f64,
    pub modifiers: Modifiers,
}
