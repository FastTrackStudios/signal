//! Trie data structure for efficient key sequence matching.
//!
//! A `KeyTrie` maps sequences of `KeyChord`s to `LeafAction`s. Single-key
//! bindings are leaves directly under the root. Multi-key sequences (like
//! `g g` → "go to top") form interior `TrieNode`s with children.

use std::collections::HashMap;

use crate::command::ActionId;
use crate::key::KeyChord;
use crate::mode::ModeId;

// region: --- Types

/// A node in the key binding trie.
///
/// Each node is either a terminal (`Leaf`) that maps to an action,
/// or an interior `Node` with children keyed by `KeyChord`.
#[derive(Debug, Clone)]
pub enum KeyTrie {
    /// A terminal binding that triggers an action.
    Leaf(LeafAction),
    /// An interior node with child bindings.
    Node(TrieNode),
}

/// Result of looking up an accumulated key sequence.
#[derive(Debug, Clone)]
pub enum TrieLookup {
    Match(LeafAction),
    Prefix,
    Miss,
}

/// An interior trie node holding child bindings.
#[derive(Debug, Clone)]
pub struct TrieNode {
    /// Human-readable name for this group (e.g., "goto", "window").
    pub name: String,
    /// Whether this node represents a sticky sub-mode.
    pub sticky: bool,
    /// Child bindings keyed by the next chord in the sequence.
    pub children: HashMap<KeyChord, KeyTrie>,
    /// Default action if no child matches. Used for "fallthrough" bindings.
    pub default: Option<Box<LeafAction>>,
}

/// The action at a trie leaf.
#[derive(Debug, Clone)]
pub enum LeafAction {
    /// Execute a named action.
    Action(ActionId),
    /// Switch the base editing mode.
    SwitchMode(ModeId),
    /// Push a transient sub-mode.
    PushMode(ModeId),
    /// Register an operator for operator-pending mode.
    Operator(String),
    /// Register a motion for operator composition.
    Motion(String),
    /// Register a text object for operator composition.
    TextObject(String),
    /// Execute a sequence of actions in order.
    Sequence(Vec<ActionId>),
    /// Explicitly unbind this key (blocks parent/default bindings).
    Unbind,
}

impl TrieNode {
    /// Create a new empty trie node.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sticky: false,
            children: HashMap::new(),
            default: None,
        }
    }

    /// Set the sticky flag (builder pattern).
    pub fn with_sticky(mut self, sticky: bool) -> Self {
        self.sticky = sticky;
        self
    }

    /// Set the default fallthrough action (builder pattern).
    pub fn with_default(mut self, action: LeafAction) -> Self {
        self.default = Some(Box::new(action));
        self
    }

    /// Insert a binding at the given key sequence path.
    ///
    /// For single-key bindings, `path` has one element. For multi-key
    /// sequences like `g g`, `path` has two elements. Intermediate nodes
    /// are created as needed.
    pub fn insert(&mut self, path: &[KeyChord], action: LeafAction) {
        match path {
            [] => {}
            [chord] => {
                self.children.insert(chord.clone(), KeyTrie::Leaf(action));
            }
            [chord, rest @ ..] => {
                let child = self
                    .children
                    .entry(chord.clone())
                    .or_insert_with(|| KeyTrie::Node(TrieNode::new("")));

                match child {
                    KeyTrie::Node(node) => node.insert(rest, action),
                    KeyTrie::Leaf(_) => {
                        // Overwrite leaf with a node containing the deeper binding.
                        let mut node = TrieNode::new("");
                        node.insert(rest, action);
                        *child = KeyTrie::Node(node);
                    }
                }
            }
        }
    }

    /// Look up a single chord in this node's children.
    pub fn get(&self, chord: &KeyChord) -> Option<&KeyTrie> {
        self.children.get(chord)
    }

    /// Merge another trie node into this one.
    ///
    /// Children from `other` override matching children in `self`.
    /// Non-conflicting children are preserved from both.
    pub fn merge(&mut self, other: TrieNode) {
        for (chord, other_trie) in other.children {
            match (self.children.get_mut(&chord), other_trie) {
                (Some(KeyTrie::Node(existing)), KeyTrie::Node(incoming)) => {
                    existing.merge(incoming);
                }
                (_, incoming) => {
                    self.children.insert(chord, incoming);
                }
            }
        }

        if other.default.is_some() {
            self.default = other.default;
        }
        if !other.name.is_empty() {
            self.name = other.name;
        }
    }
}

impl KeyTrie {
    /// Create an empty root trie.
    pub fn new() -> Self {
        Self::Node(TrieNode::new("root"))
    }

    /// Bind a single-key action.
    pub fn bind(&mut self, chord: KeyChord, action: ActionId) {
        self.bind_leaf(vec![chord], LeafAction::Action(action));
    }

    /// Bind a sequence to an action.
    pub fn bind_sequence(&mut self, sequence: Vec<KeyChord>, action: ActionId) {
        self.bind_leaf(sequence, LeafAction::Action(action));
    }

    /// Bind a single key to a mode switch.
    pub fn bind_mode_switch(&mut self, chord: KeyChord, mode: ModeId) {
        self.bind_leaf(vec![chord], LeafAction::SwitchMode(mode));
    }

    /// Bind an arbitrary leaf action.
    pub fn bind_leaf(&mut self, sequence: Vec<KeyChord>, action: LeafAction) {
        if sequence.is_empty() {
            return;
        }
        let root = self.ensure_root_node();
        root.insert(&sequence, action);
    }

    /// Resolve a sequence against this trie.
    pub fn lookup(&self, sequence: &[KeyChord]) -> TrieLookup {
        if sequence.is_empty() {
            return TrieLookup::Miss;
        }

        let mut current = match self {
            KeyTrie::Node(node) => node,
            KeyTrie::Leaf(_) => return TrieLookup::Miss,
        };

        for (idx, chord) in sequence.iter().enumerate() {
            let Some(next) = current.get(chord) else {
                return TrieLookup::Miss;
            };
            let is_last = idx + 1 == sequence.len();
            match next {
                KeyTrie::Leaf(action) => {
                    return if is_last {
                        TrieLookup::Match(action.clone())
                    } else {
                        TrieLookup::Miss
                    };
                }
                KeyTrie::Node(node) => {
                    if is_last {
                        return TrieLookup::Prefix;
                    }
                    current = node;
                }
            }
        }

        TrieLookup::Miss
    }

    fn ensure_root_node(&mut self) -> &mut TrieNode {
        if let KeyTrie::Leaf(_) = self {
            *self = KeyTrie::Node(TrieNode::new("root"));
        }
        match self {
            KeyTrie::Node(node) => node,
            KeyTrie::Leaf(_) => unreachable!("leaf replaced with node above"),
        }
    }
}

impl Default for KeyTrie {
    fn default() -> Self {
        Self::new()
    }
}

// endregion: --- Types

// region: --- Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::{KeyCode, Modifiers};

    // -- Support & Fixtures

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    fn chord(c: &str) -> KeyChord {
        KeyChord::plain(KeyCode::Character(c.to_string()))
    }

    fn ctrl_chord(c: &str) -> KeyChord {
        KeyChord::new(
            KeyCode::Character(c.to_string()),
            Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            },
        )
    }

    // -- Tests

    #[test]
    fn test_trie_single_key_lookup_returns_leaf() -> Result<()> {
        // -- Setup
        let mut root = TrieNode::new("root");
        root.insert(
            &[chord("j")],
            LeafAction::Action(ActionId::new("move.down")),
        );

        // -- Exec
        let result = root.get(&chord("j"));

        // -- Check
        assert!(
            matches!(result, Some(KeyTrie::Leaf(LeafAction::Action(id))) if id.as_str() == "move.down")
        );

        Ok(())
    }

    #[test]
    fn test_trie_two_key_sequence_first_returns_node_second_returns_leaf() -> Result<()> {
        // -- Setup
        let mut root = TrieNode::new("root");
        root.insert(
            &[chord("g"), chord("g")],
            LeafAction::Action(ActionId::new("goto.top")),
        );

        // -- Exec: first key
        let first = root.get(&chord("g"));

        // -- Check: first key returns a Node
        let node = match first {
            Some(KeyTrie::Node(n)) => n,
            _ => panic!("expected Node for first key of sequence"),
        };

        // -- Exec: second key
        let second = node.get(&chord("g"));

        // -- Check: second key returns Leaf
        assert!(
            matches!(second, Some(KeyTrie::Leaf(LeafAction::Action(id))) if id.as_str() == "goto.top")
        );

        Ok(())
    }

    #[test]
    fn test_trie_merge_override_behavior() -> Result<()> {
        // -- Setup
        let mut base = TrieNode::new("base");
        base.insert(
            &[chord("j")],
            LeafAction::Action(ActionId::new("move.down")),
        );
        base.insert(&[chord("k")], LeafAction::Action(ActionId::new("move.up")));

        let mut overlay = TrieNode::new("overlay");
        overlay.insert(
            &[chord("j")],
            LeafAction::Action(ActionId::new("custom.down")),
        );

        // -- Exec
        base.merge(overlay);

        // -- Check: `j` was overridden, `k` preserved
        assert!(matches!(
            base.get(&chord("j")),
            Some(KeyTrie::Leaf(LeafAction::Action(id))) if id.as_str() == "custom.down"
        ));
        assert!(matches!(
            base.get(&chord("k")),
            Some(KeyTrie::Leaf(LeafAction::Action(id))) if id.as_str() == "move.up"
        ));

        Ok(())
    }

    #[test]
    fn test_trie_sticky_node_flag_preserved() -> Result<()> {
        // -- Setup
        let sticky_node = TrieNode::new("goto").with_sticky(true);
        let mut root = TrieNode::new("root");
        root.children.insert(chord("g"), KeyTrie::Node(sticky_node));

        // -- Exec
        let result = root.get(&chord("g"));

        // -- Check
        match result {
            Some(KeyTrie::Node(n)) => {
                assert!(n.sticky);
                assert_eq!(n.name, "goto");
            }
            _ => panic!("expected Node"),
        }

        Ok(())
    }

    #[test]
    fn test_trie_missing_key_returns_none() -> Result<()> {
        // -- Setup
        let root = TrieNode::new("root");

        // -- Exec & Check
        assert!(root.get(&chord("x")).is_none());

        Ok(())
    }

    #[test]
    fn test_trie_insert_with_modifiers() -> Result<()> {
        // -- Setup
        let mut root = TrieNode::new("root");
        root.insert(
            &[ctrl_chord("s")],
            LeafAction::Action(ActionId::new("file.save")),
        );

        // -- Exec & Check: Ctrl+S matches
        assert!(root.get(&ctrl_chord("s")).is_some());
        // Plain 's' does not match
        assert!(root.get(&chord("s")).is_none());

        Ok(())
    }

    #[test]
    fn test_trie_leaf_action_variants() -> Result<()> {
        // -- Setup
        let mut root = TrieNode::new("root");
        root.insert(&[chord("i")], LeafAction::SwitchMode(ModeId::insert()));
        root.insert(&[chord("d")], LeafAction::Operator("delete".to_string()));
        root.insert(&[chord("w")], LeafAction::Motion("word".to_string()));
        root.insert(
            &[chord("q")],
            LeafAction::Sequence(vec![ActionId::new("save"), ActionId::new("quit")]),
        );

        // -- Check
        assert!(matches!(
            root.get(&chord("i")),
            Some(KeyTrie::Leaf(LeafAction::SwitchMode(_)))
        ));
        assert!(matches!(
            root.get(&chord("d")),
            Some(KeyTrie::Leaf(LeafAction::Operator(s))) if s == "delete"
        ));
        assert!(matches!(
            root.get(&chord("w")),
            Some(KeyTrie::Leaf(LeafAction::Motion(s))) if s == "word"
        ));
        assert!(matches!(
            root.get(&chord("q")),
            Some(KeyTrie::Leaf(LeafAction::Sequence(v))) if v.len() == 2
        ));

        Ok(())
    }

    #[test]
    fn test_trie_merge_deep_nodes() -> Result<()> {
        // -- Setup
        let mut base = TrieNode::new("root");
        base.insert(
            &[chord("g"), chord("g")],
            LeafAction::Action(ActionId::new("goto.top")),
        );

        let mut overlay = TrieNode::new("root");
        overlay.insert(
            &[chord("g"), chord("d")],
            LeafAction::Action(ActionId::new("goto.definition")),
        );

        // -- Exec
        base.merge(overlay);

        // -- Check: both `g g` and `g d` exist
        let g_node = match base.get(&chord("g")) {
            Some(KeyTrie::Node(n)) => n,
            _ => panic!("expected Node for 'g'"),
        };
        assert!(matches!(
            g_node.get(&chord("g")),
            Some(KeyTrie::Leaf(LeafAction::Action(id))) if id.as_str() == "goto.top"
        ));
        assert!(matches!(
            g_node.get(&chord("d")),
            Some(KeyTrie::Leaf(LeafAction::Action(id))) if id.as_str() == "goto.definition"
        ));

        Ok(())
    }

    #[test]
    fn test_trie_default_fallthrough() -> Result<()> {
        // -- Setup
        let node = TrieNode::new("fallthrough")
            .with_default(LeafAction::Action(ActionId::new("default.action")));

        // -- Check
        let default = node.default.as_ref().unwrap();
        assert!(
            matches!(default.as_ref(), LeafAction::Action(id) if id.as_str() == "default.action")
        );

        Ok(())
    }

    #[test]
    fn test_trie_unbind_variant() -> Result<()> {
        // -- Setup
        let mut root = TrieNode::new("root");
        root.insert(&[chord("q")], LeafAction::Unbind);

        // -- Check
        assert!(matches!(
            root.get(&chord("q")),
            Some(KeyTrie::Leaf(LeafAction::Unbind))
        ));

        Ok(())
    }
}

// endregion: --- Tests
