//! Note trait and implementations
//!
//! Represents musical notes with enharmonic spelling support

use super::accidental::{Accidental, WithAccidental};
use crate::parsing::token::{Token, TokenType};
use facet::Facet;
use std::fmt;

/// Trait for musical notes
pub trait Note: fmt::Debug + fmt::Display {
    /// Get the note name (e.g., "C#", "Eb", "F")
    fn name(&self) -> String;

    /// Get the semitone value (0-11, where C=0)
    fn semitone(&self) -> u8;

    /// Transpose by a number of semitones
    fn transpose(&self, semitones: i8) -> Box<dyn Note>;

    /// Get possible enharmonic spellings
    fn enharmonic_spellings(&self) -> Vec<String>;

    /// Clone as a boxed trait object
    fn clone_box(&self) -> Box<dyn Note>;
}

/// Musical note implementation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
pub struct MusicalNote {
    /// The note name as written (e.g., "C#", "Db", "F")
    pub name: String,
    /// The letter name (C-B)
    pub letter: char,
    /// The accidental (if any)
    pub accidental: Option<Accidental>,
    /// The semitone value (0-11)
    pub semitone: u8,
}

impl MusicalNote {
    pub fn new(name: String, semitone: u8) -> Self {
        // Parse letter and accidental from the name
        let chars: Vec<char> = name.chars().collect();
        let letter = if !chars.is_empty() {
            chars[0].to_ascii_uppercase()
        } else {
            'C' // Default
        };

        let accidental = if name.contains("##") {
            Some(Accidental::DoubleSharp)
        } else if name.contains("bb") {
            Some(Accidental::DoubleFlat)
        } else if name.contains('#') {
            Some(Accidental::Sharp)
        } else if name.contains('b') {
            Some(Accidental::Flat)
        } else {
            None
        };

        Self {
            name,
            letter,
            accidental,
            semitone: semitone % 12,
        }
    }

    /// Parse a note from a string
    pub fn from_string(s: &str) -> Option<Self> {
        if s.is_empty() {
            return None;
        }

        let chars: Vec<char> = s.chars().collect();
        let base_note = chars[0].to_uppercase().next()?;

        // Get base semitone
        let base_semitone = match base_note {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };

        // Handle accidentals
        let mut semitone: i32 = base_semitone;
        let mut i = 1;
        while i < chars.len() {
            match chars[i] {
                '#' => semitone += 1,
                'b' => semitone = semitone.wrapping_sub(1),
                _ => break,
            }
            i += 1;
        }
        let semitone = semitone.rem_euclid(12) as u8;

        Some(Self::new(s.to_string(), semitone))
    }

    /// Create note from semitone with preferred sharp/flat spelling
    pub fn from_semitone(semitone: u8, prefer_sharp: bool) -> Self {
        let semitone = semitone % 12;
        let name = if prefer_sharp {
            match semitone {
                0 => "C",
                1 => "C#",
                2 => "D",
                3 => "D#",
                4 => "E",
                5 => "F",
                6 => "F#",
                7 => "G",
                8 => "G#",
                9 => "A",
                10 => "A#",
                11 => "B",
                _ => unreachable!(),
            }
        } else {
            match semitone {
                0 => "C",
                1 => "Db",
                2 => "D",
                3 => "Eb",
                4 => "E",
                5 => "F",
                6 => "Gb",
                7 => "G",
                8 => "Ab",
                9 => "A",
                10 => "Bb",
                11 => "B",
                _ => unreachable!(),
            }
        };
        Self::new(name.to_string(), semitone)
    }

    /// Create a note from a letter and optional accidental
    pub fn from_letter_and_accidental(letter: char, accidental: Option<Accidental>) -> Self {
        let base_semitone = match letter.to_ascii_uppercase() {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => panic!("Invalid note letter: {}", letter),
        };

        let semitone = match accidental {
            None | Some(Accidental::Natural) => base_semitone,
            Some(Accidental::Sharp) => (base_semitone + 1) % 12,
            Some(Accidental::Flat) => (base_semitone + 11) % 12,
            Some(Accidental::DoubleSharp) => (base_semitone + 2) % 12,
            Some(Accidental::DoubleFlat) => (base_semitone + 10) % 12,
        };

        let name = format!(
            "{}{}",
            letter.to_ascii_uppercase(),
            match accidental {
                Some(Accidental::Sharp) => "#",
                Some(Accidental::Flat) => "b",
                Some(Accidental::DoubleSharp) => "##",
                Some(Accidental::DoubleFlat) => "bb",
                _ => "",
            }
        );

        Self::new(name, semitone)
    }

    /// Convenience constructors for common notes
    pub fn c() -> Self {
        Self::new("C".to_string(), 0)
    }
    pub fn d() -> Self {
        Self::new("D".to_string(), 2)
    }
    pub fn e() -> Self {
        Self::new("E".to_string(), 4)
    }
    pub fn f() -> Self {
        Self::new("F".to_string(), 5)
    }
    pub fn g() -> Self {
        Self::new("G".to_string(), 7)
    }
    pub fn a() -> Self {
        Self::new("A".to_string(), 9)
    }
    pub fn b() -> Self {
        Self::new("B".to_string(), 11)
    }
}

impl Note for MusicalNote {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn semitone(&self) -> u8 {
        self.semitone
    }

    fn transpose(&self, semitones: i8) -> Box<dyn Note> {
        let new_semitone = ((self.semitone as i16 + semitones as i16 + 120) % 12) as u8;
        let prefer_sharp = self.name.contains('#');
        Box::new(MusicalNote::from_semitone(new_semitone, prefer_sharp))
    }

    fn enharmonic_spellings(&self) -> Vec<String> {
        match self.semitone {
            0 => vec!["C".to_string(), "B#".to_string()],
            1 => vec!["C#".to_string(), "Db".to_string()],
            2 => vec!["D".to_string()],
            3 => vec!["D#".to_string(), "Eb".to_string()],
            4 => vec!["E".to_string(), "Fb".to_string()],
            5 => vec!["F".to_string(), "E#".to_string()],
            6 => vec!["F#".to_string(), "Gb".to_string()],
            7 => vec!["G".to_string()],
            8 => vec!["G#".to_string(), "Ab".to_string()],
            9 => vec!["A".to_string()],
            10 => vec!["A#".to_string(), "Bb".to_string()],
            11 => vec!["B".to_string(), "Cb".to_string()],
            _ => vec![],
        }
    }

    fn clone_box(&self) -> Box<dyn Note> {
        Box::new(self.clone())
    }
}

impl MusicalNote {
    /// Convert this note to LilyPond notation
    ///
    /// Uses English syntax: "s" for sharps and "f" for flats
    /// Examples: C# -> cs, Db -> df, F -> f
    pub fn to_lilypond(&self) -> String {
        let name = self.name.to_lowercase();
        // English syntax: s for sharp, f for flat
        name.replace("#", "s").replace("b", "f")
    }

    /// Get the letter name from a note (e.g., "C", "D", "E")
    pub fn letter(&self) -> char {
        self.name.chars().next().unwrap_or('C')
    }

    /// Get the numeric index of a letter (C=0, D=1, E=2, F=3, G=4, A=5, B=6)
    pub fn letter_index(letter: char) -> u8 {
        match letter.to_ascii_uppercase() {
            'C' => 0,
            'D' => 1,
            'E' => 2,
            'F' => 3,
            'G' => 4,
            'A' => 5,
            'B' => 6,
            _ => 0,
        }
    }

    /// Get a letter from a numeric index (0=C, 1=D, 2=E, 3=F, 4=G, 5=A, 6=B)
    pub fn letter_from_index(index: u8) -> char {
        match index % 7 {
            0 => 'C',
            1 => 'D',
            2 => 'E',
            3 => 'F',
            4 => 'G',
            5 => 'A',
            6 => 'B',
            _ => 'C',
        }
    }

    /// Generate an enharmonically correct note from a root and semantic interval
    ///
    /// This ensures the note uses the correct letter name based on the interval.
    /// For example, a major third above C should be "E" not "Fb", even though they
    /// sound the same.
    ///
    /// # Arguments
    /// * `root` - The root note
    /// * `semitones` - The actual semitone distance from root (can be > 12 for voicings)
    /// * `semantic_interval` - The scale degree (1=unison, 2=second, 3=third, etc.)
    ///
    /// # Returns
    /// A MusicalNote with the correct enharmonic spelling
    ///
    /// # Example
    /// ```
    /// use keyflow_proto::primitives::{MusicalNote, Note};
    ///
    /// // C + major third (4 semitones, semantic interval 3) = E
    /// let c = MusicalNote::c();
    /// let e = MusicalNote::enharmonic_from_root(&c, 4, 3);
    /// assert_eq!(e.name(), "E");
    ///
    /// // C + minor third (3 semitones, semantic interval 3) = Eb
    /// let eb = MusicalNote::enharmonic_from_root(&c, 3, 3);
    /// assert_eq!(eb.name(), "Eb");
    /// ```
    pub fn enharmonic_from_root(
        root: &MusicalNote,
        semitones: u8,
        semantic_interval: u8,
    ) -> MusicalNote {
        use crate::primitives::Accidental;

        // Calculate the target letter based on the semantic interval
        let root_letter_index = Self::letter_index(root.letter());
        let target_letter_index = (root_letter_index + (semantic_interval - 1)) % 7;
        let target_letter = Self::letter_from_index(target_letter_index);

        // Calculate the target semitone
        let target_semitone = (root.semitone + (semitones % 12)) % 12;

        // Get the base semitone for the target letter
        let base_semitone = match target_letter {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => 0,
        };

        // Calculate the accidental needed
        let semitone_diff = (target_semitone + 12 - base_semitone) % 12;
        let accidental = match semitone_diff {
            0 => None,
            1 => Some(Accidental::Sharp),
            2 => Some(Accidental::DoubleSharp),
            11 => Some(Accidental::Flat),
            10 => Some(Accidental::DoubleFlat),
            _ => {
                // For differences > 2 or < 10, we have a weird case
                // Default to sharp for now (this shouldn't happen in normal music)
                if semitone_diff < 6 {
                    Some(Accidental::Sharp)
                } else {
                    Some(Accidental::Flat)
                }
            }
        };

        Self::from_letter_and_accidental(target_letter, accidental)
    }
}

impl fmt::Display for MusicalNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Token result from parsing a note name
#[derive(Debug, Clone, PartialEq)]
pub struct MusicalNoteToken {
    pub letter: char,
    pub accidental: Option<Accidental>,
    pub tokens_consumed: usize,
}

impl MusicalNoteToken {
    /// Try to parse a note name from the start of a token stream
    /// Returns Some(MusicalNoteToken) if successful, None if tokens don't represent a note
    pub fn parse(tokens: &[Token]) -> Option<Self> {
        if tokens.is_empty() {
            return None;
        }

        // First token must be a letter (A-G or a-g)
        let letter = match &tokens[0].token_type {
            TokenType::Letter(c) if Self::is_note_letter(*c) => c.to_ascii_uppercase(),
            _ => return None,
        };

        let mut consumed = 1;
        let mut accidental = None;

        // Check for accidentals (can have multiple)
        while consumed < tokens.len() {
            match &tokens[consumed].token_type {
                TokenType::Sharp => {
                    accidental = Some(match accidental {
                        None | Some(Accidental::Natural) => Accidental::Sharp,
                        Some(Accidental::Sharp) => Accidental::DoubleSharp,
                        _ => return None, // Invalid combination
                    });
                    consumed += 1;
                }
                TokenType::Flat => {
                    accidental = Some(match accidental {
                        None | Some(Accidental::Natural) => Accidental::Flat,
                        Some(Accidental::Flat) => Accidental::DoubleFlat,
                        _ => return None, // Invalid combination
                    });
                    consumed += 1;
                }
                TokenType::Letter('b') if consumed == 1 => {
                    // Special case: 'b' right after note letter is treated as flat
                    // This works for all notes including Bb (B-flat)
                    accidental = Some(match accidental {
                        None | Some(Accidental::Natural) => Accidental::Flat,
                        Some(Accidental::Flat) => Accidental::DoubleFlat,
                        _ => return None, // Invalid combination
                    });
                    consumed += 1;
                }
                _ => break,
            }
        }

        Some(MusicalNoteToken {
            letter,
            accidental,
            tokens_consumed: consumed,
        })
    }

    fn is_note_letter(c: char) -> bool {
        matches!(
            c.to_ascii_uppercase(),
            'A' | 'B' | 'C' | 'D' | 'E' | 'F' | 'G'
        )
    }
}

impl WithAccidental for MusicalNoteToken {
    fn sharp(mut self) -> Self {
        self.accidental = Some(match self.accidental {
            None | Some(Accidental::Natural) => Accidental::Sharp,
            Some(Accidental::Sharp) => Accidental::DoubleSharp,
            Some(Accidental::Flat) => Accidental::Natural,
            Some(Accidental::DoubleFlat) => Accidental::Flat,
            Some(acc) => acc,
        });
        self
    }

    fn flat(mut self) -> Self {
        self.accidental = Some(match self.accidental {
            None | Some(Accidental::Natural) => Accidental::Flat,
            Some(Accidental::Flat) => Accidental::DoubleFlat,
            Some(Accidental::Sharp) => Accidental::Natural,
            Some(Accidental::DoubleSharp) => Accidental::Sharp,
            Some(acc) => acc,
        });
        self
    }

    fn double_sharp(mut self) -> Self {
        self.accidental = Some(Accidental::DoubleSharp);
        self
    }

    fn double_flat(mut self) -> Self {
        self.accidental = Some(Accidental::DoubleFlat);
        self
    }

    fn natural(mut self) -> Self {
        self.accidental = Some(Accidental::Natural);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_from_string() {
        let note = MusicalNote::from_string("C").unwrap();
        assert_eq!(note.semitone(), 0);
        assert_eq!(note.name(), "C");

        let note = MusicalNote::from_string("C#").unwrap();
        assert_eq!(note.semitone(), 1);

        let note = MusicalNote::from_string("Db").unwrap();
        assert_eq!(note.semitone(), 1);

        let note = MusicalNote::from_string("F#").unwrap();
        assert_eq!(note.semitone(), 6);
    }

    #[test]
    fn test_note_transpose() {
        let c = MusicalNote::c();
        let d = c.transpose(2);
        assert_eq!(d.semitone(), 2);

        let b = c.transpose(-1);
        assert_eq!(b.semitone(), 11);
    }

    #[test]
    fn test_enharmonic_spellings() {
        let note = MusicalNote::from_string("C#").unwrap();
        let spellings = note.enharmonic_spellings();
        assert!(spellings.contains(&"C#".to_string()));
        assert!(spellings.contains(&"Db".to_string()));
    }

    #[test]
    fn test_from_semitone() {
        let note = MusicalNote::from_semitone(1, true);
        assert_eq!(note.name(), "C#");

        let note = MusicalNote::from_semitone(1, false);
        assert_eq!(note.name(), "Db");
    }

    #[test]
    fn test_parse_note_token_simple() {
        use crate::parsing::Lexer;
        let mut lexer = Lexer::new("C".to_string());
        let tokens = lexer.tokenize();
        let result = MusicalNoteToken::parse(&tokens).unwrap();

        assert_eq!(result.letter, 'C');
        assert_eq!(result.accidental, None);
        assert_eq!(result.tokens_consumed, 1);
    }

    #[test]
    fn test_parse_note_token_sharp() {
        use crate::parsing::Lexer;
        let mut lexer = Lexer::new("C#".to_string());
        let tokens = lexer.tokenize();
        let result = MusicalNoteToken::parse(&tokens).unwrap();

        assert_eq!(result.letter, 'C');
        assert_eq!(result.accidental, Some(Accidental::Sharp));
        assert_eq!(result.tokens_consumed, 2);
    }

    #[test]
    fn test_parse_note_token_flat() {
        use crate::parsing::Lexer;
        let mut lexer = Lexer::new("Eb".to_string());
        let tokens = lexer.tokenize();
        let result = MusicalNoteToken::parse(&tokens).unwrap();

        assert_eq!(result.letter, 'E');
        assert_eq!(result.accidental, Some(Accidental::Flat));
        assert_eq!(result.tokens_consumed, 2);
    }

    #[test]
    fn test_parse_note_token_double_sharp() {
        use crate::parsing::Lexer;
        let mut lexer = Lexer::new("F##".to_string());
        let tokens = lexer.tokenize();
        let result = MusicalNoteToken::parse(&tokens).unwrap();

        assert_eq!(result.letter, 'F');
        assert_eq!(result.accidental, Some(Accidental::DoubleSharp));
        assert_eq!(result.tokens_consumed, 3);
    }

    #[test]
    fn test_note_b_not_flat() {
        use crate::parsing::Lexer;
        let mut lexer = Lexer::new("B".to_string());
        let tokens = lexer.tokenize();
        let result = MusicalNoteToken::parse(&tokens).unwrap();

        assert_eq!(result.letter, 'B');
        assert_eq!(result.accidental, None);
        assert_eq!(result.tokens_consumed, 1);
    }
}
