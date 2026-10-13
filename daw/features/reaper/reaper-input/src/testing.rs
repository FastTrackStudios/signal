//! Test injection API for programmatic input simulation.
//!
//! Provides functions to simulate key presses, inject key sequences, execute
//! actions by name, and query state — all without going through REAPER's
//! TranslateAccel window hook. Designed for integration tests.
//!
//! # Example
//! ```rust,ignore
//! use reaper_input::testing;
//!
//! // Simulate pressing 'g' then 'g' (goto start)
//! let cmds = testing::inject_key('g', Mods::NONE);
//! assert!(cmds.iter().any(|c| matches!(c, InputCommand::Pending { .. })));
//!
//! let cmds = testing::inject_key('g', Mods::NONE);
//! assert!(cmds.iter().any(|c| matches!(c, InputCommand::Action(_))));
//!
//! // Or inject a full sequence at once
//! let cmds = testing::inject_sequence("<C-s>");
//!
//! // Query state
//! assert_eq!(testing::active_preset(), "FastTrackStudio");
//! assert!(testing::is_enabled());
//! ```

use input::{InputCommand, InputEvent, KeyCode, KeyEvent, Modifiers};

use crate::input::processor;

// ---------------------------------------------------------------------------
// Modifier helpers
// ---------------------------------------------------------------------------

/// Modifier flags for test key injection.
#[derive(Clone, Copy, Debug, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Mods {
    pub const NONE: Self = Self {
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    };
    pub const CTRL: Self = Self {
        ctrl: true,
        shift: false,
        alt: false,
        meta: false,
    };
    pub const SHIFT: Self = Self {
        ctrl: false,
        shift: true,
        alt: false,
        meta: false,
    };
    pub const ALT: Self = Self {
        ctrl: false,
        shift: false,
        alt: true,
        meta: false,
    };
    pub const CTRL_SHIFT: Self = Self {
        ctrl: true,
        shift: true,
        alt: false,
        meta: false,
    };

    fn to_modifiers(self) -> Modifiers {
        Modifiers {
            ctrl: self.ctrl,
            shift: self.shift,
            alt: self.alt,
            meta: self.meta,
        }
    }
}

// ---------------------------------------------------------------------------
// Key injection
// ---------------------------------------------------------------------------

/// Inject a single key press into the input processor.
///
/// Returns the commands produced (Action, Pending, Unhandled, etc.).
/// Does NOT execute the actions — the caller decides what to do with them.
pub fn inject_key(key: char, mods: Mods) -> Vec<InputCommand> {
    let event = InputEvent::Key(KeyEvent {
        key: KeyCode::Character(key.to_string()),
        modifiers: mods.to_modifiers(),
    });
    inject_event(event)
}

/// Inject a special key press (F-keys, arrows, etc.) into the input processor.
pub fn inject_special_key(key: KeyCode, mods: Mods) -> Vec<InputCommand> {
    let event = InputEvent::Key(KeyEvent {
        key,
        modifiers: mods.to_modifiers(),
    });
    inject_event(event)
}

/// Inject a raw `InputEvent` into the processor.
pub fn inject_event(event: InputEvent) -> Vec<InputCommand> {
    let proc = processor::get_processor();
    let mut proc = proc.write().unwrap();
    proc.process_event(event)
}

/// Inject a key sequence string (e.g. `gg`, `<C-s>`, `<S-Tab>`).
///
/// Parses the sequence into individual key events and injects them one at a time.
/// Returns the commands from the LAST key in the sequence (which is typically
/// the one that completes the binding).
pub fn inject_sequence(sequence: &str) -> Vec<InputCommand> {
    let chords = parse_sequence_to_events(sequence);
    let mut last_cmds = Vec::new();
    for event in chords {
        last_cmds = inject_event(event);
    }
    last_cmds
}

/// Inject a sequence and collect ALL commands from every key press.
pub fn inject_sequence_all(sequence: &str) -> Vec<Vec<InputCommand>> {
    let chords = parse_sequence_to_events(sequence);
    chords.into_iter().map(inject_event).collect()
}

// ---------------------------------------------------------------------------
// Action execution
// ---------------------------------------------------------------------------

/// Execute a REAPER action by command name string.
///
/// This calls the action directly via REAPER's API, bypassing the input
/// processor entirely. Useful for testing action handlers.
///
/// Note: Requires a running REAPER instance (for Main_OnCommand).
pub fn execute_action(command_name: &str) {
    use reaper_high::Reaper;
    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();

    // Try numeric first
    if let Ok(id) = command_name.parse::<u32>() {
        medium_reaper.low().Main_OnCommand(id as i32, 0);
        return;
    }

    // Named command lookup
    let c_name = std::ffi::CString::new(command_name).unwrap();
    let cmd_id = unsafe { medium_reaper.low().NamedCommandLookup(c_name.as_ptr()) };
    if cmd_id != 0 {
        medium_reaper.low().Main_OnCommand(cmd_id, 0);
    } else {
        // Try with underscore prefix
        let c_name2 = std::ffi::CString::new(format!("_{command_name}")).unwrap();
        let cmd_id2 = unsafe { medium_reaper.low().NamedCommandLookup(c_name2.as_ptr()) };
        if cmd_id2 != 0 {
            medium_reaper.low().Main_OnCommand(cmd_id2, 0);
        }
    }
}

// ---------------------------------------------------------------------------
// State queries
// ---------------------------------------------------------------------------

/// Get the current active preset name.
pub fn active_preset() -> String {
    processor::active_preset_name()
}

/// Get available presets as (id, display_name) pairs.
pub fn available_presets() -> Vec<(String, String)> {
    processor::available_presets()
}

/// Get available overrides.
pub fn available_overrides() -> Vec<String> {
    processor::available_overrides()
}

/// Check if an override is active.
pub fn is_override_active(name: &str) -> bool {
    processor::is_override_active(name)
}

/// Get the pending key sequence display string (if any).
pub fn pending_display() -> Option<String> {
    processor::pending_display()
}

/// Check if the input system is enabled.
pub fn is_enabled() -> bool {
    crate::is_enabled()
}

/// Check if input interception is active.
pub fn is_intercepting() -> bool {
    crate::is_intercepting()
}

/// Get the current profile name.
pub fn current_profile() -> Option<String> {
    crate::current_profile().map(|p| p.as_str().to_string())
}

/// Reset the processor state (clear pending sequence, etc.).
pub fn reset_state() {
    let proc = processor::get_processor();
    let mut proc = proc.write().unwrap();
    proc.clear_pending();
}

/// Set the input context for subsequent key injections.
pub fn set_context(context: crate::input::KeybindContext) {
    let proc = processor::get_processor();
    let mut proc = proc.write().unwrap();
    proc.set_reaper_context(context);
}

// ---------------------------------------------------------------------------
// Sequence parsing
// ---------------------------------------------------------------------------

/// Parse a key sequence string into InputEvents.
///
/// Supports:
/// - Single chars: "a", "g", "1"
/// - Modifier combos: "<C-s>", "<S-Tab>", "<C-S-i>", "<M-w>"
/// - Multi-key sequences: "gg", "tL" (each char becomes a separate event)
/// - Special keys: "<Esc>", "<Enter>", "<Tab>", "<Space>", "<F1>"-"<F12>"
/// - Arrow keys: "<Up>", "<Down>", "<Left>", "<Right>"
fn parse_sequence_to_events(sequence: &str) -> Vec<InputEvent> {
    let mut events = Vec::new();
    let mut chars = sequence.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '<' {
            // Parse a <...> bracketed key
            let mut bracket = String::new();
            for c in chars.by_ref() {
                if c == '>' {
                    break;
                }
                bracket.push(c);
            }
            if let Some(event) = parse_bracket_key(&bracket) {
                events.push(event);
            }
        } else {
            // Plain character
            events.push(InputEvent::Key(KeyEvent {
                key: KeyCode::Character(ch.to_string()),
                modifiers: Modifiers::default(),
            }));
        }
    }

    events
}

/// Parse a bracketed key like "C-s", "S-Tab", "C-S-i", "M-w", "F1", "Esc".
fn parse_bracket_key(bracket: &str) -> Option<InputEvent> {
    let mut mods = Modifiers::default();
    let mut remaining = bracket;

    // Parse modifier prefixes: C-, S-, A-, M-, D-
    loop {
        if let Some(rest) = remaining.strip_prefix("C-") {
            mods.ctrl = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix("S-") {
            mods.shift = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix("A-") {
            mods.alt = true;
            remaining = rest;
        } else if let Some(rest) = remaining
            .strip_prefix("M-")
            .or_else(|| remaining.strip_prefix("D-"))
        {
            mods.meta = true;
            remaining = rest;
        } else {
            break;
        }
    }

    // Parse the key name
    let key = match remaining.to_lowercase().as_str() {
        "esc" | "escape" => KeyCode::Escape,
        "enter" | "return" | "cr" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "space" => KeyCode::Character(" ".to_string()),
        "bs" | "backspace" => KeyCode::Backspace,
        "del" | "delete" => KeyCode::Delete,
        "up" => KeyCode::ArrowUp,
        "down" => KeyCode::ArrowDown,
        "left" => KeyCode::ArrowLeft,
        "right" => KeyCode::ArrowRight,
        // Home/End not in KeyCode enum — treat as unhandled
        "home" | "end" => return None,
        s if s.starts_with('f') && s.len() <= 3 => {
            if let Ok(n) = s[1..].parse::<u8>() {
                KeyCode::F(n)
            } else {
                return None;
            }
        }
        s if s.len() == 1 => KeyCode::Character(s.to_string()),
        _ => return None,
    };

    Some(InputEvent::Key(KeyEvent {
        key,
        modifiers: mods,
    }))
}
