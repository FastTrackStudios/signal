//! Typed constants and structures for REAPER's HWND child window IDs and window class names.
//!
//! Compiled from Meo-Ada Mespotine's documentation of REAPER's window hierarchy.

pub mod class_names;
pub mod dialogs;

pub use class_names::WindowClass;

use serde::{Deserialize, Serialize};

/// A typed wrapper around a child window ID (the integer passed to `GetDlgItem`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChildId(pub u32);

impl ChildId {
    /// Returns the raw numeric ID.
    pub fn raw(self) -> u32 {
        self.0
    }
}

impl From<u32> for ChildId {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<ChildId> for u32 {
    fn from(v: ChildId) -> u32 {
        v.0
    }
}

impl core::fmt::Display for ChildId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}
