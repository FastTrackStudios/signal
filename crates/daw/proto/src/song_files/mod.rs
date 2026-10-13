//! A project's song folder, served to peers: what a client streaming the
//! song in needs that is not the DAW's state — the prepared session, the
//! chart, the proxies and their page indexes, the waveform caches.
//!
//! A client that does not have the song mirrors the small files (so it
//! opens the song the one way songs open) and streams the proxies by range
//! in the order they will be heard.

#[cfg(not(target_arch = "wasm32"))]
pub mod folder;
mod service;
mod types;

pub use service::*;
pub use types::*;
