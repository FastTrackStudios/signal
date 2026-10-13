//! Transport sync — follow a backend's transport to the sample from
//! another process: the server's sync clock to ping, and clock-stamped
//! playhead positions.
//!
//! The maths (clock-offset estimation, projection, drift correction)
//! is `daw-transport-sync`'s; this module is its wire surface. Any
//! backend with a per-buffer snapshot serves it — daw-standalone from
//! its transport engine, REAPER from daw-audio-sync's audio hook.

mod service;
mod types;

pub use service::*;
pub use types::*;
