//! Text cues for instrument-specific directions
//!
//! Provides a system for adding notes and directions to specific instrument groups
//! using @ notation (e.g., @keys "synth here", @drums "crash on 3")
//!
//! Beat-specific cues can be specified with colon notation:
//! - `@keys:2 "hit"` - cue at beat 2
//! - `@drums:3 "crash"` - cue at beat 3

use crate::time::AbsolutePosition;
use facet::Facet;
use std::fmt;

/// Represents an instrument group that can be targeted by cues
#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum InstrumentGroup {
    All,
    Keys,
    Drums,
    Bass,
    Guitar,
    Vocals,
    Custom(String),
}

impl InstrumentGroup {
    /// Parse an instrument group from a string
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "all" => InstrumentGroup::All,
            "keys" | "key" | "keyboard" | "piano" => InstrumentGroup::Keys,
            "drums" | "drum" | "percussion" | "perc" => InstrumentGroup::Drums,
            "bass" => InstrumentGroup::Bass,
            "guitar" | "git" | "gtr" => InstrumentGroup::Guitar,
            "vocals" | "vox" | "voice" => InstrumentGroup::Vocals,
            _ => InstrumentGroup::Custom(s.to_string()),
        }
    }

    /// Get the RGBA color for this instrument group.
    ///
    /// Colors match the FastTrack Studio instrument palette:
    /// - All: Red
    /// - Keys: Green
    /// - Drums: Orange
    /// - Bass: Purple
    /// - Guitar: Dark Blue
    /// - Vocals: Teal
    /// - Custom: Gray
    pub fn color_rgba(&self) -> (u8, u8, u8, u8) {
        match self {
            InstrumentGroup::All => (0xCC, 0x00, 0x00, 0xFF), // Red
            InstrumentGroup::Keys => (0x22, 0x8B, 0x22, 0xFF), // Forest green
            InstrumentGroup::Drums => (0xE6, 0x7E, 0x22, 0xFF), // Orange
            InstrumentGroup::Bass => (0x7B, 0x2D, 0x8E, 0xFF), // Purple
            InstrumentGroup::Guitar => (0x1A, 0x3C, 0x8E, 0xFF), // Dark blue
            InstrumentGroup::Vocals => (0x00, 0x80, 0x80, 0xFF), // Teal
            InstrumentGroup::Custom(_) => (0x66, 0x66, 0x66, 0xFF), // Gray
        }
    }
}

impl fmt::Display for InstrumentGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstrumentGroup::All => write!(f, "all"),
            InstrumentGroup::Keys => write!(f, "keys"),
            InstrumentGroup::Drums => write!(f, "drums"),
            InstrumentGroup::Bass => write!(f, "bass"),
            InstrumentGroup::Guitar => write!(f, "guitar"),
            InstrumentGroup::Vocals => write!(f, "vocals"),
            InstrumentGroup::Custom(name) => write!(f, "{}", name),
        }
    }
}

/// Represents a text cue for a specific instrument group
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct TextCue {
    pub group: InstrumentGroup,
    pub text: String,
    /// Optional beat position within the measure (1-indexed, matches musical convention)
    /// If None, applies to the start of the measure
    pub beat: Option<u8>,
    /// Position in the song (set during position calculation)
    pub position: Option<AbsolutePosition>,
}

impl TextCue {
    pub fn new(group: InstrumentGroup, text: String) -> Self {
        Self {
            group,
            text,
            beat: None,
            position: None,
        }
    }

    /// Create a text cue at a specific beat
    pub fn at_beat(group: InstrumentGroup, text: String, beat: u8) -> Self {
        Self {
            group,
            text,
            beat: Some(beat),
            position: None,
        }
    }

    /// Set the beat position
    pub fn with_beat(mut self, beat: u8) -> Self {
        self.beat = Some(beat);
        self
    }

    /// Set the absolute position
    pub fn with_position(mut self, position: AbsolutePosition) -> Self {
        self.position = Some(position);
        self
    }

    /// Parse a text cue from a line starting with @
    ///
    /// Format: @`<group>` "`<text>`" or @`<group>`:`<beat>` "`<text>`"
    /// Examples:
    /// - @keys "synth here"
    /// - @keys:2 "hit on beat 2"
    /// - @drums:3 "crash"
    pub fn parse(line: &str) -> Result<Self, String> {
        let line = line.trim();

        if !line.starts_with('@') {
            return Err("Text cue must start with @".to_string());
        }

        // Remove @ symbol
        let content = line[1..].trim();

        // Find the start of the quoted text
        if let Some(quote_start) = content.find('"') {
            let group_part = content[..quote_start].trim();
            let rest = &content[quote_start + 1..];

            // Parse group and optional beat from group_part
            // Format: "keys" or "keys:2"
            let (group_str, beat) = if let Some(colon_pos) = group_part.find(':') {
                let group = &group_part[..colon_pos];
                let beat_str = &group_part[colon_pos + 1..];
                let beat = beat_str
                    .parse::<u8>()
                    .map_err(|_| format!("Invalid beat number: {}", beat_str))?;
                (group, Some(beat))
            } else {
                (group_part, None)
            };

            // Find the closing quote
            if let Some(quote_end) = rest.find('"') {
                let text = rest[..quote_end].to_string();
                let group = InstrumentGroup::parse(group_str);

                let mut cue = TextCue::new(group, text);
                cue.beat = beat;
                Ok(cue)
            } else {
                Err("Missing closing quote for text cue".to_string())
            }
        } else {
            Err("Text cue must have quoted text".to_string())
        }
    }
}

impl fmt::Display for TextCue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.group)?;
        if let Some(beat) = self.beat {
            write!(f, ":{}", beat)?;
        }
        write!(f, " \"{}\"", self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instrument_group_parsing() {
        assert_eq!(InstrumentGroup::parse("keys"), InstrumentGroup::Keys);
        assert_eq!(InstrumentGroup::parse("DRUMS"), InstrumentGroup::Drums);
        assert_eq!(InstrumentGroup::parse("bass"), InstrumentGroup::Bass);
        assert_eq!(InstrumentGroup::parse("guitar"), InstrumentGroup::Guitar);
        assert_eq!(InstrumentGroup::parse("vocals"), InstrumentGroup::Vocals);
        assert_eq!(InstrumentGroup::parse("all"), InstrumentGroup::All);

        match InstrumentGroup::parse("synth") {
            InstrumentGroup::Custom(name) => assert_eq!(name, "synth"),
            _ => panic!("Expected Custom variant"),
        }
    }

    #[test]
    fn test_text_cue_parsing() {
        let cue = TextCue::parse("@keys \"synth here\"").unwrap();
        assert_eq!(cue.group, InstrumentGroup::Keys);
        assert_eq!(cue.text, "synth here");
        assert_eq!(cue.beat, None);

        let cue2 = TextCue::parse("@drums \"crash on 3\"").unwrap();
        assert_eq!(cue2.group, InstrumentGroup::Drums);
        assert_eq!(cue2.text, "crash on 3");
        assert_eq!(cue2.beat, None);

        let cue3 = TextCue::parse("  @guitar \"let ring\"  ").unwrap();
        assert_eq!(cue3.group, InstrumentGroup::Guitar);
        assert_eq!(cue3.text, "let ring");
    }

    #[test]
    fn test_text_cue_with_beat() {
        let cue = TextCue::parse("@keys:2 \"hit\"").unwrap();
        assert_eq!(cue.group, InstrumentGroup::Keys);
        assert_eq!(cue.text, "hit");
        assert_eq!(cue.beat, Some(2));

        let cue2 = TextCue::parse("@drums:3 \"crash\"").unwrap();
        assert_eq!(cue2.group, InstrumentGroup::Drums);
        assert_eq!(cue2.text, "crash");
        assert_eq!(cue2.beat, Some(3));

        let cue3 = TextCue::parse("@bass:1 \"fill\"").unwrap();
        assert_eq!(cue3.group, InstrumentGroup::Bass);
        assert_eq!(cue3.text, "fill");
        assert_eq!(cue3.beat, Some(1));
    }

    #[test]
    fn test_text_cue_errors() {
        assert!(TextCue::parse("keys \"synth here\"").is_err());
        assert!(TextCue::parse("@keys synth here").is_err());
        assert!(TextCue::parse("@keys \"missing quote").is_err());
        assert!(TextCue::parse("@keys:abc \"invalid beat\"").is_err());
    }

    #[test]
    fn test_text_cue_display() {
        let cue = TextCue::new(InstrumentGroup::Keys, "synth here".to_string());
        assert_eq!(format!("{}", cue), "@keys \"synth here\"");

        let cue_with_beat = TextCue::at_beat(InstrumentGroup::Drums, "crash".to_string(), 3);
        assert_eq!(format!("{}", cue_with_beat), "@drums:3 \"crash\"");
    }

    #[test]
    fn test_text_cue_round_trip() {
        // Parse, display, parse again - should be identical
        let original = "@keys:2 \"synth stab\"";
        let cue = TextCue::parse(original).unwrap();
        let displayed = format!("{}", cue);
        let reparsed = TextCue::parse(&displayed).unwrap();

        assert_eq!(cue.group, reparsed.group);
        assert_eq!(cue.text, reparsed.text);
        assert_eq!(cue.beat, reparsed.beat);
    }
}
