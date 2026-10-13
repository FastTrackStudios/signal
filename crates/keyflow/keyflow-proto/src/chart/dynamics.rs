//! Dynamic and intensity markings
//!
//! Provides a system for adding dynamic/intensity cues to the chart
//! using angle bracket notation (e.g., `<Build>`, `<Down>`, `<Go Crazy>`, `<Soft>`)
//!
//! Unlike traditional dynamics (pp, mp, mf, ff), these are free-form
//! text descriptions that communicate energy and intensity to performers.
//!
//! Note: We use angle brackets `<>` instead of square brackets `[]` to avoid
//! conflict with custom section syntax which uses `[Section Name]`.

use crate::time::AbsolutePosition;
use facet::Facet;
use std::fmt;

/// Represents a dynamic/intensity marking in the chart
///
/// These are free-form text descriptions like "Build", "Down", "Go Crazy"
/// that indicate energy levels and intensity changes to performers.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DynamicMarking {
    /// The text of the dynamic marking (e.g., "Build", "Down", "Soft")
    pub text: String,

    /// Optional beat position within the measure (0-indexed)
    /// If None, applies to the start of the measure
    pub beat: Option<u8>,

    /// Position in the song (set during position calculation)
    pub position: Option<AbsolutePosition>,
}

impl DynamicMarking {
    /// Create a new dynamic marking
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            beat: None,
            position: None,
        }
    }

    /// Create a dynamic marking at a specific beat
    pub fn at_beat(text: impl Into<String>, beat: u8) -> Self {
        Self {
            text: text.into(),
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

    /// Parse a dynamic marking from a string
    ///
    /// Format: `<text>` or `<text>:beat`
    /// Examples:
    /// - `<Build>` - Dynamic marking "Build"
    /// - `<Go Crazy>` - Dynamic marking "Go Crazy"
    /// - `<Hit>:3` - Dynamic marking "Hit" on beat 3
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();

        if !s.starts_with('<') {
            return Err("Dynamic marking must start with <".to_string());
        }

        // Find the closing bracket
        let close_bracket = s.find('>').ok_or("Missing closing bracket >")?;

        let text = s[1..close_bracket].trim().to_string();

        if text.is_empty() {
            return Err("Dynamic marking text cannot be empty".to_string());
        }

        // Check for beat specification after the bracket
        let after_bracket = &s[close_bracket + 1..];
        let beat = if let Some(stripped) = after_bracket.strip_prefix(':') {
            let beat_str = stripped.trim();
            Some(
                beat_str
                    .parse::<u8>()
                    .map_err(|_| format!("Invalid beat number: {}", beat_str))?,
            )
        } else {
            None
        };

        Ok(DynamicMarking {
            text,
            beat,
            position: None,
        })
    }
}

impl fmt::Display for DynamicMarking {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.text)?;
        if let Some(beat) = self.beat {
            write!(f, ":{}", beat)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_dynamic() {
        let dyn_mark = DynamicMarking::parse("<Build>").unwrap();
        assert_eq!(dyn_mark.text, "Build");
        assert_eq!(dyn_mark.beat, None);
    }

    #[test]
    fn test_parse_dynamic_with_spaces() {
        let dyn_mark = DynamicMarking::parse("<Go Crazy>").unwrap();
        assert_eq!(dyn_mark.text, "Go Crazy");
        assert_eq!(dyn_mark.beat, None);
    }

    #[test]
    fn test_parse_dynamic_with_beat() {
        let dyn_mark = DynamicMarking::parse("<Hit>:3").unwrap();
        assert_eq!(dyn_mark.text, "Hit");
        assert_eq!(dyn_mark.beat, Some(3));
    }

    #[test]
    fn test_parse_dynamic_trimmed() {
        let dyn_mark = DynamicMarking::parse("  <Soft>  ").unwrap();
        assert_eq!(dyn_mark.text, "Soft");
    }

    #[test]
    fn test_display() {
        let dyn_mark = DynamicMarking::new("Build");
        assert_eq!(format!("{}", dyn_mark), "<Build>");

        let dyn_mark_beat = DynamicMarking::at_beat("Hit", 3);
        assert_eq!(format!("{}", dyn_mark_beat), "<Hit>:3");
    }

    #[test]
    fn test_parse_errors() {
        assert!(DynamicMarking::parse("Build").is_err()); // Missing brackets
        assert!(DynamicMarking::parse("<>").is_err()); // Empty text
        assert!(DynamicMarking::parse("<Build").is_err()); // Missing closing bracket
    }
}
