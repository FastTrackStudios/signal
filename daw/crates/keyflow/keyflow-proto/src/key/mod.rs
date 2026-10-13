//! Key System
//!
//! Musical keys, scales, and key-relative operations

pub mod definition;
pub mod scale;
pub mod spelling;

// Backward compatibility module
pub mod keys {
    pub use super::definition::Key;
    pub use super::scale::{ScaleMode, ScaleType};
}

pub use definition::Key;
pub use scale::{
    DiatonicFamily, DiatonicMode, HarmonicMinorFamily, HarmonicMinorMode, MelodicMinorFamily,
    MelodicMinorMode, ScaleFamily, ScaleMode, ScaleType,
};
pub use spelling::{KeySpelling, NoteSpelling, SpellingMode};
