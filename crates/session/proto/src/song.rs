//! Song domain types
//!
//! Represents a song as a structured collection of sections derived from
//! DAW markers and regions.
//!
//! This module re-exports section types from keyflow for consistency
//! across the codebase, while providing session-specific wrappers where needed.

use crate::{SectionId, SongId};
use daw_proto::{Position, TimeSignature};
use facet::Facet;

// Re-export section types from keyflow as the single source of truth
pub use keyflow::Chart;
pub use keyflow::sections::SectionType;
pub use keyflow::sections::colors::{SectionColors, colors_for_section_type};

/// A comment marker within a song
///
/// Comments are markers that aren't structural (not SONGSTART, SONGEND, =START, =END, COUNT-IN).
/// They serve as reminders or notes displayed above the progress bar.
/// Examples: "Keys only", "Build energy", "Watch conductor", "Count-In" (when mid-song)
///
/// Markers prefixed with `>` are section-only comments (e.g., ">Watch tempo")
/// and will only appear in the section progress bar, not the main song progress bar.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct Comment {
    /// DAW marker ID (if applicable)
    pub id: Option<u32>,
    /// Comment text (the marker name, with `>` prefix stripped if present)
    pub text: String,
    /// Position in seconds (absolute project time)
    pub position_seconds: f64,
    /// Color for visual representation (from marker color, None for default)
    pub color: Option<u32>,
    /// Whether this is a count-in marker that appears mid-song
    /// (used for special styling)
    pub is_count_in: bool,
    /// Whether this comment should only appear in the section progress bar
    /// (marker was prefixed with `>`)
    pub section_only: bool,
}

/// Chord detected from a MIDI-derived chart source.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SongDetectedChord {
    /// Chord symbol text
    pub symbol: String,
    /// Start position in PPQ ticks
    pub start_ppq: i64,
    /// End position in PPQ ticks
    pub end_ppq: i64,
    /// Root pitch in MIDI note numbers
    pub root_pitch: u8,
    /// Max velocity observed in chord notes
    pub velocity: u8,
}

/// Delta payload for chart hydration updates.
///
/// This is sent separately from base `Song` structure updates so high-frequency
/// setlist and transport flows do not clone large chart text blobs.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SongChartHydration {
    /// DAW project GUID for this chart source.
    pub project_guid: String,
    /// Generated chart source text.
    pub chart_text: String,
    /// Detected chords from the source MIDI.
    pub detected_chords: Vec<SongDetectedChord>,
    /// Source fingerprint used for cache invalidation/live refresh.
    pub chart_fingerprint: String,
}

impl Comment {
    /// Create a new comment
    #[must_use]
    pub fn new(text: &str, position_seconds: f64) -> Self {
        let (clean_text, section_only) = Self::parse_section_only_prefix(text);
        Self {
            id: None,
            text: clean_text,
            position_seconds,
            color: None,
            is_count_in: false,
            section_only,
        }
    }

    /// Create a count-in comment (for mid-song count-in markers)
    #[must_use]
    pub fn count_in(position_seconds: f64) -> Self {
        Self {
            id: None,
            text: "Count-In".to_string(),
            position_seconds,
            color: None,
            is_count_in: true,
            section_only: false,
        }
    }

    /// Parse the `>` prefix from marker text
    /// Returns (`cleaned_text`, `is_section_only`)
    fn parse_section_only_prefix(text: &str) -> (String, bool) {
        let trimmed = text.trim();
        trimmed.strip_prefix('>').map_or_else(
            || (trimmed.to_string(), false),
            |rest| (rest.trim().to_string(), true),
        )
    }

    /// Get the color as a CSS hex string
    #[must_use]
    pub fn color_hex(&self) -> Option<String> {
        self.color.map(|c| {
            // REAPER colors are in BGR format, convert to RGB hex.
            let byte = |shifted: u32| u8::try_from(shifted & 0xFF).unwrap_or(0);
            let r = byte(c >> 16);
            let g = byte(c >> 8);
            let b = byte(c);
            format!("#{b:02x}{g:02x}{r:02x}")
        })
    }
}

/// A section within a song (derived from a region or markers)
///
/// Sections represent structural parts of a song like verses, choruses, bridges.
/// They are typically extracted from DAW regions or marker pairs.
///
/// This is a session-specific wrapper that provides direct access to time positions
/// for UI rendering. For chart parsing, use `keyflow::sections::Section` directly.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct Section {
    /// Stable unique identifier for this section.
    pub section_id: SectionId,
    /// DAW region/marker ID (if applicable)
    pub id: Option<u32>,
    /// Section name (e.g., "Verse 1", "Chorus") - without the comment portion
    pub name: String,
    /// Optional comment/description (e.g., "Woodwinds" from 'Interlude C "Woodwinds"')
    pub comment: Option<String>,
    /// Type of section (from keyflow-proto)
    pub section_type: SectionType,
    /// Start position in seconds
    pub start_seconds: f64,
    /// End position in seconds
    pub end_seconds: f64,
    /// Section number (e.g., 1 for "Verse 1", 2 for "Verse 2")
    pub number: Option<u32>,
    /// Color for visual representation (0 for default)
    pub color: Option<u32>,
}

impl Section {
    /// Get the duration of this section in seconds
    #[must_use]
    pub fn duration(&self) -> f64 {
        self.end_seconds - self.start_seconds
    }

    /// Check if a position falls within this section
    #[must_use]
    pub fn contains(&self, seconds: f64) -> bool {
        seconds >= self.start_seconds && seconds < self.end_seconds
    }

    /// Get display name for this section (name only, without comment)
    #[must_use]
    pub fn display_name(&self) -> String {
        self.name.clone()
    }

    /// Get display name with comment if present (e.g., "Interlude C (Woodwinds)")
    #[must_use]
    pub fn display_name_with_comment(&self) -> String {
        self.comment.as_ref().map_or_else(
            || self.name.clone(),
            |comment| format!("{} ({})", self.name, comment),
        )
    }

    /// Get a short display name for space-constrained UI contexts
    ///
    /// Converts section names to abbreviated forms:
    /// - "Interlude A" -> "INT A"
    /// - "Verse 1" -> "VS 1"
    /// - "Chorus 2" -> "CH 2"
    /// - "Pre-Chorus" -> "PRE-CH"
    /// - "Bridge 1" -> "BR 1"
    /// - "Outro A" -> "OUT A"
    #[must_use]
    pub fn short_display(&self) -> String {
        let abbrev = self.section_type.abbreviation();

        // Extract any suffix (number or letter) from the name
        let suffix = self.extract_suffix();

        if suffix.is_empty() {
            abbrev
        } else {
            format!("{abbrev} {suffix}")
        }
    }

    /// Extract suffix (number/letter) from section name
    fn extract_suffix(&self) -> String {
        let name = &self.name;

        // Find where the suffix starts - look for trailing numbers/letters after a space
        if let Some(space_idx) = name.rfind(' ') {
            // `space_idx` is the byte offset of an ASCII space, so
            // `space_idx + 1` always lands on a char boundary.
            let suffix = name
                .get(space_idx.saturating_add(1)..)
                .unwrap_or_default()
                .trim();
            if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_alphanumeric()) {
                return suffix.to_string();
            }
        }

        // Also handle cases like "Verse1" without space
        let trailing: String = name
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
            .collect::<String>()
            .chars()
            .rev()
            .collect();

        if !trailing.is_empty() && trailing.len() < name.len() && trailing.len() <= 3 {
            trailing
        } else {
            String::new()
        }
    }

    /// Get bright color for UI display using keyflow's color palette
    #[must_use]
    pub fn bright_color(&self) -> String {
        colors_for_section_type(&self.section_type).bright_hex()
    }

    /// Get muted color for UI display using keyflow's color palette
    #[must_use]
    pub fn muted_color(&self) -> String {
        colors_for_section_type(&self.section_type).muted_hex()
    }

    /// Get the section's semantic colors
    #[must_use]
    pub fn colors(&self) -> SectionColors {
        colors_for_section_type(&self.section_type)
    }

    /// Calculate progress percentage (0-100) based on transport position
    #[must_use]
    pub fn progress(&self, transport_position: f64) -> f64 {
        let section_duration = self.duration();

        if section_duration <= 0.0 {
            return 0.0;
        }

        let relative_position = (transport_position - self.start_seconds).max(0.0);
        (relative_position / section_duration).min(1.0) * 100.0
    }
}

/// A song in the setlist
///
/// Songs are extracted from DAW projects using markers and regions.
/// A song typically has a start marker (SONGSTART), end marker (SONGEND),
/// and regions defining its internal structure (sections).
#[derive(Clone, PartialEq, Facet)]
pub struct Song {
    /// Stable unique identifier for this song.
    pub id: SongId,
    /// Song name
    pub name: String,
    /// DAW project GUID this song belongs to
    pub project_guid: String,
    /// Start position in seconds (from SONGSTART marker or region start)
    pub start_seconds: f64,
    /// End position in seconds (from SONGEND marker or region end)
    pub end_seconds: f64,
    /// Count-in duration before song start (optional)
    pub count_in_seconds: Option<f64>,
    /// Sections within this song
    pub sections: Vec<Section>,
    /// Comment markers within this song (non-structural markers)
    pub comments: Vec<Comment>,
    /// Tempo at song start (if available)
    pub tempo: Option<f64>,
    /// Time signature at song start (if available)
    pub time_signature: Option<TimeSignature>,
    /// Measure positions within this song (for beat grid display)
    pub measure_positions: Vec<Position>,
    /// Generated chart text from MIDI analysis (if available)
    pub chart_text: Option<String>,
    /// Parsed chart from `chart_text` for immediate UI use (if available)
    pub parsed_chart: Option<Chart>,
    /// Detected chords from the MIDI source used for chart generation
    pub detected_chords: Vec<SongDetectedChord>,
    /// Source fingerprint used for cache invalidation/live refresh
    pub chart_fingerprint: Option<String>,
    /// Per-song advance mode override. `None` = use the setlist default.
    pub advance_mode: Option<crate::setlist::AdvanceMode>,
    /// Color inherited from the SONG-lane region (REAPER native color, if set).
    pub color: Option<u32>,
}

impl std::fmt::Debug for Song {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Song")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("project_guid", &self.project_guid)
            .field("start_seconds", &self.start_seconds)
            .field("end_seconds", &self.end_seconds)
            .field("count_in_seconds", &self.count_in_seconds)
            .field("sections", &self.sections)
            .field("comments", &self.comments)
            .field("tempo", &self.tempo)
            .field("time_signature", &self.time_signature)
            .field("measure_positions", &self.measure_positions)
            .field(
                "chart_text",
                &self.chart_text.as_ref().map(std::string::String::len),
            )
            .field(
                "parsed_chart",
                &self.parsed_chart.as_ref().map(|_| "parsed"),
            )
            .field("detected_chords", &self.detected_chords)
            .field("chart_fingerprint", &self.chart_fingerprint)
            .field("advance_mode", &self.advance_mode)
            .field("color", &self.color)
            .finish()
    }
}

impl Song {
    /// Returns this song's advance mode, falling back to the provided setlist default.
    #[must_use]
    pub fn effective_advance_mode(
        &self,
        default: crate::setlist::AdvanceMode,
    ) -> crate::setlist::AdvanceMode {
        self.advance_mode.unwrap_or(default)
    }

    /// Get the total duration of the song in seconds
    #[must_use]
    pub fn duration(&self) -> f64 {
        self.end_seconds - self.start_seconds
    }

    /// Get the duration including count-in
    #[must_use]
    pub fn duration_with_count_in(&self) -> f64 {
        self.duration() + self.count_in_seconds.unwrap_or(0.0)
    }

    /// Find the section at a given absolute position (in project time)
    #[must_use]
    pub fn section_at(&self, seconds: f64) -> Option<&Section> {
        self.sections.iter().find(|s| s.contains(seconds))
    }

    /// Find the section at a given position, returning both index and reference
    #[must_use]
    pub fn section_at_position(&self, seconds: f64) -> Option<&Section> {
        self.section_at(seconds)
    }

    /// Find the section at a given position with its index
    #[must_use]
    pub fn section_at_position_with_index(&self, seconds: f64) -> Option<(usize, &Section)> {
        self.sections
            .iter()
            .enumerate()
            .find(|(_, s)| s.contains(seconds))
    }

    /// Get start position in seconds (for compatibility with method-style access)
    #[must_use]
    pub const fn start_seconds(&self) -> f64 {
        self.start_seconds
    }

    /// Get end position in seconds (for compatibility with method-style access)
    #[must_use]
    pub const fn end_seconds(&self) -> f64 {
        self.end_seconds
    }

    /// Get song-relative position (0.0 = start of song)
    #[must_use]
    pub fn relative_position(&self, absolute_seconds: f64) -> f64 {
        absolute_seconds - self.start_seconds
    }

    /// Get absolute position from song-relative position
    #[must_use]
    pub fn absolute_position(&self, relative_seconds: f64) -> f64 {
        self.start_seconds + relative_seconds
    }

    /// Get bright color for UI display.
    ///
    /// Prefers the REAPER region color (from the SONG-lane region), then falls
    /// back to the first section's color, then a default blue.
    #[must_use]
    pub fn bright_color(&self) -> String {
        if let Some(hex) = self.reaper_color_hex() {
            return hex;
        }
        self.sections
            .first()
            .map_or_else(|| "#3b82f6".to_string(), Section::bright_color) // default blue
    }

    /// Get muted (darker) color for UI display.
    ///
    /// Prefers a darkened version of the REAPER region color, then falls back
    /// to the first section's muted color, then a default dark blue.
    #[must_use]
    pub fn muted_color(&self) -> String {
        if let Some(c) = self.color {
            // REAPER native color: 0x01BBGGRR.
            let r = u8::try_from(c & 0xFF).unwrap_or(0);
            let g = u8::try_from((c >> 8) & 0xFF).unwrap_or(0);
            let b = u8::try_from((c >> 16) & 0xFF).unwrap_or(0);
            // Darken by ~40%. `std` has no non-`as` float-to-int
            // conversion; the input is always in 0.0..=255.0 here.
            #[allow(
                clippy::as_conversions,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss
            )]
            let darken = |component: u8| (f32::from(component) * 0.6) as u8;
            let r = darken(r);
            let g = darken(g);
            let b = darken(b);
            return format!("#{r:02x}{g:02x}{b:02x}");
        }
        self.sections
            .first()
            .map_or_else(|| "#1e3a8a".to_string(), Section::muted_color) // default blue-900
    }

    /// Convert REAPER native color (0x01BBGGRR) to CSS hex (#RRGGBB).
    fn reaper_color_hex(&self) -> Option<String> {
        let c = self.color?;
        let r = c & 0xFF;
        let g = (c >> 8) & 0xFF;
        let b = (c >> 16) & 0xFF;
        Some(format!("#{r:02x}{g:02x}{b:02x}"))
    }

    /// Calculate progress percentage (0-100) based on transport position
    #[must_use]
    pub fn progress(&self, transport_position: f64) -> f64 {
        let song_duration = self.duration();

        if song_duration <= 0.0 {
            return 0.0;
        }

        let relative_position = (transport_position - self.start_seconds).max(0.0);
        (relative_position / song_duration).min(1.0) * 100.0
    }
}
