//! A project's song folder, as a peer streaming the song in reads it —
//! the `daw_proto::SongFiles` service as a handle.

use std::ops::Range;
use std::sync::Arc;

use daw_proto::ProjectContext;
use daw_proto::song_files::{MAX_READ, SongFile};

use crate::{DawClients, Result};

/// One project's song folder on the DAW this client speaks to.
#[derive(Clone)]
pub struct SongFiles {
    guid: String,
    clients: Arc<DawClients>,
}

impl SongFiles {
    pub(crate) fn new(guid: String, clients: Arc<DawClients>) -> Self {
        Self { guid, clients }
    }

    /// Every file in the song folder with its size; the project itself (the
    /// path to open — a `.session` folder is listed with size 0) first.
    pub async fn list(&self) -> Result<Vec<SongFile>> {
        Ok(self
            .clients
            .song_files
            .list(ProjectContext::project(&self.guid))
            .await??)
    }

    /// The bytes `range` of `path`, in reads of at most
    /// [`MAX_READ`](daw_proto::song_files::MAX_READ).
    pub async fn read(&self, path: &str, range: Range<u64>) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut at = range.start;
        while at < range.end {
            let want = u32::try_from((range.end - at).min(u64::from(MAX_READ))).unwrap_or(MAX_READ);
            let got = self
                .clients
                .song_files
                .read(
                    ProjectContext::project(&self.guid),
                    path.to_owned(),
                    at,
                    want,
                )
                .await??;
            if got.is_empty() {
                break;
            }
            at += u64::try_from(got.len()).unwrap_or(0);
            out.extend_from_slice(&got);
        }
        Ok(out)
    }
}
