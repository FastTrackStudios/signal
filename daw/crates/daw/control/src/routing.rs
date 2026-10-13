//! Routing handles for sends, receives, and hardware outputs

use std::sync::Arc;

use crate::Result;
use crate::{DawClients, Error};
use daw_proto::{
    ProjectContext,
    routing::{RouteLocation, RouteRef, RouteType, SendMode, TrackRoute},
    track::TrackRef,
};

/// Sends accessor for a track
#[derive(Clone)]
pub struct Sends {
    track_guid: String,
    project_id: String,
    clients: Arc<DawClients>,
}

impl Sends {
    /// Create a new sends handle
    pub(crate) fn new(track_guid: String, project_id: String, clients: Arc<DawClients>) -> Self {
        Self {
            track_guid,
            project_id,
            clients,
        }
    }

    /// Helper to create project context
    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    /// Helper to create track reference
    fn track_ref(&self) -> TrackRef {
        TrackRef::Guid(self.track_guid.clone())
    }

    /// Get all sends
    pub async fn all(&self) -> Result<Vec<TrackRoute>> {
        let sends = self
            .clients
            .routing
            .sends(self.context(), self.track_ref())
            .await?;
        Ok(sends)
    }

    /// Get send by index
    pub async fn by_index(&self, index: u32) -> Result<Option<RouteHandle>> {
        let location = RouteLocation::send(self.track_ref(), RouteRef::Index(index));
        let route = self
            .clients
            .routing
            .get_route(self.context(), location)
            .await?;

        Ok(route.map(|_| {
            RouteHandle::new(
                self.track_guid.clone(),
                RouteType::Send,
                RouteRef::Index(index),
                self.project_id.clone(),
                self.clients.clone(),
            )
        }))
    }

    /// Get the send pointing at the given destination track GUID, if
    /// any. Resolves via the service's `ByDestination` lookup.
    pub async fn by_destination(&self, dest_track_guid: &str) -> Result<Option<RouteHandle>> {
        let dest = TrackRef::Guid(dest_track_guid.to_string());
        let location = RouteLocation::send(self.track_ref(), RouteRef::ByDestination(dest.clone()));
        let route = self
            .clients
            .routing
            .get_route(self.context(), location)
            .await?;
        Ok(route.map(|_| {
            RouteHandle::new(
                self.track_guid.clone(),
                RouteType::Send,
                RouteRef::ByDestination(dest),
                self.project_id.clone(),
                self.clients.clone(),
            )
        }))
    }

    // =========================================================================
    // Streaming
    // =========================================================================

    // `subscribe` retired with the architect::rpc port — routing
    // event streaming lives on a sibling trait.

    /// Add a send to another track
    pub async fn add_to(&self, dest_track_guid: &str) -> Result<RouteHandle> {
        let index = self
            .clients
            .routing
            .add_send(
                self.context(),
                self.track_ref(),
                TrackRef::Guid(dest_track_guid.to_string()),
            )
            .await?
            .ok_or_else(|| Error::Other("Failed to create send".to_string()))?;

        Ok(RouteHandle::new(
            self.track_guid.clone(),
            RouteType::Send,
            RouteRef::Index(index),
            self.project_id.clone(),
            self.clients.clone(),
        ))
    }
}

impl std::fmt::Debug for Sends {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sends")
            .field("track_guid", &self.track_guid)
            .field("project_id", &self.project_id)
            .finish()
    }
}

/// Receives accessor for a track
#[derive(Clone)]
pub struct Receives {
    track_guid: String,
    project_id: String,
    clients: Arc<DawClients>,
}

impl Receives {
    /// Create a new receives handle
    pub(crate) fn new(track_guid: String, project_id: String, clients: Arc<DawClients>) -> Self {
        Self {
            track_guid,
            project_id,
            clients,
        }
    }

    /// Helper to create project context
    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    /// Helper to create track reference
    fn track_ref(&self) -> TrackRef {
        TrackRef::Guid(self.track_guid.clone())
    }

    /// Get all receives
    pub async fn all(&self) -> Result<Vec<TrackRoute>> {
        let receives = self
            .clients
            .routing
            .receives(self.context(), self.track_ref())
            .await?;
        Ok(receives)
    }

    /// Get receive by index
    pub async fn by_index(&self, index: u32) -> Result<Option<RouteHandle>> {
        let location = RouteLocation::receive(self.track_ref(), RouteRef::Index(index));
        let route = self
            .clients
            .routing
            .get_route(self.context(), location)
            .await?;

        Ok(route.map(|_| {
            RouteHandle::new(
                self.track_guid.clone(),
                RouteType::Receive,
                RouteRef::Index(index),
                self.project_id.clone(),
                self.clients.clone(),
            )
        }))
    }
}

impl std::fmt::Debug for Receives {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Receives")
            .field("track_guid", &self.track_guid)
            .field("project_id", &self.project_id)
            .finish()
    }
}

/// Hardware outputs accessor for a track
#[derive(Clone)]
pub struct HardwareOutputs {
    track_guid: String,
    project_id: String,
    clients: Arc<DawClients>,
}

impl HardwareOutputs {
    /// Create a new hardware outputs handle
    pub(crate) fn new(track_guid: String, project_id: String, clients: Arc<DawClients>) -> Self {
        Self {
            track_guid,
            project_id,
            clients,
        }
    }

    /// Helper to create project context
    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    /// Helper to create track reference
    fn track_ref(&self) -> TrackRef {
        TrackRef::Guid(self.track_guid.clone())
    }

    /// Get all hardware outputs
    pub async fn all(&self) -> Result<Vec<TrackRoute>> {
        let outputs = self
            .clients
            .routing
            .hardware_outputs(self.context(), self.track_ref())
            .await?;
        Ok(outputs)
    }

    /// Get hardware output by index
    pub async fn by_index(&self, index: u32) -> Result<Option<RouteHandle>> {
        let location = RouteLocation::hardware_output(self.track_ref(), RouteRef::Index(index));
        let route = self
            .clients
            .routing
            .get_route(self.context(), location)
            .await?;

        Ok(route.map(|_| {
            RouteHandle::new(
                self.track_guid.clone(),
                RouteType::HardwareOutput,
                RouteRef::Index(index),
                self.project_id.clone(),
                self.clients.clone(),
            )
        }))
    }

    /// Add a hardware output
    pub async fn add(&self, hw_output_index: u32) -> Result<RouteHandle> {
        let index = self
            .clients
            .routing
            .add_hardware_output(self.context(), self.track_ref(), hw_output_index)
            .await?
            .ok_or_else(|| Error::Other("Failed to create hardware output".to_string()))?;

        Ok(RouteHandle::new(
            self.track_guid.clone(),
            RouteType::HardwareOutput,
            RouteRef::Index(index),
            self.project_id.clone(),
            self.clients.clone(),
        ))
    }
}

impl std::fmt::Debug for HardwareOutputs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HardwareOutputs")
            .field("track_guid", &self.track_guid)
            .field("project_id", &self.project_id)
            .finish()
    }
}

// =============================================================================
// RouteHandle
// =============================================================================

/// Handle to a single route (send, receive, or hardware output)
#[derive(Clone)]
pub struct RouteHandle {
    track_guid: String,
    route_type: RouteType,
    route_ref: RouteRef,
    project_id: String,
    clients: Arc<DawClients>,
}

impl RouteHandle {
    /// Create a new route handle
    pub(crate) fn new(
        track_guid: String,
        route_type: RouteType,
        route_ref: RouteRef,
        project_id: String,
        clients: Arc<DawClients>,
    ) -> Self {
        Self {
            track_guid,
            route_type,
            route_ref,
            project_id,
            clients,
        }
    }

    /// Helper to create project context
    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    /// Helper to create route location
    fn location(&self) -> RouteLocation {
        RouteLocation::new(
            TrackRef::Guid(self.track_guid.clone()),
            self.route_type,
            self.route_ref.clone(),
        )
    }

    // =========================================================================
    // Info
    // =========================================================================

    /// Get full route state
    pub async fn info(&self) -> Result<TrackRoute> {
        self.clients
            .routing
            .get_route(self.context(), self.location())
            .await?
            .ok_or_else(|| Error::Other("Route not found".to_string()))
    }

    // =========================================================================
    // Levels
    // =========================================================================

    /// Get route volume
    pub async fn volume(&self) -> Result<f64> {
        Ok(self.info().await?.volume)
    }

    /// Set route volume
    pub async fn set_volume(&self, volume: f64) -> Result<()> {
        self.clients
            .routing
            .set_volume(self.context(), self.location(), volume)
            .await??;
        Ok(())
    }

    /// Get route pan
    pub async fn pan(&self) -> Result<f64> {
        Ok(self.info().await?.pan)
    }

    /// Set route pan
    pub async fn set_pan(&self, pan: f64) -> Result<()> {
        self.clients
            .routing
            .set_pan(self.context(), self.location(), pan)
            .await??;
        Ok(())
    }

    // =========================================================================
    // State
    // =========================================================================

    /// Mute the route
    pub async fn mute(&self) -> Result<()> {
        self.clients
            .routing
            .set_muted(self.context(), self.location(), true)
            .await??;
        Ok(())
    }

    /// Unmute the route
    pub async fn unmute(&self) -> Result<()> {
        self.clients
            .routing
            .set_muted(self.context(), self.location(), false)
            .await??;
        Ok(())
    }

    /// Check if muted
    pub async fn is_muted(&self) -> Result<bool> {
        Ok(self.info().await?.muted)
    }

    /// Set mono summing
    pub async fn set_mono(&self, mono: bool) -> Result<()> {
        self.clients
            .routing
            .set_mono(self.context(), self.location(), mono)
            .await??;
        Ok(())
    }

    /// Set phase inversion
    pub async fn set_phase_inverted(&self, inverted: bool) -> Result<()> {
        self.clients
            .routing
            .set_phase(self.context(), self.location(), inverted)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Mode (for sends)
    // =========================================================================

    /// Set send mode (only for sends)
    pub async fn set_send_mode(&self, mode: SendMode) -> Result<()> {
        if self.route_type != RouteType::Send {
            return Err(Error::Other("Send mode only applies to sends".to_string()));
        }
        self.clients
            .routing
            .set_send_mode(
                self.context(),
                TrackRef::Guid(self.track_guid.clone()),
                self.route_ref.clone(),
                mode,
            )
            .await??;
        Ok(())
    }

    // =========================================================================
    // Channel Mapping
    // =========================================================================

    /// Set the source audio channels for this route.
    ///
    /// `start_channel` is 0-indexed (0 = ch 1/2 if stereo, 2 = ch 3/4, etc.).
    /// `num_channels` is typically 2 for stereo.
    pub async fn set_source_channels(&self, start_channel: u32, num_channels: u32) -> Result<()> {
        self.clients
            .routing
            .set_source_channels(self.context(), self.location(), start_channel, num_channels)
            .await??;
        Ok(())
    }

    /// Set the destination audio channels for this route.
    ///
    /// `start_channel` is 0-indexed. `num_channels` is typically 2 for stereo.
    pub async fn set_dest_channels(&self, start_channel: u32, num_channels: u32) -> Result<()> {
        self.clients
            .routing
            .set_dest_channels(self.context(), self.location(), start_channel, num_channels)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Operations
    // =========================================================================

    /// Remove this route
    pub async fn remove(&self) -> Result<()> {
        self.clients
            .routing
            .remove_route(self.context(), self.location())
            .await??;
        Ok(())
    }
}

impl std::fmt::Debug for RouteHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouteHandle")
            .field("track_guid", &self.track_guid)
            .field("route_type", &self.route_type)
            .field("route_ref", &self.route_ref)
            .field("project_id", &self.project_id)
            .finish()
    }
}
