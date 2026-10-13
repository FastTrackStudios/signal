//! Read-only / pure-function operations on `.RPP` project files.

use super::{CombineSetlistOptions, CombineSetlistResult, ProjectSummary};

#[architect::rpc]
pub trait DawFileOps {
    /// Parse the `.RPP` at `path` and return a high-level summary.
    /// `error` field is populated on failure (rather than returning
    /// `Result`) so the caller can surface the same shape regardless.
    fn summarize_project(&self, path: &str) -> ProjectSummary;

    /// Combine an `.RPL` setlist into a single `.RPP` saved at
    /// `output`. When `output` is empty, the combined file is
    /// written next to `input` using the input's stem.
    fn combine_setlist(
        &self,
        input: &str,
        output: &str,
        options: CombineSetlistOptions,
    ) -> CombineSetlistResult;
}
