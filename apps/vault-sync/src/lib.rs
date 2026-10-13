//! Library half of `session-vault-sync` — see `src/main.rs` for the CLI
//! and `Cargo.toml` for what this tool is and isn't. Exposed as a lib so
//! `apps/session-player` can reuse the Tracks-folder scanner and song
//! link-naming convention without re-implementing them.

pub mod library;
pub mod live_bus;
pub mod rpp;
pub mod setlist;
pub mod vault;
