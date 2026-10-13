//! `SongService` implementation — builds songs from the current DAW project.

use crate::song::builder::SongBuilder;
use daw::reaper::Reaper;
use daw::service::{ProjectContext, Projects};
use session_proto::{SessionServiceError, Song, SongService};
use tracing::{debug, info, warn};

/// Implementation of `SongService`
#[derive(Clone)]
pub struct SongServiceImpl;

impl Default for SongServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl SongServiceImpl {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl SongService for SongServiceImpl {
    async fn build_from_current_project(&self) -> Result<Song, SessionServiceError> {
        debug!("build_from_current_project called");

        let Some(project) = Reaper.current() else {
            warn!("Failed to get current project");
            return Err(SessionServiceError::DawError(
                "Failed to get current project".to_string(),
            ));
        };

        info!("Got current project: {}", project.guid);

        // Build song(s) from project — return the first song
        match SongBuilder::build_native(ProjectContext::Project(project.guid)) {
            Ok(songs) => {
                let song = songs
                    .into_iter()
                    .next()
                    .ok_or_else(|| SessionServiceError::not_found("Song", "current project"))?;
                info!(
                    "SONG BUILT: {} ({} sections)",
                    song.name,
                    song.sections.len()
                );
                Ok(song)
            }
            Err(e) => {
                warn!("Failed to build song from current project: {}", e);
                Err(SessionServiceError::HydrationError(format!(
                    "Failed to build song from current project: {e}"
                )))
            }
        }
    }

    async fn song(&self, project_guid: String) -> Result<Song, SessionServiceError> {
        info!("SONG SERVICE: song called for project: {}", project_guid);

        if Reaper.get(&project_guid).is_none() {
            warn!("Failed to get project {}", project_guid);
            return Err(SessionServiceError::DawError(format!(
                "Failed to get project {project_guid}"
            )));
        }

        // Build song(s) from project — return the first song
        match SongBuilder::build_native(ProjectContext::Project(project_guid.clone())) {
            Ok(songs) => {
                let song = songs
                    .into_iter()
                    .next()
                    .ok_or_else(|| SessionServiceError::not_found("Song", &project_guid))?;
                info!(
                    "SONG BUILT: {} ({} sections)",
                    song.name,
                    song.sections.len()
                );
                Ok(song)
            }
            Err(e) => {
                warn!("Failed to build song from project {}: {}", project_guid, e);
                Err(SessionServiceError::HydrationError(format!(
                    "Failed to build song from project {project_guid}: {e}"
                )))
            }
        }
    }
}
