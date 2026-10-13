//! Markers handle — per-project facade over the architect-emitted
//! `MarkersClient`.
//!
//! Plumbs the `project_id` into every call site so consumer code looks
//! like `markers.add(10.5, "Verse 1")` instead of repeating
//! `ProjectContext::Project(...)` everywhere. The underlying trait is
//! stateless; this handle is the per-project ergonomic shell.
//!
//! Range-queries, lane methods, transport-style "goto" operations, and
//! event subscriptions were retired alongside the architect::rpc port —
//! the sync `Markers` trait doesn't speak them. Range queries can be
//! reconstructed locally with `markers.all().await?.iter().filter(...)`
//! when needed; lane + goto + subscribe live on follow-on traits when
//! we add them.

use std::sync::Arc;

use crate::DawClients;
use crate::Result;
use daw_proto::{Marker, ProjectContext};

/// Markers handle for a specific project.
///
/// Cheap to clone — only holds a project guid and an `Arc<DawClients>`.
///
/// # Example
///
/// ```no_run
/// use daw_control::Daw;
///
/// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
/// let daw = Daw::new(handle);
/// let project = daw.current_project().await?;
/// let markers = project.markers();
///
/// let all = markers.all().await?;
/// let id = markers.add(10.5, "Verse 1").await?;
/// markers.rename(id, "Chorus").await?;
/// markers.set_color(id, 0xFF0000).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Markers {
    project_id: String,
    clients: Arc<DawClients>,
}

impl Markers {
    pub(crate) fn new(project_id: String, clients: Arc<DawClients>) -> Self {
        Self {
            project_id,
            clients,
        }
    }

    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    /// Every marker in the project, ordered by position.
    pub async fn all(&self) -> Result<Vec<Marker>> {
        Ok(self.clients.marker.all(self.context()).await?)
    }

    /// One marker by id, if it still exists.
    pub async fn get(&self, id: u32) -> Result<Option<Marker>> {
        Ok(self.clients.marker.get(self.context(), id).await?)
    }

    /// Total count of markers.
    pub async fn count(&self) -> Result<u32> {
        Ok(self.clients.marker.count(self.context()).await?)
    }

    /// Insert a marker at `position` seconds with the given name.
    /// Returns the new id.
    pub async fn add(&self, position: f64, name: &str) -> Result<u32> {
        Ok(self
            .clients
            .marker
            .add(self.context(), position, name.to_string())
            .await??)
    }

    /// [`Markers::add`], but the marker carries `guid` instead of one the
    /// backend makes up — how a peer re-creates a marker another engine
    /// made. A `guid` a marker or region of the project already has is an
    /// error (`DawError::AlreadyExists`); REAPER answers `NotSupported`.
    pub async fn add_with_guid(&self, guid: &str, position: f64, name: &str) -> Result<u32> {
        Ok(self
            .clients
            .marker
            .add_with_guid(self.context(), guid.to_string(), position, name.to_string())
            .await??)
    }

    /// Remove a marker by id.
    pub async fn remove(&self, id: u32) -> Result<()> {
        self.clients.marker.remove(self.context(), id).await??;
        Ok(())
    }

    /// Move a marker to a new position in seconds.
    pub async fn move_to(&self, id: u32, position: f64) -> Result<()> {
        self.clients
            .marker
            .set_position(self.context(), id, position)
            .await??;
        Ok(())
    }

    /// Rename a marker.
    pub async fn rename(&self, id: u32, name: &str) -> Result<()> {
        self.clients
            .marker
            .rename(self.context(), id, name.to_string())
            .await??;
        Ok(())
    }

    /// Set the marker's color. `0` clears to the DAW default.
    pub async fn set_color(&self, id: u32, color: u32) -> Result<()> {
        self.clients
            .marker
            .set_color(self.context(), id, color)
            .await??;
        Ok(())
    }

    /// Set the marker lane. `None` returns it to the DAW's default lane.
    pub async fn set_lane(&self, id: u32, lane: Option<u32>) -> Result<()> {
        self.clients
            .marker
            .set_lane(self.context(), id, lane)
            .await??;
        Ok(())
    }

    /// Subscribe to marker add/remove/modify events for this project.
    /// The server streams every open project's events; filtering to
    /// this handle's `project_guid` happens client-side. Drop the
    /// returned stream to unsubscribe.
    pub async fn subscribe(
        &self,
    ) -> Result<crate::EventStream<daw_proto::marker::MarkerStreamEvent>> {
        let (raw_tx, raw_rx) = vox::channel();
        let stream = self.clients.marker_stream.clone();
        let want = self.project_id.clone();
        Ok(crate::EventStream::spawn(
            async move {
                let _ = stream.events(raw_tx).await;
            },
            raw_rx,
            Box::new(move |ev| ev.project_guid == want),
        ))
    }
}

impl std::fmt::Debug for Markers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Markers")
            .field("project_id", &self.project_id)
            .finish()
    }
}
