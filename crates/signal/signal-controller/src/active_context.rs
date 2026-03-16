//! Active context state machine for variation switching.
//!
//! Tracks what the user is currently working with — a Profile, a Rig, or a
//! Song — so that "Switch to Variation N" can resolve N to a concrete entity
//! (patch, scene, or section) without the action needing to know which mode
//! is active.
//!
//! The context is owned by [`SignalController`] and shared across all clones
//! via `Arc<RwLock<..>>`. UI navigation sets the context; action handlers read it.

use signal_proto::{
    profile::ProfileId,
    rig::RigId,
    song::SongId,
};

/// The currently active collection context for variation switching.
///
/// "Switch to Variation N" resolves against this to determine what to activate:
/// - `Profile` → activate the Nth patch
/// - `Rig` → switch to the Nth scene
/// - `Song` → jump to the Nth section
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveContext {
    /// No context active — variation actions are no-ops.
    None,

    /// A profile is active. Variations map to patches by index.
    Profile {
        id: ProfileId,
        /// 0-based index of the currently active patch.
        active_index: usize,
    },

    /// A rig is active. Variations map to scenes by index.
    Rig {
        id: RigId,
        /// 0-based index of the currently active scene.
        active_index: usize,
    },

    /// A song is active. Variations map to sections by index.
    Song {
        id: SongId,
        /// 0-based index of the currently active section.
        active_index: usize,
    },
}

impl ActiveContext {
    /// Get the 0-based active variation index, if any context is set.
    pub fn active_index(&self) -> Option<usize> {
        match self {
            ActiveContext::None => None,
            ActiveContext::Profile { active_index, .. }
            | ActiveContext::Rig { active_index, .. }
            | ActiveContext::Song { active_index, .. } => Some(*active_index),
        }
    }

    /// Set the active variation index within the current context.
    /// Returns `false` if context is `None`.
    pub fn set_active_index(&mut self, index: usize) -> bool {
        match self {
            ActiveContext::None => false,
            ActiveContext::Profile { active_index, .. }
            | ActiveContext::Rig { active_index, .. }
            | ActiveContext::Song { active_index, .. } => {
                *active_index = index;
                true
            }
        }
    }

    /// Returns `true` if no context is set.
    pub fn is_none(&self) -> bool {
        matches!(self, ActiveContext::None)
    }
}

impl Default for ActiveContext {
    fn default() -> Self {
        ActiveContext::None
    }
}

/// Thread-safe wrapper for `ActiveContext`.
///
/// All `SignalController` clones share the same `ActiveContextState` via `Arc`.
#[derive(Debug, Clone, Default)]
pub struct ActiveContextState {
    inner: std::sync::Arc<std::sync::RwLock<ActiveContext>>,
}

impl ActiveContextState {
    /// Read the current context.
    pub fn get(&self) -> ActiveContext {
        self.inner.read().expect("lock poisoned").clone()
    }

    /// Replace the current context.
    pub fn set(&self, ctx: ActiveContext) {
        *self.inner.write().expect("lock poisoned") = ctx;
    }

    /// Update the active index within the current context.
    /// Returns `false` if context is `None`.
    pub fn set_active_index(&self, index: usize) -> bool {
        self.inner
            .write()
            .expect("lock poisoned")
            .set_active_index(index)
    }
}
