//! Track module — canonical home for everything tracks-related.
//!
//! - [`Track`], [`TrackRef`], [`InputMonitoringMode`], [`RecordInput`] : data
//! - [`Tracks`] : the service trait, decorated with `#[architect::rpc]`
//! - [`TrackEvent`] / [`TrackError`] : event + error types
//! - [`TrackHierarchy`] + builder : structural helpers
//!
//! Backends impl `Tracks` directly; in-process callers use it as a
//! plain sync API; remote callers reach the same surface via the
//! auto-emitted [`TracksClient`] over vox.

mod error;
mod event;
mod ext;
mod hierarchy;
mod hierarchy_builder;
mod lanes;
mod service;
mod test_utils;
#[allow(clippy::module_inception)]
mod track;

pub use error::TrackError;
pub use event::{TrackEvent, TrackStreamEvent};
pub use ext::{TrackShape, TrackTree, TracksExt};
pub use hierarchy::{FolderDepthChange, TrackHierarchy, TrackNode};
pub use hierarchy_builder::{AddChildren, TrackHierarchyBuilder};
pub use lanes::{Comp, CompArea, LaneComping};
pub use service::*;

// vox-emitted names from the architect macro mirror. Aliased to short
// names so mounting reads `track::descriptor()` + `track::serve(Reaper)`.

pub use test_utils::{
    TrackGroup, TrackStructureBuilder, assert_tracks_equal, display_tracklist, format_tracklist,
};
pub use track::{
    GROUP_SLOTS, GroupFamily, GroupFlagChange, GroupModifier, GroupModifierChange, GroupRole,
    InputMonitoringMode, LaneDisplay, RecordInput, ReorderTracksBehavior, Track, TrackGrouping,
    TrackRef, check_group_slot, group_slot_bit,
};
