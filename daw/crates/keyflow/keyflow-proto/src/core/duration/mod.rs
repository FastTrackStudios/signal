//! Duration system for music notation.
//!
//! This module provides:
//! - [`NoteValue`] - Canonical note value enum
//! - [`Ticks<PPQ>`] - Type-safe tick representation with const generic PPQ
//! - [`TupletRatio`] - Tuplet ratio for irregular rhythms
//! - [`NotationDuration`] - Unified duration for cross-system conversion
//! - Conversion traits for flexible duration handling

mod notation;
mod note_value;
mod ticks;
mod traits;
mod tuplet;

pub use notation::{DurationContext, NotationDuration, RhythmType, ToNotationDuration};
pub use note_value::NoteValue;
pub use ticks::{ReaperTicks, StandardTicks, Ticks};
pub use traits::{FromTicks, ToBeats, ToTicks};
pub use tuplet::TupletRatio;
