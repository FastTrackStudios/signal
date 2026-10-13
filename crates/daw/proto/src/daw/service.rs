//! Root sync `Daw` handle.
//!
//! Relocated from the retired `crate::sync` module. The trait is the
//! entry point for all sync DAW operations on a backend
//! (`ReaperMainThread`, `Standalone`, …). It's a regular sync trait
//! rather than an architect::rpc service because its methods aren't
//! per-project (current_project/project/projects) or are
//! infrastructure (show_console_msg/last_touched_fx).

use crate::DawResult;

use crate::project::Projects;

pub trait Daw {
    type Project<'a>: Projects + 'a
    where
        Self: 'a;

    /// Handle to the currently focused project tab.
    fn current_project(&self) -> DawResult<Self::Project<'_>>;

    /// Handle to a specific project by GUID.
    fn project(&self, guid: &str) -> DawResult<Self::Project<'_>>;

    /// All open projects.
    fn projects(&self) -> Vec<crate::ProjectInfo>;

    /// Print to the REAPER console / equivalent.
    fn show_console_msg(&self, msg: &str);

    /// Last-touched FX param across the host (None if nothing touched yet).
    fn last_touched_fx(&self) -> Option<crate::LastTouchedFx>;
}
