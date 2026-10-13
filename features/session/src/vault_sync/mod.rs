//! Task vault notes and live-setlist projects from the Song Library.
//!
//! The session app's `session-vault-sync` and `session-player` binaries
//! (apps/session/desktop/src/bin) share the Tracks-folder scanner and the
//! song link-naming convention through it.

pub mod library;
pub mod live_bus;
pub mod rpp;
pub mod setlist;
pub mod vault;
