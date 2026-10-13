//! Which-Key declarative entry types.
//!
//! Provides the `WhichKeyEntry` enum used to define prefix key trees
//! declaratively. These trees are consumed by `bridge::preset_to_keymap_config`
//! which converts them into `KeymapConfig` entries for the unified
//! `InputProcessor`.

use facet::Facet;

/// Declarative entry for building a which-key tree.
#[derive(Debug, Clone, Facet)]
#[repr(C)]
pub enum WhichKeyEntry {
    Leaf {
        key: String,
        label: String,
        action: String,
    },
    Branch {
        key: String,
        label: String,
        action: Option<String>,
        children: Vec<WhichKeyEntry>,
    },
}

impl WhichKeyEntry {
    pub fn leaf(key: &str, label: &str, action: &str) -> Self {
        Self::Leaf {
            key: key.into(),
            label: label.into(),
            action: action.into(),
        }
    }

    pub fn branch(key: &str, label: &str, children: Vec<WhichKeyEntry>) -> Self {
        Self::Branch {
            key: key.into(),
            label: label.into(),
            action: None,
            children,
        }
    }

    pub fn branch_with_action(
        key: &str,
        label: &str,
        action: &str,
        children: Vec<WhichKeyEntry>,
    ) -> Self {
        Self::Branch {
            key: key.into(),
            label: label.into(),
            action: Some(action.into()),
            children,
        }
    }
}
