//! Theming: the shared theme model and context (audio-controls'), plus
//! REAPER theme import.

pub use audio_controls::theming::*;

/// REAPER theme import (palette/params → [`Theme`]) — `reaper-theme` feature.
#[cfg(feature = "reaper-theme")]
pub mod reaper_import;
#[cfg(feature = "reaper-theme")]
pub use reaper_import::theme_from_reaper;
