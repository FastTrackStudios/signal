//! `impl Screensets for Standalone` — stub. Returns errors.

use daw_proto::{
    CaptureScreensetRequest, Screenset, ScreensetOptions, ScreensetResult, ScreensetSummary,
    Screensets,
};

use crate::sync::Standalone;

impl Screensets for Standalone {
    fn capture(&self, _request: CaptureScreensetRequest) -> ScreensetResult {
        ScreensetResult::error("screensets unsupported on standalone backend")
    }

    fn save(&self, _screenset: Screenset, _options: ScreensetOptions) -> ScreensetResult {
        ScreensetResult::error("screensets unsupported on standalone backend")
    }

    fn list(&self, _options: ScreensetOptions) -> Vec<ScreensetSummary> {
        Vec::new()
    }

    fn get(&self, _id: &str, _options: ScreensetOptions) -> Option<Screenset> {
        None
    }

    fn apply(&self, _id: &str, _options: ScreensetOptions) -> ScreensetResult {
        ScreensetResult::error("screensets unsupported on standalone backend")
    }

    fn delete(&self, _id: &str, _options: ScreensetOptions) -> ScreensetResult {
        ScreensetResult::error("screensets unsupported on standalone backend")
    }
}
