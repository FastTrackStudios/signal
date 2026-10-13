//! Input State Management
//!
//! Manages the state of key sequences, modes, and context for FTS-Input.

use crate::input::error::{lock_mut, lock_or_default};
use std::sync::{Mutex, OnceLock};

/// The current mode of the input system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Normal mode - standard operations
    #[default]
    Normal,
    /// Visual timeline mode - motions extend time selection
    VisualTimeline,
    /// Visual track mode - motions extend track selection
    VisualTrack,
}

/// The context where commands are executed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Context {
    /// Main arrange view
    #[default]
    Main,
    /// MIDI editor
    Midi,
    /// MIDI Event List Editor
    MidiEventListEditor,
    /// MIDI Inline Editor
    MidiInlineEditor,
    /// Media Explorer
    MediaExplorer,
    /// Crossfade Editor
    CrossfadeEditor,
    /// The FTS Expression Editor panel (registered by the extension via
    /// [`crate::input::window_detection::set_panel_context_probe`]).
    /// Its own input surface: only bindings declared for it fire, and
    /// unbound keys go to the panel itself, never to REAPER.
    ExpressionEditor,
    /// Global (applies to both main and midi)
    Global,
}

/// A single keypress in a sequence
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPress {
    /// The key character (e.g. `t`, `L`, `<ESC>`)
    pub key: String,

    /// Current context
    pub context: Context,
}

/// The current command state
#[derive(Debug, Clone)]
pub struct CommandState {
    /// Current key sequence being built (e.g., "tL")
    pub key_sequence: String,

    /// Current mode
    pub mode: Mode,

    /// Current context
    pub context: Context,

    /// Whether we're recording a macro
    pub macro_recording: bool,

    /// The register for macro recording/playback
    pub macro_register: Option<char>,

    /// Last executed command (for repeat)
    pub last_command: Option<Command>,

    /// Timeline selection side (left or right)
    pub timeline_selection_side: String,
}

/// A complete command ready to execute
#[derive(Debug, Clone)]
pub struct Command {
    /// The action sequence (e.g., ["timeline_operator", "timeline_motion"])
    pub action_sequence: Vec<ActionType>,

    /// The action keys/identifiers for each action type
    pub action_keys: Vec<ActionKey>,

    /// The mode this command was executed in
    pub mode: Mode,

    /// The context this command was executed in
    pub context: Context,
}

/// Action types that can be composed
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ActionType {
    Command,
    TimelineMotion,
    TimelineOperator,
    TimelineSelector,
    TrackMotion,
    TrackOperator,
    TrackSelector,
    VisualTimelineCommand,
    VisualTrackCommand,
}

/// An action key with optional metadata
#[derive(Debug, Clone)]
pub struct ActionKey {
    /// The action identifier (command ID or name)
    pub identifier: String,

    /// Optional repetition count (for numeric prefixes like "5p")
    pub repetition_count: Option<u32>,

    /// Optional register (for macro operations)
    pub register: Option<char>,
}

/// Global state storage (in-memory for now, TODO: persist to REAPER's external state)
static INPUT_STATE: OnceLock<Mutex<CommandState>> = OnceLock::new();

impl CommandState {
    /// Get the current command state
    pub fn get() -> CommandState {
        let mutex = INPUT_STATE.get_or_init(|| Mutex::new(CommandState::default()));
        lock_or_default(mutex)
    }

    /// Set the command state
    pub fn set(state: CommandState) {
        let mutex = INPUT_STATE.get_or_init(|| Mutex::new(CommandState::default()));
        lock_mut(mutex, |current| {
            *current = state;
        });
        // TODO: Persist to REAPER's external state for persistence across sessions
    }

    /// Reset to default state
    pub fn reset() {
        Self::set(CommandState::default());
    }

    /// Append a keypress to the current sequence
    pub fn append_key(&mut self, keypress: KeyPress) {
        // If ESC, reset sequence
        if keypress.key == "<ESC>" {
            self.key_sequence = String::new();
            return;
        }

        // Update context if sequence is empty
        if self.key_sequence.is_empty() {
            self.context = keypress.context;
        }

        // Append key to sequence
        self.key_sequence.push_str(&keypress.key);
    }

    /// Clear the key sequence
    pub fn clear_sequence(&mut self) {
        self.key_sequence.clear();
    }
}

impl Default for CommandState {
    fn default() -> Self {
        Self {
            key_sequence: String::new(),
            mode: Mode::Normal,
            context: Context::Main,
            macro_recording: false,
            macro_register: None,
            last_command: None,
            timeline_selection_side: "left".to_string(),
        }
    }
}
