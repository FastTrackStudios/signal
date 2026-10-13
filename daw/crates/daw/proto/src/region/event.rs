//! Region event types
//!
//! Events are emitted when region state changes.

use super::Region;
use facet::Facet;

/// Events emitted when regions change
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum RegionEvent {
    /// A new region was added
    Added(Region),
    /// A region was removed (contains the ID)
    Removed(u32),
    /// The same region, under a new number.
    ///
    /// REAPER identifies a region only by its number, and its
    /// "Renumber ... in timeline order" action reassigns those
    /// wholesale. Diffed by id, that reads as every region being
    /// deleted and a stranger appearing where it stood — so a client
    /// holding a selection loses it, and one holding a drag moves the
    /// wrong thing.
    ///
    /// The poller pairs a removal with an addition that matches it in
    /// everything but the number, and says this instead. `from` is the
    /// number the client knew; the region carries the one to use now.
    Renumbered { from: u32, region: Region },
    /// A region was modified
    Changed(Region),
    /// Multiple regions changed (e.g., project reload)
    RegionsChanged(Vec<Region>),
}

/// Streaming envelope — pairs a [`RegionEvent`] with the project it
/// applies to. Sibling of [`crate::marker::event::MarkerStreamEvent`].
#[derive(Debug, Clone, Facet)]
pub struct RegionStreamEvent {
    pub project_guid: String,
    pub event: RegionEvent,
}

#[cfg(feature = "vox")]
#[allow(unsafe_code)]
mod reborrow_impls {
    use super::{RegionEvent, RegionStreamEvent};
    unsafe impl vox_types::Reborrow for RegionEvent {
        type Ref<'a> = RegionEvent;
    }
    unsafe impl vox_types::Reborrow for RegionStreamEvent {
        type Ref<'a> = RegionStreamEvent;
    }
}
