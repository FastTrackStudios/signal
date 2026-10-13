//! Core input processing state machine.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::command::{ActionId, InputArgs, InputCommand};
use crate::config::{
    ConfigError, KeymapConfig, mode_definition_from_config, parse_key_sequence, parse_leaf_action,
    parse_mouse_pattern, parse_scroll_pattern, parse_when_expr,
};
use crate::context::{ActionContext, WhenExpr};
use crate::event::{InputEvent, KeyEvent, MouseEvent};
use crate::key::{KeyChord, KeyCode, Modifiers};
use crate::macros::MacroRecorder;
use crate::mode::{ModeDefinition, ModeId, ModeStack};
use crate::mouse::MouseBindingTable;
use crate::scroll::ScrollBindingTable;
use crate::trie::{KeyTrie, LeafAction, TrieLookup};

/// The core input processing state machine.
pub struct InputProcessor {
    modes: ModeStack,
    keymaps: HashMap<ModeId, KeyTrie>,
    context_keymaps: HashMap<ModeId, Vec<(WhenExpr, KeyTrie)>>,
    mouse_bindings: HashMap<ModeId, MouseBindingTable>,
    scroll_bindings: HashMap<ModeId, ScrollBindingTable>,
    sequence: Vec<KeyChord>,
    timeout: Duration,
    count_prefix: Option<u32>,
    pending_operator: Option<String>,
    macro_recorder: MacroRecorder,
    pending_macro_record_register: bool,
    pending_macro_play_register: bool,
    is_playing_back_macro: bool,
    /// Chords currently held down (between keydown and keyup).
    held_keys: HashSet<KeyChord>,
    /// `true` if the anchor (first chord of `sequence`) has had a matching
    /// keyup observed since the sequence began. Used to distinguish a held
    /// prefix (e.g. holding `g` while tapping `q`/`w`/`e`) from a vim-style
    /// double-tap (e.g. `gg`), which look identical by chord-content alone.
    anchor_released_during_sequence: bool,
    /// If set, sticky-prefix mode is active: after each successful match,
    /// the sequence is rewound to this anchor instead of cleared. Cleared
    /// on keyup of the anchor.
    sticky_anchor: Option<KeyChord>,
    /// Number of leaf actions that have fired during the current sticky
    /// sequence. Used so the host can hide a lingering which-key overlay
    /// when the anchor is released after one or more actions have run
    /// (the user is done) while keeping it open on a bare hold+release
    /// (the user is still exploring).
    sticky_actions_fired: u32,
}

impl InputProcessor {
    /// Create a new processor starting in Normal mode with empty keymaps.
    pub fn new() -> Self {
        Self {
            modes: ModeStack::new(ModeId::normal()),
            keymaps: HashMap::new(),
            context_keymaps: HashMap::new(),
            mouse_bindings: HashMap::new(),
            scroll_bindings: HashMap::new(),
            sequence: Vec::new(),
            timeout: Duration::from_millis(1000),
            count_prefix: None,
            pending_operator: None,
            macro_recorder: MacroRecorder::new(),
            pending_macro_record_register: false,
            pending_macro_play_register: false,
            is_playing_back_macro: false,
            held_keys: HashSet::new(),
            anchor_released_during_sequence: false,
            sticky_anchor: None,
            sticky_actions_fired: 0,
        }
    }

    /// Notify the processor that a key has been physically released.
    ///
    /// Called by hosts that can observe keyup events (e.g. the REAPER
    /// TranslateAccel handler). Used to power "hold-prefix-active"
    /// behaviour: as long as the anchor key of a which-key prefix is held,
    /// successful matches preserve the prefix so subsequent leaf keys keep
    /// firing without re-pressing it.
    ///
    /// Returns `true` when the host should hide a lingering which-key
    /// overlay because a sticky-prefix sequence that fired at least one
    /// action just ended. Bare hold-and-release with no action fired
    /// returns `false` so the overlay can keep guiding the user.
    pub fn notify_key_release(&mut self, chord: KeyChord) -> bool {
        self.held_keys.remove(&chord);
        if let Some(anchor) = self.sequence.first()
            && anchor == &chord
        {
            self.anchor_released_during_sequence = true;
        }
        let should_hide_overlay =
            self.sticky_anchor.as_ref() == Some(&chord) && self.sticky_actions_fired > 0;
        if self.sticky_anchor.as_ref() == Some(&chord) {
            self.sticky_anchor = None;
            self.sticky_actions_fired = 0;
            self.sequence.clear();
            self.count_prefix = None;
            self.pending_operator = None;
            self.pending_macro_record_register = false;
            self.pending_macro_play_register = false;
        }
        should_hide_overlay
    }

    /// Build an input processor from config.
    pub fn from_config(config: KeymapConfig) -> Result<Self, ConfigError> {
        let mut proc = Self::new();

        if !config.modes.contains_key(ModeId::NORMAL) {
            proc.add_mode(ModeDefinition::new(ModeId::normal(), "NORMAL"));
        }

        for (mode_name, mode_cfg) in &config.modes {
            proc.add_mode(mode_definition_from_config(mode_name, mode_cfg));
        }

        for (mode_name, bindings) in &config.keymap {
            let mode = ModeId::new(mode_name);
            let mut trie = KeyTrie::new();
            for (seq, action) in bindings {
                let sequence = parse_key_sequence(seq)?;
                let leaf = parse_leaf_action(action);
                trie.bind_leaf(sequence, leaf);
            }
            proc.set_keymap(mode, trie);
        }

        for (mode_name, layers) in &config.keymap_context {
            let mode = ModeId::new(mode_name);
            let mut parsed_layers = Vec::new();
            for layer in layers {
                let mut trie = KeyTrie::new();
                for (seq, action) in &layer.bindings {
                    let sequence = parse_key_sequence(seq)?;
                    let leaf = parse_leaf_action(action);
                    trie.bind_leaf(sequence, leaf);
                }
                parsed_layers.push((parse_when_expr(&layer.when), trie));
            }
            proc.context_keymaps.insert(mode, parsed_layers);
        }

        for (mode_name, bindings) in &config.mouse {
            let mode = ModeId::new(mode_name);
            let mut table = MouseBindingTable::new();
            for (pattern, action) in bindings {
                let parsed = parse_mouse_pattern(pattern)?;
                table.insert(parsed, WhenExpr::True, ActionId::new(action));
            }
            proc.mouse_bindings.insert(mode, table);
        }

        for (mode_name, bindings) in &config.scroll {
            let mode = ModeId::new(mode_name);
            let mut table = ScrollBindingTable::new();
            for (pattern, action) in bindings {
                let parsed = parse_scroll_pattern(pattern)?;
                table.insert(parsed, WhenExpr::True, ActionId::new(action));
            }
            proc.scroll_bindings.insert(mode, table);
        }

        Ok(proc)
    }

    /// Replace processor state from a new config.
    pub fn reload_config(&mut self, config: KeymapConfig) -> Result<(), ConfigError> {
        *self = Self::from_config(config)?;
        Ok(())
    }

    pub fn add_mode(&mut self, def: ModeDefinition) {
        self.modes.add_mode(def);
    }

    pub fn set_keymap(&mut self, mode: ModeId, trie: KeyTrie) {
        self.keymaps.insert(mode, trie);
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub fn current_mode(&self) -> &ModeId {
        self.modes.current()
    }

    pub fn pending_display(&self) -> Option<String> {
        if self.sequence.is_empty() {
            None
        } else {
            Some(
                self.sequence
                    .iter()
                    .map(chord_display)
                    .collect::<Vec<_>>()
                    .join(""),
            )
        }
    }

    pub fn needs_timeout(&self) -> bool {
        !self.sequence.is_empty()
    }

    /// Returns `true` if the anchor (first chord) of the pending sequence
    /// is still physically held down by the user. Hosts that drive timeout
    /// checks from a periodic tick should use this to avoid expiring a
    /// which-key prefix while the user is still actively holding it —
    /// otherwise a held prefix flickers on every tick as the sequence is
    /// cleared and immediately re-entered by OS auto-repeat.
    pub fn is_anchor_held(&self) -> bool {
        match self.sequence.first() {
            Some(anchor) => self.held_keys.contains(anchor),
            None => false,
        }
    }

    pub fn timeout_duration(&self) -> Duration {
        self.timeout
    }

    pub fn mode_stack(&self) -> &ModeStack {
        &self.modes
    }

    /// Read-only access to keymaps for all modes.
    pub fn keymaps(&self) -> &HashMap<ModeId, KeyTrie> {
        &self.keymaps
    }

    /// Read-only access to context-conditional keymaps for all modes.
    pub fn context_keymaps(&self) -> &HashMap<ModeId, Vec<(WhenExpr, KeyTrie)>> {
        &self.context_keymaps
    }

    /// Read-only access to mouse bindings for all modes.
    pub fn mouse_bindings(&self) -> &HashMap<ModeId, MouseBindingTable> {
        &self.mouse_bindings
    }

    /// Read-only access to scroll bindings for all modes.
    pub fn scroll_bindings(&self) -> &HashMap<ModeId, ScrollBindingTable> {
        &self.scroll_bindings
    }

    pub fn is_recording_macro(&self) -> bool {
        self.macro_recorder.is_recording()
    }

    pub fn process(&mut self, event: InputEvent, ctx: &ActionContext) -> Vec<InputCommand> {
        match event {
            InputEvent::Key(ref key_event) => {
                let chord = KeyChord::new(key_event.key.clone(), key_event.modifiers);
                self.process_key_chord(chord, event, ctx, false)
            }
            InputEvent::Mouse(ref mouse_event) => self.process_mouse_event(mouse_event, ctx),
            InputEvent::Scroll(ref scroll_event) => self.process_scroll_event(scroll_event, ctx),
            _ => vec![InputCommand::Unhandled(event)],
        }
    }

    pub fn timeout_expired(&mut self) -> Vec<InputCommand> {
        if self.sequence.is_empty() {
            return Vec::new();
        }

        let first = self.sequence.first().cloned();
        self.sequence.clear();
        self.count_prefix = None;

        if let Some(first) = first {
            vec![InputCommand::Unhandled(InputEvent::Key(KeyEvent {
                key: first.key,
                modifiers: first.modifiers,
            }))]
        } else {
            Vec::new()
        }
    }

    fn process_mouse_event(&self, event: &MouseEvent, ctx: &ActionContext) -> Vec<InputCommand> {
        let mode = self.modes.current();
        let action = self
            .mouse_bindings
            .get(mode)
            .and_then(|table| table.match_event(event, ctx));

        if let Some(action) = action {
            return vec![InputCommand::ActionWithArgs {
                action,
                args: InputArgs::default(),
            }];
        }

        vec![InputCommand::Unhandled(InputEvent::Mouse(event.clone()))]
    }

    fn process_scroll_event(
        &self,
        event: &crate::event::ScrollEvent,
        ctx: &ActionContext,
    ) -> Vec<InputCommand> {
        let mode = self.modes.current();
        let action = self
            .scroll_bindings
            .get(mode)
            .and_then(|table| table.match_event(event, ctx));

        if let Some(action) = action {
            return vec![InputCommand::ActionWithArgs {
                action,
                args: InputArgs::default(),
            }];
        }

        vec![InputCommand::Unhandled(InputEvent::Scroll(event.clone()))]
    }

    fn process_key_chord(
        &mut self,
        chord: KeyChord,
        original_event: InputEvent,
        ctx: &ActionContext,
        from_playback: bool,
    ) -> Vec<InputCommand> {
        if chord.key == KeyCode::Escape && chord.modifiers == Modifiers::NONE {
            if self.sequence.is_empty() {
                let current_mode = self.modes.current().clone();
                if let TrieLookup::Match(action) =
                    self.lookup_in_mode(&current_mode, ctx, std::slice::from_ref(&chord))
                {
                    return self.execute_leaf_action(action);
                }
            }
            return self.handle_escape();
        }

        if !from_playback {
            if let Some(commands) = self.handle_macro_controls(&chord, ctx) {
                return commands;
            }
            if self.handle_count_prefix(&chord, ctx) {
                return Vec::new();
            }
        }

        if self.macro_recorder.is_recording() && !from_playback && !self.is_playing_back_macro {
            self.macro_recorder.record(chord.clone());
        }

        // Track held chords so that prefix anchors can be detected as still
        // physically pressed at the moment a leaf binding fires. Macro-playback
        // synthesises chords without real keydown/keyup pairs, so it is excluded.
        //
        // OS keyboard auto-repeat (a keydown for a chord already in
        // `held_keys`) needs to be suppressed inside a which-key sequence so
        // that:
        //   • holding the anchor (e.g. `z`) does not re-enter the prefix
        //     and re-flash the overlay on every repeat, and does not
        //     accidentally match a `z z` leaf;
        //   • holding a leaf under a sticky prefix (e.g. holding `h` after
        //     `z` → `_SWS_TOGZOOMHORIZ`) does not toggle 30×/sec.
        // Outside any active sequence, auto-repeat passes through so
        // continuous actions like cursor movement keep working.
        if !from_playback {
            let is_auto_repeat = self.held_keys.contains(&chord);
            let in_sequence = !self.sequence.is_empty() || self.sticky_anchor.is_some();
            if is_auto_repeat && in_sequence {
                return Vec::new();
            }
            self.held_keys.insert(chord.clone());
        }

        // Starting a fresh sequence: reset the release-tracking flag so a
        // subsequent match can tell whether the anchor stayed held.
        if self.sequence.is_empty() {
            self.anchor_released_during_sequence = false;
        }

        self.sequence.push(chord);

        let current_mode = self.modes.current().clone();
        let lookup = self.lookup_in_mode(&current_mode, ctx, &self.sequence);

        match lookup {
            TrieLookup::Match(action) => {
                // Hold-to-keep-prefix: if the anchor (first chord) of a
                // multi-chord match is still physically held, rewind the
                // sequence to that anchor so the next keypress resolves as a
                // sibling of the just-fired leaf.
                let anchor = self.sequence.first().cloned();
                let multi_chord = self.sequence.len() > 1;
                let anchor_held = anchor
                    .as_ref()
                    .is_some_and(|c| !from_playback && self.held_keys.contains(c));
                let anchor_uninterrupted = !self.anchor_released_during_sequence;
                if multi_chord && anchor_held && anchor_uninterrupted {
                    let anchor = anchor.expect("checked above");
                    self.sequence.clear();
                    self.sequence.push(anchor.clone());
                    self.sticky_anchor = Some(anchor);
                    self.sticky_actions_fired = self.sticky_actions_fired.saturating_add(1);
                } else {
                    self.sequence.clear();
                }
                self.execute_leaf_action(action)
            }
            TrieLookup::Prefix => {
                let display = self
                    .sequence
                    .iter()
                    .map(chord_display)
                    .collect::<Vec<_>>()
                    .join("");
                vec![InputCommand::Pending { display }]
            }
            TrieLookup::Miss => {
                self.sequence.clear();
                self.sticky_anchor = None;
                self.sticky_actions_fired = 0;
                self.anchor_released_during_sequence = false;
                self.count_prefix = None;
                self.pending_operator = None;
                self.handle_unmatched(original_event, &current_mode)
            }
        }
    }

    fn lookup_in_mode(&self, mode: &ModeId, ctx: &ActionContext, keys: &[KeyChord]) -> TrieLookup {
        let mut has_prefix = false;

        if let Some(layers) = self.context_keymaps.get(mode) {
            for (when, trie) in layers {
                if !when.evaluate(ctx) {
                    continue;
                }
                match trie.lookup(keys) {
                    TrieLookup::Match(action) => return TrieLookup::Match(action),
                    TrieLookup::Prefix => has_prefix = true,
                    TrieLookup::Miss => {}
                }
            }
        }

        if let Some(trie) = self.keymaps.get(mode) {
            match trie.lookup(keys) {
                TrieLookup::Match(action) => return TrieLookup::Match(action),
                TrieLookup::Prefix => has_prefix = true,
                TrieLookup::Miss => {}
            }
        }

        if has_prefix {
            TrieLookup::Prefix
        } else {
            TrieLookup::Miss
        }
    }

    fn handle_count_prefix(&mut self, chord: &KeyChord, ctx: &ActionContext) -> bool {
        if !self.sequence.is_empty() {
            return false;
        }
        if chord.modifiers != Modifiers::NONE {
            return false;
        }
        let KeyCode::Character(ref ch) = chord.key else {
            return false;
        };
        let mut chars = ch.chars();
        let Some(c) = chars.next() else {
            return false;
        };
        if chars.next().is_some() || !c.is_ascii_digit() {
            return false;
        }
        if c == '0' && self.count_prefix.is_none() {
            return false;
        }

        // If the user has explicitly bound this digit (as a leaf or a
        // prefix to deeper bindings) in the current mode, defer to the
        // normal trie lookup so that user binding wins over the
        // vim-style count parser. Without this an overlay that binds
        // "1"/"2"/… to actions would never fire — the digit would be
        // silently accumulated as a count for the next non-digit key.
        let current_mode = self.modes.current().clone();
        let probe = [chord.clone()];
        let probe_lookup = self.lookup_in_mode(&current_mode, ctx, &probe);
        if !matches!(probe_lookup, TrieLookup::Miss) {
            return false;
        }

        let digit = (c as u8 - b'0') as u32;
        self.count_prefix = Some(
            self.count_prefix
                .unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(digit),
        );
        true
    }

    fn handle_macro_controls(
        &mut self,
        chord: &KeyChord,
        ctx: &ActionContext,
    ) -> Option<Vec<InputCommand>> {
        if chord.modifiers != Modifiers::NONE {
            return None;
        }
        let KeyCode::Character(ref text) = chord.key else {
            return None;
        };
        let mut chars = text.chars();
        let ch = chars.next()?;
        if chars.next().is_some() {
            return None;
        }

        if self.pending_macro_record_register {
            self.pending_macro_record_register = false;
            if ch.is_ascii_lowercase() {
                self.macro_recorder.start_recording(ch);
            }
            self.count_prefix = None;
            return Some(Vec::new());
        }

        if self.pending_macro_play_register {
            self.pending_macro_play_register = false;
            return Some(if ch == '@' {
                self.play_macro(None, ctx)
            } else if ch.is_ascii_lowercase() {
                self.play_macro(Some(ch), ctx)
            } else {
                Vec::new()
            });
        }

        // Macro triggers only fire as standalone keys. If a multi-key sequence
        // is already in progress, fall through so the trie can resolve it
        // (e.g. `g q` in a which-key prefix must reach the leaf binding,
        // not be eaten as `q`-record-macro).
        let in_sequence = !self.sequence.is_empty();
        // The macro keys are vim's defaults, not a claim over the keymap: a
        // profile that binds `q` or `@` itself has it (a DAW quantizes on
        // `q`), and macros go without that key.
        let mode = self.modes.current().clone();
        let bound = !matches!(
            self.lookup_in_mode(&mode, ctx, std::slice::from_ref(chord)),
            TrieLookup::Miss
        );
        if bound {
            return None;
        }

        if ch == 'q' && !in_sequence {
            if self.macro_recorder.is_recording() {
                self.macro_recorder.stop_recording();
            } else {
                self.pending_macro_record_register = true;
            }
            self.sequence.clear();
            return Some(Vec::new());
        }

        if ch == '@' && !in_sequence {
            self.pending_macro_play_register = true;
            self.sequence.clear();
            return Some(Vec::new());
        }

        None
    }

    fn play_macro(&mut self, register: Option<char>, ctx: &ActionContext) -> Vec<InputCommand> {
        if self.is_playing_back_macro {
            return Vec::new();
        }

        let repeat = self.count_prefix.take().unwrap_or(1);
        let sequence = match register {
            Some(reg) => self.macro_recorder.play(reg),
            None => self.macro_recorder.play_last(),
        };

        let Some(sequence) = sequence else {
            return Vec::new();
        };

        self.is_playing_back_macro = true;
        let mut out = Vec::new();
        for _ in 0..repeat {
            for chord in &sequence {
                let event = InputEvent::Key(KeyEvent {
                    key: chord.key.clone(),
                    modifiers: chord.modifiers,
                });
                out.extend(self.process_key_chord(chord.clone(), event, ctx, true));
            }
        }
        self.is_playing_back_macro = false;
        out
    }

    fn handle_escape(&mut self) -> Vec<InputCommand> {
        self.pending_macro_record_register = false;
        self.pending_macro_play_register = false;

        if !self.sequence.is_empty() {
            self.sequence.clear();
            self.count_prefix = None;
            self.pending_operator = None;
            return Vec::new();
        }

        if self.modes.depth() > 1 {
            self.count_prefix = None;
            self.pending_operator = None;
            return self.modes.pop();
        }

        if self.modes.current() != &ModeId::normal() {
            self.count_prefix = None;
            self.pending_operator = None;
            return self.modes.switch_base(ModeId::normal());
        }

        Vec::new()
    }

    fn handle_unmatched(&self, original_event: InputEvent, mode: &ModeId) -> Vec<InputCommand> {
        if let Some(def) = self.modes.definition(mode)
            && def.passthrough_text
            && let InputEvent::Key(ref key_event) = original_event
            && let KeyCode::Character(ref ch) = key_event.key
            && key_event.modifiers == Modifiers::NONE
        {
            return vec![InputCommand::InsertText(ch.clone())];
        }

        vec![InputCommand::Unhandled(original_event)]
    }

    fn execute_leaf_action(&mut self, action: LeafAction) -> Vec<InputCommand> {
        match action {
            LeafAction::Action(id) => vec![self.action_command(id)],
            LeafAction::SwitchMode(mode_id) => {
                self.execute_command(InputCommand::SwitchMode(mode_id))
            }
            LeafAction::PushMode(mode_id) => self.execute_command(InputCommand::PushMode(mode_id)),
            LeafAction::Operator(op) => {
                self.pending_operator = Some(op);
                Vec::new()
            }
            LeafAction::Motion(motion) => {
                if let Some(operator) = self.pending_operator.take() {
                    vec![InputCommand::ActionWithArgs {
                        action: ActionId::new(format!("operator.{operator}")),
                        args: InputArgs {
                            count: self.count_prefix.take(),
                            operator: Some(operator),
                            motion: Some(motion),
                            text_object: None,
                            register: None,
                        },
                    }]
                } else {
                    Vec::new()
                }
            }
            LeafAction::TextObject(text_object) => {
                if let Some(operator) = self.pending_operator.take() {
                    vec![InputCommand::ActionWithArgs {
                        action: ActionId::new(format!("operator.{operator}")),
                        args: InputArgs {
                            count: self.count_prefix.take(),
                            operator: Some(operator),
                            motion: None,
                            text_object: Some(text_object),
                            register: None,
                        },
                    }]
                } else {
                    Vec::new()
                }
            }
            LeafAction::Sequence(actions) => {
                let count = self.count_prefix.take();
                actions
                    .into_iter()
                    .map(|action| {
                        if count.is_some() {
                            InputCommand::ActionWithArgs {
                                action,
                                args: InputArgs {
                                    count,
                                    ..InputArgs::default()
                                },
                            }
                        } else {
                            InputCommand::Action(action)
                        }
                    })
                    .collect()
            }
            LeafAction::Unbind => Vec::new(),
        }
    }

    fn action_command(&mut self, action: ActionId) -> InputCommand {
        if let Some(count) = self.count_prefix.take() {
            InputCommand::ActionWithArgs {
                action,
                args: InputArgs {
                    count: Some(count),
                    ..InputArgs::default()
                },
            }
        } else {
            InputCommand::Action(action)
        }
    }

    fn execute_command(&mut self, command: InputCommand) -> Vec<InputCommand> {
        self.count_prefix = None;
        self.pending_operator = None;

        match command {
            InputCommand::SwitchMode(ref mode_id) => {
                let mut cmds = self.modes.switch_base(mode_id.clone());
                cmds.insert(0, command);
                cmds
            }
            InputCommand::PushMode(ref mode_id) => {
                let mut cmds = self.modes.push(mode_id.clone());
                cmds.insert(0, command);
                cmds
            }
            InputCommand::PopMode => {
                let mut cmds = self.modes.pop();
                cmds.insert(0, command);
                cmds
            }
            _ => vec![command],
        }
    }
}

impl Default for InputProcessor {
    fn default() -> Self {
        Self::new()
    }
}

fn chord_display(chord: &KeyChord) -> String {
    let mut s = String::new();
    if chord.modifiers.ctrl {
        s.push_str("C-");
    }
    if chord.modifiers.alt {
        s.push_str("A-");
    }
    if chord.modifiers.shift {
        s.push_str("S-");
    }
    if chord.modifiers.meta {
        s.push_str("M-");
    }
    match &chord.key {
        KeyCode::Character(c) => s.push_str(c),
        KeyCode::Escape => s.push_str("Esc"),
        KeyCode::Enter => s.push_str("Enter"),
        KeyCode::Tab => s.push_str("Tab"),
        KeyCode::Backspace => s.push_str("BS"),
        KeyCode::Delete => s.push_str("Del"),
        KeyCode::ArrowUp => s.push_str("Up"),
        KeyCode::ArrowDown => s.push_str("Down"),
        KeyCode::ArrowLeft => s.push_str("Left"),
        KeyCode::ArrowRight => s.push_str("Right"),
        KeyCode::F(n) => {
            s.push('F');
            s.push_str(&n.to_string());
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    fn key_event(key: KeyCode) -> InputEvent {
        InputEvent::Key(KeyEvent {
            key,
            modifiers: Modifiers::NONE,
        })
    }

    fn char_event(ch: char) -> InputEvent {
        key_event(KeyCode::Character(ch.to_string()))
    }

    fn chord(ch: char) -> KeyChord {
        KeyChord::plain(KeyCode::Character(ch.to_string()))
    }

    fn make_processor() -> InputProcessor {
        let mut proc = InputProcessor::new();

        proc.add_mode(
            ModeDefinition::new(ModeId::normal(), "NORMAL")
                .with_on_enter(vec![ActionId::new("mode.normal.enter")])
                .with_on_exit(vec![ActionId::new("mode.normal.exit")]),
        );
        proc.add_mode(
            ModeDefinition::new(ModeId::insert(), "INSERT")
                .with_passthrough_text(true)
                .with_on_enter(vec![ActionId::new("mode.insert.enter")])
                .with_on_exit(vec![ActionId::new("mode.insert.exit")]),
        );
        proc.add_mode(
            ModeDefinition::new(ModeId::visual(), "VISUAL")
                .with_on_enter(vec![ActionId::new("mode.visual.enter")])
                .with_on_exit(vec![ActionId::new("mode.visual.exit")]),
        );

        let mut normal_trie = KeyTrie::new();
        normal_trie.bind(chord('j'), ActionId::new("cursor.down"));
        normal_trie.bind(chord('k'), ActionId::new("cursor.up"));
        normal_trie.bind_mode_switch(chord('i'), ModeId::insert());
        normal_trie.bind_mode_switch(chord('v'), ModeId::visual());
        normal_trie.bind_sequence(vec![chord('g'), chord('g')], ActionId::new("cursor.top"));
        normal_trie.bind_sequence(vec![chord('g'), chord('e')], ActionId::new("cursor.end"));
        proc.set_keymap(ModeId::normal(), normal_trie);

        let insert_trie = KeyTrie::new();
        proc.set_keymap(ModeId::insert(), insert_trie);

        proc
    }

    fn extract_action_ids(commands: &[InputCommand]) -> Vec<&str> {
        commands
            .iter()
            .filter_map(|cmd| match cmd {
                InputCommand::Action(id) => Some(id.as_str()),
                InputCommand::ActionWithArgs { action, .. } => Some(action.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_single_key_action_normal_mode() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        let commands = proc.process(char_event('j'), &ctx);

        assert_eq!(commands.len(), 1);
        let ids = extract_action_ids(&commands);
        assert_eq!(ids, vec!["cursor.down"]);
        assert!(!proc.needs_timeout());

        Ok(())
    }

    #[test]
    fn test_sticky_prefix_rewinds_to_anchor_while_held() -> Result<()> {
        // Holding the prefix anchor (`g`) and tapping leaf keys should keep
        // the sequence rewound to the anchor so subsequent taps resolve as
        // siblings of the matched leaf.
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        // Hold `g` down (no release).
        let commands = proc.process(char_event('g'), &ctx);
        assert!(matches!(&commands[0], InputCommand::Pending { display } if display == "g"));

        // First tap of `e` matches `g e` while `g` is still held.
        let commands = proc.process(char_event('e'), &ctx);
        assert!(!proc.notify_key_release(chord('e')));
        assert_eq!(extract_action_ids(&commands), vec!["cursor.end"]);

        // Sequence should be rewound to `[g]`, not cleared.
        assert!(proc.needs_timeout());
        assert_eq!(proc.pending_display(), Some("g".to_string()));

        // Second tap of `e` resolves as `g e` again without re-pressing `g`.
        let commands = proc.process(char_event('e'), &ctx);
        assert!(!proc.notify_key_release(chord('e')));
        assert_eq!(extract_action_ids(&commands), vec!["cursor.end"]);
        assert!(proc.needs_timeout());

        // Releasing the anchor clears the sticky state and signals the host
        // to hide the overlay (one or more actions fired during the sequence).
        assert!(proc.notify_key_release(chord('g')));
        assert!(!proc.needs_timeout());
        assert_eq!(proc.pending_display(), None);

        Ok(())
    }

    #[test]
    fn test_held_anchor_auto_repeat_is_suppressed() -> Result<()> {
        // Holding the prefix anchor itself should not re-enter the prefix
        // on every OS auto-repeat keydown (which would re-flash the
        // overlay) nor accidentally match a same-key two-chord binding
        // such as `gg`.
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        let commands = proc.process(char_event('g'), &ctx);
        assert!(matches!(&commands[0], InputCommand::Pending { display } if display == "g"));

        // OS auto-repeat: keydown of `g` again with no prior keyup.
        let commands = proc.process(char_event('g'), &ctx);
        assert!(
            extract_action_ids(&commands).is_empty(),
            "anchor auto-repeat should not fire `g g`",
        );
        assert_eq!(
            proc.pending_display(),
            Some("g".to_string()),
            "sequence should remain at the anchor",
        );

        Ok(())
    }

    #[test]
    fn test_sticky_prefix_suppresses_auto_repeat() -> Result<()> {
        // Holding the prefix anchor and holding a leaf key should fire the
        // bound action exactly once; OS keyboard auto-repeat (a second
        // keydown without an intervening keyup) must be suppressed so toggle
        // actions don't flip on/off rapidly.
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        proc.process(char_event('g'), &ctx);

        let commands = proc.process(char_event('e'), &ctx);
        assert_eq!(extract_action_ids(&commands), vec!["cursor.end"]);

        // OS auto-repeat: keydown of `e` again with no prior keyup.
        let commands = proc.process(char_event('e'), &ctx);
        assert!(
            extract_action_ids(&commands).is_empty(),
            "auto-repeat keydown should be swallowed while sticky prefix is active",
        );

        Ok(())
    }

    #[test]
    fn test_bare_hold_release_keeps_overlay_visible() -> Result<()> {
        // Holding the prefix anchor and releasing it without firing any
        // child action should NOT signal the host to hide the overlay; the
        // user is still exploring.
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        proc.process(char_event('g'), &ctx);
        assert!(!proc.notify_key_release(chord('g')));

        Ok(())
    }

    #[test]
    fn test_two_key_sequence_with_pending() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        let commands = proc.process(char_event('g'), &ctx);

        assert_eq!(commands.len(), 1);
        assert!(matches!(&commands[0], InputCommand::Pending { display } if display == "g"));
        assert!(proc.needs_timeout());
        assert_eq!(proc.pending_display(), Some("g".to_string()));

        // Release the first `g` before the second press so the processor
        // treats this as a vim-style `gg` double-tap rather than a held
        // prefix (in which case sticky-prefix mode would activate).
        proc.notify_key_release(chord('g'));

        let commands = proc.process(char_event('g'), &ctx);

        let ids = extract_action_ids(&commands);
        assert_eq!(ids, vec!["cursor.top"]);
        assert!(!proc.needs_timeout());
        assert_eq!(proc.pending_display(), None);

        Ok(())
    }

    #[test]
    fn test_mode_switch_via_keybinding() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();
        assert_eq!(proc.current_mode(), &ModeId::normal());

        let commands = proc.process(char_event('i'), &ctx);

        assert_eq!(proc.current_mode(), &ModeId::insert());
        assert!(matches!(&commands[0], InputCommand::SwitchMode(m) if *m == ModeId::insert()));
        let action_ids = extract_action_ids(&commands);
        assert_eq!(action_ids, vec!["mode.normal.exit", "mode.insert.enter"]);

        Ok(())
    }

    #[test]
    fn test_insert_mode_passthrough() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        proc.process(char_event('i'), &ctx);
        assert_eq!(proc.current_mode(), &ModeId::insert());

        let commands = proc.process(char_event('h'), &ctx);

        assert_eq!(commands.len(), 1);
        assert!(matches!(&commands[0], InputCommand::InsertText(s) if s == "h"));

        let commands = proc.process(char_event('e'), &ctx);
        assert!(matches!(&commands[0], InputCommand::InsertText(s) if s == "e"));

        Ok(())
    }

    #[test]
    fn test_escape_cancels_pending_and_returns_to_normal() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        let commands = proc.process(char_event('g'), &ctx);
        assert!(matches!(&commands[0], InputCommand::Pending { .. }));
        assert!(proc.needs_timeout());

        let commands = proc.process(key_event(KeyCode::Escape), &ctx);

        assert!(commands.is_empty());
        assert!(!proc.needs_timeout());
        assert_eq!(proc.current_mode(), &ModeId::normal());

        Ok(())
    }

    #[test]
    fn test_escape_binding_runs_when_idle() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        let mut normal_trie = KeyTrie::new();
        normal_trie.bind(
            KeyChord::plain(KeyCode::Escape),
            ActionId::new("selection.clear"),
        );
        proc.set_keymap(ModeId::normal(), normal_trie);

        let commands = proc.process(key_event(KeyCode::Escape), &ctx);

        let ids = extract_action_ids(&commands);
        assert_eq!(ids, vec!["selection.clear"]);
        assert_eq!(proc.current_mode(), &ModeId::normal());
        assert!(!proc.needs_timeout());

        Ok(())
    }

    #[test]
    fn test_escape_exits_insert_to_normal() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        proc.process(char_event('i'), &ctx);
        assert_eq!(proc.current_mode(), &ModeId::insert());

        let commands = proc.process(key_event(KeyCode::Escape), &ctx);

        assert_eq!(proc.current_mode(), &ModeId::normal());
        let action_ids = extract_action_ids(&commands);
        assert_eq!(action_ids, vec!["mode.insert.exit", "mode.normal.enter"]);

        Ok(())
    }

    #[test]
    fn test_new_starts_in_normal_mode() -> Result<()> {
        let proc = InputProcessor::new();

        assert_eq!(proc.current_mode(), &ModeId::normal());
        assert!(!proc.needs_timeout());
        assert_eq!(proc.pending_display(), None);

        Ok(())
    }

    #[test]
    fn test_unmatched_key_in_normal_mode() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        let commands = proc.process(char_event('z'), &ctx);

        assert_eq!(commands.len(), 1);
        assert!(matches!(&commands[0], InputCommand::Unhandled(_)));

        Ok(())
    }

    #[test]
    fn test_timeout_expired_clears_pending() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        proc.process(char_event('g'), &ctx);
        assert!(proc.needs_timeout());

        let commands = proc.timeout_expired();

        assert!(!proc.needs_timeout());
        assert_eq!(commands.len(), 1);
        assert!(matches!(&commands[0], InputCommand::Unhandled(_)));

        Ok(())
    }

    #[test]
    fn test_sequence_miss_second_key() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        let commands = proc.process(char_event('g'), &ctx);
        assert!(matches!(&commands[0], InputCommand::Pending { .. }));

        let commands = proc.process(char_event('z'), &ctx);

        assert!(!proc.needs_timeout());
        assert_eq!(commands.len(), 1);
        assert!(matches!(&commands[0], InputCommand::Unhandled(_)));

        Ok(())
    }

    #[test]
    fn test_non_key_events_forwarded_as_unhandled() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        let commands = proc.process(InputEvent::FocusGained, &ctx);

        assert_eq!(commands.len(), 1);
        assert!(matches!(
            &commands[0],
            InputCommand::Unhandled(InputEvent::FocusGained)
        ));

        Ok(())
    }

    #[test]
    fn test_macro_record_and_playback_replays_actions() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        proc.process(char_event('q'), &ctx);
        proc.process(char_event('a'), &ctx);
        proc.process(char_event('j'), &ctx);
        proc.process(char_event('j'), &ctx);
        proc.process(char_event('j'), &ctx);
        proc.process(char_event('q'), &ctx);

        let commands = {
            proc.process(char_event('@'), &ctx);
            proc.process(char_event('a'), &ctx)
        };

        let ids = extract_action_ids(&commands);
        assert_eq!(ids, vec!["cursor.down", "cursor.down", "cursor.down"]);

        Ok(())
    }

    #[test]
    fn a_bound_q_is_the_bindings_not_a_macro() -> Result<()> {
        let mut proc = make_processor();
        let mut normal = KeyTrie::new();
        normal.bind(chord('q'), ActionId::new("quantize"));
        normal.bind(chord('j'), ActionId::new("cursor.down"));
        proc.set_keymap(ModeId::normal(), normal);
        let ctx = ActionContext::new();

        let commands = proc.process(char_event('q'), &ctx);
        assert_eq!(extract_action_ids(&commands), vec!["quantize"]);
        // Nothing is being recorded: the next key is itself.
        let commands = proc.process(char_event('j'), &ctx);
        assert_eq!(extract_action_ids(&commands), vec!["cursor.down"]);

        Ok(())
    }

    #[test]
    fn test_macro_count_prefix_repeats_playback() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        proc.process(char_event('q'), &ctx);
        proc.process(char_event('a'), &ctx);
        proc.process(char_event('j'), &ctx);
        proc.process(char_event('q'), &ctx);

        proc.process(char_event('3'), &ctx);
        proc.process(char_event('@'), &ctx);
        let commands = proc.process(char_event('a'), &ctx);

        let ids = extract_action_ids(&commands);
        assert_eq!(ids, vec!["cursor.down", "cursor.down", "cursor.down"]);

        Ok(())
    }

    #[test]
    fn test_macro_repeat_last_register_with_double_at() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        proc.process(char_event('q'), &ctx);
        proc.process(char_event('a'), &ctx);
        proc.process(char_event('j'), &ctx);
        proc.process(char_event('q'), &ctx);

        proc.process(char_event('@'), &ctx);
        let first = proc.process(char_event('a'), &ctx);
        assert_eq!(extract_action_ids(&first), vec!["cursor.down"]);

        proc.process(char_event('@'), &ctx);
        let second = proc.process(char_event('@'), &ctx);
        assert_eq!(extract_action_ids(&second), vec!["cursor.down"]);

        Ok(())
    }

    #[test]
    fn test_macro_does_not_record_playback() -> Result<()> {
        let mut proc = make_processor();
        let ctx = ActionContext::new();

        proc.process(char_event('q'), &ctx);
        proc.process(char_event('a'), &ctx);
        proc.process(char_event('j'), &ctx);
        proc.process(char_event('q'), &ctx);

        proc.process(char_event('q'), &ctx);
        proc.process(char_event('b'), &ctx);
        proc.process(char_event('@'), &ctx);
        proc.process(char_event('a'), &ctx);
        proc.process(char_event('q'), &ctx);

        proc.process(char_event('@'), &ctx);
        let commands = proc.process(char_event('b'), &ctx);

        assert!(commands.is_empty());

        Ok(())
    }
}
