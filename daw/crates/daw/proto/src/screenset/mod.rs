//! Screensets — types + service trait.

mod service;
mod types;

pub use service::*;
pub use types::{
    CaptureScreensetRequest, Screenset, ScreensetKind, ScreensetMonitor, ScreensetOptions,
    ScreensetRect, ScreensetResult, ScreensetScope, ScreensetSelection, ScreensetSummary,
    ScreensetTrackVisibility, ScreensetWindow,
};
