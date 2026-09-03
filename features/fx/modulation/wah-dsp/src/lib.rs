//! FTS Wah — auto-wah with envelope follower and MSEG pattern control.
//!
//! Signal flow: Input → Envelope Follower → Filter Cutoff Modulation → Resonant Filter → Mix → Output.
//!
//! The filter cutoff is driven by a combination of:
//! - Envelope follower (auto-wah)
//! - MSEG pattern (rhythmic wah)
//! - Static position (manual wah)

// Realtime guard. This crate runs on an audio callback, so the calls in
// clippy.toml's disallowed-methods list (locks, env, sleep) are real bugs here
// even though they are allowed workspace-wide off the audio thread.
#![deny(clippy::disallowed_methods)]

pub mod chain;
pub mod filter;
