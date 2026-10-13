//! Song building / lookup service.

use super::error::SessionServiceError;
use crate::song::Song;

pub mod song_service {
    use super::{SessionServiceError, Song};

    #[architect::rpc]
    pub trait SongService {
        /// Build a song from the current active DAW project
        ///
        /// Analyzes the current project's markers (SONGSTART/SONGEND) and regions
        /// to extract song structure including sections, tempo, and time signature.
        ///
        /// Returns None if no valid song structure is found.
        async fn build_from_current_project(&self) -> Result<Song, SessionServiceError>;

        /// Get song information for a specific project by GUID
        ///
        /// Loads and analyzes the specified project to extract song information.
        async fn song(&self, project_guid: String) -> Result<Song, SessionServiceError>;
    }
}

pub use song_service::{
    Service as SongServiceLayer, SongService, SongServiceClient, SongServiceDispatcher,
    layer as song_service_layer, serve as serve_song_service, song_service_rpc_service_descriptor,
    song_service_service_descriptor,
};
