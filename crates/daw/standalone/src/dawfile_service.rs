//! `impl DawFileOps for Standalone` — stub.

use daw_proto::{CombineSetlistOptions, CombineSetlistResult, DawFileOps, ProjectSummary};

use crate::sync::Standalone;

impl DawFileOps for Standalone {
    fn summarize_project(&self, path: &str) -> ProjectSummary {
        let mut s = ProjectSummary::default();
        s.path = path.to_string();
        s.error = "standalone has no dawfile parser".to_string();
        s
    }

    fn combine_setlist(
        &self,
        input: &str,
        output: &str,
        _options: CombineSetlistOptions,
    ) -> CombineSetlistResult {
        let mut r = CombineSetlistResult::default();
        r.input = input.to_string();
        r.output = output.to_string();
        r.error = "standalone has no dawfile parser".to_string();
        r
    }
}
