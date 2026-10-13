//! Undo scope and project part flags
//!
//! Defines what parts of the project are affected by an undoable operation.

use facet::Facet;

/// When creating an undo point, this defines what parts of the project might have been affected by
/// the undoable operation.
#[derive(Clone, Debug, Default, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum UndoScope {
    /// Everything could have been affected.
    ///
    /// This is the safest variant but can lead to very large undo states.
    #[default]
    All,
    /// A combination of the given project parts could have been affected.
    ///
    /// If you miss some parts, *undo* can behave in weird ways.
    Scoped(Vec<ProjectPart>),
}

impl UndoScope {
    /// Create an undo scope that includes all project parts
    pub fn all() -> Self {
        Self::All
    }

    /// Create an undo scope for specific project parts
    pub fn scoped(parts: impl IntoIterator<Item = ProjectPart>) -> Self {
        Self::Scoped(parts.into_iter().collect())
    }

    /// Create an undo scope for items only
    pub fn items() -> Self {
        Self::Scoped(vec![ProjectPart::Items])
    }

    /// Create an undo scope for track configuration only
    pub fn track_cfg() -> Self {
        Self::Scoped(vec![ProjectPart::TrackCfg])
    }

    /// Create an undo scope for FX only
    pub fn fx() -> Self {
        Self::Scoped(vec![ProjectPart::Fx])
    }
}

/// Parts of a REAPER project that can be affected by undoable operations
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Facet)]
#[repr(u32)]
pub enum ProjectPart {
    /// Freeze state
    Freeze = 1,
    /// Track/master FX
    Fx = 2,
    /// Track items
    Items = 4,
    /// Loop selection, markers, regions and extensions
    MiscCfg = 8,
    /// Track/master vol/pan/routing and all envelopes (master included)
    TrackCfg = 16,
}
