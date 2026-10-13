//! Key chord type for key+modifier combinations.

// region: --- Key Types

/// A keyboard key code.
///
/// Mirrors the `KeyCode` from `actions-keybindings` so the input crate can
/// remain self-contained while the upstream `vox-session` dependency is broken.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    /// Function key (1-12).
    F(u8),
}

/// Modifier key state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        meta: false,
    };
}

/// A key chord: a single key press with modifiers.
///
/// This is the fundamental unit used as trie keys and HashMap keys
/// for keybinding lookups.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub key: KeyCode,
    pub modifiers: Modifiers,
}

impl KeyChord {
    pub fn new(key: KeyCode, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    /// Create a chord with no modifiers.
    pub fn plain(key: KeyCode) -> Self {
        Self {
            key,
            modifiers: Modifiers::NONE,
        }
    }
}

// endregion: --- Key Types
