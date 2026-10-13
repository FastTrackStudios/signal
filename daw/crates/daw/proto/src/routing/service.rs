//! Routing service — sends, receives, hardware outputs.

use super::{RouteRef, RouteType, SendMode, TrackRoute};
use crate::DawResult;
use crate::batch::{ProjectArg, TrackArg};
use crate::project::ProjectContext;
use crate::track::TrackRef;

/// Specifies a route location (track + route type + route reference).
#[derive(Clone, Debug, facet::Facet)]
pub struct RouteLocation {
    pub track: TrackRef,
    pub route_type: RouteType,
    pub route: RouteRef,
}

impl RouteLocation {
    pub fn new(track: TrackRef, route_type: RouteType, route: RouteRef) -> Self {
        Self {
            track,
            route_type,
            route,
        }
    }

    pub fn send(track: TrackRef, route: RouteRef) -> Self {
        Self::new(track, RouteType::Send, route)
    }

    pub fn receive(track: TrackRef, route: RouteRef) -> Self {
        Self::new(track, RouteType::Receive, route)
    }

    pub fn hardware_output(track: TrackRef, route: RouteRef) -> Self {
        Self::new(track, RouteType::HardwareOutput, route)
    }
}

#[architect::rpc(ops(ProjectContext as ProjectArg, TrackRef as TrackArg), scopes(project: ProjectContext, track: TrackRef))]
pub trait Routing {
    // ── Queries ────────────────────────────────────────────────────

    fn sends(&self, project: ProjectContext, track: TrackRef) -> Vec<TrackRoute>;
    fn receives(&self, project: ProjectContext, track: TrackRef) -> Vec<TrackRoute>;
    fn hardware_outputs(&self, project: ProjectContext, track: TrackRef) -> Vec<TrackRoute>;

    fn get_route(&self, project: ProjectContext, location: RouteLocation) -> Option<TrackRoute>;

    fn send_count(&self, project: ProjectContext, track: TrackRef) -> u32;
    fn receive_count(&self, project: ProjectContext, track: TrackRef) -> u32;

    // ── CRUD ───────────────────────────────────────────────────────

    /// Add a send from source track to destination. Returns new index.
    fn add_send(&self, project: ProjectContext, source: TrackRef, dest: TrackRef) -> Option<u32>;

    /// Add a hardware output. Returns its index.
    fn add_hardware_output(
        &self,
        project: ProjectContext,
        track: TrackRef,
        hw_output: u32,
    ) -> Option<u32>;

    fn remove_route(&self, project: ProjectContext, location: RouteLocation) -> DawResult<()>;

    // ── Levels ─────────────────────────────────────────────────────

    fn set_volume(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        volume: f64,
    ) -> DawResult<()>;

    fn set_pan(&self, project: ProjectContext, location: RouteLocation, pan: f64) -> DawResult<()>;

    // ── State ──────────────────────────────────────────────────────

    fn set_muted(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        muted: bool,
    ) -> DawResult<()>;

    fn set_mono(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        mono: bool,
    ) -> DawResult<()>;

    fn set_phase(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        inverted: bool,
    ) -> DawResult<()>;

    fn is_muted(&self, project: ProjectContext, location: RouteLocation) -> bool;

    // ── Send mode ──────────────────────────────────────────────────

    fn set_send_mode(
        &self,
        project: ProjectContext,
        track: TrackRef,
        route: RouteRef,
        mode: SendMode,
    ) -> DawResult<()>;

    // ── Channel mapping ────────────────────────────────────────────

    fn set_source_channels(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        start_channel: u32,
        num_channels: u32,
    ) -> DawResult<()>;

    fn set_dest_channels(
        &self,
        project: ProjectContext,
        location: RouteLocation,
        start_channel: u32,
        num_channels: u32,
    ) -> DawResult<()>;

    // ── Parent send (folder routing) ───────────────────────────────

    fn parent_send_enabled(&self, project: ProjectContext, track: TrackRef) -> bool;

    fn set_parent_send_enabled(
        &self,
        project: ProjectContext,
        track: TrackRef,
        enabled: bool,
    ) -> DawResult<()>;
}
