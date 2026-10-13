//! `SongFiles` for the standalone engine: the folder a project was opened
//! from, served to a peer streaming the song in.

use daw_proto::song_files::SongFile;
#[cfg(not(target_arch = "wasm32"))]
use daw_proto::song_files::folder;
use daw_proto::{DawError, DawResult, ProjectContext};

use crate::sync::Standalone;

impl Standalone {
    /// The file (a `.RPP` or a `.session` folder) `project` was opened from.
    fn project_file(&self, project: &ProjectContext) -> DawResult<String> {
        let guid = match project {
            ProjectContext::Project(guid) => guid.clone(),
            ProjectContext::Current => self
                .state
                .lock()
                .ok()
                .and_then(|s| s.current_project_guid.clone())
                .ok_or_else(|| DawError::NotFound("no current project".into()))?,
        };
        self.with_project(&guid, |p| p.info.path.clone())
            .map_err(|_| DawError::NotFound(format!("no project {guid}")))
    }
}

impl daw_proto::SongFiles for Standalone {
    async fn list(&self, project: ProjectContext) -> DawResult<Vec<SongFile>> {
        #[cfg(not(target_arch = "wasm32"))]
        return folder::list(&self.project_file(&project)?);
        #[cfg(target_arch = "wasm32")]
        return Err(no_folder(&project));
    }

    async fn read(
        &self,
        project: ProjectContext,
        path: String,
        start: u64,
        len: u32,
    ) -> DawResult<Vec<u8>> {
        #[cfg(not(target_arch = "wasm32"))]
        return folder::read(&self.project_file(&project)?, &path, start, len);
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (path, start, len);
            return Err(no_folder(&project));
        }
    }
}

/// A page has no disk: its songs arrived from elsewhere, and it serves
/// none on.
#[cfg(target_arch = "wasm32")]
fn no_folder(project: &ProjectContext) -> DawError {
    DawError::NotFound(format!("{project:?}: a browser serves no song folder"))
}
