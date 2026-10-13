//! Input service data types — key codes, events, filters.

use facet::Facet;

/// Platform-agnostic keyboard key code.
///
/// The host converts platform-specific codes (Windows VK, macOS
/// keycodes) into this enum before streaming to extensions.
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
pub enum KeyCode {
    /// A printable character key (lowercase). Space is `" "`.
    Character(String),
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    /// Function key (1–24).
    F(u8),
}

/// Modifier key state at the time of a key event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Facet)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// Type of keyboard message.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Facet)]
pub enum KeyMsgKind {
    KeyDown,
    KeyUp,
    SysKeyDown,
    SysKeyUp,
    Char,
}

/// Which DAW window context has focus.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Facet)]
pub enum InputContext {
    /// Main arrange view.
    Main,
    /// MIDI editor (floating or docked).
    Midi,
    /// Inline MIDI editor in arrange.
    MidiInline,
    /// Media explorer.
    MediaExplorer,
    /// Global context (applies everywhere).
    Global,
}

/// Keyboard event from the host's input hook.
#[derive(Debug, Clone, Facet)]
pub struct KeyEvent {
    pub key: KeyCode,
    pub modifiers: KeyModifiers,
    pub msg_kind: KeyMsgKind,
    pub context: InputContext,
    /// Whether a text input field currently has focus.
    pub is_text_focused: bool,
}

/// Describes which keys the host should eat (intercept).
///
/// The extension uploads this filter to the host. The host evaluates
/// it synchronously — no RPC round-trip per keypress.
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum KeyFilter {
    /// Eat all keys (except when text fields are focused).
    EatAll,
    /// Pass all keys through to the DAW (extension is passive).
    PassAll,
    /// Eat only keys matching specific patterns.
    EatMatching { patterns: Vec<KeyPattern> },
}

/// A specific key + modifier combination to match against.
#[derive(Debug, Clone, Facet)]
pub struct KeyPattern {
    pub key: KeyCode,
    pub modifiers: KeyModifiers,
    /// If true, modifiers must match exactly (no extras allowed).
    pub exact_modifiers: bool,
}

/// Events streamed from the host to extension processes.
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum InputEvent {
    /// A keyboard event that was eaten by the filter.
    Key(KeyEvent),
    /// A mouse wheel event.
    MouseWheel {
        delta: i16,
        horizontal: bool,
        context: InputContext,
    },
}

// SelfRef compatibility: InputEvent has no lifetime parameters,
// so Ref<'a> = Self.
#[allow(unsafe_code)]
unsafe impl vox_types::Reborrow for InputEvent {
    type Ref<'a> = InputEvent;
}
