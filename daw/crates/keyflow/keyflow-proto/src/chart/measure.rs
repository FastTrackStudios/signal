//! Core Chart Types
//!
//! Defines the core data structures for chart representation

use super::commands::Command;
use super::melody::Melody;
use super::track::{Track, TrackType};
pub use super::types::Measure;
use crate::chord::{Chord, ChordRhythm, PushPullAmount};
use crate::parsing::TextSpan;
use crate::primitives::RootNotation;
use crate::sections::Section;
use crate::time::{AbsolutePosition, MusicalDuration};
use facet::Facet;

/// Represents a chord instance with position and timing information
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct ChordInstance {
    /// Root notation (preserves original format: note, degree, or roman)
    pub root: RootNotation,

    /// Full chord symbol for display (e.g., "Gmaj7", "C9", "Em7")
    pub full_symbol: String,

    /// Parsed chord data
    pub parsed: Chord,

    /// Rhythm notation (slashes, duration, etc.)
    pub rhythm: ChordRhythm,

    /// Original token before processing (e.g., "1", "I", "g", "Gmaj7")
    pub original_token: String,

    /// Duration in measure.beats.subdivision format
    pub duration: MusicalDuration,

    /// Position in the song
    pub position: AbsolutePosition,

    /// Push/pull timing adjustment: (is_push, amount)
    /// - true = push (anticipate, play earlier)
    /// - false = pull (delay, play later)
    pub push_pull: Option<(bool, PushPullAmount)>,

    /// Commands applied to this chord (fermata, accent, etc.)
    pub commands: Vec<Command>,

    /// Source text span for click-to-highlight and editing
    /// Links this chord back to the original input text that generated it.
    pub source_span: Option<TextSpan>,
}

impl ChordInstance {
    pub fn new(
        root: RootNotation,
        full_symbol: String,
        parsed: Chord,
        rhythm: ChordRhythm,
        original_token: String,
        duration: MusicalDuration,
        position: AbsolutePosition,
    ) -> Self {
        Self {
            root,
            full_symbol,
            parsed,
            rhythm,
            original_token,
            duration,
            position,
            push_pull: None,
            commands: Vec::new(),
            source_span: None,
        }
    }

    pub fn with_push_pull(mut self, push_pull: Option<(bool, PushPullAmount)>) -> Self {
        self.push_pull = push_pull;
        self
    }

    pub fn with_commands(mut self, commands: Vec<Command>) -> Self {
        self.commands = commands;
        self
    }

    pub fn add_command(mut self, command: Command) -> Self {
        self.commands.push(command);
        self
    }

    /// Set the source text span for this chord
    pub fn with_source_span(mut self, span: TextSpan) -> Self {
        self.source_span = Some(span);
        self
    }

    /// Convert this chord instance to LilyPond chordmode notation
    ///
    /// # Arguments
    /// * `key` - Optional key context for resolving scale degrees and roman numerals
    ///
    /// # Returns
    /// LilyPond chord notation with optional duration (e.g., "cis:maj74", "des:m78", "c:maj72.")
    /// In LilyPond, durations are appended directly: c4, c2, c1, c4. (for dotted)
    pub fn to_lilypond(&self, key: Option<&crate::key::Key>) -> String {
        // Convert chord to LilyPond format
        let chord_str = self.parsed.to_lilypond(key);

        // Add duration if specified
        // In LilyPond chordmode, duration comes directly after the chord: c4, c2, c1, c4.
        if let Some(duration) = self.rhythm_to_lilypond_duration() {
            format!("{}{}", chord_str, duration)
        } else {
            chord_str
        }
    }

    /// Convert rhythm to LilyPond duration notation
    /// Returns duration string that can be appended directly to chord (e.g., "4", "2", "1", "4.")
    fn rhythm_to_lilypond_duration(&self) -> Option<String> {
        use crate::chord::LilySyntax;

        // Use the lily_parts() method to extract duration info
        if let Some((duration, dotted, _triplet)) = self.rhythm.lily_parts() {
            let dur_str = match duration {
                LilySyntax::Whole => "1",
                LilySyntax::Half => "2",
                LilySyntax::Quarter => "4",
                LilySyntax::Eighth => "8",
                LilySyntax::Sixteenth => "16",
                LilySyntax::ThirtySecond => "32",
            };
            if dotted {
                // Dotted duration: "4." not ".4"
                Some(format!("{}.", dur_str))
            } else {
                Some(dur_str.to_string())
            }
        } else {
            None
        }
    }
}

/// Represents a rhythm slash (stemless notehead indicating a beat)
///
/// Used in lead sheet notation to show rhythm without specific pitches.
/// Slashes are generated for beats that don't have explicit chords or rests.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct RhythmSlash {
    /// Beat number within the measure (0-indexed)
    pub beat: u8,

    /// Position within the song (for rendering)
    pub position: AbsolutePosition,
}

impl RhythmSlash {
    /// Create a new rhythm slash at the given beat
    pub fn new(beat: u8, position: AbsolutePosition) -> Self {
        Self { beat, position }
    }
}

/// Represents a rest in the rhythm (no chord, just silence)
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct RestInstance {
    /// Rhythm notation (duration, triplet, etc.)
    pub rhythm: ChordRhythm,

    /// Duration in measure.beats.subdivision format
    pub duration: MusicalDuration,

    /// Position in the song
    pub position: AbsolutePosition,

    /// Original token (e.g., "r8t", "r4", "r2")
    pub original_token: String,
}

impl RestInstance {
    /// Create a new rest instance
    pub fn new(
        rhythm: ChordRhythm,
        duration: MusicalDuration,
        position: AbsolutePosition,
        original_token: String,
    ) -> Self {
        Self {
            rhythm,
            duration,
            position,
            original_token,
        }
    }
}

/// Represents a space in the rhythm (invisible placeholder - measure will be filled with auto slashes)
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct SpaceInstance {
    /// Rhythm notation (duration, triplet, etc.)
    pub rhythm: ChordRhythm,

    /// Duration in measure.beats.subdivision format
    pub duration: MusicalDuration,

    /// Position in the song
    pub position: AbsolutePosition,

    /// Original token (e.g., "s1", "s2", "s4")
    pub original_token: String,
}

impl SpaceInstance {
    /// Create a new space instance
    pub fn new(
        rhythm: ChordRhythm,
        duration: MusicalDuration,
        position: AbsolutePosition,
        original_token: String,
    ) -> Self {
        Self {
            rhythm,
            duration,
            position,
            original_token,
        }
    }
}

/// A rhythm element - a chord, rest, or space
///
/// This represents a single element in a measure's rhythm pattern.
/// Measures with explicit rhythm notation (like `r8t Ab9_8t r8t r4t F9_8t r2`)
/// use this to preserve both chords and rests in their written order.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum RhythmElement {
    /// A chord with rhythm
    Chord(ChordInstance),
    /// A rest (silence)
    Rest(RestInstance),
    /// A space (invisible placeholder - triggers auto-fill with slashes)
    Space(SpaceInstance),
}

impl RhythmElement {
    /// Get the duration of this element
    pub fn duration(&self) -> &MusicalDuration {
        match self {
            RhythmElement::Chord(c) => &c.duration,
            RhythmElement::Rest(r) => &r.duration,
            RhythmElement::Space(s) => &s.duration,
        }
    }

    /// Get the rhythm notation for this element
    pub fn rhythm(&self) -> &ChordRhythm {
        match self {
            RhythmElement::Chord(c) => &c.rhythm,
            RhythmElement::Rest(r) => &r.rhythm,
            RhythmElement::Space(s) => &s.rhythm,
        }
    }

    /// Check if this element is a rest
    pub fn is_rest(&self) -> bool {
        matches!(self, RhythmElement::Rest(_))
    }

    /// Check if this element is a chord
    pub fn is_chord(&self) -> bool {
        matches!(self, RhythmElement::Chord(_))
    }

    /// Get the position of this element
    pub fn position(&self) -> &AbsolutePosition {
        match self {
            RhythmElement::Chord(c) => &c.position,
            RhythmElement::Rest(r) => &r.position,
            RhythmElement::Space(s) => &s.position,
        }
    }

    /// Check if this element is a space (invisible placeholder)
    pub fn is_space(&self) -> bool {
        matches!(self, RhythmElement::Space(_))
    }

    /// Get a reference to the inner ChordInstance if this is a chord.
    pub fn as_chord(&self) -> Option<&ChordInstance> {
        match self {
            RhythmElement::Chord(c) => Some(c),
            _ => None,
        }
    }

    /// Get a reference to the inner RestInstance if this is a rest.
    pub fn as_rest(&self) -> Option<&RestInstance> {
        match self {
            RhythmElement::Rest(r) => Some(r),
            _ => None,
        }
    }

    /// Get a reference to the inner SpaceInstance if this is a space.
    pub fn as_space(&self) -> Option<&SpaceInstance> {
        match self {
            RhythmElement::Space(s) => Some(s),
            _ => None,
        }
    }
}

/// Represents a section with its tracks
///
/// A section can have multiple tracks running in parallel (chords, melody, rhythm, etc.)
/// The default track type is Chords, which maintains backward compatibility.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct ChartSection {
    /// Section information (type, number, etc.)
    pub section: Section,

    /// All tracks in this section (chords, melody, rhythm, etc.)
    pub tracks: Vec<Track>,

    /// True if this section was recalled from a template
    pub from_template: bool,

    /// Source text span for this section header
    /// Links this section back to the original input text that generated it.
    pub source_span: Option<TextSpan>,

    /// If from template, the span of the original template definition
    pub template_span: Option<TextSpan>,
}

impl ChartSection {
    pub fn new(section: Section) -> Self {
        Self {
            section,
            tracks: Vec::new(),
            from_template: false,
            source_span: None,
            template_span: None,
        }
    }

    /// Create a section with a default chord track containing the given measures
    pub fn with_measures(mut self, measures: Vec<Measure>) -> Self {
        // Create or update the chord track
        if let Some(chord_track) = self.chord_track_mut() {
            chord_track.measures = measures;
        } else {
            self.tracks.push(Track::chords(measures));
        }
        self
    }

    /// Add a track to this section
    pub fn with_track(mut self, track: Track) -> Self {
        self.tracks.push(track);
        self
    }

    /// Set the source text span for this section
    pub fn with_source_span(mut self, span: TextSpan) -> Self {
        self.source_span = Some(span);
        self
    }

    /// Set the template span (for sections recalled from templates)
    pub fn with_template_span(mut self, span: TextSpan) -> Self {
        self.template_span = Some(span);
        self
    }

    /// Create a section from a template recall
    pub fn from_template(section: Section, measures: Vec<Measure>) -> Self {
        Self {
            section,
            tracks: vec![Track::chords(measures)],
            from_template: true,
            source_span: None,
            template_span: None,
        }
    }

    /// Create a section from a template with full span information
    pub fn from_template_with_spans(
        section: Section,
        measures: Vec<Measure>,
        source_span: TextSpan,
        template_span: TextSpan,
    ) -> Self {
        Self {
            section,
            tracks: vec![Track::chords(measures)],
            from_template: true,
            source_span: Some(source_span),
            template_span: Some(template_span),
        }
    }

    // ==================== Backward Compatibility Methods ====================

    /// Get the primary chord track (for backward compatibility)
    pub fn chord_track(&self) -> Option<&Track> {
        self.tracks
            .iter()
            .find(|t| t.track_type == TrackType::Chords)
    }

    /// Get mutable reference to the primary chord track
    pub fn chord_track_mut(&mut self) -> Option<&mut Track> {
        self.tracks
            .iter_mut()
            .find(|t| t.track_type == TrackType::Chords)
    }

    /// Get all measures from the chord track (backward compatibility)
    ///
    /// Most existing code accesses `section.measures` - this provides
    /// the same interface via method call.
    pub fn measures(&self) -> &[Measure] {
        self.chord_track()
            .map(|t| t.measures.as_slice())
            .unwrap_or(&[])
    }

    /// Get mutable reference to measures in the chord track
    pub fn measures_mut(&mut self) -> &mut Vec<Measure> {
        // Ensure we have a chord track
        if self.chord_track().is_none() {
            self.tracks.push(Track::chords(Vec::new()));
        }
        &mut self.chord_track_mut().unwrap().measures
    }

    /// Get melody tracks
    pub fn melody_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks
            .iter()
            .filter(|t| t.track_type == TrackType::Melody)
    }

    /// Get rhythm tracks
    pub fn rhythm_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks
            .iter()
            .filter(|t| t.track_type == TrackType::Rhythm)
    }

    /// Add a melody track to this section
    pub fn add_melody_track(&mut self, melody: Melody, name: Option<String>) {
        let mut track = Track::melody(melody);
        if let Some(n) = name {
            track = track.with_name(n);
        }
        self.tracks.push(track);
    }
}

/// Represents a key change event in the chart
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct KeyChange {
    /// Position where the key change occurs
    pub position: AbsolutePosition,

    /// The key before the change (None if this is the initial key)
    pub from_key: Option<crate::key::Key>,

    /// The new key after the change
    pub to_key: crate::key::Key,

    /// Section index where this occurs
    pub section_index: usize,
}

impl KeyChange {
    pub fn new(
        position: AbsolutePosition,
        from_key: Option<crate::key::Key>,
        to_key: crate::key::Key,
        section_index: usize,
    ) -> Self {
        Self {
            position,
            from_key,
            to_key,
            section_index,
        }
    }
}

/// Represents a time signature change at a specific position
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct TimeSignatureChange {
    /// Position where the time signature change occurs
    pub position: AbsolutePosition,

    /// The new time signature
    pub time_signature: crate::time::TimeSignature,

    /// Section index where this occurs
    pub section_index: usize,
}

impl TimeSignatureChange {
    pub fn new(
        position: AbsolutePosition,
        time_signature: crate::time::TimeSignature,
        section_index: usize,
    ) -> Self {
        Self {
            position,
            time_signature,
            section_index,
        }
    }
}

/// Represents a tempo change at a specific position
///
/// Tempo changes are indicated with arrow notation: `->140bpm` or `->120`
/// They can be placed inline with chords or on their own line.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct TempoChange {
    /// Position where the tempo change occurs
    pub position: AbsolutePosition,

    /// The tempo before the change (None if this is at the very start)
    pub from_tempo: Option<crate::time::Tempo>,

    /// The new tempo after the change
    pub to_tempo: crate::time::Tempo,

    /// Section index where this occurs
    pub section_index: usize,
}

impl TempoChange {
    pub fn new(
        position: AbsolutePosition,
        from_tempo: Option<crate::time::Tempo>,
        to_tempo: crate::time::Tempo,
        section_index: usize,
    ) -> Self {
        Self {
            position,
            from_tempo,
            to_tempo,
            section_index,
        }
    }

    /// Parse a tempo change from a string
    ///
    /// Format: `->NNNbpm` or `->NNN`
    /// Examples:
    /// - `->140bpm` - change to 140 BPM
    /// - `->120` - change to 120 BPM
    pub fn parse_syntax(s: &str) -> Option<crate::time::Tempo> {
        let s = s.trim();

        if !s.starts_with("->") {
            return None;
        }

        let tempo_part = s[2..].trim();

        // Remove optional "bpm" suffix
        let bpm_str = tempo_part
            .strip_suffix("bpm")
            .or_else(|| tempo_part.strip_suffix("BPM"))
            .unwrap_or(tempo_part);

        bpm_str
            .parse::<u16>()
            .ok()
            .map(|bpm| crate::time::Tempo::from_bpm(f64::from(bpm)))
    }
}
