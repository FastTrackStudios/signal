//! Input Matcher
//!
//! Matches key sequences against bindings and builds commands.

use crate::input::bindings::{BindingEntry, Bindings};
use crate::input::state::{ActionKey, ActionType, Command, CommandState};

/// Possible action sequences for a given mode and context
#[derive(Debug, Clone)]
pub struct ActionSequence {
    pub sequence: Vec<ActionType>,
}

/// Action sequences organized by mode
pub struct ActionSequences {
    pub normal: Vec<Vec<ActionType>>,
    pub visual_timeline: Vec<Vec<ActionType>>,
    pub visual_track: Vec<Vec<ActionType>>,
}

impl ActionSequences {
    /// Get action sequences for a mode
    pub fn for_mode(&self, mode: &crate::input::state::Mode) -> &Vec<Vec<ActionType>> {
        match mode {
            crate::input::state::Mode::Normal => &self.normal,
            crate::input::state::Mode::VisualTimeline => &self.visual_timeline,
            crate::input::state::Mode::VisualTrack => &self.visual_track,
        }
    }
}

/// Default action sequences (similar to reaper-keys)
pub fn default_action_sequences() -> ActionSequences {
    ActionSequences {
        normal: vec![
            vec![ActionType::TimelineOperator, ActionType::TimelineMotion],
            vec![ActionType::TimelineOperator, ActionType::TimelineSelector],
            vec![ActionType::TimelineMotion],
            vec![ActionType::Command],
        ],
        visual_timeline: vec![
            vec![ActionType::VisualTimelineCommand],
            vec![ActionType::TimelineOperator],
            vec![ActionType::TimelineSelector],
            vec![ActionType::TimelineMotion],
            vec![ActionType::Command],
        ],
        visual_track: vec![
            vec![ActionType::VisualTrackCommand],
            vec![ActionType::TrackOperator],
            vec![ActionType::TrackSelector],
            vec![ActionType::TrackMotion],
            vec![ActionType::Command],
        ],
    }
}

/// Completions for partial key sequences
#[derive(Debug, Clone)]
pub struct Completions {
    pub action_type: ActionType,
    pub possible_keys: Vec<String>,
}

/// Match a key sequence against bindings and build a command
pub fn match_sequence(
    state: &CommandState,
    bindings: &Bindings,
    sequences: &ActionSequences,
) -> Option<(Command, Option<Completions>)> {
    let key_sequence = &state.key_sequence;
    let mode_sequences = sequences.for_mode(&state.mode);

    // Try to match against each possible action sequence
    for action_sequence in mode_sequences {
        if let Some(command) = try_match_sequence(key_sequence, action_sequence, bindings, state) {
            return Some((command, None));
        }
    }

    // If no match, return completions
    let completions = build_completions(key_sequence, sequences, bindings, state);
    Some((
        Command {
            action_sequence: vec![],
            action_keys: vec![],
            mode: state.mode,
            context: state.context,
        },
        completions,
    ))
}

/// Try to match a key sequence against a specific action sequence pattern
fn try_match_sequence(
    key_sequence: &str,
    action_sequence: &[ActionType],
    bindings: &Bindings,
    state: &CommandState,
) -> Option<Command> {
    let mut remaining = key_sequence;
    let mut action_keys = Vec::new();

    for action_type in action_sequence {
        if let Some((new_remaining, action_key)) =
            extract_action_key(remaining, action_type, bindings, state)
        {
            action_keys.push(action_key);
            remaining = new_remaining;
        } else {
            return None;
        }
    }

    // If we consumed the entire sequence, we have a match
    if remaining.is_empty() {
        Some(Command {
            action_sequence: action_sequence.to_vec(),
            action_keys,
            mode: state.mode,
            context: state.context,
        })
    } else {
        None
    }
}

/// Extract an action key from the beginning of a key sequence
fn extract_action_key<'a>(
    key_sequence: &'a str,
    action_type: &ActionType,
    bindings: &Bindings,
    state: &CommandState,
) -> Option<(&'a str, ActionKey)> {
    // Get bindings for this action type
    let bindings_map = bindings.get_bindings(action_type, state.context)?;

    // Try to match from longest to shortest
    let mut best_match: Option<(&str, ActionKey)> = None;
    let mut best_len = 0;

    for (binding_key, entry) in bindings_map {
        if key_sequence.starts_with(binding_key)
            && binding_key.len() > best_len
            && let BindingEntry::Action(action_id) = entry
        {
            best_len = binding_key.len();
            best_match = Some((
                &key_sequence[binding_key.len()..],
                ActionKey {
                    identifier: action_id.clone(),
                    repetition_count: None,
                    register: None,
                },
            ));
        }
    }

    best_match
}

/// Build completions for a partial key sequence
fn build_completions(
    key_sequence: &str,
    sequences: &ActionSequences,
    bindings: &Bindings,
    state: &CommandState,
) -> Option<Completions> {
    // Find which action type we're currently matching
    let mode_sequences = sequences.for_mode(&state.mode);

    for action_sequence in mode_sequences {
        // Try to match as much as possible
        let mut remaining = key_sequence;
        let mut matched_types = Vec::new();

        for action_type in action_sequence {
            if let Some((new_remaining, _)) =
                extract_action_key(remaining, action_type, bindings, state)
            {
                matched_types.push(action_type.clone());
                remaining = new_remaining;
            } else {
                // This is the action type we need completions for
                if let Some(bindings_map) = bindings.get_bindings(action_type, state.context) {
                    let possible_keys: Vec<String> = bindings_map
                        .keys()
                        .filter(|k| k.starts_with(remaining))
                        .cloned()
                        .collect();

                    if !possible_keys.is_empty() {
                        return Some(Completions {
                            action_type: action_type.clone(),
                            possible_keys,
                        });
                    }
                }
                break;
            }
        }
    }

    None
}
