//! `SongFiles` for the REAPER backend: the folder the project was saved
//! in, served to a peer streaming the song in — a window driving REAPER
//! (Remote or Cue) mirrors its small files and streams its proxies from
//! here, as from a standalone engine. The serving is
//! [`daw_proto::song_files::folder`]'s, the same for every backend; this
//! only finds the project's file, on REAPER's main thread.

use daw_proto::song_files::{SongFile, folder};
use daw_proto::{DawError, DawResult, ProjectContext};
use reaper_high::Reaper as ReaperHigh;

use crate::project_context::find_project_by_guid;

/// The file `project` was saved as (empty when it never was).
async fn project_file(project: ProjectContext) -> DawResult<String> {
    crate::main_thread::query(move || {
        let reaper = ReaperHigh::get();
        let project = match &project {
            ProjectContext::Current => reaper.current_project(),
            ProjectContext::Project(guid) => find_project_by_guid(guid)?,
        };
        Some(project.file().map(|f| f.to_string()).unwrap_or_default())
    })
    .await
    .flatten()
    .ok_or_else(|| DawError::NotFound("no such project".into()))
}

impl daw_proto::SongFiles for crate::Reaper {
    async fn list(&self, project: ProjectContext) -> DawResult<Vec<SongFile>> {
        folder::list(&project_file(project).await?)
    }

    async fn read(
        &self,
        project: ProjectContext,
        path: String,
        start: u64,
        len: u32,
    ) -> DawResult<Vec<u8>> {
        folder::read(&project_file(project).await?, &path, start, len)
    }
}
