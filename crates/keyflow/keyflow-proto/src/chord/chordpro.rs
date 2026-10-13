//! ChordPro format support — **legacy compact view**.
//!
//! These types are now a *projection* of the comprehensive ChordPro 6.07
//! AST defined in the dedicated `keyflow-chordpro` crate. They are kept as
//! a stable target for existing callers, but new code should depend on
//! `keyflow-chordpro` directly to access:
//!
//! - typed `DirectiveKind` (covers every directive in the cheat sheet)
//! - `[*annotation]` markers
//! - `{define}` chord definitions with frets / fingers / keys
//! - conditional directives (`{title-en: …}`)
//! - line continuation with `\`
//! - `\uXXXX` escape expansion
//! - source spans for editor / LSP integration
//!
//! See `keyflow_text::chart::parser::parse_chordpro` for the bridge.

use facet::Facet;

/// A complete ChordPro document
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct ChordProDocument {
    /// Global directives (title, artist, key, etc.)
    pub directives: Vec<ChordProDirective>,

    /// Sections of lyrics with chords
    pub sections: Vec<ChordProSection>,
}

impl ChordProDocument {
    /// Create a new ChordPro document
    pub fn new() -> Self {
        Self {
            directives: Vec::new(),
            sections: Vec::new(),
        }
    }

    /// Add a directive
    pub fn with_directive(mut self, directive: ChordProDirective) -> Self {
        self.directives.push(directive);
        self
    }

    /// Add a section
    pub fn with_section(mut self, section: ChordProSection) -> Self {
        self.sections.push(section);
        self
    }

    /// Find a directive by name (case-insensitive)
    pub fn find_directive(&self, name: &str) -> Option<&ChordProDirective> {
        self.directives
            .iter()
            .find(|d| d.name.eq_ignore_ascii_case(name))
    }
}

impl Default for ChordProDocument {
    fn default() -> Self {
        Self::new()
    }
}

/// A ChordPro directive (metadata)
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct ChordProDirective {
    /// Directive name (e.g., "title", "artist", "key")
    pub name: String,

    /// Directive value
    pub value: String,
}

impl ChordProDirective {
    /// Create a new directive
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    /// Create a title directive
    pub fn title(value: impl Into<String>) -> Self {
        Self::new("title", value)
    }

    /// Create an artist directive
    pub fn artist(value: impl Into<String>) -> Self {
        Self::new("artist", value)
    }

    /// Create a key directive
    pub fn key(value: impl Into<String>) -> Self {
        Self::new("key", value)
    }
}

/// A section in ChordPro format (verse, chorus, etc.)
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct ChordProSection {
    /// Optional section label (e.g., "Verse", "Chorus")
    pub label: Option<String>,

    /// Lines of content in this section
    pub lines: Vec<ChordProLine>,
}

impl ChordProSection {
    /// Create a new section
    pub fn new() -> Self {
        Self {
            label: None,
            lines: Vec::new(),
        }
    }

    /// Set the label
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Add a line
    pub fn with_line(mut self, line: ChordProLine) -> Self {
        self.lines.push(line);
        self
    }
}

impl Default for ChordProSection {
    fn default() -> Self {
        Self::new()
    }
}

/// A single line in a ChordPro section
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum ChordProLine {
    /// Chord-over-lyric line: `[C]Text [G]more text`
    Lyric(Vec<ChordProChunk>),

    /// Pure chord line (no lyrics)
    ChordOnly(Vec<ChordProChunk>),

    /// Comment line
    Comment(String),

    /// Empty line
    Empty,
}

/// A chunk of text with an optional leading chord
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct ChordProChunk {
    /// Optional chord symbol (e.g., "C", "G7", "Am")
    pub chord: Option<String>,

    /// Lyric text that follows the chord
    pub text: String,
}

impl ChordProChunk {
    /// Create a chunk with text only (no chord)
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            chord: None,
            text: text.into(),
        }
    }

    /// Create a chunk with chord and text
    pub fn with_chord(chord: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            chord: Some(chord.into()),
            text: text.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chordpro_document() {
        let doc = ChordProDocument::new()
            .with_directive(ChordProDirective::title("Amazing Grace"))
            .with_directive(ChordProDirective::artist("Traditional"));

        assert_eq!(doc.directives.len(), 2);
        assert!(doc.find_directive("title").is_some());
    }

    #[test]
    fn test_chordpro_chunk() {
        let chunk = ChordProChunk::with_chord("C", "Verse");
        assert_eq!(chunk.chord, Some("C".to_string()));
        assert_eq!(chunk.text, "Verse");
    }
}
