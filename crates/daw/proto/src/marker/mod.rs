//! Marker module — canonical home for everything markers-related.
//!
//! - [`Marker`] : the wire struct
//! - [`Markers`] : the service trait, decorated with `#[architect::rpc]`
//! - [`MarkerEvent`] / [`MarkerError`] : event + error types
//!
//! The architect macro derives the async vox face from the sync
//! `Markers` trait: backends impl `Markers` directly, in-process
//! callers use it as a plain sync API, and remote callers reach the
//! same surface via the auto-emitted [`MarkersClient`] over vox.

mod error;
mod event;
#[allow(clippy::module_inception)]
mod marker;
mod service;

pub use error::MarkerError;
pub use event::{MarkerEvent, MarkerStreamEvent};
pub use marker::Marker;
pub use service::*;

// vox-emitted names from the auto-generated mirror trait. Re-exported
// with shorter aliases (`descriptor`, `dispatcher`) so consumer
// mounting code reads `marker::descriptor()` and `marker::serve(Reaper)`
// rather than juggling the underscored mirror names directly.
