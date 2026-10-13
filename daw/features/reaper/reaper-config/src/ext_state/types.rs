//! Type definitions for REAPER external state persistence files.

use serde::{Deserialize, Serialize};

/// A single key-value pair within an extension state section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtStateEntry {
    /// The state key.
    pub key: String,
    /// The state value (arbitrary string).
    pub value: String,
}

/// One named section in `reaper-extstate.ini`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtStateSection {
    /// The section name (extension identifier).
    pub name: String,
    /// All key-value pairs within this section.
    pub entries: Vec<ExtStateEntry>,
}

/// The complete contents of `reaper-extstate.ini`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtState {
    /// All sections in file order.
    pub sections: Vec<ExtStateSection>,
}
