//! Core Chart Types
//!
//! Defines the core data structures for chart representation

use super::chord_syllable_alignment::SectionAlignment;
use super::commands::Command;
use super::cues::TextCue;
use super::dynamics::DynamicMarking;
use super::melody::Melody;
use super::notations::{
    BarlineStyle, Dynamic, FiguredBass, Hairpin, RepeatMark, StaffText, SuspensionFigure, Volta,
};
use super::track::{Track, TrackType};
use crate::chord::{Chord, ChordRhythm, PushPullAmount};
use crate::key::Key;
use crate::parsing::TextSpan;
use crate::primitives::RootNotation;
use crate::sections::Section;
use crate::time::{AbsolutePosition, MusicalDuration, MusicalPosition};
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

    /// Verbatim display text to render instead of the computed chord symbol.
    /// Used by floating slash-bass tokens like `/D`, which inherit the prior
    /// chord's root harmonically (`Bb/D`) but should print just `/D`.
    pub display_override: Option<String>,
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
            display_override: None,
        }
    }

    /// Set verbatim display text (e.g. `/D` for a floating slash-bass chord).
    pub fn with_display_override(mut self, text: impl Into<String>) -> Self {
        self.display_override = Some(text.into());
        self
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

    /// Resolve a key-relative chord (written as a Nashville number or Roman
    /// numeral) to its absolute letter-name symbol in `key` — e.g. `5maj7` in
    /// C major → `Gmaj7`, `V7` in C → `G7`.
    ///
    /// Returns `None` for chords already written as note names (nothing to
    /// resolve) or whose root doesn't resolve against `key`. Resolution swaps
    /// the chord's root for the absolute note and re-renders via `Chord`'s
    /// `Display`, so quality / extensions / bass survive unchanged.
    ///
    /// Callers that span key changes should pass the key in effect at this
    /// chord's position (see [`Chart::key_at_position`]) rather than a single
    /// chart-level key.
    ///
    /// [`Chart::key_at_position`]: crate::chart::Chart::key_at_position
    #[must_use]
    pub fn resolved_symbol(&self, key: &Key) -> Option<String> {
        if !self.root.is_key_relative() {
            return None;
        }
        let note = self.parsed.root.resolve(Some(key))?;
        let mut absolute = self.parsed.clone();
        absolute.root = RootNotation::from_note_name(note);
        Some(absolute.to_string())
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

/// Represents a single measure with chords
///
/// A measure is ALWAYS 1.0.0 in duration (one measure).
/// The time signature defines how many beats are in that measure.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Measure {
    /// Chords in this measure (legacy field for backward compatibility)
    pub chords: Vec<ChordInstance>,

    /// Complete rhythm elements in this measure (chords and rests in order)
    ///
    /// This field preserves the full rhythm pattern as written, including rests.
    /// For example, `r8t Ab9_8t r8t r4t F9_8t r2` stores 6 rhythm elements:
    /// - Rest(eighth triplet)
    /// - Chord(Ab9, eighth triplet)
    /// - Rest(eighth triplet)
    /// - Rest(quarter triplet)
    /// - Chord(F9, eighth triplet)
    /// - Rest(half)
    ///
    /// When this field is non-empty, the engraver should use it instead of
    /// generating slash fills from the `chords` field.
    pub rhythm_elements: Vec<RhythmElement>,

    /// Rhythm slashes for beats without explicit chords
    /// Generated by `generate_rhythm_slashes()` based on chord positions
    pub rhythm_slashes: Vec<RhythmSlash>,

    /// Time signature for this measure (defines beats per measure)
    pub time_signature: (u8, u8),

    /// Repeat count (1 = no repeat, 2+ = repeat this measure n times)
    pub repeat_count: usize,

    /// Text cues for instrument directions
    pub text_cues: Vec<TextCue>,

    /// Dynamic/intensity markings ([Build], [Down], [Go Crazy], etc.)
    pub dynamics: Vec<DynamicMarking>,

    /// Classical dynamic markings (mp/mf/f/ff/fff with SMuFL glyphs).
    pub classical_dynamics: Vec<Dynamic>,

    /// Crescendo / decrescendo hairpins anchored to this measure.
    pub hairpins: Vec<Hairpin>,

    /// Stacked-numeral figured-bass annotations anchored to a beat.
    pub figured_bass: Vec<FiguredBass>,

    /// Suspension/resolution figures (`4-3`, `2-3`, `3`) anchored to a beat.
    pub suspensions: Vec<SuspensionFigure>,

    /// Free-form text directions attached to a beat (`StaffText`).
    pub staff_text: Vec<StaffText>,

    /// Volta (1st/2nd ending) starting at this measure, if any.
    pub volta_start: Option<Volta>,

    /// Style of the barline that closes this measure.
    pub end_barline: BarlineStyle,

    /// Repeat decoration on the barline that opens this measure.
    pub start_repeat: RepeatMark,

    /// Repeat decoration on the barline that closes this measure.
    pub end_repeat: RepeatMark,

    /// Melodies in this measure (inline m{} blocks)
    pub melodies: Vec<Melody>,

    /// Source text span for this measure
    /// Links this measure back to the original input text that generated it.
    pub source_span: Option<TextSpan>,

    /// Source measure number when imported from MusicXML — the value of
    /// `<measure number="...">`. `None` for keyflow-text-authored charts.
    /// Useful for asserting that count-in measures keep their original
    /// numbering (musical measure 1 == xml measure 3 in Lord of the Fight).
    pub source_measure_number: Option<u32>,

    /// Source measure width from MusicXML's `<measure width="...">`, in
    /// tenths. This is a layout hint from the exporting notation program;
    /// the engraver may use it to preserve source-relative spacing.
    pub source_measure_width: Option<f64>,
}

impl Measure {
    pub fn new() -> Self {
        Self {
            chords: Vec::new(),
            rhythm_elements: Vec::new(),
            rhythm_slashes: Vec::new(),
            time_signature: (4, 4), // Default to 4/4
            repeat_count: 1,        // Default to no repeat
            text_cues: Vec::new(),
            dynamics: Vec::new(),
            classical_dynamics: Vec::new(),
            hairpins: Vec::new(),
            figured_bass: Vec::new(),
            suspensions: Vec::new(),
            staff_text: Vec::new(),
            volta_start: None,
            end_barline: BarlineStyle::default(),
            start_repeat: RepeatMark::default(),
            end_repeat: RepeatMark::default(),
            melodies: Vec::new(),
            source_span: None,
            source_measure_number: None,
            source_measure_width: None,
        }
    }

    /// Check if this measure has explicit rhythm elements (chords and rests)
    pub fn has_explicit_rhythm(&self) -> bool {
        !self.rhythm_elements.is_empty()
    }

    pub fn with_chords(mut self, chords: Vec<ChordInstance>) -> Self {
        self.chords = chords;
        self
    }

    pub fn with_time_signature(mut self, time_signature: (u8, u8)) -> Self {
        self.time_signature = time_signature;
        self
    }

    /// Set the source text span for this measure
    pub fn with_source_span(mut self, span: TextSpan) -> Self {
        self.source_span = Some(span);
        self
    }

    /// Get the duration of this measure (always 1.0.0)
    pub fn duration(&self) -> MusicalDuration {
        MusicalDuration::new(1, 0, 0)
    }

    /// Get the number of beats in this measure based on time signature
    pub fn beats_per_measure(&self) -> u8 {
        self.time_signature.0
    }

    /// Generate rhythm slashes for beats that don't have explicit chords or rests.
    ///
    /// This fills in the gaps between chords with slash noteheads, which is
    /// standard lead sheet notation. Beats with explicit rests are not filled.
    ///
    /// # Arguments
    /// * `measure_number` - The measure number (0-indexed) for position tracking
    /// * `section_index` - The section index for position tracking
    pub fn generate_rhythm_slashes(&mut self, measure_number: i32, section_index: usize) {
        use crate::chord::ChordRhythm;

        let beats = self.beats_per_measure();
        let mut suppressed_beats: Vec<bool> = vec![false; beats as usize];

        // Only suppress slashes for explicit rests or explicit slash notation
        // Chord symbols don't suppress slashes - they appear above the staff
        for chord in &self.chords {
            let chord_beat = chord.position.total_duration.beat.max(0) as usize;

            // Explicit rests suppress slashes
            if chord.rhythm.is_rest() {
                if chord_beat < beats as usize {
                    suppressed_beats[chord_beat] = true;
                }
            } else if let ChordRhythm::Slashes { count, .. } = &chord.rhythm {
                // Explicit slash notation (G////) means rhythm is already notated
                for i in 0..(*count as usize) {
                    let beat_idx = chord_beat + i;
                    if beat_idx < beats as usize {
                        suppressed_beats[beat_idx] = true;
                    }
                }
            }
            // Default chord symbols don't suppress - we generate slashes for all beats
        }

        // Generate slashes for all beats except suppressed ones
        self.rhythm_slashes.clear();
        for beat in 0..beats {
            if !suppressed_beats[beat as usize] {
                let position = AbsolutePosition::new(
                    MusicalPosition::new(measure_number, beat as i32, 0),
                    section_index,
                );
                self.rhythm_slashes.push(RhythmSlash::new(beat, position));
            }
        }
    }

    /// Check if rhythm slashes have been generated
    pub fn has_rhythm_slashes(&self) -> bool {
        !self.rhythm_slashes.is_empty()
    }
}

impl Default for Measure {
    fn default() -> Self {
        Self::new()
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

    /// Chord-syllable alignment (computed in post-processing when both chords and lyrics exist)
    pub alignment: Option<SectionAlignment>,

    /// True if this section was synthesized by the parser because chord content
    /// appeared with no section header written (e.g. a bare `1 4 6 5` chart).
    /// Such sections have no header text, so the engraver must not draw a
    /// section label for them and the editor must not place a name badge.
    pub implicit: bool,
}

impl ChartSection {
    pub fn new(section: Section) -> Self {
        Self {
            section,
            tracks: Vec::new(),
            from_template: false,
            source_span: None,
            template_span: None,
            alignment: None,
            implicit: false,
        }
    }

    /// Mark this section as implicit (no section header was written for it).
    #[must_use]
    pub fn as_implicit(mut self) -> Self {
        self.implicit = true;
        self
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
            alignment: None,
            implicit: false,
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
            alignment: None,
            implicit: false,
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

    /// Get the first lyrics track (if any)
    pub fn lyrics_track(&self) -> Option<&Track> {
        self.tracks
            .iter()
            .find(|t| t.track_type == TrackType::Lyrics)
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
