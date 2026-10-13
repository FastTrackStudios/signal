//! Static QWERTY keyboard layout definition for the visualizer.
//!
//! Modeled after the MPL ReaImGui script's block/row/position system.
//! Three blocks: main keyboard, navigation cluster, numpad.

use input::key::KeyCode;

/// A single key on the visual keyboard.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyDef {
    /// Display label (what's printed on the key cap).
    pub label: &'static str,
    /// Corresponding `KeyCode` for binding lookup.
    pub key_code: KeyCode,
    /// Width in standard key units (1.0 = normal key).
    pub width: f32,
}

/// A row of keys.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyRow {
    /// Keys in this row, left to right.
    pub keys: Vec<KeyDef>,
    /// Row height multiplier (1.0 = standard).
    pub height: f32,
}

/// A block of key rows (main, nav, numpad).
#[derive(Debug, Clone, PartialEq)]
pub struct KeyBlock {
    pub name: &'static str,
    pub rows: Vec<KeyRow>,
}

fn key(label: &'static str, code: KeyCode) -> KeyDef {
    KeyDef {
        label,
        key_code: code,
        width: 1.0,
    }
}

fn wide(label: &'static str, code: KeyCode, width: f32) -> KeyDef {
    KeyDef {
        label,
        key_code: code,
        width,
    }
}

fn char_key(label: &'static str) -> KeyDef {
    key(label, KeyCode::Character(label.to_lowercase()))
}

fn char_wide(label: &'static str, width: f32) -> KeyDef {
    wide(label, KeyCode::Character(label.to_lowercase()), width)
}

/// Build the full QWERTY keyboard layout.
pub fn qwerty_layout() -> Vec<KeyBlock> {
    vec![main_block(), nav_block()]
}

fn main_block() -> KeyBlock {
    KeyBlock {
        name: "main",
        rows: vec![
            // Row 0: Function keys
            KeyRow {
                height: 0.8,
                keys: vec![
                    key("Esc", KeyCode::Escape),
                    key("F1", KeyCode::F(1)),
                    key("F2", KeyCode::F(2)),
                    key("F3", KeyCode::F(3)),
                    key("F4", KeyCode::F(4)),
                    key("F5", KeyCode::F(5)),
                    key("F6", KeyCode::F(6)),
                    key("F7", KeyCode::F(7)),
                    key("F8", KeyCode::F(8)),
                    key("F9", KeyCode::F(9)),
                    key("F10", KeyCode::F(10)),
                    key("F11", KeyCode::F(11)),
                    key("F12", KeyCode::F(12)),
                ],
            },
            // Row 1: Number row
            KeyRow {
                height: 1.0,
                keys: vec![
                    char_key("`"),
                    char_key("1"),
                    char_key("2"),
                    char_key("3"),
                    char_key("4"),
                    char_key("5"),
                    char_key("6"),
                    char_key("7"),
                    char_key("8"),
                    char_key("9"),
                    char_key("0"),
                    char_key("-"),
                    char_key("="),
                    wide("BS", KeyCode::Backspace, 2.0),
                ],
            },
            // Row 2: QWERTY row
            KeyRow {
                height: 1.0,
                keys: vec![
                    wide("Tab", KeyCode::Tab, 1.5),
                    char_key("Q"),
                    char_key("W"),
                    char_key("E"),
                    char_key("R"),
                    char_key("T"),
                    char_key("Y"),
                    char_key("U"),
                    char_key("I"),
                    char_key("O"),
                    char_key("P"),
                    char_key("["),
                    char_key("]"),
                    char_wide("\\", 1.5),
                ],
            },
            // Row 3: Home row
            KeyRow {
                height: 1.0,
                keys: vec![
                    // "Caps" doesn't have a KeyCode — we use a placeholder
                    wide("Caps", KeyCode::Character("capslock".to_string()), 2.0),
                    char_key("A"),
                    char_key("S"),
                    char_key("D"),
                    char_key("F"),
                    char_key("G"),
                    char_key("H"),
                    char_key("J"),
                    char_key("K"),
                    char_key("L"),
                    char_key(";"),
                    char_key("'"),
                    wide("Enter", KeyCode::Enter, 2.0),
                ],
            },
            // Row 4: Bottom row
            KeyRow {
                height: 1.0,
                keys: vec![
                    wide("Shift", KeyCode::Character("shift".to_string()), 2.5),
                    char_key("Z"),
                    char_key("X"),
                    char_key("C"),
                    char_key("V"),
                    char_key("B"),
                    char_key("N"),
                    char_key("M"),
                    char_key(","),
                    char_key("."),
                    char_key("/"),
                    wide("Shift", KeyCode::Character("shift".to_string()), 2.5),
                ],
            },
            // Row 5: Space row
            KeyRow {
                height: 1.0,
                keys: vec![
                    wide("Ctrl", KeyCode::Character("ctrl".to_string()), 1.5),
                    wide("Alt", KeyCode::Character("alt".to_string()), 1.5),
                    wide("Cmd", KeyCode::Character("meta".to_string()), 1.5),
                    wide("Space", KeyCode::Character(" ".to_string()), 6.0),
                    wide("Cmd", KeyCode::Character("meta".to_string()), 1.5),
                    wide("Alt", KeyCode::Character("alt".to_string()), 1.5),
                    wide("Ctrl", KeyCode::Character("ctrl".to_string()), 1.5),
                ],
            },
        ],
    }
}

fn nav_block() -> KeyBlock {
    KeyBlock {
        name: "nav",
        rows: vec![
            // Row 0: empty (aligns with F-key row)
            KeyRow {
                height: 0.8,
                keys: vec![],
            },
            // Row 1: Ins/Home/PgUp
            KeyRow {
                height: 1.0,
                keys: vec![
                    key("Ins", KeyCode::Character("insert".to_string())),
                    key("Home", KeyCode::Character("home".to_string())),
                    key("PgUp", KeyCode::Character("pageup".to_string())),
                ],
            },
            // Row 2: Del/End/PgDn
            KeyRow {
                height: 1.0,
                keys: vec![
                    key("Del", KeyCode::Delete),
                    key("End", KeyCode::Character("end".to_string())),
                    key("PgDn", KeyCode::Character("pagedown".to_string())),
                ],
            },
            // Row 3: empty spacer
            KeyRow {
                height: 1.0,
                keys: vec![],
            },
            // Row 4: Arrow keys
            KeyRow {
                height: 1.0,
                keys: vec![
                    // spacer width handled by the component's gap logic
                    key("↑", KeyCode::ArrowUp),
                ],
            },
            // Row 5: Arrow bottom row
            KeyRow {
                height: 1.0,
                keys: vec![
                    key("←", KeyCode::ArrowLeft),
                    key("↓", KeyCode::ArrowDown),
                    key("→", KeyCode::ArrowRight),
                ],
            },
        ],
    }
}
