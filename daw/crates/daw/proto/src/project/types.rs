//! Project data types — `ProjectContext`, `ProjectInfo`.

use facet::Facet;

/// Context specifying which project to operate on.
///
/// Used by all DAW services to target operations at a specific project
/// or the currently active project.
#[repr(u8)]
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Facet)]
pub enum ProjectContext {
    /// The currently active/focused project (default).
    #[default]
    Current,
    /// A specific project identified by its GUID.
    Project(String),
}

impl ProjectContext {
    pub fn current() -> Self {
        Self::Current
    }

    pub fn project(guid: impl Into<String>) -> Self {
        Self::Project(guid.into())
    }
}

/// Project metadata — GUID, name, filesystem path.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct ProjectInfo {
    pub guid: String,
    pub name: String,
    pub path: String,
}
