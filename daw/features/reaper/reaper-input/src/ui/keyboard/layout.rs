//! Physical keyboard layout definition (QWERTY).
//!
//! Defines the key positions, sizes, and labels for rendering a keyboard
//! visualization. Based on the input-dioxus visualizer layout.

use input::KeyCode;

/// A single key on the keyboard.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyDef {
    /// Display label on the key cap.
    pub label: &'static str,
    /// The KeyCode this key represents (for binding lookup).
    pub code: Option<KeyCode>,
    /// Width multiplier (1.0 = standard key, 2.0 = double width, etc.)
    pub width: f64,
}

/// A row of keys.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyRow {
    pub keys: Vec<KeyDef>,
    /// Height multiplier (1.0 = standard, 0.8 = compact for function row).
    pub height: f64,
}

/// A block of key rows (main keyboard, nav cluster, etc.)
#[derive(Clone, Debug, PartialEq)]
pub struct KeyBlock {
    pub name: &'static str,
    pub rows: Vec<KeyRow>,
}

fn key(label: &'static str, code: KeyCode) -> KeyDef {
    KeyDef {
        label,
        code: Some(code),
        width: 1.0,
    }
}
fn key_w(label: &'static str, code: KeyCode, width: f64) -> KeyDef {
    KeyDef {
        label,
        code: Some(code),
        width,
    }
}
fn spacer(width: f64) -> KeyDef {
    KeyDef {
        label: "",
        code: None,
        width,
    }
}
fn char_key(c: &'static str) -> KeyDef {
    KeyDef {
        label: c,
        code: Some(KeyCode::Character(c.to_lowercase())),
        width: 1.0,
    }
}

/// Returns the full QWERTY keyboard layout.
pub fn qwerty_layout() -> Vec<KeyBlock> {
    vec![
        KeyBlock {
            name: "main",
            rows: vec![
                // Function row
                KeyRow {
                    height: 0.8,
                    keys: vec![
                        key("Esc", KeyCode::Escape),
                        spacer(0.5),
                        key("F1", KeyCode::F(1)),
                        key("F2", KeyCode::F(2)),
                        key("F3", KeyCode::F(3)),
                        key("F4", KeyCode::F(4)),
                        spacer(0.25),
                        key("F5", KeyCode::F(5)),
                        key("F6", KeyCode::F(6)),
                        key("F7", KeyCode::F(7)),
                        key("F8", KeyCode::F(8)),
                        spacer(0.25),
                        key("F9", KeyCode::F(9)),
                        key("F10", KeyCode::F(10)),
                        key("F11", KeyCode::F(11)),
                        key("F12", KeyCode::F(12)),
                    ],
                },
                // Number row
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
                        key_w("Bksp", KeyCode::Backspace, 2.0),
                    ],
                },
                // QWERTY row
                KeyRow {
                    height: 1.0,
                    keys: vec![
                        key_w("Tab", KeyCode::Tab, 1.5),
                        char_key("q"),
                        char_key("w"),
                        char_key("e"),
                        char_key("r"),
                        char_key("t"),
                        char_key("y"),
                        char_key("u"),
                        char_key("i"),
                        char_key("o"),
                        char_key("p"),
                        char_key("["),
                        char_key("]"),
                        key_w("\\", KeyCode::Character("\\".into()), 1.5),
                    ],
                },
                // Home row
                KeyRow {
                    height: 1.0,
                    keys: vec![
                        spacer(1.75), // Caps lock (no binding)
                        char_key("a"),
                        char_key("s"),
                        char_key("d"),
                        char_key("f"),
                        char_key("g"),
                        char_key("h"),
                        char_key("j"),
                        char_key("k"),
                        char_key("l"),
                        char_key(";"),
                        char_key("'"),
                        key_w("Enter", KeyCode::Enter, 2.25),
                    ],
                },
                // Bottom row
                KeyRow {
                    height: 1.0,
                    keys: vec![
                        spacer(2.25), // Left Shift
                        char_key("z"),
                        char_key("x"),
                        char_key("c"),
                        char_key("v"),
                        char_key("b"),
                        char_key("n"),
                        char_key("m"),
                        char_key(","),
                        char_key("."),
                        char_key("/"),
                        spacer(2.75), // Right Shift
                    ],
                },
                // Space row
                KeyRow {
                    height: 1.0,
                    keys: vec![
                        spacer(1.25), // Ctrl
                        spacer(1.25), // Super
                        spacer(1.25), // Alt
                        key_w("Space", KeyCode::Character(" ".into()), 6.25),
                        spacer(1.25), // Alt
                        spacer(1.25), // Super
                        spacer(1.25), // Menu
                        spacer(1.25), // Ctrl
                    ],
                },
            ],
        },
        KeyBlock {
            name: "nav",
            rows: vec![
                // Empty row (aligned with function row)
                KeyRow {
                    height: 0.8,
                    keys: vec![spacer(3.0)],
                },
                // Ins/Home/PgUp
                KeyRow {
                    height: 1.0,
                    keys: vec![
                        key("Ins", KeyCode::Delete),  // Placeholder
                        key("Home", KeyCode::Delete), // No Home in KeyCode
                        key("PgUp", KeyCode::Delete),
                    ],
                },
                // Del/End/PgDn
                KeyRow {
                    height: 1.0,
                    keys: vec![
                        key("Del", KeyCode::Delete),
                        key("End", KeyCode::Delete),
                        key("PgDn", KeyCode::Delete),
                    ],
                },
                // Empty row
                KeyRow {
                    height: 1.0,
                    keys: vec![spacer(3.0)],
                },
                // Arrow keys
                KeyRow {
                    height: 1.0,
                    keys: vec![spacer(1.0), key("↑", KeyCode::ArrowUp), spacer(1.0)],
                },
                KeyRow {
                    height: 1.0,
                    keys: vec![
                        key("←", KeyCode::ArrowLeft),
                        key("↓", KeyCode::ArrowDown),
                        key("→", KeyCode::ArrowRight),
                    ],
                },
            ],
        },
    ]
}

/// Base size in pixels for a 1.0 width key. Large enough to list several
/// bindings inside a single key cap.
pub const KEY_UNIT: f64 = 96.0;
/// Gap between keys in pixels.
pub const KEY_GAP: f64 = 3.0;
/// Gap between key blocks.
pub const BLOCK_GAP: f64 = 16.0;
