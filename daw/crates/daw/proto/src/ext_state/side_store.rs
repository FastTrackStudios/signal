//! Project-scoped side state that `daw` stores but never interprets.
//!
//! A feature that needs to remember something about a project — which
//! takes a user corrected, how they arranged a view — has nowhere good
//! to put it. A typed field on `Track` or `Item` is not an option: on
//! the REAPER side there is nowhere to store a new track field except
//! ext-state, so a typed field would be standalone-only and the REAPER
//! backend would fake it with a side table anyway. And a field per
//! feature does not scale.
//!
//! So `daw` offers a **namespace** and stores bytes under it, never
//! parsing them. Two features cannot collide, because the namespace is
//! part of the key.
//!
//! Written against the [`ExtState`] *trait* rather than a backend, which
//! is what lets the whole loop be exercised with no DAW running — the
//! same choice the rest of this domain makes.
//!
//! ## One blob per namespace, not one per entity
//!
//! State inside a namespace is usually keyed by entity guid, and the
//! obvious shape would be one ext-state key per guid. That is rejected:
//! a partially-written set of keys is indistinguishable from a valid
//! one, and a consumer cannot tell a missing entry from a lost one. One
//! blob is all-or-nothing, which pairs correctly with a consumer whose
//! rule on confusion is "use defaults". The guid map lives *inside* the
//! blob, where the consumer already has to parse it.
//!
//! ## Writing
//!
//! Writes go straight through. REAPER's project ext-state **is** the
//! in-memory project store — verified against live REAPER in #189, where
//! a value set and read back with no save between returns the new value
//! — so the project holds it, marks itself dirty, and writes to disk on
//! save. There is no project-save hook in the tree and none is needed.
//! Marking dirty is the point rather than a side effect: a correction
//! that is a project's only change must still prompt to save.

use super::service::ExtState;
use crate::DawResult;
use crate::project::ProjectContext;

/// The ext-state section every namespace lives under.
///
/// One section keeps namespaces visibly related inside a `.RPP` and
/// leaves the key free to carry the namespace itself.
pub const SECTION: &str = "fts.side";

/// Read a namespace's blob, or `None` when nothing is stored.
pub fn load<D: ExtState + ?Sized>(
    daw: &D,
    project: ProjectContext,
    namespace: &str,
) -> Option<String> {
    daw.get_project(project, SECTION, namespace)
        .filter(|s| !s.trim().is_empty())
}

/// Replace a namespace's blob, marking the project dirty.
pub fn store<D: ExtState + ?Sized>(
    daw: &D,
    project: ProjectContext,
    namespace: &str,
    blob: &str,
) -> DawResult<()> {
    daw.set_project(project, SECTION, namespace, blob)
}

/// Forget everything in a namespace.
pub fn clear<D: ExtState + ?Sized>(
    daw: &D,
    project: ProjectContext,
    namespace: &str,
) -> DawResult<()> {
    // REAPER's convention: an empty value is a delete.
    daw.set_project(project, SECTION, namespace, "")
}
