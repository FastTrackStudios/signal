//! Key Bindings Definitions
//!
//! Defines the key sequence bindings for FTS-Input.
//! Similar to reaper-keys' bindings.lua, but in Rust.

use crate::input::state::{ActionType, Context};
use std::collections::HashMap;

/// A binding entry - maps a key sequence to an action
#[derive(Debug, Clone)]
pub enum BindingEntry {
    /// A direct action binding
    Action(String),
    /// A folder containing nested bindings
    Folder {
        name: String,
        bindings: HashMap<String, BindingEntry>,
    },
}

/// Bindings organized by action type and context
#[derive(Debug, Clone)]
pub struct Bindings {
    /// Global bindings (apply to all contexts)
    pub global: HashMap<ActionType, HashMap<String, BindingEntry>>,
    /// Main arrange view bindings
    pub main: HashMap<ActionType, HashMap<String, BindingEntry>>,
    /// MIDI editor bindings
    pub midi: HashMap<ActionType, HashMap<String, BindingEntry>>,
    /// MIDI Event List Editor bindings
    pub midi_event_list_editor: HashMap<ActionType, HashMap<String, BindingEntry>>,
    /// MIDI Inline Editor bindings
    pub midi_inline_editor: HashMap<ActionType, HashMap<String, BindingEntry>>,
    /// Media Explorer bindings
    pub media_explorer: HashMap<ActionType, HashMap<String, BindingEntry>>,
    /// Crossfade Editor bindings
    pub crossfade_editor: HashMap<ActionType, HashMap<String, BindingEntry>>,
}

impl Default for Bindings {
    /// Create default bindings (similar to reaper-keys defaults)
    fn default() -> Self {
        let mut bindings = Bindings {
            global: HashMap::new(),
            main: HashMap::new(),
            midi: HashMap::new(),
            midi_event_list_editor: HashMap::new(),
            midi_inline_editor: HashMap::new(),
            media_explorer: HashMap::new(),
            crossfade_editor: HashMap::new(),
        };

        // Initialize timeline_motion bindings (global)
        let mut timeline_motion = HashMap::new();
        timeline_motion.insert(
            "0".to_string(),
            BindingEntry::Action("ProjectStart".to_string()),
        );
        timeline_motion.insert(
            "f".to_string(),
            BindingEntry::Action("PlayPosition".to_string()),
        );
        timeline_motion.insert(
            "x".to_string(),
            BindingEntry::Action("MousePosition".to_string()),
        );
        timeline_motion.insert(
            "[".to_string(),
            BindingEntry::Action("LoopStart".to_string()),
        );
        timeline_motion.insert("]".to_string(), BindingEntry::Action("LoopEnd".to_string()));
        timeline_motion.insert(
            "<left>".to_string(),
            BindingEntry::Action("PrevMarker".to_string()),
        );
        timeline_motion.insert(
            "<right>".to_string(),
            BindingEntry::Action("NextMarker".to_string()),
        );
        timeline_motion.insert(
            "h".to_string(),
            BindingEntry::Action("LeftGridDivision".to_string()),
        );
        timeline_motion.insert(
            "l".to_string(),
            BindingEntry::Action("RightGridDivision".to_string()),
        );
        timeline_motion.insert(
            "H".to_string(),
            BindingEntry::Action("PrevMeasure".to_string()),
        );
        timeline_motion.insert(
            "L".to_string(),
            BindingEntry::Action("NextMeasure".to_string()),
        );
        bindings
            .global
            .insert(ActionType::TimelineMotion, timeline_motion);

        // Initialize timeline_operator bindings (global)
        let mut timeline_operator = HashMap::new();
        timeline_operator.insert("r".to_string(), BindingEntry::Action("Record".to_string()));
        timeline_operator.insert(
            "t".to_string(),
            BindingEntry::Action("PlayAndLoop".to_string()),
        );
        bindings
            .global
            .insert(ActionType::TimelineOperator, timeline_operator);

        // Initialize command bindings (global)
        let mut command = HashMap::new();
        command.insert(
            "<ESC>".to_string(),
            BindingEntry::Action("Reset".to_string()),
        );
        command.insert(
            "<return>".to_string(),
            BindingEntry::Action("StartStop".to_string()),
        );
        command.insert(
            ".".to_string(),
            BindingEntry::Action("RepeatLastCommand".to_string()),
        );
        command.insert(
            "@".to_string(),
            BindingEntry::Action("PlayMacro".to_string()),
        );
        command.insert(
            ",".to_string(),
            BindingEntry::Action("RecordMacro".to_string()),
        );
        command.insert(
            "v".to_string(),
            BindingEntry::Action("SetModeVisualTimeline".to_string()),
        );
        bindings.global.insert(ActionType::Command, command);

        bindings
    }
}

impl Bindings {
    /// Get bindings for a specific action type and context
    pub fn get_bindings(
        &self,
        action_type: &ActionType,
        context: Context,
    ) -> Option<&HashMap<String, BindingEntry>> {
        let context_bindings = match context {
            Context::Main => &self.main,
            Context::Midi => &self.midi,
            Context::MidiEventListEditor => &self.midi_event_list_editor,
            Context::MidiInlineEditor => &self.midi_inline_editor,
            Context::MediaExplorer => &self.media_explorer,
            Context::CrossfadeEditor => &self.crossfade_editor,
            // The expression editor resolves its bindings through
            // `processor::resolve_exact_context`, never this table.
            Context::ExpressionEditor => &self.global,
            Context::Global => &self.global,
        };

        // Try context-specific first, then global
        context_bindings
            .get(action_type)
            .or_else(|| self.global.get(action_type))
    }
}
