//! Keystroke accumulator for multi-key sequences with timeout.
//!
//! `SequenceState` buffers incoming `KeyChord`s and walks the `KeyTrie`
//! one step at a time. It reports whether the sequence matched a leaf,
//! is still pending more input, found no match, or timed out.

use std::time::{Duration, Instant};

use crate::key::KeyChord;
use crate::trie::{KeyTrie, LeafAction, TrieNode};

// region: --- Types

/// Result of feeding a chord to the sequence accumulator.
#[derive(Debug)]
pub enum SequenceResult<'a> {
    /// The sequence resolved to a complete binding.
    Matched(LeafAction),
    /// More keys are needed — the given node has further children.
    Pending(&'a TrieNode),
    /// The chord did not match any binding at the current position.
    NoMatch,
    /// The pending sequence timed out before completing.
    Timeout(Vec<KeyChord>),
}

/// Accumulator that buffers key chords and walks a trie.
///
/// Created once per input processor. Call [`feed`](Self::feed) for each
/// incoming chord, and [`timeout_expired`](Self::timeout_expired) on a
/// timer tick to detect stale sequences.
#[derive(Debug)]
pub struct SequenceState {
    /// Chords accumulated so far in the current (incomplete) sequence.
    pending_keys: Vec<KeyChord>,
    /// When the first chord of the current sequence was received.
    started_at: Option<Instant>,
    /// How long to wait before declaring a timeout.
    timeout: Duration,
}

/// Default sequence timeout (1 second).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(1);

impl SequenceState {
    /// Create a new accumulator with the default timeout.
    pub fn new() -> Self {
        Self {
            pending_keys: Vec::new(),
            started_at: None,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Create a new accumulator with a custom timeout duration.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            pending_keys: Vec::new(),
            started_at: None,
            timeout,
        }
    }

    /// Feed a chord and resolve against the trie root (or current position).
    ///
    /// The caller provides the `root` trie node. On the first chord of a
    /// sequence, the accumulator looks up the chord in `root`. On subsequent
    /// chords it walks deeper by looking up in the child node stored from
    /// the previous `Pending` result.
    ///
    /// The `current` parameter should be `root` when starting a new sequence,
    /// or the `TrieNode` from the previous `Pending` result when continuing.
    pub fn feed<'a>(&mut self, chord: KeyChord, current: &'a TrieNode) -> SequenceResult<'a> {
        // Start the timeout clock on the first chord of a new sequence
        if self.pending_keys.is_empty() {
            self.started_at = Some(Instant::now());
        }

        match current.get(&chord) {
            Some(KeyTrie::Leaf(action)) => {
                let action = action.clone();
                self.reset();
                SequenceResult::Matched(action)
            }
            Some(KeyTrie::Node(node)) => {
                self.pending_keys.push(chord);
                SequenceResult::Pending(node)
            }
            None => {
                self.reset();
                SequenceResult::NoMatch
            }
        }
    }

    /// Check whether the current pending sequence has exceeded the timeout.
    ///
    /// Returns `Some(Timeout(...))` with the accumulated keys if expired,
    /// or `None` if no sequence is pending or it hasn't expired yet.
    pub fn timeout_expired(&mut self) -> Option<SequenceResult<'static>> {
        let started = self.started_at?;
        if started.elapsed() >= self.timeout {
            let keys = std::mem::take(&mut self.pending_keys);
            self.started_at = None;
            Some(SequenceResult::Timeout(keys))
        } else {
            None
        }
    }

    /// Whether a multi-key sequence is in progress.
    pub fn is_pending(&self) -> bool {
        !self.pending_keys.is_empty()
    }

    /// Human-readable display of the pending key sequence.
    ///
    /// Used to show the user what keys have been buffered so far
    /// (e.g., "g" while waiting for the second key of `g g`).
    pub fn pending_display(&self) -> String {
        self.pending_keys
            .iter()
            .map(format_chord)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Discard any accumulated state and start fresh.
    pub fn reset(&mut self) {
        self.pending_keys.clear();
        self.started_at = None;
    }
}

impl Default for SequenceState {
    fn default() -> Self {
        Self::new()
    }
}

// region: --- Helpers

/// Format a `KeyChord` for display.
fn format_chord(chord: &KeyChord) -> String {
    let mut parts = Vec::new();
    if chord.modifiers.ctrl {
        parts.push("C");
    }
    if chord.modifiers.alt {
        parts.push("A");
    }
    if chord.modifiers.shift {
        parts.push("S");
    }
    if chord.modifiers.meta {
        parts.push("M");
    }

    let key_str = match &chord.key {
        crate::key::KeyCode::Character(c) => c.clone(),
        crate::key::KeyCode::ArrowUp => "Up".to_string(),
        crate::key::KeyCode::ArrowDown => "Down".to_string(),
        crate::key::KeyCode::ArrowLeft => "Left".to_string(),
        crate::key::KeyCode::ArrowRight => "Right".to_string(),
        crate::key::KeyCode::Enter => "Enter".to_string(),
        crate::key::KeyCode::Escape => "Esc".to_string(),
        crate::key::KeyCode::Tab => "Tab".to_string(),
        crate::key::KeyCode::Backspace => "BS".to_string(),
        crate::key::KeyCode::Delete => "Del".to_string(),
        crate::key::KeyCode::F(n) => format!("F{n}"),
    };
    parts.push(&key_str);

    if parts.len() > 1 {
        format!("<{}>", parts.join("-"))
    } else {
        key_str
    }
}

// endregion: --- Helpers

// region: --- Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::ActionId;
    use crate::key::{KeyCode, Modifiers};

    // -- Support & Fixtures

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    fn chord(c: &str) -> KeyChord {
        KeyChord::plain(KeyCode::Character(c.to_string()))
    }

    fn make_test_trie() -> TrieNode {
        let mut root = TrieNode::new("root");
        // Single key: j → move.down
        root.insert(
            &[chord("j")],
            LeafAction::Action(ActionId::new("move.down")),
        );
        // Two-key sequence: g g → goto.top
        root.insert(
            &[chord("g"), chord("g")],
            LeafAction::Action(ActionId::new("goto.top")),
        );
        // Two-key sequence: g d → goto.definition
        root.insert(
            &[chord("g"), chord("d")],
            LeafAction::Action(ActionId::new("goto.definition")),
        );
        root
    }

    // -- Tests

    #[test]
    fn test_sequence_single_key_match_returns_matched() -> Result<()> {
        // -- Setup
        let root = make_test_trie();
        let mut state = SequenceState::new();

        // -- Exec
        let result = state.feed(chord("j"), &root);

        // -- Check
        assert!(
            matches!(&result, SequenceResult::Matched(LeafAction::Action(id)) if id.as_str() == "move.down")
        );
        assert!(!state.is_pending());

        Ok(())
    }

    #[test]
    fn test_sequence_two_key_first_pending_second_matched() -> Result<()> {
        // -- Setup
        let root = make_test_trie();
        let mut state = SequenceState::new();

        // -- Exec: first key
        let result1 = state.feed(chord("g"), &root);

        // -- Check: first key returns Pending
        let node = match result1 {
            SequenceResult::Pending(n) => n,
            other => panic!("expected Pending, got {other:?}"),
        };
        assert!(state.is_pending());
        assert_eq!(state.pending_display(), "g");

        // -- Exec: second key against the pending node
        let result2 = state.feed(chord("g"), node);

        // -- Check: second key returns Matched
        assert!(
            matches!(&result2, SequenceResult::Matched(LeafAction::Action(id)) if id.as_str() == "goto.top")
        );
        assert!(!state.is_pending());

        Ok(())
    }

    #[test]
    fn test_sequence_invalid_key_after_pending_returns_no_match() -> Result<()> {
        // -- Setup
        let root = make_test_trie();
        let mut state = SequenceState::new();

        // -- Exec: first key (valid prefix)
        let result1 = state.feed(chord("g"), &root);
        let node = match result1 {
            SequenceResult::Pending(n) => n,
            other => panic!("expected Pending, got {other:?}"),
        };

        // -- Exec: second key (invalid — 'x' is not a child of 'g')
        let result2 = state.feed(chord("x"), node);

        // -- Check
        assert!(matches!(result2, SequenceResult::NoMatch));
        assert!(!state.is_pending());

        Ok(())
    }

    #[test]
    fn test_sequence_timeout_on_pending_returns_accumulated_keys() -> Result<()> {
        // -- Setup
        let root = make_test_trie();
        let mut state = SequenceState::with_timeout(Duration::from_millis(0));

        // -- Exec: start a sequence
        let result = state.feed(chord("g"), &root);
        assert!(matches!(result, SequenceResult::Pending(_)));
        assert!(state.is_pending());

        // Wait a tiny bit for the zero-duration timeout to fire
        std::thread::sleep(Duration::from_millis(1));

        // -- Exec: check timeout
        let timeout_result = state.timeout_expired();

        // -- Check
        match timeout_result {
            Some(SequenceResult::Timeout(keys)) => {
                assert_eq!(keys.len(), 1);
                assert_eq!(keys[0], chord("g"));
            }
            other => panic!("expected Timeout, got {other:?}"),
        }
        assert!(!state.is_pending());

        Ok(())
    }

    #[test]
    fn test_sequence_no_timeout_when_not_pending() -> Result<()> {
        // -- Setup
        let mut state = SequenceState::new();

        // -- Exec & Check
        assert!(state.timeout_expired().is_none());

        Ok(())
    }

    #[test]
    fn test_sequence_no_timeout_when_not_expired() -> Result<()> {
        // -- Setup
        let root = make_test_trie();
        let mut state = SequenceState::with_timeout(Duration::from_secs(60));

        // -- Exec: start a sequence with a very long timeout
        let _ = state.feed(chord("g"), &root);
        assert!(state.is_pending());

        // -- Check: timeout hasn't fired yet
        assert!(state.timeout_expired().is_none());
        assert!(state.is_pending());

        Ok(())
    }

    #[test]
    fn test_sequence_reset_clears_state() -> Result<()> {
        // -- Setup
        let root = make_test_trie();
        let mut state = SequenceState::new();
        let _ = state.feed(chord("g"), &root);
        assert!(state.is_pending());

        // -- Exec
        state.reset();

        // -- Check
        assert!(!state.is_pending());
        assert_eq!(state.pending_display(), "");

        Ok(())
    }

    #[test]
    fn test_sequence_pending_display_multiple_keys() -> Result<()> {
        // -- Setup
        let mut root = TrieNode::new("root");
        // Three-key sequence: g d x → some.action
        root.insert(
            &[chord("g"), chord("d"), chord("x")],
            LeafAction::Action(ActionId::new("some.action")),
        );
        let mut state = SequenceState::new();

        // -- Exec: feed first two keys
        let r1 = state.feed(chord("g"), &root);
        let node1 = match r1 {
            SequenceResult::Pending(n) => n,
            other => panic!("expected Pending, got {other:?}"),
        };
        let r2 = state.feed(chord("d"), node1);
        assert!(matches!(r2, SequenceResult::Pending(_)));

        // -- Check
        assert_eq!(state.pending_display(), "g d");

        Ok(())
    }

    #[test]
    fn test_sequence_pending_display_with_modifiers() -> Result<()> {
        // -- Setup
        let ctrl_g = KeyChord::new(
            KeyCode::Character("g".to_string()),
            Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            },
        );
        let mut root = TrieNode::new("root");
        root.insert(
            &[ctrl_g.clone(), chord("d")],
            LeafAction::Action(ActionId::new("some.action")),
        );
        let mut state = SequenceState::new();

        // -- Exec
        let _ = state.feed(ctrl_g, &root);

        // -- Check
        assert_eq!(state.pending_display(), "<C-g>");

        Ok(())
    }

    #[test]
    fn test_sequence_matched_resets_for_next_sequence() -> Result<()> {
        // -- Setup
        let root = make_test_trie();
        let mut state = SequenceState::new();

        // -- Exec: complete a single-key match
        let r1 = state.feed(chord("j"), &root);
        assert!(matches!(r1, SequenceResult::Matched(_)));

        // -- Check: state is clean for the next sequence
        assert!(!state.is_pending());
        assert_eq!(state.pending_display(), "");

        // -- Exec: start a new sequence
        let r2 = state.feed(chord("g"), &root);
        assert!(matches!(r2, SequenceResult::Pending(_)));
        assert_eq!(state.pending_display(), "g");

        Ok(())
    }
}

// endregion: --- Tests
