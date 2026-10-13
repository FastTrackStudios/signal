//! Modal editing: mode identifiers, definitions, and the mode stack.
//!
//! The `ModeStack` manages a base editing mode (bottom of stack) with
//! optional transient sub-modes pushed on top. Transitions emit
//! on_enter/on_exit actions so the system can react to mode changes.

use std::collections::HashMap;

use crate::command::{ActionId, InputCommand};

// region: --- Core Types

/// Identifier for an editing mode.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModeId(pub String);

impl ModeId {
    pub const NORMAL: &str = "normal";
    pub const INSERT: &str = "insert";
    pub const VISUAL: &str = "visual";
    pub const COMMAND: &str = "command";

    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn normal() -> Self {
        Self(Self::NORMAL.to_string())
    }

    pub fn insert() -> Self {
        Self(Self::INSERT.to_string())
    }

    pub fn visual() -> Self {
        Self(Self::VISUAL.to_string())
    }

    pub fn command() -> Self {
        Self(Self::COMMAND.to_string())
    }
}

impl std::fmt::Display for ModeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Definition of an editing mode.
#[derive(Debug, Clone)]
pub struct ModeDefinition {
    /// Unique mode identifier.
    pub id: ModeId,
    /// Human-readable display name (e.g., "NORMAL", "INSERT").
    pub display_name: String,
    /// Whether unmatched character keys are passed through as text input.
    pub passthrough_text: bool,
    /// Whether this mode persists after a single action (sub-mode behavior).
    pub sticky: bool,
    /// Actions to execute when entering this mode.
    pub on_enter: Vec<ActionId>,
    /// Actions to execute when exiting this mode.
    pub on_exit: Vec<ActionId>,
}

impl ModeDefinition {
    pub fn new(id: ModeId, display_name: impl Into<String>) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            passthrough_text: false,
            sticky: false,
            on_enter: Vec::new(),
            on_exit: Vec::new(),
        }
    }

    pub fn with_passthrough_text(mut self, passthrough: bool) -> Self {
        self.passthrough_text = passthrough;
        self
    }

    pub fn with_sticky(mut self, sticky: bool) -> Self {
        self.sticky = sticky;
        self
    }

    pub fn with_on_enter(mut self, actions: Vec<ActionId>) -> Self {
        self.on_enter = actions;
        self
    }

    pub fn with_on_exit(mut self, actions: Vec<ActionId>) -> Self {
        self.on_exit = actions;
        self
    }
}

// endregion: --- Core Types

// region: --- ModeStack

/// A stack of editing modes.
///
/// The bottom of the stack is always the base mode. Sub-modes can be
/// pushed on top for transient behaviors (e.g., a "goto" sub-mode
/// triggered by pressing `g`). Transitions emit on_exit/on_enter
/// actions from the mode definitions.
pub struct ModeStack {
    stack: Vec<ModeId>,
    definitions: HashMap<ModeId, ModeDefinition>,
}

impl ModeStack {
    /// Create a new mode stack with the given initial base mode.
    pub fn new(initial: ModeId) -> Self {
        Self {
            stack: vec![initial],
            definitions: HashMap::new(),
        }
    }

    /// Register a mode definition.
    pub fn add_mode(&mut self, def: ModeDefinition) {
        self.definitions.insert(def.id.clone(), def);
    }

    /// Returns the current (topmost) mode.
    pub fn current(&self) -> &ModeId {
        self.stack.last().expect("mode stack must never be empty")
    }

    /// Returns the base (bottom) mode.
    pub fn base(&self) -> &ModeId {
        self.stack.first().expect("mode stack must never be empty")
    }

    /// Returns the full stack depth.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Switch the base mode, clearing all sub-modes.
    ///
    /// Emits on_exit actions for every mode being removed (top to bottom),
    /// then on_enter actions for the new base mode.
    pub fn switch_base(&mut self, new_base: ModeId) -> Vec<InputCommand> {
        let mut commands = Vec::new();

        // Exit all current modes, top to bottom
        for mode_id in self.stack.iter().rev() {
            if let Some(def) = self.definitions.get(mode_id) {
                for action in &def.on_exit {
                    commands.push(InputCommand::Action(action.clone()));
                }
            }
        }

        // Replace the entire stack with the new base
        self.stack.clear();
        self.stack.push(new_base.clone());

        // Enter the new base mode
        if let Some(def) = self.definitions.get(&new_base) {
            for action in &def.on_enter {
                commands.push(InputCommand::Action(action.clone()));
            }
        }

        commands
    }

    /// Push a sub-mode on top of the stack.
    ///
    /// Emits on_enter actions for the pushed mode.
    pub fn push(&mut self, mode: ModeId) -> Vec<InputCommand> {
        let mut commands = Vec::new();

        self.stack.push(mode.clone());

        if let Some(def) = self.definitions.get(&mode) {
            for action in &def.on_enter {
                commands.push(InputCommand::Action(action.clone()));
            }
        }

        commands
    }

    /// Pop the topmost sub-mode from the stack.
    ///
    /// No-op if only the base mode remains (stack depth == 1).
    /// Emits on_exit actions for the popped mode.
    pub fn pop(&mut self) -> Vec<InputCommand> {
        if self.stack.len() <= 1 {
            return Vec::new();
        }

        let mut commands = Vec::new();
        let popped = self.stack.pop().expect("checked len > 1");

        if let Some(def) = self.definitions.get(&popped) {
            for action in &def.on_exit {
                commands.push(InputCommand::Action(action.clone()));
            }
        }

        commands
    }

    /// Get the definition for a mode, if registered.
    pub fn definition(&self, id: &ModeId) -> Option<&ModeDefinition> {
        self.definitions.get(id)
    }

    /// Get the definition for the current mode, if registered.
    pub fn current_definition(&self) -> Option<&ModeDefinition> {
        self.definitions.get(self.current())
    }
}

// endregion: --- ModeStack

// region: --- Tests

#[cfg(test)]
mod tests {
    use super::*;

    // -- Support & Fixtures

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    fn make_normal_def() -> ModeDefinition {
        ModeDefinition::new(ModeId::normal(), "NORMAL")
            .with_on_enter(vec![ActionId::new("mode.normal.enter")])
            .with_on_exit(vec![ActionId::new("mode.normal.exit")])
    }

    fn make_insert_def() -> ModeDefinition {
        ModeDefinition::new(ModeId::insert(), "INSERT")
            .with_passthrough_text(true)
            .with_on_enter(vec![ActionId::new("mode.insert.enter")])
            .with_on_exit(vec![ActionId::new("mode.insert.exit")])
    }

    fn make_visual_def() -> ModeDefinition {
        ModeDefinition::new(ModeId::visual(), "VISUAL")
            .with_on_enter(vec![ActionId::new("mode.visual.enter")])
            .with_on_exit(vec![ActionId::new("mode.visual.exit")])
    }

    fn make_goto_submode_def() -> ModeDefinition {
        ModeDefinition::new(ModeId::new("goto"), "GOTO")
            .with_sticky(true)
            .with_on_enter(vec![ActionId::new("mode.goto.enter")])
            .with_on_exit(vec![ActionId::new("mode.goto.exit")])
    }

    fn extract_action_ids(commands: &[InputCommand]) -> Vec<&str> {
        commands
            .iter()
            .filter_map(|cmd| match cmd {
                InputCommand::Action(id) => Some(id.as_str()),
                _ => None,
            })
            .collect()
    }

    fn make_stack_with_modes() -> ModeStack {
        let mut stack = ModeStack::new(ModeId::normal());
        stack.add_mode(make_normal_def());
        stack.add_mode(make_insert_def());
        stack.add_mode(make_visual_def());
        stack.add_mode(make_goto_submode_def());
        stack
    }

    // -- Tests

    #[test]
    fn test_mode_stack_new_starts_with_initial_mode() -> Result<()> {
        // -- Exec
        let stack = ModeStack::new(ModeId::normal());

        // -- Check
        assert_eq!(stack.current(), &ModeId::normal());
        assert_eq!(stack.depth(), 1);

        Ok(())
    }

    #[test]
    fn test_mode_stack_switch_base_emits_exit_enter_actions() -> Result<()> {
        // -- Setup
        let mut stack = make_stack_with_modes();

        // -- Exec
        let commands = stack.switch_base(ModeId::insert());

        // -- Check
        let ids = extract_action_ids(&commands);
        assert_eq!(ids, vec!["mode.normal.exit", "mode.insert.enter"]);
        assert_eq!(stack.current(), &ModeId::insert());
        assert_eq!(stack.depth(), 1);

        Ok(())
    }

    #[test]
    fn test_mode_stack_push_pop_submode() -> Result<()> {
        // -- Setup
        let mut stack = make_stack_with_modes();

        // -- Exec: push sub-mode
        let push_cmds = stack.push(ModeId::new("goto"));

        // -- Check push
        let push_ids = extract_action_ids(&push_cmds);
        assert_eq!(push_ids, vec!["mode.goto.enter"]);
        assert_eq!(stack.current(), &ModeId::new("goto"));
        assert_eq!(stack.depth(), 2);

        // -- Exec: pop sub-mode
        let pop_cmds = stack.pop();

        // -- Check pop
        let pop_ids = extract_action_ids(&pop_cmds);
        assert_eq!(pop_ids, vec!["mode.goto.exit"]);
        assert_eq!(stack.current(), &ModeId::normal());
        assert_eq!(stack.depth(), 1);

        Ok(())
    }

    #[test]
    fn test_mode_stack_pop_single_mode_is_noop() -> Result<()> {
        // -- Setup
        let mut stack = make_stack_with_modes();
        assert_eq!(stack.depth(), 1);

        // -- Exec
        let commands = stack.pop();

        // -- Check
        assert!(commands.is_empty());
        assert_eq!(stack.current(), &ModeId::normal());
        assert_eq!(stack.depth(), 1);

        Ok(())
    }

    #[test]
    fn test_mode_stack_switch_base_clears_submodes() -> Result<()> {
        // -- Setup
        let mut stack = make_stack_with_modes();
        stack.push(ModeId::new("goto"));
        assert_eq!(stack.depth(), 2);

        // -- Exec
        let commands = stack.switch_base(ModeId::insert());

        // -- Check: should exit goto (top), then normal (base), then enter insert
        let ids = extract_action_ids(&commands);
        assert_eq!(
            ids,
            vec!["mode.goto.exit", "mode.normal.exit", "mode.insert.enter"]
        );
        assert_eq!(stack.current(), &ModeId::insert());
        assert_eq!(stack.depth(), 1);

        Ok(())
    }

    #[test]
    fn test_mode_stack_add_mode_and_definition_lookup() -> Result<()> {
        // -- Setup & Exec
        let stack = make_stack_with_modes();

        // -- Check
        let normal_def = stack.definition(&ModeId::normal()).unwrap();
        assert_eq!(normal_def.display_name, "NORMAL");
        assert!(!normal_def.passthrough_text);

        let insert_def = stack.definition(&ModeId::insert()).unwrap();
        assert_eq!(insert_def.display_name, "INSERT");
        assert!(insert_def.passthrough_text);

        let goto_def = stack.definition(&ModeId::new("goto")).unwrap();
        assert!(goto_def.sticky);

        assert!(stack.definition(&ModeId::new("nonexistent")).is_none());

        Ok(())
    }

    #[test]
    fn test_mode_stack_current_definition() -> Result<()> {
        // -- Setup
        let mut stack = make_stack_with_modes();

        // -- Check: base mode
        let def = stack.current_definition().unwrap();
        assert_eq!(def.display_name, "NORMAL");

        // -- Exec: push sub-mode
        stack.push(ModeId::new("goto"));

        // -- Check: sub-mode is now current
        let def = stack.current_definition().unwrap();
        assert_eq!(def.display_name, "GOTO");

        Ok(())
    }

    #[test]
    fn test_mode_stack_multiple_pushes_and_pops() -> Result<()> {
        // -- Setup
        let mut stack = make_stack_with_modes();
        let window_mode = ModeDefinition::new(ModeId::new("window"), "WINDOW")
            .with_on_enter(vec![ActionId::new("mode.window.enter")])
            .with_on_exit(vec![ActionId::new("mode.window.exit")]);
        stack.add_mode(window_mode);

        // -- Exec: push two sub-modes
        stack.push(ModeId::new("goto"));
        stack.push(ModeId::new("window"));

        // -- Check
        assert_eq!(stack.depth(), 3);
        assert_eq!(stack.current(), &ModeId::new("window"));
        assert_eq!(stack.base(), &ModeId::normal());

        // -- Exec: pop one
        let cmds = stack.pop();
        let ids = extract_action_ids(&cmds);
        assert_eq!(ids, vec!["mode.window.exit"]);
        assert_eq!(stack.current(), &ModeId::new("goto"));

        // -- Exec: pop another
        let cmds = stack.pop();
        let ids = extract_action_ids(&cmds);
        assert_eq!(ids, vec!["mode.goto.exit"]);
        assert_eq!(stack.current(), &ModeId::normal());

        // -- Exec: pop on base is no-op
        let cmds = stack.pop();
        assert!(cmds.is_empty());
        assert_eq!(stack.current(), &ModeId::normal());

        Ok(())
    }

    #[test]
    fn test_mode_stack_switch_base_without_definitions() -> Result<()> {
        // -- Setup: no definitions registered
        let mut stack = ModeStack::new(ModeId::normal());

        // -- Exec
        let commands = stack.switch_base(ModeId::insert());

        // -- Check: no actions emitted but mode changed
        assert!(commands.is_empty());
        assert_eq!(stack.current(), &ModeId::insert());

        Ok(())
    }

    #[test]
    fn test_mode_id_constants() -> Result<()> {
        // -- Check
        assert_eq!(ModeId::NORMAL, "normal");
        assert_eq!(ModeId::INSERT, "insert");
        assert_eq!(ModeId::VISUAL, "visual");
        assert_eq!(ModeId::COMMAND, "command");

        assert_eq!(ModeId::normal().as_str(), "normal");
        assert_eq!(ModeId::insert().as_str(), "insert");

        Ok(())
    }
}

// endregion: --- Tests
