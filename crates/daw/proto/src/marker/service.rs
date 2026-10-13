//! Markers service trait.
//!
//! One canonical declaration, decorated with `#[architect::rpc]`. The
//! macro derives the async vox client + a `serve` function from this
//! sync trait — backends impl `Markers` directly (zero-cost in-process
//! call sites), and remote callers reach the same surface via the
//! auto-emitted `MarkersClient` over vox. See `architect/DESIGN.md`.
//!
//! Backends are **stateless singletons** — they don't carry per-project
//! state. Project context flows through method calls. That keeps one
//! backend type per runtime (`Reaper`, `Standalone`, `RppFile`, …)
//! and lets a single mount serve every project a binary touches.

use crate::batch::ProjectArg;
use crate::marker::event::MarkerStreamEvent;
use crate::{DawResult, Marker, ProjectContext};

/// Operations on the markers of a project. `ProjectContext` flows
/// through each call so backends can serve any project.
#[architect::rpc(ops(ProjectContext as ProjectArg))]
pub trait Markers {
    /// Every marker in the project, ordered by position.
    fn all(&self, project: ProjectContext) -> Vec<Marker>;

    /// One marker by its DAW-assigned id, if it still exists.
    fn get(&self, project: ProjectContext, id: u32) -> Option<Marker>;

    /// Total count of markers (cheaper than `all().len()` for
    /// backends that can answer without materializing the full list).
    fn count(&self, project: ProjectContext) -> u32;

    /// Insert a marker at `position` seconds with the given name.
    /// Returns the DAW-assigned id.
    fn add(&self, project: ProjectContext, position: f64, name: &str) -> DawResult<u32>;

    /// [`Markers::add`], but the marker carries `guid` (its
    /// [`Marker::guid`]) instead of one the backend makes up — how a
    /// peer re-creates a marker another engine made. Returns the
    /// DAW-assigned id, as `add` does.
    ///
    /// A `guid` a marker or region of the project already has is
    /// [`DawError::AlreadyExists`](crate::DawError::AlreadyExists).
    /// REAPER cannot set a marker's GUID through its API, so the REAPER
    /// backend answers `NotSupported`.
    fn add_with_guid(
        &self,
        project: ProjectContext,
        guid: &str,
        position: f64,
        name: &str,
    ) -> DawResult<u32>;

    /// Remove the marker with the given id.
    fn remove(&self, project: ProjectContext, id: u32) -> DawResult<()>;

    /// Move the marker to a new position in seconds.
    fn set_position(&self, project: ProjectContext, id: u32, position: f64) -> DawResult<()>;

    /// Rename the marker.
    fn rename(&self, project: ProjectContext, id: u32, name: &str) -> DawResult<()>;

    /// Set the marker's color. `0` clears to the DAW default.
    fn set_color(&self, project: ProjectContext, id: u32, color: u32) -> DawResult<()>;

    /// Move the marker to a ruler lane. `None` returns it to the DAW default.
    fn set_lane(&self, project: ProjectContext, id: u32, lane: Option<u32>) -> DawResult<()>;

    /// Marker changes across all open projects, as they happen.
    /// Subscribers filter by `project_guid` on the envelope.
    #[subscribe]
    fn events(&self) -> MarkerStreamEvent;
}
