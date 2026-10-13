//! Routing events for subscriptions

use super::{RouteType, TrackRoute};
use facet::Facet;

/// Events related to routing changes
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum RoutingEvent {
    /// A route was created
    RouteCreated {
        project_guid: String,
        source_track_guid: String,
        route: TrackRoute,
    },
    /// A route was deleted
    RouteDeleted {
        project_guid: String,
        source_track_guid: String,
        route_type: RouteType,
        route_index: u32,
    },
    /// A route's volume changed
    VolumeChanged {
        project_guid: String,
        source_track_guid: String,
        route_type: RouteType,
        route_index: u32,
        volume: f64,
    },
    /// A route's pan changed
    PanChanged {
        project_guid: String,
        source_track_guid: String,
        route_type: RouteType,
        route_index: u32,
        pan: f64,
    },
    /// A route's mute state changed
    MuteChanged {
        project_guid: String,
        source_track_guid: String,
        route_type: RouteType,
        route_index: u32,
        muted: bool,
    },
    /// Parent send state changed
    ParentSendChanged {
        project_guid: String,
        track_guid: String,
        enabled: bool,
    },
}
