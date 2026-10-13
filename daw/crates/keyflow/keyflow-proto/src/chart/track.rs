//! Track System
//!
//! Tracks allow parallel content types (chords, melodies, rhythms) to run
//! simultaneously within a section, similar to LilyPond's voice system.
//!
//! # Syntax
//!
//! ```text
//! VS 8
//! [chords] Cmaj7/// Dm7/// Em7/// Fmaj7///
//! [melody] m{ C_8 D_8 E_4 F_4 G_4 A_4 B_4 C'_4 }
//! [rhythm] //// //// //// ////
//! ```
//!
//! Or with default chords track (no marker needed):
//! ```text
//! VS 8
//! Cmaj7/// Dm7/// Em7/// Fmaj7///
//! [melody] m{ C_8 D_8 E_4 F_4 G_4 A_4 B_4 C'_4 }
//! ```

use super::lyrics::LyricLine;
use super::melody::Melody;
use super::types::Measure;
use facet::Facet;

/// Type of content a track contains
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet, Default)]
#[repr(u8)]
pub enum TrackType {
    /// Chord symbols with rhythm notation (default)
    #[default]
    Chords,
    /// Melodic line that spans the section
    Melody,
    /// Explicit rhythm pattern
    Rhythm,
    /// Lyrics/words
    Lyrics,
}

impl TrackType {
    /// Parse a track type from a marker string (without brackets)
    ///
    /// # Examples
    /// - "chords" -> Chords
    /// - "melody" -> Melody
    /// - "melody lead" -> Melody (with name "lead")
    /// - "rhythm" -> Rhythm
    pub fn parse(s: &str) -> Option<(Self, Option<String>)> {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        let track_type = match parts[0].to_lowercase().as_str() {
            "chords" | "chord" => Self::Chords,
            "melody" | "mel" => Self::Melody,
            "rhythm" | "rhythms" | "rhy" => Self::Rhythm,
            "lyrics" | "lyric" | "words" => Self::Lyrics,
            _ => return None,
        };

        // Get optional name (everything after the type)
        let name = if parts.len() > 1 {
            Some(parts[1..].join(" "))
        } else {
            None
        };

        Some((track_type, name))
    }

    /// Get the default marker string for this track type
    pub fn marker(&self) -> &'static str {
        match self {
            Self::Chords => "chords",
            Self::Melody => "melody",
            Self::Rhythm => "rhythm",
            Self::Lyrics => "lyrics",
        }
    }
}

/// A single track within a section
///
/// Tracks run in parallel and can contain different types of content.
/// The default track type is Chords, which doesn't require a marker.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Track {
    /// Type of track content
    pub track_type: TrackType,

    /// Optional custom name (e.g., "lead", "bass line", "harmony")
    pub name: Option<String>,

    /// For Chords/Rhythm tracks: measures with content
    pub measures: Vec<Measure>,

    /// For Melody tracks: the melody content spanning the section
    pub melody: Option<Melody>,

    /// For Lyrics tracks: the lyric line with syllables and optional chord attachments
    pub lyrics: Option<LyricLine>,

    /// Display color for this track/voice (karaoke vocal lanes — lead, harmony,
    /// audience…). A CSS-style string (`#E2574C`, `red`). `None` = default.
    #[facet(default)]
    pub color: Option<String>,
}

impl Track {
    /// Create a new chord track from measures
    pub fn chords(measures: Vec<Measure>) -> Self {
        Self {
            track_type: TrackType::Chords,
            name: None,
            measures,
            melody: None,
            lyrics: None,
            color: None,
        }
    }

    /// Create a new melody track
    pub fn melody(melody: Melody) -> Self {
        Self {
            track_type: TrackType::Melody,
            name: None,
            measures: Vec::new(),
            melody: Some(melody),
            lyrics: None,
            color: None,
        }
    }

    /// Create a new rhythm track from measures
    pub fn rhythm(measures: Vec<Measure>) -> Self {
        Self {
            track_type: TrackType::Rhythm,
            name: None,
            measures,
            melody: None,
            lyrics: None,
            color: None,
        }
    }

    /// Create a new lyrics track
    pub fn lyrics(lyric_line: LyricLine) -> Self {
        Self {
            track_type: TrackType::Lyrics,
            name: None,
            measures: Vec::new(),
            melody: None,
            lyrics: Some(lyric_line),
            color: None,
        }
    }

    /// Set the track name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the track/voice display color.
    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Check if this is the default chord track
    pub fn is_default_chord_track(&self) -> bool {
        self.track_type == TrackType::Chords && self.name.is_none()
    }
}

impl Default for Track {
    fn default() -> Self {
        Self {
            track_type: TrackType::Chords,
            name: None,
            measures: Vec::new(),
            melody: None,
            lyrics: None,
            color: None,
        }
    }
}
