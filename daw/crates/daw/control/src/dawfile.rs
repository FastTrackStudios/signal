//! Wrapper around [`DawFileService`] — pure-function project file ops
//! (parsing `.RPP` summaries, combining `.RPL` setlists).
//!
//! Exposed via [`Daw::dawfile`](crate::Daw::dawfile) so any client
//! (CLI, UI, extensions) can reuse the same dawfile-reaper logic without
//! linking the parser crate directly.

use std::sync::Arc;

use daw_proto::{CombineSetlistOptions, CombineSetlistResult, ProjectSummary};

use crate::DawClients;

/// Pure-function file operations served by `daw-bridge` (no REAPER context).
#[derive(Clone)]
pub struct DawFile {
    clients: Arc<DawClients>,
}

impl DawFile {
    pub(crate) fn new(clients: Arc<DawClients>) -> Self {
        Self { clients }
    }

    /// Summarize a `.RPP` project file at `path`.
    pub async fn summarize_project(
        &self,
        path: impl Into<String>,
    ) -> crate::Result<ProjectSummary> {
        Ok(self.clients.dawfile.summarize_project(path.into()).await?)
    }

    /// Combine an `.RPL` setlist at `input` into a single `.RPP`. Pass an
    /// empty string for `output` to write next to `input` using its stem.
    pub async fn combine_setlist(
        &self,
        input: impl Into<String>,
        output: impl Into<String>,
        options: CombineSetlistOptions,
    ) -> crate::Result<CombineSetlistResult> {
        Ok(self
            .clients
            .dawfile
            .combine_setlist(input.into(), output.into(), options)
            .await?)
    }
}
