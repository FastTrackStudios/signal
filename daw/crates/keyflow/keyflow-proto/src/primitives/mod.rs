//! Musical Primitives
//!
//! Core musical building blocks: notes, intervals, root notations, and tokens

pub mod accidental;
pub mod interval;
pub mod note;
pub mod roman_numeral;
pub mod root_notation;
pub mod scale_degree;
pub mod tokens;

pub use accidental::{Accidental, AccidentalType, WithAccidental};
pub use interval::Interval;
pub use note::{MusicalNote, MusicalNoteToken, Note};
pub use roman_numeral::RomanNumeralToken;
pub use root_notation::{RomanCase, RootFormat, RootNotation};
pub use scale_degree::ScaleDegreeToken;
pub use tokens::MusicalToken;
