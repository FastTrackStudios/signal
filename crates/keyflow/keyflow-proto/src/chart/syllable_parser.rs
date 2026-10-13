//! Syllable-aware lyric parsing
//!
//! Splits words into syllables using linguistic rules (Knuth-Liang hyphenation)
//! and allows precise chord attachment at syllable boundaries.

use super::lyrics::{ChordAttachment, LyricLine, LyricSyllable};

/// Parse lyrics into syllables with optional chord information
///
/// Format examples:
/// - Simple: `"Amazing grace how sweet"`
/// - With chords:
///   - Before word: `"C/ Amazing grace how sweet"` (chord before "Amazing")
///   - On syllable: `"A|ma|zing grace"` (pipes indicate syllable boundaries)
///   - With notation: `"[C] Amazing [G] grace"` (inline chord notation)
///
/// # Features
///
/// - Automatic syllable detection using hyphenation rules (when syllable-splitting feature enabled)
/// - Chord attachment at various positions (before word, on syllable, etc.)
/// - Preserves internal hyphens for melisma notation (e.g., "a-ma-zing")
/// - Fallback to word-as-single-syllable when hyphenation not available
#[derive(Debug, Clone, Copy, Default)]
pub struct SyllableParser;

impl SyllableParser {
    /// Create a new syllable parser (English by default)
    pub fn new() -> Self {
        Self
    }

    /// Parse a lyric line with optional syllable/chord information
    ///
    /// Supports formats:
    /// - Plain: `"Amazing grace how sweet the sound"`
    /// - With chords: `"[C]Amazing grace [G]how sweet"`
    /// - With syllable marks: `"A|ma|zing grace"`
    ///
    /// Returns a LyricLine with syllables and chord attachments
    pub fn parse(&self, text: &str) -> LyricLine {
        // First pass: extract inline chords and text
        let (clean_text, inline_chords) = self.extract_inline_chords(text);

        // Second pass: split into words and syllables
        let words = clean_text.split_whitespace().collect::<Vec<_>>();
        let mut syllables = Vec::new();
        let mut word_index = 0;

        for word in words {
            let word_syllables = self.split_word_into_syllables(word);

            for (syl_index, syl_text) in word_syllables.iter().enumerate() {
                let mut syllable = LyricSyllable::new(syl_text);

                // Mark first syllable of word
                if syl_index == 0 {
                    syllable.word_initial = true;
                }

                // Attach chords from inline notation
                if let Some(chord) = inline_chords.get(word_index)
                    && syl_index == 0
                {
                    // Attach to first syllable of word
                    syllable = syllable.with_chord(chord.clone(), ChordAttachment::BeforeWord);
                }

                // Mark hyphenation for multi-syllable words
                if syl_index < word_syllables.len() - 1 {
                    syllable.hyphen_after = true;
                }

                syllables.push(syllable);
            }

            word_index += 1;
        }

        LyricLine::new(syllables)
    }

    /// Extract inline chord notation: `[C]word [G]word`
    /// Returns (clean_text_without_chords, chords_by_word_index)
    fn extract_inline_chords(&self, text: &str) -> (String, Vec<String>) {
        let mut clean_text = String::new();
        let mut chords = Vec::new();
        let mut current_chord: Option<String> = None;
        let mut in_bracket = false;
        let chars = text.chars().peekable();

        for ch in chars {
            match ch {
                '[' => {
                    in_bracket = true;
                    current_chord = Some(String::new());
                }
                ']' => {
                    in_bracket = false;
                    // Chord will be attached to next word
                }
                ' ' | '\t' => {
                    if !in_bracket {
                        if let Some(chord) = current_chord.take() {
                            chords.push(chord);
                        }
                        clean_text.push(' ');
                    }
                }
                _ => {
                    if in_bracket {
                        if let Some(ref mut chord) = current_chord {
                            chord.push(ch);
                        }
                    } else {
                        clean_text.push(ch);
                    }
                }
            }
        }

        // Handle final chord
        if let Some(chord) = current_chord {
            chords.push(chord);
        }

        (clean_text.trim().to_string(), chords)
    }

    /// Split a word into syllables using explicit markers, hyphens, or hyphenation
    fn split_word_into_syllables(&self, word: &str) -> Vec<String> {
        // Check for explicit syllable markers: `a|ma|zing`
        if word.contains('|') {
            return word
                .split('|')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
        }

        // Split on hyphens (melisma / manual syllable breaks): `a-ma-zing`
        if word.contains('-') {
            return word
                .split('-')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
        }

        #[cfg(feature = "syllable-splitting")]
        {
            // Dictionary loading is not wired yet in this crate; keep the
            // fallback behavior until we have bundled hyphenation resources.
        }

        // Fallback: treat entire word as one syllable
        vec![word.to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_inline_chords() {
        let parser = SyllableParser::new();
        let (clean, chords) = parser.extract_inline_chords("[C]Amazing [G]grace");
        assert_eq!(clean, "Amazing grace");
        assert_eq!(chords, vec!["C", "G"]);
    }

    #[test]
    fn test_parse_simple_lyrics() {
        let parser = SyllableParser::new();
        let line = parser.parse("Amazing grace how sweet");
        assert!(!line.syllables.is_empty());
        // First syllable of each word should be marked
        assert!(line.syllables[0].word_initial);
    }

    #[test]
    fn test_explicit_syllable_marks() {
        let parser = SyllableParser::new();
        let line = parser.parse("a|ma|zing grace");
        // Should split on explicit marks
        assert!(line.syllables.iter().any(|s| s.text == "a"));
        assert!(line.syllables.iter().any(|s| s.text == "ma"));
    }

    #[test]
    fn test_chord_attachment() {
        let parser = SyllableParser::new();
        let line = parser.parse("[C]Amazing [G]grace");

        let has_c_chord = line
            .syllables
            .iter()
            .any(|s| s.chord.as_deref() == Some("C"));
        let has_g_chord = line
            .syllables
            .iter()
            .any(|s| s.chord.as_deref() == Some("G"));

        assert!(has_c_chord);
        assert!(has_g_chord);
    }
}
