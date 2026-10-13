//! Regions service trait.
//!
//! Stateless singleton backends — `ProjectContext` flows through every
//! call. `#[architect::rpc]` derives the async vox client + `serve`
//! function; backends impl `Regions` directly. Same pattern as
//! `marker/service.rs` and `track/service.rs`.
//!
//! Scope trimmed from the previous async `RegionService`. Retired
//! surfaces (range queries, lane placement, render matrix, navigation
//! goto, subscribe) land on follow-on sibling traits if/when a real
//! consumer needs them.

use super::Region;
use super::event::RegionStreamEvent;
use crate::batch::ProjectArg;
use crate::{DawResult, ProjectContext, TimeRange};
use facet::Facet;

/// Lane placement request — kept here so retired batch ops can still
/// name it without dragging in the full lane surface.
#[derive(Clone, Debug, Facet)]
pub struct AddRegionInLaneRequest {
    pub start: f64,
    pub end: f64,
    pub name: String,
    pub lane: u32,
}

/// Operations on the regions of a project.
#[architect::rpc(ops(ProjectContext as ProjectArg))]
pub trait Regions {
    /// Every region in the project, ordered by position.
    fn all(&self, project: ProjectContext) -> Vec<Region>;

    /// One region by id, if it still exists.
    fn get(&self, project: ProjectContext, id: u32) -> Option<Region>;

    /// Total count of regions.
    fn count(&self, project: ProjectContext) -> u32;

    /// Insert a region spanning `[start, end]` seconds with the given
    /// name. Returns the DAW-assigned id.
    fn add(&self, project: ProjectContext, start: f64, end: f64, name: &str) -> DawResult<u32>;

    /// [`Regions::add`], but the region carries `guid` (its
    /// [`Region::guid`]) instead of one the backend makes up — how a
    /// peer re-creates a region another engine made. `range` is the
    /// span `add` takes as `start`/`end` (bundled: the RPC surface
    /// allows four parameters). Returns the DAW-assigned id.
    ///
    /// A `guid` a marker or region of the project already has is
    /// [`DawError::AlreadyExists`](crate::DawError::AlreadyExists).
    /// REAPER cannot set a region's GUID through its API, so the REAPER
    /// backend answers `NotSupported`.
    fn add_with_guid(
        &self,
        project: ProjectContext,
        guid: &str,
        range: TimeRange,
        name: &str,
    ) -> DawResult<u32>;

    /// Remove the region with the given id.
    fn remove(&self, project: ProjectContext, id: u32) -> DawResult<()>;

    /// Resize the region to the new `[start, end]` bounds in seconds.
    fn set_bounds(&self, project: ProjectContext, id: u32, start: f64, end: f64) -> DawResult<()>;

    /// Rename the region.
    fn rename(&self, project: ProjectContext, id: u32, name: &str) -> DawResult<()>;

    /// Set the region's color. `0` clears to the DAW default.
    fn set_color(&self, project: ProjectContext, id: u32, color: u32) -> DawResult<()>;

    /// Pin the region to a specific ruler lane (1-based). `None`
    /// clears the assignment back to the default region lane. Mirrors
    /// `Markers::set_lane` for the region half of REAPER's
    /// region+marker list — needed when callers (e.g. the setlist
    /// stamper) place a region in a lane other than the project's
    /// default region lane.
    fn set_lane(&self, project: ProjectContext, id: u32, lane: Option<u32>) -> DawResult<()>;

    /// Region changes across all open projects, as they happen.
    /// Subscribers filter by `project_guid` on the envelope.
    #[subscribe]
    fn events(&self) -> RegionStreamEvent;
}
