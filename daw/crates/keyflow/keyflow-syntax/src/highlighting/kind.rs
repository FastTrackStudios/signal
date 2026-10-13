//! Highlight kind definitions and span types.
//!
//! Defines the semantic categories for syntax highlighting and the
//! structure for representing highlighted regions in source text.

use crate::parsing::TextSpan;
use facet::Facet;
use serde::{Deserialize, Serialize};

/// A highlighted region in source text.
///
/// Combines a source span with its semantic highlight kind,
/// enabling renderers to apply appropriate styling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct HighlightSpan {
    /// The source text region this highlight covers
    pub span: TextSpan,
    /// The semantic category of this region
    pub kind: HighlightKind,
}

impl HighlightSpan {
    /// Create a new highlight span.
    #[must_use]
    pub const fn new(span: TextSpan, kind: HighlightKind) -> Self {
        Self { span, kind }
    }

    /// Create a highlight span from byte offsets.
    #[must_use]
    pub const fn from_range(start: usize, len: usize, kind: HighlightKind) -> Self {
        Self {
            span: TextSpan::new(start, len),
            kind,
        }
    }

    /// Get the byte range this span covers.
    #[must_use]
    pub fn as_range(&self) -> std::ops::Range<usize> {
        self.span.as_range()
    }

    /// Extract the source text for this span.
    #[must_use]
    pub fn extract<'a>(&self, source: &'a str) -> Option<&'a str> {
        self.span.extract(source)
    }
}

/// Semantic categories for syntax highlighting.
///
/// Each variant represents a distinct semantic element in Keyflow notation,
/// allowing themes to style different parts of the chart appropriately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum HighlightKind {
    // ==================== Chord Components ====================
    /// Root note of a chord (G, C#, Bb)
    Root,

    /// Scale degree root (1, 2, 3, 4, 5, 6, 7)
    ScaleDegree,

    /// Roman numeral root (I, IV, V, vi, ii)
    RomanNumeral,

    /// Accidental symbol (#, b, ##, bb)
    Accidental,

    /// Quality marker (maj, m, dim, aug, sus)
    Quality,

    /// Extension number (7, 9, 11, 13)
    Extension,

    /// Chord modifier (b5, #11, add9, no3)
    Modifier,

    /// Slash chord bass note (/E, /G)
    Bass,

    /// Bass slash separator (/)
    BassSlash,

    // ==================== Rhythm Notation ====================
    /// Lily-style duration (_8, _4, _16)
    Duration,

    /// Slash rhythm notation (/, //, ///, ////)
    SlashRhythm,

    /// Rest notation (r4, r8)
    Rest,

    /// Space/tacet notation (s4, s8)
    Space,

    /// Push marker (')
    Push,

    /// Pull marker (')
    Pull,

    /// Triplet marker (t)
    Triplet,

    /// Duration dots (.)
    Dot,

    // ==================== Document Structure ====================
    /// Section type keyword (vs, ch, intro, bridge)
    Section,

    /// Section measure count (4, 8, +1)
    MeasureCount,

    /// Section comment/annotation ("Build", "Down")
    SectionComment,

    /// Custom section brackets ([Hits], [SOLO Keys])
    SectionBracket,

    /// Measure separator (|)
    MeasureSeparator,

    // ==================== Metadata ====================
    /// Song title
    Title,

    /// Artist name
    Artist,

    /// Tempo value (120bpm)
    Tempo,

    /// Time signature (4/4, 6/8)
    TimeSignature,

    /// Key signature (#G, bEb)
    Key,

    /// Tempo change arrow (->)
    TempoArrow,

    // ==================== Commands and Annotations ====================
    /// Command marker (@fermata, @accent)
    Command,

    /// Dynamic marking ([Build], [Down])
    Dynamic,

    /// Text cue (instrument directions)
    TextCue,

    // ==================== Other ====================
    /// Comment text (; comment)
    Comment,

    /// Comment prefix character (;)
    CommentMarker,

    /// Memory recall (%, %%)
    MemoryRecall,

    /// Repeat marker
    Repeat,

    /// Track marker ([Chords], [Melody])
    TrackMarker,

    /// Melody inline block (m{...})
    MelodyBlock,

    /// Whitespace (for preserving formatting)
    Whitespace,

    /// Unknown or unparseable content
    Unknown,
}

impl HighlightKind {
    /// Get the CSS class name for this highlight kind.
    #[must_use]
    pub const fn css_class(&self) -> &'static str {
        match self {
            Self::Root => "kf-root",
            Self::ScaleDegree => "kf-degree",
            Self::RomanNumeral => "kf-roman",
            Self::Accidental => "kf-accidental",
            Self::Quality => "kf-quality",
            Self::Extension => "kf-extension",
            Self::Modifier => "kf-modifier",
            Self::Bass => "kf-bass",
            Self::BassSlash => "kf-bass-slash",
            Self::Duration => "kf-duration",
            Self::SlashRhythm => "kf-slash-rhythm",
            Self::Rest => "kf-rest",
            Self::Space => "kf-space",
            Self::Push => "kf-push",
            Self::Pull => "kf-pull",
            Self::Triplet => "kf-triplet",
            Self::Dot => "kf-dot",
            Self::Section => "kf-section",
            Self::MeasureCount => "kf-measure-count",
            Self::SectionComment => "kf-section-comment",
            Self::SectionBracket => "kf-section-bracket",
            Self::MeasureSeparator => "kf-measure-sep",
            Self::Title => "kf-title",
            Self::Artist => "kf-artist",
            Self::Tempo => "kf-tempo",
            Self::TimeSignature => "kf-time-sig",
            Self::Key => "kf-key",
            Self::TempoArrow => "kf-tempo-arrow",
            Self::Command => "kf-command",
            Self::Dynamic => "kf-dynamic",
            Self::TextCue => "kf-text-cue",
            Self::Comment => "kf-comment",
            Self::CommentMarker => "kf-comment-marker",
            Self::MemoryRecall => "kf-memory-recall",
            Self::Repeat => "kf-repeat",
            Self::TrackMarker => "kf-track-marker",
            Self::MelodyBlock => "kf-melody-block",
            Self::Whitespace => "kf-ws",
            Self::Unknown => "kf-unknown",
        }
    }

    /// Get a human-readable description of this highlight kind.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::Root => "chord root note",
            Self::ScaleDegree => "scale degree",
            Self::RomanNumeral => "roman numeral",
            Self::Accidental => "accidental",
            Self::Quality => "chord quality",
            Self::Extension => "chord extension",
            Self::Modifier => "chord modifier",
            Self::Bass => "bass note",
            Self::BassSlash => "bass slash",
            Self::Duration => "duration",
            Self::SlashRhythm => "slash rhythm",
            Self::Rest => "rest",
            Self::Space => "space",
            Self::Push => "push (anticipation)",
            Self::Pull => "pull (delay)",
            Self::Triplet => "triplet marker",
            Self::Dot => "duration dot",
            Self::Section => "section type",
            Self::MeasureCount => "measure count",
            Self::SectionComment => "section comment",
            Self::SectionBracket => "section bracket",
            Self::MeasureSeparator => "measure separator",
            Self::Title => "song title",
            Self::Artist => "artist name",
            Self::Tempo => "tempo",
            Self::TimeSignature => "time signature",
            Self::Key => "key signature",
            Self::TempoArrow => "tempo change",
            Self::Command => "command",
            Self::Dynamic => "dynamic marking",
            Self::TextCue => "text cue",
            Self::Comment => "comment",
            Self::CommentMarker => "comment marker",
            Self::MemoryRecall => "memory recall",
            Self::Repeat => "repeat marker",
            Self::TrackMarker => "track marker",
            Self::MelodyBlock => "melody block",
            Self::Whitespace => "whitespace",
            Self::Unknown => "unknown",
        }
    }

    /// Check if this kind represents a chord component.
    #[must_use]
    pub const fn is_chord_component(&self) -> bool {
        matches!(
            self,
            Self::Root
                | Self::ScaleDegree
                | Self::RomanNumeral
                | Self::Accidental
                | Self::Quality
                | Self::Extension
                | Self::Modifier
                | Self::Bass
                | Self::BassSlash
        )
    }

    /// Check if this kind represents rhythm notation.
    #[must_use]
    pub const fn is_rhythm(&self) -> bool {
        matches!(
            self,
            Self::Duration
                | Self::SlashRhythm
                | Self::Rest
                | Self::Space
                | Self::Push
                | Self::Pull
                | Self::Triplet
                | Self::Dot
        )
    }

    /// Check if this kind represents document structure.
    #[must_use]
    pub const fn is_structure(&self) -> bool {
        matches!(
            self,
            Self::Section
                | Self::MeasureCount
                | Self::SectionComment
                | Self::SectionBracket
                | Self::MeasureSeparator
        )
    }

    /// Check if this kind represents metadata.
    #[must_use]
    pub const fn is_metadata(&self) -> bool {
        matches!(
            self,
            Self::Title
                | Self::Artist
                | Self::Tempo
                | Self::TimeSignature
                | Self::Key
                | Self::TempoArrow
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_span_creation() {
        let span = HighlightSpan::new(TextSpan::new(0, 5), HighlightKind::Root);
        assert_eq!(span.span.start, 0);
        assert_eq!(span.span.len, 5);
        assert_eq!(span.kind, HighlightKind::Root);
    }

    #[test]
    fn test_highlight_span_from_range() {
        let span = HighlightSpan::from_range(10, 3, HighlightKind::Quality);
        assert_eq!(span.span.start, 10);
        assert_eq!(span.span.len, 3);
        assert_eq!(span.kind, HighlightKind::Quality);
    }

    #[test]
    fn test_highlight_span_extract() {
        let source = "Gmaj7";
        let span = HighlightSpan::from_range(1, 3, HighlightKind::Quality);
        assert_eq!(span.extract(source), Some("maj"));
    }

    #[test]
    fn test_css_classes() {
        assert_eq!(HighlightKind::Root.css_class(), "kf-root");
        assert_eq!(HighlightKind::Quality.css_class(), "kf-quality");
        assert_eq!(HighlightKind::Extension.css_class(), "kf-extension");
        assert_eq!(HighlightKind::Section.css_class(), "kf-section");
    }

    #[test]
    fn test_kind_categories() {
        assert!(HighlightKind::Root.is_chord_component());
        assert!(HighlightKind::Quality.is_chord_component());
        assert!(!HighlightKind::Section.is_chord_component());

        assert!(HighlightKind::Duration.is_rhythm());
        assert!(HighlightKind::Push.is_rhythm());
        assert!(!HighlightKind::Root.is_rhythm());

        assert!(HighlightKind::Section.is_structure());
        assert!(HighlightKind::MeasureSeparator.is_structure());
        assert!(!HighlightKind::Root.is_structure());

        assert!(HighlightKind::Title.is_metadata());
        assert!(HighlightKind::Tempo.is_metadata());
        assert!(!HighlightKind::Root.is_metadata());
    }
}
