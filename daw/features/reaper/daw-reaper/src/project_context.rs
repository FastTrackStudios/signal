//! Project Context Resolution
//!
//! Utilities for converting between daw-proto's ProjectContext and REAPER's project context.

use daw_proto::ProjectContext;
use reaper_high::{Project, Reaper};
use reaper_medium::{ProjectContext as ReaperProjectContext, ProjectRef, ReaProject};

/// Maximum number of project tabs to scan when enumerating or looking up projects.
///
/// REAPER doesn't have a hard tab limit, so this is a practical upper bound.
/// All project-scanning loops in the codebase must use this constant to ensure
/// consistency (e.g. `create()` can find tabs that `find_project_by_guid()` also sees).
pub const MAX_PROJECT_TABS: u32 = 256;

/// Get a stable identifier for a REAPER project tab.
///
/// Uses REAPER's native `GetSetProjectInfo_String("PROJECT_GUID", ...)` which
/// returns a stable GUID that persists across project pointer reallocations.
/// Falls back to a hash of the file path for saved projects if the native GUID
/// is unavailable, and finally to the raw pointer (least stable, but always works).
pub fn project_guid(project: &Project) -> String {
    // Try REAPER's native project GUID first — stable across pointer reallocations
    let reaper = Reaper::get();
    let low = reaper.medium_reaper().low();
    let key = c"PROJECT_GUID";
    let mut buf = [0u8; 128];
    let buf_ptr = buf.as_mut_ptr() as *mut std::ffi::c_char;
    let ok = unsafe {
        low.GetSetProjectInfo_String(project.raw().as_ptr(), key.as_ptr(), buf_ptr, false)
    };
    if ok {
        let guid_str = unsafe { std::ffi::CStr::from_ptr(buf_ptr) }
            .to_string_lossy()
            .to_string();
        if !guid_str.is_empty() {
            return guid_str;
        }
    }

    // Fallback: hash the file path for saved projects
    let path = project.file().map(|p| p.to_string()).unwrap_or_default();
    if !path.is_empty() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        return format!("{:x}", hasher.finish());
    }

    // Last resort: raw pointer (unstable across REAPER reallocations)
    format!("reaper-ptr-{:x}", project.raw().as_ptr() as usize)
}

/// Find a REAPER project by its GUID
///
/// Iterates through all open project tabs to find the one matching the given GUID.
/// Returns the raw ReaProject pointer that can be used with REAPER APIs.
pub fn find_project_by_guid_raw(guid: &str) -> Option<ReaProject> {
    let reaper = Reaper::get();

    for tab_index in 0..MAX_PROJECT_TABS {
        if let Some(result) = reaper
            .medium_reaper()
            .enum_projects(ProjectRef::Tab(tab_index), 0)
        {
            let project = Project::new(result.project);
            if project_guid(&project) == guid {
                return Some(result.project);
            }
        } else {
            // No more projects
            break;
        }
    }

    tracing::warn!("find_project_by_guid: no tab matched guid={guid}");
    None
}

/// Find a REAPER project by its GUID
///
/// Iterates through all open project tabs to find the one matching the given GUID.
/// Returns the high-level Project wrapper.
pub fn find_project_by_guid(guid: &str) -> Option<Project> {
    find_project_by_guid_raw(guid).map(Project::new)
}

/// Find a track on a project by GUID (linear scan).
pub(crate) fn find_track_by_guid(project: &Project, guid: &str) -> Option<reaper_high::Track> {
    for i in 0..project.track_count() {
        if let Some(track) = project.track_by_index(i)
            && track.guid().to_string_without_braces() == guid
        {
            return Some(track);
        }
    }
    None
}

/// Convert a daw-proto ProjectContext to a REAPER ProjectContext
///
/// If the project is found by GUID, returns a ProjectContext::Proj with the raw pointer.
/// Otherwise falls back to CurrentProject.
pub fn resolve_project_context(ctx: &ProjectContext) -> ReaperProjectContext {
    match ctx {
        ProjectContext::Current => ReaperProjectContext::CurrentProject,
        ProjectContext::Project(guid) => {
            if let Some(rea_project) = find_project_by_guid_raw(guid) {
                ReaperProjectContext::Proj(rea_project)
            } else {
                // Fallback to current project if not found
                tracing::warn!(
                    "Project with GUID {} not found, using current project",
                    guid
                );
                ReaperProjectContext::CurrentProject
            }
        }
    }
}
