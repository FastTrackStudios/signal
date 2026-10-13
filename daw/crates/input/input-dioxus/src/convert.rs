//! Convert Dioxus DOM events into `input` events.

use dioxus::prelude::{
    InteractionLocation, Key, KeyboardEvent, Modifiers as DioxusModifiers, ModifiersInteraction,
    MouseEvent, PointerInteraction, WheelEvent,
};
use input::{InputEvent, KeyCode, KeyEvent, Modifiers, MouseAction, MouseButton, ScrollEvent};

/// Convert a Dioxus key value to an input key code.
pub fn convert_key(key: &Key) -> Option<KeyCode> {
    match key {
        Key::Character(c) => Some(KeyCode::Character(c.to_lowercase())),
        Key::ArrowUp => Some(KeyCode::ArrowUp),
        Key::ArrowDown => Some(KeyCode::ArrowDown),
        Key::ArrowLeft => Some(KeyCode::ArrowLeft),
        Key::ArrowRight => Some(KeyCode::ArrowRight),
        Key::Enter => Some(KeyCode::Enter),
        Key::Escape => Some(KeyCode::Escape),
        Key::Tab => Some(KeyCode::Tab),
        Key::Backspace => Some(KeyCode::Backspace),
        Key::Delete => Some(KeyCode::Delete),
        Key::F1 => Some(KeyCode::F(1)),
        Key::F2 => Some(KeyCode::F(2)),
        Key::F3 => Some(KeyCode::F(3)),
        Key::F4 => Some(KeyCode::F(4)),
        Key::F5 => Some(KeyCode::F(5)),
        Key::F6 => Some(KeyCode::F(6)),
        Key::F7 => Some(KeyCode::F(7)),
        Key::F8 => Some(KeyCode::F(8)),
        Key::F9 => Some(KeyCode::F(9)),
        Key::F10 => Some(KeyCode::F(10)),
        Key::F11 => Some(KeyCode::F(11)),
        Key::F12 => Some(KeyCode::F(12)),
        _ => None,
    }
}

/// Convert Dioxus modifiers to input modifiers.
pub fn convert_modifiers(m: &DioxusModifiers) -> Modifiers {
    Modifiers {
        ctrl: m.ctrl(),
        alt: m.alt(),
        shift: m.shift(),
        meta: m.meta(),
    }
}

/// Convert a Dioxus keyboard event to an input keyboard event.
///
/// Returns `None` for unrecognized keys.
pub fn convert_keyboard_event(e: &KeyboardEvent) -> Option<InputEvent> {
    let key = convert_key(&e.key())?;
    Some(InputEvent::Key(KeyEvent {
        key,
        modifiers: convert_modifiers(&e.modifiers()),
    }))
}

/// Convert a Dioxus mouse event to an input mouse event with explicit action.
pub fn convert_mouse_event(e: &MouseEvent, action: MouseAction) -> InputEvent {
    let coords = e.client_coordinates();
    let button =
        e.trigger_button()
            .map_or(MouseButton::Left, |b| match format!("{b:?}").as_str() {
                "Primary" | "Main" => MouseButton::Left,
                "Secondary" => MouseButton::Right,
                "Auxiliary" | "Middle" => MouseButton::Middle,
                _ => MouseButton::Left,
            });

    InputEvent::Mouse(input::MouseEvent {
        button,
        action,
        x: coords.x,
        y: coords.y,
        modifiers: convert_modifiers(&e.modifiers()),
    })
}

/// Convert a Dioxus wheel event to an input scroll event.
pub fn convert_wheel_event(e: &WheelEvent) -> InputEvent {
    let delta = e.delta().strip_units();
    InputEvent::Scroll(ScrollEvent {
        delta_x: delta.x,
        delta_y: delta.y,
        modifiers: convert_modifiers(&e.modifiers()),
    })
}
