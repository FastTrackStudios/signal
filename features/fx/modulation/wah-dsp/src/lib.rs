//! FTS Wah — auto-wah with envelope follower and MSEG pattern control.
//!
//! Signal flow: Input → Envelope Follower → Filter Cutoff Modulation → Resonant Filter → Mix → Output.
//!
//! The filter cutoff is driven by a combination of:
//! - Envelope follower (auto-wah)
//! - MSEG pattern (rhythmic wah)
//! - Static position (manual wah)

pub mod chain;
pub mod filter;
