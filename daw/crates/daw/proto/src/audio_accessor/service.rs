//! Audio accessor service — handle-based random-access sample reading.

use super::{AudioSampleData, GetSamplesRequest};
use crate::item::{ItemRef, TakeRef};
use crate::project::ProjectContext;
use crate::track::TrackRef;

#[architect::rpc]
pub trait AudioAccessors {
    /// Create an audio accessor for a track. Returns opaque handle ID.
    fn create_track_accessor(&self, project: ProjectContext, track: TrackRef) -> Option<String>;

    /// Create an audio accessor for a take.
    fn create_take_accessor(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> Option<String>;

    /// True if the accessor's underlying audio has changed since
    /// creation.
    fn has_state_changed(&self, accessor_id: &str) -> bool;

    /// Read interleaved sample data.
    fn get_samples(&self, request: GetSamplesRequest) -> AudioSampleData;

    /// Destroy an accessor and free its resources.
    fn destroy_accessor(&self, accessor_id: &str);
}
