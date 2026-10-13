//! Extension State — persistent key-value storage (architect::rpc port).
//!
//! Sync trait + `#[architect::rpc]` derives the async vox client +
//! dispatcher. Stateless singleton backends — `ProjectContext` and
//! the section/key strings flow through every call.
//!
//! Global keys live in REAPER's `reaper-extstate.ini` (`persist=true`)
//! or memory (`persist=false`); project-scoped keys live inside the
//! `.RPP` file.
//!
//! Typed access via serde is provided client-side in `daw-control`,
//! not here — keeps the proto layer free of serde deps.

use crate::DawResult;
use crate::batch::ProjectArg;
use crate::project::ProjectContext;

#[architect::rpc(ops(ProjectContext as ProjectArg))]
pub trait ExtState {
    /// Get a global value. `None` if unset or empty.
    fn get(&self, section: &str, key: &str) -> Option<String>;

    /// Set a global value. `persist=true` writes through to
    /// `reaper-extstate.ini`; `false` keeps it in memory only.
    fn set(&self, section: &str, key: &str, value: &str, persist: bool) -> DawResult<()>;

    /// Delete a global value. `persist=true` removes it from
    /// persistent storage as well.
    fn delete(&self, section: &str, key: &str, persist: bool) -> DawResult<()>;

    /// Whether a global value is set.
    fn has(&self, section: &str, key: &str) -> bool;

    /// Project-scoped get. Returns `None` if unset.
    fn get_project(&self, project: ProjectContext, section: &str, key: &str) -> Option<String>;

    /// Project-scoped set. Value lives inside the `.RPP` file.
    fn set_project(
        &self,
        project: ProjectContext,
        section: &str,
        key: &str,
        value: &str,
    ) -> DawResult<()>;

    /// Project-scoped delete.
    fn delete_project(&self, project: ProjectContext, section: &str, key: &str) -> DawResult<()>;

    /// Whether a project-scoped value is set.
    fn has_project(&self, project: ProjectContext, section: &str, key: &str) -> bool;
}
