//! Accidental modifiers for musical notation
//!
//! Provides both a trait for accidental-aware types and a concrete enum for accidentals

use facet::Facet;

/// Trait for types that can have accidentals applied to them
pub trait WithAccidental {
    /// Apply a sharp to this element
    fn sharp(self) -> Self;

    /// Apply a flat to this element
    fn flat(self) -> Self;

    /// Apply a double sharp to this element
    fn double_sharp(self) -> Self;

    /// Apply a double flat to this element
    fn double_flat(self) -> Self;

    /// Reset to natural (no accidental)
    fn natural(self) -> Self;
}

/// Concrete accidental type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum Accidental {
    DoubleFlat,
    Flat,
    Natural,
    Sharp,
    DoubleSharp,
}

impl Accidental {
    /// Get the semitone offset for this accidental
    pub fn semitone_offset(&self) -> i8 {
        match self {
            Accidental::DoubleFlat => -2,
            Accidental::Flat => -1,
            Accidental::Natural => 0,
            Accidental::Sharp => 1,
            Accidental::DoubleSharp => 2,
        }
    }

    /// Get the string representation
    pub fn to_str(&self) -> &'static str {
        match self {
            Accidental::DoubleFlat => "bb",
            Accidental::Flat => "b",
            Accidental::Natural => "",
            Accidental::Sharp => "#",
            Accidental::DoubleSharp => "##",
        }
    }
}

// Re-export for cleaner imports
pub use self::Accidental as AccidentalType;
