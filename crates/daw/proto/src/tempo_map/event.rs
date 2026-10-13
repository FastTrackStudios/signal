//! Tempo map event types
//!
//! Events are emitted when tempo map state changes.

use super::TempoPoint;
use facet::Facet;

/// Events emitted when the tempo map changes
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum TempoMapEvent {
    /// A new tempo point was added
    PointAdded(TempoPoint),
    /// A tempo point was removed (contains the index)
    PointRemoved(u32),
    /// A tempo point was modified
    PointChanged(TempoPoint),
    /// The entire tempo map changed (e.g., project reload)
    MapChanged(Vec<TempoPoint>),
}

/// Streaming envelope — pairs a [`TempoMapEvent`] with the project
/// it applies to.
#[derive(Debug, Clone, Facet)]
pub struct TempoMapStreamEvent {
    pub project_guid: String,
    pub event: TempoMapEvent,
}

#[cfg(feature = "vox")]
#[allow(unsafe_code)]
mod reborrow_impls {
    use super::{TempoMapEvent, TempoMapStreamEvent};
    unsafe impl vox_types::Reborrow for TempoMapEvent {
        type Ref<'a> = TempoMapEvent;
    }
    unsafe impl vox_types::Reborrow for TempoMapStreamEvent {
        type Ref<'a> = TempoMapStreamEvent;
    }
}
