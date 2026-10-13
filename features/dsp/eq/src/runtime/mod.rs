//! What actually runs on the audio thread, once the coefficients exist.
//!
//! The split from [`crate::design`] is the important one in this crate: design
//! is arithmetic that happens when a parameter changes, and runtime is
//! arithmetic that happens per sample. Only the second is realtime-critical,
//! and only the first is allowed to be expensive.

pub mod band;
pub mod chain;
pub mod response;
pub mod section;
