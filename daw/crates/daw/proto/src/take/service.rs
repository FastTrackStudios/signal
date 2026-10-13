//! Takes service trait — architect::rpc port of the retired async
//! `TakeService`. Stateless singleton backends — `ProjectContext` and
//! `ItemRef`/`TakeRef` flow through every call.

use crate::batch::ProjectArg;
use crate::item::{
    AddTakeMarkerAtPositionRequest, ItemRef, SourceType, Take, TakeMarker, TakeMarkerCreate,
    TakeMarkerUpdate, TakeRef,
};
use crate::primitives::Duration;
use crate::{DawResult, ProjectContext};

#[architect::rpc(ops(ProjectContext as ProjectArg))]
pub trait Takes {
    fn get_takes(&self, project: ProjectContext, item: ItemRef) -> Vec<Take>;

    fn get_take(&self, project: ProjectContext, item: ItemRef, take: TakeRef) -> Option<Take>;

    fn get_active_take(&self, project: ProjectContext, item: ItemRef) -> Option<Take>;

    fn take_count(&self, project: ProjectContext, item: ItemRef) -> u32;

    fn add_take(&self, project: ProjectContext, item: ItemRef) -> Option<String>;

    fn delete_take(&self, project: ProjectContext, item: ItemRef, take: TakeRef) -> DawResult<()>;

    fn set_active_take(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> DawResult<()>;

    fn set_name(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        name: String,
    ) -> DawResult<()>;

    fn set_color(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        color: Option<u32>,
    ) -> DawResult<()>;

    fn set_volume(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        volume: f64,
    ) -> DawResult<()>;

    fn set_play_rate(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        rate: f64,
    ) -> DawResult<()>;

    fn set_pitch(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        semitones: f64,
    ) -> DawResult<()>;

    fn set_preserve_pitch(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        preserve: bool,
    ) -> DawResult<()>;

    fn set_start_offset(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        offset: Duration,
    ) -> DawResult<()>;

    fn set_source_file(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        path: String,
    ) -> DawResult<()>;

    fn get_source_type(&self, project: ProjectContext, item: ItemRef, take: TakeRef) -> SourceType;

    fn get_take_markers(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> Vec<TakeMarker>;

    fn add_take_marker(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        marker: TakeMarkerCreate,
    ) -> Option<u32>;

    fn set_take_marker(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        update: TakeMarkerUpdate,
    ) -> DawResult<()>;

    fn delete_take_marker(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        index: u32,
    ) -> DawResult<()>;

    /// Replace every marker on the take's source with `markers`, as one
    /// change — a detection run writes thousands. Each keeps its `id` (an
    /// empty one is given one); `index` is ignored and renumbered.
    fn set_take_markers(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        markers: Vec<TakeMarker>,
    ) -> DawResult<()>;

    fn add_take_marker_at_position(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        request: AddTakeMarkerAtPositionRequest,
    ) -> Option<u32>;

    fn run_take_rating_action(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        command_id: u32,
    ) -> DawResult<()>;
}
