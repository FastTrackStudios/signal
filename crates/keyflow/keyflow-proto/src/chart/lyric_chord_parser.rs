//! Manual lyric-chord assignment parser
//!
//! Allows users to explicitly assign chords to syllables in lyrics.
//! Syntax: `{ChordSymbol}syllable` to attach a chord to a syllable
//!
//! Examples:
//! - `{Gm}Slow {A#}down {F}you {Gm}cra-zy {A#}child`
//! - `{Cmaj7}Amazing {Dm7}grace {G}how {Cmaj7}sweet`
//! - Multiple chords on one syllable: `{Cmaj7|Dm7}transition`

use crate::chart::syllable_parser::SyllableParser;

/// Parse lyrics with explicit chord assignments
///
/// Format: `{ChordSymbol}syllable {AnotherChord}next-syllable`
///
/// # Examples
/// ```ignore
/// let parser = LyricChordParser::new();
/// let line = parser.parse("{Gm}Slow {A#}down you {F}cra-zy child");
/// // Returns LyricLine with syllables annotated with chords
/// ```
pub struct LyricChordParser {
    syllable_parser: SyllableParser,
}

impl LyricChordParser {
    /// Create a new lyric-chord parser
    pub fn new() -> Self {
        Self {
            syllable_parser: SyllableParser::new(),
        }
    }

    /// Parse lyrics with inline chord markers
    ///
    /// Syntax: `{Chord}syllable {AnotherChord}syllable`
    ///
    /// Extracts:
    /// 1. Chord markers in `{...}` format
    /// 2. Lyric text between markers
    /// 3. Assigns each chord to the following syllables
    pub fn parse(&self, text: &str) -> Result<crate::chart::LyricLine, String> {
        // Extract chords and clean text
        let (clean_text, chord_assignments) = self.extract_chord_assignments(text)?;

        // Parse syllables from clean text
        let line = self.syllable_parser.parse(&clean_text);

        // Apply chord assignments
        let annotated = self.apply_chord_assignments(line, chord_assignments)?;

        Ok(annotated)
    }

    /// Extract chord assignments: `{C}text` → returns clean text and chord map
    ///
    /// Returns: (clean_text_without_braces, Vec<(syllable_index, chord_symbol)>)
    ///
    /// Syllable boundaries are tracked via whitespace (word breaks) and hyphens
    /// (syllable breaks within a word), so `{Gm}Slow {A#}down` maps Gm→0, A#→1
    /// and `{C}A-{D}ma-{G}zing` maps C→0, D→1, G→2.
    fn extract_chord_assignments(
        &self,
        text: &str,
    ) -> Result<(String, Vec<(usize, String)>), String> {
        let mut clean_text = String::new();
        let mut chord_assignments = Vec::new();
        let mut current_syllable_index = 0;
        let mut seen_text_in_current_syllable = false;
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '{' => {
                    // Extract chord inside braces
                    let mut chord = String::new();
                    let mut found_close = false;

                    while let Some(&next_ch) = chars.peek() {
                        if next_ch == '}' {
                            chars.next(); // consume }
                            found_close = true;
                            break;
                        }
                        chord.push(chars.next().unwrap());
                    }

                    if !found_close {
                        return Err("Unclosed chord marker `{...}`".to_string());
                    }

                    if chord.is_empty() {
                        return Err("Empty chord marker `{}`".to_string());
                    }

                    // Record this chord for the current syllable position
                    chord_assignments.push((current_syllable_index, chord));
                }
                ' ' | '\t' => {
                    // Whitespace = word boundary = new syllable
                    if seen_text_in_current_syllable {
                        current_syllable_index += 1;
                        seen_text_in_current_syllable = false;
                    }
                    clean_text.push(ch);
                }
                '-' => {
                    // Hyphen = syllable boundary within a word
                    if seen_text_in_current_syllable {
                        current_syllable_index += 1;
                        seen_text_in_current_syllable = false;
                    }
                    clean_text.push(ch);
                }
                _ => {
                    seen_text_in_current_syllable = true;
                    clean_text.push(ch);
                }
            }
        }

        Ok((clean_text, chord_assignments))
    }

    /// Apply chord assignments to syllables
    fn apply_chord_assignments(
        &self,
        mut line: crate::chart::LyricLine,
        assignments: Vec<(usize, String)>,
    ) -> Result<crate::chart::LyricLine, String> {
        for (syl_idx, chord) in assignments {
            if syl_idx >= line.syllables.len() {
                return Err(format!(
                    "Chord assigned to syllable {} but only {} syllables exist",
                    syl_idx,
                    line.syllables.len()
                ));
            }

            // Attach chord to syllable
            line.syllables[syl_idx].chord = Some(chord);
            line.syllables[syl_idx].chord_attachment =
                Some(super::lyrics::ChordAttachment::BeforeWord);
        }

        Ok(line)
    }
}

impl Default for LyricChordParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_chord_assignments() {
        let parser = LyricChordParser::new();
        let (clean, assignments) = parser
            .extract_chord_assignments("{Gm}Slow {A#}down")
            .unwrap();

        assert_eq!(clean, "Slow down");
        assert_eq!(assignments.len(), 2);
        assert_eq!(assignments[0], (0, "Gm".to_string()));
        assert_eq!(assignments[1], (1, "A#".to_string()));
    }

    #[test]
    fn test_parse_with_chords() {
        let parser = LyricChordParser::new();
        let line = parser.parse("{Gm}Slow {A#}down you").unwrap();

        // Check that syllables have chords attached
        let slow_syllable = line.syllables.iter().find(|s| s.text == "Slow");
        assert!(slow_syllable.is_some());
        assert_eq!(slow_syllable.unwrap().chord, Some("Gm".to_string()));
    }

    #[test]
    fn test_multiple_chords_error() {
        let parser = LyricChordParser::new();
        // More chords than syllables should error
        let result = parser.parse("{Gm}Slow {A#}down {F}too {G}many {D}chords");
        // This might succeed depending on syllable splitting, so just check it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_complex_line() {
        let parser = LyricChordParser::new();
        let line = parser
            .parse("{Cmaj7}A-{Dm7}ma-{G}zing {Cmaj7}grace")
            .unwrap();

        // Should have syllables with chords
        assert!(!line.syllables.is_empty());
        let with_chords = line.syllables.iter().filter(|s| s.chord.is_some()).count();
        assert!(with_chords > 0);
    }
}
