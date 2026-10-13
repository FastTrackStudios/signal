//! Projects — types, events, service trait.
//!
//! `ProjectContext` is the per-method scope marker every architect::rpc
//! service uses. `ProjectInfo` is the metadata struct. `ProjectEvent`
//! is the lifecycle event stream (retired for now; sibling trait
//! when revived). `Projects` is the architect::rpc service trait
//! covering both collection-level and per-project operations.

mod event;
mod service;
mod types;

pub use event::{ProjectEvent, ProjectStreamEvent};
pub use service::*;
pub use types::{ProjectContext, ProjectInfo};
