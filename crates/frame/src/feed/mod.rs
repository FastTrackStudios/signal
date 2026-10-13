//! Feeds: the live values a surface shows — protocol, in-memory feeds and DSP-derived meters.

pub mod proto;

#[cfg(feature = "runtime")]
pub mod memory;

#[cfg(feature = "dsp")]
pub mod dsp;
