//! `SetlistBuilder` - Build setlists from open DAW projects
//!
//! Scans open projects and combines their song structures into a setlist.

use crate::song::builder::SongBuilder;
#[cfg(feature = "reaper")]
use daw::reaper::Reaper;
use daw::rpc::Daw;
#[cfg(feature = "reaper")]
use daw::service::{ProjectContext, Projects};
use session_proto::Setlist;
use tracing::{debug, warn};

/// Builder for assembling setlists from open DAW projects
pub struct SetlistBuilder;

impl SetlistBuilder {
    /// Build a setlist from open REAPER projects using sync native service traits.
    ///
    /// # Errors
    ///
    /// Returns an error if extracting song structure from any open project fails.
    #[cfg(feature = "reaper")]
    pub fn build_from_open_projects_native() -> eyre::Result<Setlist> {
        debug!("SETLIST BUILDER: Building native setlist from open projects...");

        let projects = Reaper.list();
        debug!("Found {} open projects", projects.len());

        let mut songs = Vec::new();
        for (idx, project) in projects.iter().enumerate() {
            debug!(
                "Processing native project {}: {}",
                idx.saturating_add(1),
                project.guid
            );
            match SongBuilder::build_native(ProjectContext::Project(project.guid.clone())) {
                Ok(project_songs) => {
                    for song in &project_songs {
                        debug!(
                            "  Song extracted: {} ({} sections)",
                            song.name,
                            song.sections.len()
                        );
                    }
                    songs.extend(project_songs);
                }
                Err(e) => {
                    warn!(
                        "  Failed to extract song from project {}: {}",
                        project.guid, e
                    );
                }
            }
        }

        Ok(Setlist {
            id: None,
            name: Self::generate_setlist_name(&songs),
            advance_mode: session_proto::AdvanceMode::default(),
            songs,
        })
    }

    /// Build a setlist from all currently open DAW projects
    ///
    /// Iterates through all open projects, attempts to extract a Song from each,
    /// and combines them into a complete Setlist. Projects that don't contain
    /// valid song structure are skipped with a warning.
    ///
    /// # Errors
    ///
    /// Returns an error if fetching the list of projects fails, or if the DAW
    /// connection is unavailable.
    pub async fn build_from_open_projects(daw: &Daw) -> eyre::Result<Setlist> {
        debug!("============================================================");
        debug!("SETLIST BUILDER: Building setlist from open projects...");
        debug!("============================================================");

        // Get all open projects
        let projects = daw.projects().await?;
        debug!("Found {} open projects", projects.len());

        for (i, project) in projects.iter().enumerate() {
            debug!("  Project {}: {}", i, project.guid());
        }

        let mut songs = Vec::new();
        for (idx, project) in projects.into_iter().enumerate() {
            let guid = project.guid().to_string();
            debug!("------------------------------------------------------------");
            debug!("Processing project {}: {}", idx.saturating_add(1), guid);

            match SongBuilder::build(&project).await {
                Ok(project_songs) => {
                    for song in &project_songs {
                        debug!(
                            "  Song extracted: {} ({} sections)",
                            song.name,
                            song.sections.len()
                        );
                    }
                    songs.extend(project_songs);
                }
                Err(e) => {
                    warn!("  ✗ Failed to extract song from project {}: {}", guid, e);
                }
            }
        }

        debug!("Setlist complete: {} songs extracted", songs.len());

        // Sort songs by start position (they should already be in project tab order)
        // songs.sort_by(|a, b| {
        //     a.start_seconds
        //         .partial_cmp(&b.start_seconds)
        //         .unwrap_or(std::cmp::Ordering::Equal)
        // });

        Ok(Setlist {
            id: None,
            name: Self::generate_setlist_name(&songs),
            advance_mode: session_proto::AdvanceMode::default(),
            songs,
        })
    }

    /// Generate a setlist name from the songs
    ///
    /// Uses the current date/time or a meaningful name based on song count.
    fn generate_setlist_name(songs: &[session_proto::Song]) -> String {
        if songs.is_empty() {
            return "Empty Setlist".to_string();
        }

        // Use current date/time
        let now = chrono::Local::now();
        format!("Setlist - {}", now.format("%Y-%m-%d"))
    }
}
