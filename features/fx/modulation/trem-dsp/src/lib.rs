//! FTS Tremolo — amplitude modulation with MSEG pattern / LFO control.
//!
//! Signal flow: Input → Stereo Split → Amplitude Modulation → Mix → Output.
//!
//! Uses the fts-modulation engine for pattern-driven or tempo-synced
//! amplitude control with stereo phase offset.

pub mod chain;
pub mod dynamics;
pub mod tremolo;
