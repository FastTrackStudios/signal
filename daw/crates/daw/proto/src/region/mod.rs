//! Region module — canonical home for everything regions-related.

mod error;
mod event;
#[allow(clippy::module_inception)]
mod region;
mod service;

pub use error::RegionError;
pub use event::{RegionEvent, RegionStreamEvent};
pub use region::Region;
pub use service::*;

// vox-emitted names from the architect macro mirror. Aliased to short
// names so mounting reads `region::descriptor()` + `region::serve(Reaper)`.
