//! FTS Tremolo — amplitude modulation with MSEG pattern / LFO control.
//!
//! Signal flow: Input → Stereo Split → Amplitude Modulation → Mix → Output.
//!
//! Uses the fts-modulation engine for pattern-driven or tempo-synced
//! amplitude control with stereo phase offset.

pub mod chain;
pub mod dynamics;
pub mod tremolo;

/// Re-export the modulator toolkit so wrappers can drive trigger modes.
pub use fts_modulation;
