//! Clean facade API for common Keyflow use cases.
//!
//! This module is intentionally small and opinionated:
//! - A prelude with the most common types.
//! - No parsing helpers (those live in `keyflow-text`).
//! - No service/RPC types (those live in `services`).

/// Commonly used types for day-to-day chart work.
pub mod prelude {
    pub use crate::ParseError;
    pub use crate::chart::{Chart, ChartPosition, ChartSection, Measure};
    pub use crate::chord::{Chord, ChordParseError, ChordRhythm, PushPullAmount};
    pub use crate::key::Key;
    pub use crate::metadata::SongMetadata;
    pub use crate::sections::{Section, SectionType};
    pub use crate::time::{MusicalDuration, MusicalPosition, Tempo, TimeSignature};
    pub use crate::{DynamicMarking, TextCue};
}
