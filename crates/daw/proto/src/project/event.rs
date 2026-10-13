//! Project lifecycle events.

use super::ProjectInfo;
use facet::Facet;

/// Events emitted when project state changes.
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum ProjectEvent {
    /// A project was opened/added.
    Opened(ProjectInfo),
    /// A project was closed (contains the GUID).
    Closed(String),
    /// The active/current project changed (contains new current project
    /// GUID, or None).
    CurrentChanged(Option<String>),
    /// A project's metadata was modified.
    Changed(ProjectInfo),
    /// Full project list refresh (e.g., after reconnection).
    ProjectsChanged(Vec<ProjectInfo>),
}

/// Streaming envelope — sibling shape to MarkerStreamEvent et al.
/// No project_guid field because the events themselves carry the
/// relevant guid (or `None` for `CurrentChanged(None)`).
#[derive(Debug, Clone, Facet)]
pub struct ProjectStreamEvent {
    pub event: ProjectEvent,
}

#[cfg(feature = "vox")]
#[allow(unsafe_code)]
mod reborrow_impls {
    use super::{ProjectEvent, ProjectStreamEvent};
    unsafe impl vox_types::Reborrow for ProjectEvent {
        type Ref<'a> = ProjectEvent;
    }
    unsafe impl vox_types::Reborrow for ProjectStreamEvent {
        type Ref<'a> = ProjectStreamEvent;
    }
}
