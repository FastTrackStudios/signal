//! Stretch marker service — non-destructive take timing.

use super::{StretchMarker, StretchMode};
use crate::error::DawResult;
use crate::item::{ItemRef, TakeRef};
use crate::project::ProjectContext;

/// Read and write a take's stretch markers.
///
/// Separate from [`crate::take::Takes`] rather than folded into it,
/// because every implementor of a trait must implement all of it and
/// timing is not something a minimal backend needs to answer. Same
/// reason [`crate::audio_accessor::AudioAccessors`] stands alone.
#[architect::rpc]
pub trait StretchMarkers {
    /// Markers on a take, in position order.
    fn get_stretch_markers(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> Vec<StretchMarker>;

    /// Add one, returning its index.
    ///
    /// The host keeps markers sorted by position, so the index a marker
    /// lands at is not necessarily the last — which is why this returns
    /// it rather than leaving the caller to assume.
    fn add_stretch_marker(&self, location: StretchTakeRef, marker: StretchMarker)
    -> DawResult<u32>;

    /// Replace the marker at `index`.
    fn set_stretch_marker(
        &self,
        location: StretchTakeRef,
        index: u32,
        marker: StretchMarker,
    ) -> DawResult<()>;

    fn delete_stretch_marker(&self, location: StretchTakeRef, index: u32) -> DawResult<()>;

    /// Remove every marker, returning the take to its recorded timing.
    fn clear_stretch_markers(&self, location: StretchTakeRef) -> DawResult<()>;

    /// Replace the whole set in one call.
    ///
    /// The operation an alignment or a timing edit actually wants:
    /// writing a map one marker at a time leaves the take in a
    /// half-warped state that the host may play, and on a long phrase
    /// that is audible.
    fn set_stretch_markers(
        &self,
        location: StretchTakeRef,
        markers: Vec<StretchMarker>,
    ) -> DawResult<()>;

    /// Which algorithm the host stretches with.
    fn set_stretch_mode(&self, location: StretchTakeRef, mode: StretchMode) -> DawResult<()>;
}

/// A take, for the stretch-marker calls.
///
/// Bundled because the RPC layer caps a method at four parameters and
/// `project + item + take` plus an argument already reaches it.
#[derive(Clone, Debug, facet::Facet)]
pub struct StretchTakeRef {
    pub project: ProjectContext,
    pub item: ItemRef,
    pub take: TakeRef,
}

impl StretchTakeRef {
    pub fn new(project: ProjectContext, item: ItemRef, take: TakeRef) -> Self {
        Self {
            project,
            item,
            take,
        }
    }
}
