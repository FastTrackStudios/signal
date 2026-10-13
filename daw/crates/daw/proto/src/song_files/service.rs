//! The `SongFiles` service.

use super::SongFile;
use crate::{DawResult, ProjectContext};

/// A project's song folder, for a peer streaming the song in.
///
/// Only what is inside the folder the project was opened from is served —
/// never a path out of it.
#[architect::rpc]
pub trait SongFiles {
    /// Every file in `project`'s song folder, with its size — and which of
    /// them is the project itself (the path a client opens), as the first.
    async fn list(&self, project: ProjectContext) -> DawResult<Vec<SongFile>>;

    /// `len` bytes (at most [`super::MAX_READ`]) of `path` from `start`;
    /// fewer at the end of the file.
    async fn read(
        &self,
        project: ProjectContext,
        path: String,
        start: u64,
        len: u32,
    ) -> DawResult<Vec<u8>>;
}
