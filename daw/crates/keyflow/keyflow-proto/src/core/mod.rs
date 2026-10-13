//! Core music theory primitives.
//!
//! This module provides canonical types for musical concepts:
//!
//! - [`NoteValue`] - Canonical note value enum (whole, half, quarter, etc.)
//! - [`Ticks<PPQ>`] - Type-safe tick representation with const generic PPQ
//! - [`TupletRatio`] - Tuplet ratio for irregular rhythms
//! - [`TimeSignature`] - Canonical time signature type
//! - [`ChordSymbol`] - Trait for unified chord symbol representation
//! - Conversion traits ([`ToTicks`], [`FromTicks`], [`ToBeats`])

pub mod chord;
pub mod duration;
pub mod time;

// Re-export primary types for convenience
pub use chord::ChordSymbol;
pub use duration::{
    DurationContext, FromTicks, NotationDuration, NoteValue, ReaperTicks, RhythmType,
    StandardTicks, Ticks, ToBeats, ToNotationDuration, ToTicks, TupletRatio,
};
pub use time::TimeSignature;
