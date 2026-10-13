//! Top-level DAW API entry point

use crate::Result;
use crate::{DawConnection, Project};

/// Main DAW API entry point (like `Reaper`)
///
/// This is the top-level entry point for the DAW client API.
/// All operations start here, similar to how `Reaper::get()` works in reaper-rs.
///
/// # Example
///
/// ```no_run
/// use daw_proto::client::Daw;
///
/// #[tokio::main]
/// async fn main() -> daw_control::Result<()> {
///     // Initialize the global connection
///     let handle = vox::connect("unix:///tmp/fts-daw.sock").await?;
///     Daw::init(handle)?;
///
///     // Get current project
///     let project = Daw::current_project().await?;
///     project.transport().play().await?;
///
///     Ok(())
/// }
/// ```
pub struct Daw;

impl Daw {
    /// Initialize the global DAW connection
    ///
    /// This must be called once at startup before using any DAW API methods.
    ///
    /// # Errors
    ///
    /// Returns an error if the DAW connection has already been initialized.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use daw_proto::client::Daw;
    /// # async {
    /// let handle = vox::connect("unix:///tmp/fts-daw.sock").await?;
    /// Daw::init(handle)?;
    /// # Ok::<(), eyre::Error>(())
    /// # };
    /// ```
    pub fn init(handle: vox::session::ConnectionHandle) -> Result<()> {
        DawConnection::init_globally(handle)
            .map_err(|_| Error::Other("DAW already initialized".to_string()))
    }

    /// Get the current/active project
    ///
    /// Returns the project that is currently focused/active in the DAW.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The DAW connection is not initialized
    /// - No current project is available
    /// - RPC communication fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use daw_proto::client::Daw;
    /// # async {
    /// let project = Daw::current_project().await?;
    /// println!("Current project: {}", project.guid());
    ///
    /// project.transport().play().await?;
    /// # Ok::<(), eyre::Error>(())
    /// # };
    /// ```
    pub async fn current_project() -> Result<Project> {
        let info = DawConnection::get()
            .project
            .current()
            .await?
            .ok_or_else(|| Error::Other("No current project".to_string()))?;

        Ok(Project::new(info.guid))
    }

    /// Get a specific project by GUID
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The DAW connection is not initialized
    /// - The project with the given GUID does not exist
    /// - RPC communication fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use daw_proto::client::Daw;
    /// # async {
    /// let project = Daw::project("550e8400-e29b-41d4-a716-446655440000").await?;
    /// project.transport().stop().await?;
    /// # Ok::<(), eyre::Error>(())
    /// # };
    /// ```
    pub async fn project(guid: impl Into<String>) -> Result<Project> {
        let guid = guid.into();

        // Verify the project exists
        DawConnection::get()
            .project
            .get(guid.clone())
            .await?
            .ok_or_else(|| Error::Other(format!("Project not found: {}", guid)))?;

        Ok(Project::new(guid))
    }

    /// List all open projects
    ///
    /// Returns a list of all projects currently open in the DAW.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The DAW connection is not initialized
    /// - RPC communication fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use daw_proto::client::Daw;
    /// # async {
    /// let projects = Daw::projects().await?;
    /// for project in projects {
    ///     println!("Project: {}", project.guid());
    /// }
    /// # Ok::<(), eyre::Error>(())
    /// # };
    /// ```
    pub async fn projects() -> Result<Vec<Project>> {
        let infos = DawConnection::get().project.list().await?;

        Ok(infos
            .into_iter()
            .map(|info| Project::new(info.guid))
            .collect())
    }
}
