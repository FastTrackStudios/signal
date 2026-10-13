//! Regions handle and operations

use std::sync::Arc;

use crate::DawClients;
use crate::Result;
use daw_proto::{ProjectContext, Region};

/// Regions handle for a specific project
///
/// This handle provides access to region operations (query, add, remove, navigate)
/// for a specific project. Like reaper-rs, it's lightweight and cheap to clone.
///
/// # Example
///
/// ```no_run
/// use daw_control::Daw;
///
/// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
/// let daw = Daw::new(handle);
/// let project = daw.current_project().await?;
/// let regions = project.regions();
///
/// // Query regions
/// let all = regions.all().await?;
/// let count = regions.count().await?;
///
/// // Add and manipulate regions
/// let id = regions.add(0.0, 30.0, "Intro").await?;
/// regions.rename(id, "Extended Intro").await?;
/// regions.set_bounds(id, 0.0, 45.0).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Regions {
    project_id: String,
    clients: Arc<DawClients>,
}

impl Regions {
    /// Create a new regions handle for a project
    pub(crate) fn new(project_id: String, clients: Arc<DawClients>) -> Self {
        Self {
            project_id,
            clients,
        }
    }

    /// Helper to create project context
    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    // =========================================================================
    // Query Methods
    // =========================================================================

    /// Get all regions in the project
    pub async fn all(&self) -> Result<Vec<Region>> {
        let regions = self.clients.region.all(self.context()).await?;
        Ok(regions)
    }

    /// Get a specific region by ID
    pub async fn get(&self, id: u32) -> Result<Option<Region>> {
        let region = self.clients.region.get(self.context(), id).await?;
        Ok(region)
    }

    /// Get the total number of regions
    pub async fn count(&self) -> Result<u32> {
        let count = self.clients.region.count(self.context()).await?;
        Ok(count)
    }

    // =========================================================================
    // Mutation Methods
    // =========================================================================

    /// Add a new region with the given bounds
    ///
    /// Returns the ID of the newly created region.
    pub async fn add(&self, start: f64, end: f64, name: &str) -> Result<u32> {
        // architect-emitted client wraps `DawResult<T>` in vox's
        // `Result<_, vox::Error>` — flatten both with `.await??`.
        let id = self
            .clients
            .region
            .add(self.context(), start, end, name.to_string())
            .await??;
        Ok(id)
    }

    /// [`Regions::add`], but the region carries `guid` instead of one the
    /// backend makes up — how a peer re-creates a region another engine
    /// made. A `guid` a marker or region of the project already has is an
    /// error (`DawError::AlreadyExists`); REAPER answers `NotSupported`.
    pub async fn add_with_guid(&self, guid: &str, start: f64, end: f64, name: &str) -> Result<u32> {
        let id = self
            .clients
            .region
            .add_with_guid(
                self.context(),
                guid.to_string(),
                daw_proto::TimeRange::from_seconds(start, end),
                name.to_string(),
            )
            .await??;
        Ok(id)
    }

    /// Remove a region by ID
    pub async fn remove(&self, id: u32) -> Result<()> {
        self.clients.region.remove(self.context(), id).await??;
        Ok(())
    }

    /// Set region bounds (start and end position)
    pub async fn set_bounds(&self, id: u32, start: f64, end: f64) -> Result<()> {
        self.clients
            .region
            .set_bounds(self.context(), id, start, end)
            .await??;
        Ok(())
    }

    /// Rename a region
    pub async fn rename(&self, id: u32, name: &str) -> Result<()> {
        self.clients
            .region
            .rename(self.context(), id, name.to_string())
            .await??;
        Ok(())
    }

    /// Set the color of a region (0 for default color)
    pub async fn set_color(&self, id: u32, color: u32) -> Result<()> {
        self.clients
            .region
            .set_color(self.context(), id, color)
            .await??;
        Ok(())
    }

    /// Set the region lane. `None` returns it to the DAW's default lane.
    pub async fn set_lane(&self, id: u32, lane: Option<u32>) -> Result<()> {
        self.clients
            .region
            .set_lane(self.context(), id, lane)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Navigation Methods
    // =========================================================================

    // =========================================================================
    // Subscriptions
    // =========================================================================

    /// Subscribe to region add/remove/modify events for this project.
    /// The server streams every open project's events; filtering to
    /// this handle's `project_guid` happens client-side. Drop the
    /// returned stream to unsubscribe.
    pub async fn subscribe(
        &self,
    ) -> Result<crate::EventStream<daw_proto::region::RegionStreamEvent>> {
        let (raw_tx, raw_rx) = vox::channel();
        let stream = self.clients.region_stream.clone();
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

impl std::fmt::Debug for Regions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Regions")
            .field("project_id", &self.project_id)
            .finish()
    }
}
