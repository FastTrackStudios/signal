//! Song domain — building a `Song` out of a DAW project, and the RPC
//! service that serves it.
//!
//! Re-exports `session_proto::song` (see `setlist` for the convention).

pub use session_proto::song::*;

pub mod builder;
#[cfg(not(target_arch = "wasm32"))]
pub mod service;

pub use builder::SongBuilder;
#[cfg(not(target_arch = "wasm32"))]
pub use service::SongServiceImpl;
