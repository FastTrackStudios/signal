//! Type definitions for REAPER keyboard mapping entries.

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

/// A complete keyboard mapping file, containing all entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeyMap {
    pub entries: Vec<Entry>,
}

/// A single entry in the keyboard mapping file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entry {
    Key(KeyBinding),
    Action(CustomAction),
    Script(ScriptBinding),
}

/// A KEY line binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBinding {
    pub modifier: u16,
    pub key: u16,
    pub command: CommandId,
    pub section: Section,
    /// If present, this key binding is global. The scope is determined by the
    /// paired global-flag KEY line that follows the binding KEY line.
    pub global: Option<GlobalScope>,
}

/// A command identifier: either a built-in numeric ID or a named action string.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandId {
    /// Built-in numeric command ID.
    Native(u32),
    /// Named action string (starts with `_`).
    Named(String),
}

/// Section identifiers for REAPER key bindings, actions, and scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Main,
    MainAlt,
    MidiEditor,
    MidiEventList,
    MidiInlineEditor,
    MediaExplorer,
    Unknown(u32),
}

impl Section {
    /// Convert a raw section integer to a `Section`.
    pub fn from_raw(value: u32) -> Self {
        match value {
            0 => Section::Main,
            100 => Section::MainAlt,
            32060 => Section::MidiEditor,
            32061 => Section::MidiEventList,
            32062 => Section::MidiInlineEditor,
            32063 => Section::MediaExplorer,
            other => Section::Unknown(other),
        }
    }

    /// Convert a `Section` back to its raw integer.
    pub fn to_raw(self) -> u32 {
        match self {
            Section::Main => 0,
            Section::MainAlt => 100,
            Section::MidiEditor => 32060,
            Section::MidiEventList => 32061,
            Section::MidiInlineEditor => 32062,
            Section::MediaExplorer => 32063,
            Section::Unknown(v) => v,
        }
    }
}

/// Global scope for a KEY binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalScope {
    /// Global (scope value 1, section 102 or 103).
    Global,
    /// Global + text fields (scope value 101, section 102 or 103).
    GlobalTextFields,
}

impl GlobalScope {
    /// Convert from the raw global_key_scope value.
    pub fn from_raw(value: u32) -> Option<Self> {
        match value {
            1 => Some(GlobalScope::Global),
            101 => Some(GlobalScope::GlobalTextFields),
            _ => None,
        }
    }

    /// Convert to the raw global_key_scope value.
    pub fn to_raw(self) -> u32 {
        match self {
            GlobalScope::Global => 1,
            GlobalScope::GlobalTextFields => 101,
        }
    }
}

bitflags! {
    /// Flags for custom/composite actions (ACT lines).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ActionFlags: u32 {
        /// Consolidate undo points.
        const CONSOLIDATE_UNDO = 1;
        /// Show in actions menu.
        const SHOW_IN_MENU = 2;
        /// Checkbox state bit 0.
        const CHECKBOX_16 = 16;
        /// Checkbox state bit 1.
        const CHECKBOX_32 = 32;
    }
}

impl Serialize for ActionFlags {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.bits().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ActionFlags {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bits = u32::deserialize(deserializer)?;
        Ok(ActionFlags::from_bits_truncate(bits))
    }
}

bitflags! {
    /// Flags for script bindings (SCR lines).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ScriptFlags: u32 {
        /// Consolidate undo points.
        const CONSOLIDATE_UNDO = 1;
        /// Show in actions menu.
        const SHOW_IN_MENU = 2;
        /// Dialog if running (default behavior, value 4).
        const DIALOG_IF_RUNNING = 4;
        /// Terminate all instances (value 256).
        const TERMINATE_INSTANCES = 256;
        /// New instance always (value 512).
        const NEW_INSTANCE = 512;
    }
}

impl Serialize for ScriptFlags {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.bits().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ScriptFlags {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bits = u32::deserialize(deserializer)?;
        Ok(ScriptFlags::from_bits_truncate(bits))
    }
}

/// A custom/composite action (ACT line).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomAction {
    pub flags: ActionFlags,
    pub section: Section,
    /// The action command ID string (without quotes).
    pub command_id: String,
    /// Human-readable description (without quotes).
    pub description: String,
    /// List of component action IDs that make up this composite action.
    pub actions: Vec<CommandId>,
}

/// A script binding (SCR line).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptBinding {
    pub flags: ScriptFlags,
    pub section: Section,
    /// The user-defined action command ID string.
    pub command_id: String,
    /// Human-readable description (without quotes).
    pub description: String,
    /// Script filename (e.g. `myscript.lua`).
    pub script_file: String,
}

// ──────────────────────────────────────────────────────────────────────────────
// Special key decoding (modifier == 255)
// ──────────────────────────────────────────────────────────────────────────────

/// Decoded variant of a special key when `modifier == 255`.
///
/// When the modifier field is 255 the key value is not a normal keyboard
/// virtual-key code; instead it encodes touch / gesture / media-keyboard
/// events.  The numeric ranges are drawn from REAPER's own documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecialKey {
    // ── Multi-touch zoom ──────────────────────────────────────────────────
    /// Multi-touch zoom in (key 72).
    MultiZoomIn,
    /// Multi-touch zoom out (key 73).
    MultiZoomOut,
    /// Multi-touch zoom reset (key 74).
    MultiZoomReset,
    /// Multi-touch zoom with finger count 1–8 (keys 200–207; finger = key - 199).
    MultiZoomFingers(u8),

    // ── Multi-touch rotate ────────────────────────────────────────────────
    /// Multi-touch rotate clockwise (key 24).
    MultiRotateCw,
    /// Multi-touch rotate counter-clockwise (key 25).
    MultiRotateCcw,
    /// Multi-touch rotate with finger count 1–8 (keys 152–159; finger = key - 151).
    MultiRotateFingers(u8),

    // ── Multi-touch swipe (horizontal) ────────────────────────────────────
    /// Multi-touch swipe right (key 40).
    MultiSwipeRight,
    /// Multi-touch swipe left (key 56).
    MultiSwipeLeft,
    /// Multi-touch horizontal swipe with finger count 1–8 (keys 168–175; finger = key - 167).
    MultiSwipeHorizFingers(u8),

    // ── Multi-touch swipe (vertical) ─────────────────────────────────────
    /// Multi-touch swipe down (keys 176–183; finger = key - 175).
    MultiSwipeVertFingers(u8),
    /// Multi-touch swipe up (keys 184–191; finger = key - 183).
    MultiSwipeUpFingers(u8),

    // ── Horizontal mousewheel ─────────────────────────────────────────────
    /// Horizontal mousewheel scroll right (key 88).
    MousewheelHorizRight,
    /// Horizontal mousewheel with finger / step modifier 1–8 (keys 216–223; n = key - 215).
    MousewheelHorizN(u8),

    // ── Normal mousewheel ─────────────────────────────────────────────────
    /// Mousewheel scroll up (key 120).
    MousewheelUp,
    /// Mousewheel scroll down (key 121).
    MousewheelDown,
    /// Mousewheel scroll up fast (key 122).
    MousewheelUpFast,
    /// Mousewheel scroll down fast (key 123).
    MousewheelDownFast,
    /// Normal mousewheel with step modifier 1–8 (keys 248–255; n = key - 247).
    MousewheelN(u8),

    // ── Media keyboard ────────────────────────────────────────────────────
    /// Media keyboard: Play (key 232, then +256 per additional modifier step).
    MediaKbdPlay,
    /// Media keyboard: Stop (key 488 = 232 + 256).
    MediaKbdStop,
    /// Media keyboard: Previous track (key 744 = 232 + 512).
    MediaKbdPrev,
    /// Media keyboard: Next track (key 1000 = 232 + 768).
    MediaKbdNext,
    /// Media keyboard: Fast-forward (key 1256 = 232 + 1024).
    MediaKbdFastForward,
    /// Media keyboard: Rewind (key 1512 = 232 + 1280).
    MediaKbdRewind,
    /// Media keyboard: Record (key 1768 = 232 + 1536).
    MediaKbdRecord,
    /// Media keyboard: Mute (key 2024 = 232 + 1792).
    MediaKbdMute,
    /// Media keyboard: Volume up (key 2280 = 232 + 2048).
    MediaKbdVolumeUp,
    /// Media keyboard: Volume down (key 2536 = 232 + 2304).
    MediaKbdVolumeDown,

    /// Any other modifier-255 key value that is not yet mapped.
    Unknown(u16),
}

impl SpecialKey {
    /// Decode a raw `key` value when `modifier == 255`.
    pub fn from_key(key: u16) -> Self {
        match key {
            // Multi-touch zoom
            72 => SpecialKey::MultiZoomIn,
            73 => SpecialKey::MultiZoomOut,
            74 => SpecialKey::MultiZoomReset,
            200..=207 => SpecialKey::MultiZoomFingers((key - 199) as u8),

            // Multi-touch rotate
            24 => SpecialKey::MultiRotateCw,
            25 => SpecialKey::MultiRotateCcw,
            152..=159 => SpecialKey::MultiRotateFingers((key - 151) as u8),

            // Multi-touch swipe horizontal
            40 => SpecialKey::MultiSwipeRight,
            56 => SpecialKey::MultiSwipeLeft,
            168..=175 => SpecialKey::MultiSwipeHorizFingers((key - 167) as u8),

            // Multi-touch swipe vertical
            176..=183 => SpecialKey::MultiSwipeVertFingers((key - 175) as u8),
            184..=191 => SpecialKey::MultiSwipeUpFingers((key - 183) as u8),

            // Horizontal mousewheel
            88 => SpecialKey::MousewheelHorizRight,
            216..=223 => SpecialKey::MousewheelHorizN((key - 215) as u8),

            // Normal mousewheel
            120 => SpecialKey::MousewheelUp,
            121 => SpecialKey::MousewheelDown,
            122 => SpecialKey::MousewheelUpFast,
            123 => SpecialKey::MousewheelDownFast,
            248..=255 => SpecialKey::MousewheelN((key - 247) as u8),

            // Media keyboard (base 232, steps of 256)
            232 => SpecialKey::MediaKbdPlay,
            488 => SpecialKey::MediaKbdStop,
            744 => SpecialKey::MediaKbdPrev,
            1000 => SpecialKey::MediaKbdNext,
            1256 => SpecialKey::MediaKbdFastForward,
            1512 => SpecialKey::MediaKbdRewind,
            1768 => SpecialKey::MediaKbdRecord,
            2024 => SpecialKey::MediaKbdMute,
            2280 => SpecialKey::MediaKbdVolumeUp,
            2536 => SpecialKey::MediaKbdVolumeDown,

            other => SpecialKey::Unknown(other),
        }
    }

    /// Encode this `SpecialKey` back to its raw `key` value (for use with modifier 255).
    pub fn to_key(self) -> u16 {
        match self {
            SpecialKey::MultiZoomIn => 72,
            SpecialKey::MultiZoomOut => 73,
            SpecialKey::MultiZoomReset => 74,
            SpecialKey::MultiZoomFingers(n) => 199 + n as u16,

            SpecialKey::MultiRotateCw => 24,
            SpecialKey::MultiRotateCcw => 25,
            SpecialKey::MultiRotateFingers(n) => 151 + n as u16,

            SpecialKey::MultiSwipeRight => 40,
            SpecialKey::MultiSwipeLeft => 56,
            SpecialKey::MultiSwipeHorizFingers(n) => 167 + n as u16,

            SpecialKey::MultiSwipeVertFingers(n) => 175 + n as u16,
            SpecialKey::MultiSwipeUpFingers(n) => 183 + n as u16,

            SpecialKey::MousewheelHorizRight => 88,
            SpecialKey::MousewheelHorizN(n) => 215 + n as u16,

            SpecialKey::MousewheelUp => 120,
            SpecialKey::MousewheelDown => 121,
            SpecialKey::MousewheelUpFast => 122,
            SpecialKey::MousewheelDownFast => 123,
            SpecialKey::MousewheelN(n) => 247 + n as u16,

            SpecialKey::MediaKbdPlay => 232,
            SpecialKey::MediaKbdStop => 488,
            SpecialKey::MediaKbdPrev => 744,
            SpecialKey::MediaKbdNext => 1000,
            SpecialKey::MediaKbdFastForward => 1256,
            SpecialKey::MediaKbdRewind => 1512,
            SpecialKey::MediaKbdRecord => 1768,
            SpecialKey::MediaKbdMute => 2024,
            SpecialKey::MediaKbdVolumeUp => 2280,
            SpecialKey::MediaKbdVolumeDown => 2536,

            SpecialKey::Unknown(k) => k,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Standard key-code name lookup
// ──────────────────────────────────────────────────────────────────────────────

/// Return a human-readable name for a standard keyboard virtual-key code
/// (used when `modifier != 255`).
///
/// Covers common non-printable keys, function keys F1–F24, and the
/// alphanumeric range.  Returns `None` for key codes that are not in the
/// lookup table — callers should fall back to showing the raw number or the
/// printable ASCII character.
pub fn key_name(key: u16) -> Option<&'static str> {
    // Common non-printable / control keys
    match key {
        8 => Some("Backspace"),
        9 => Some("Tab"),
        12 => Some("Clear"),
        13 => Some("Enter"),
        16 => Some("Shift"),
        17 => Some("Ctrl"),
        18 => Some("Alt"),
        19 => Some("Pause"),
        20 => Some("CapsLock"),
        27 => Some("Escape"),
        32 => Some("Space"),
        33 => Some("PageUp"),
        34 => Some("PageDown"),
        35 => Some("End"),
        36 => Some("Home"),
        37 => Some("Left"),
        38 => Some("Up"),
        39 => Some("Right"),
        40 => Some("Down"),
        44 => Some("PrintScreen"),
        45 => Some("Insert"),
        46 => Some("Delete"),
        // Numpad
        96 => Some("Num0"),
        97 => Some("Num1"),
        98 => Some("Num2"),
        99 => Some("Num3"),
        100 => Some("Num4"),
        101 => Some("Num5"),
        102 => Some("Num6"),
        103 => Some("Num7"),
        104 => Some("Num8"),
        105 => Some("Num9"),
        106 => Some("Multiply"),
        107 => Some("Add"),
        108 => Some("Separator"),
        109 => Some("Subtract"),
        110 => Some("Decimal"),
        111 => Some("Divide"),
        // Function keys F1–F24
        112 => Some("F1"),
        113 => Some("F2"),
        114 => Some("F3"),
        115 => Some("F4"),
        116 => Some("F5"),
        117 => Some("F6"),
        118 => Some("F7"),
        119 => Some("F8"),
        120 => Some("F9"),
        121 => Some("F10"),
        122 => Some("F11"),
        123 => Some("F12"),
        124 => Some("F13"),
        125 => Some("F14"),
        126 => Some("F15"),
        127 => Some("F16"),
        128 => Some("F17"),
        129 => Some("F18"),
        130 => Some("F19"),
        131 => Some("F20"),
        132 => Some("F21"),
        133 => Some("F22"),
        134 => Some("F23"),
        135 => Some("F24"),
        // Misc
        144 => Some("NumLock"),
        145 => Some("ScrollLock"),
        186 => Some("Semicolon"),
        187 => Some("Equals"),
        188 => Some("Comma"),
        189 => Some("Minus"),
        190 => Some("Period"),
        191 => Some("Slash"),
        192 => Some("Backtick"),
        219 => Some("LeftBracket"),
        220 => Some("Backslash"),
        221 => Some("RightBracket"),
        222 => Some("Quote"),
        _ => None,
    }
}

/// Convenience: if `modifier == 255`, decode the key as a `SpecialKey`;
/// otherwise return `None`.
pub fn decode_special_key(modifier: u16, key: u16) -> Option<SpecialKey> {
    if modifier == 255 {
        Some(SpecialKey::from_key(key))
    } else {
        None
    }
}

// ──────────────────────────────────────────────────────────────────────────────

/// Classification of a MIDI binding based on modifier value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MidiBinding {
    NoteOn { channel: u8, note: u8 },
    ControlChange { channel: u8, cc: u8 },
    ProgramChange { channel: u8, program: u8 },
    PitchBend { channel: u8 },
}

impl MidiBinding {
    /// Try to interpret a modifier + key pair as a MIDI binding.
    pub fn from_modifier_key(modifier: u16, key: u16) -> Option<Self> {
        let m = modifier;
        let k = key as u8;
        match m {
            144..=159 => Some(MidiBinding::NoteOn {
                channel: (m - 144) as u8 + 1,
                note: k,
            }),
            176..=191 => Some(MidiBinding::ControlChange {
                channel: (m - 176) as u8 + 1,
                cc: k,
            }),
            192..=207 => Some(MidiBinding::ProgramChange {
                channel: (m - 192) as u8 + 1,
                program: k,
            }),
            224..=239 => Some(MidiBinding::PitchBend {
                channel: (m - 224) as u8 + 1,
            }),
            _ => None,
        }
    }
}
