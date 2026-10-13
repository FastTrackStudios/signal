//! Setlist domain — the cached setlist, how it's built, and the actions
//! and RPC service that drive it.
//!
//! Re-exports the matching `session_proto` module, so `session::setlist`
//! is one door to both the contract (`Setlist`, `ActiveIndices`, …) and
//! the implementation. Each domain folder in this crate follows that
//! convention.

pub use session_proto::setlist::*;

#[cfg(not(target_arch = "wasm32"))]
pub mod actions;
pub mod builder;
pub mod chart_import;
pub mod service;

pub use builder::SetlistBuilder;
pub use service::SetlistServiceImpl;
