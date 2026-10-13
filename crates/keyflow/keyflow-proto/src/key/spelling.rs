//! Key-aware note spelling
//!
//! Provides proper enharmonic spelling of notes based on musical key context.
//!
//! # Spelling Modes
//!
//! - **Strict**: Every letter (A-G) appears exactly once in the scale. Uses double
//!   sharps/flats when necessary (e.g., C# major: C# D# E# F# G# A# B#)
//!
//! - **Relaxed**: Avoids double sharps/flats by allowing letter repetition
//!   (e.g., C# major with relaxed spelling: C# D# F F# G# A# C)

use crate::primitives::note::Note;
use crate::primitives::{Accidental, MusicalNote};
use facet::Facet;

/// The spelling mode determines how strictly to follow traditional notation rules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Facet)]
#[repr(u8)]
pub enum SpellingMode {
    /// Every letter A-G appears exactly once. May use double sharps/flats.
    #[default]
    Strict,
    /// Avoids double sharps/flats. May repeat letters.
    Relaxed,
}

/// Key spelling context for proper enharmonic note naming
#[derive(Debug, Clone)]
pub struct KeySpelling {
    /// The root note of the key
    root: MusicalNote,
    /// Whether this is a major key
    is_major: bool,
    /// Whether this is a sharp key (true) or flat key (false)
    prefers_sharps: bool,
    /// The number of sharps (positive) or flats (negative) in the key signature
    accidental_count: i8,
    /// Pre-computed scale spellings for each semitone (0-11)
    /// Index 0 = spelling for semitone 0 (C), etc.
    strict_spellings: [NoteSpelling; 12],
    /// Relaxed spellings that avoid double sharps/flats
    relaxed_spellings: [NoteSpelling; 12],
}

/// A note spelling with letter and accidental
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteSpelling {
    pub letter: char,
    pub accidental: Option<Accidental>,
}

impl NoteSpelling {
    pub fn new(letter: char, accidental: Option<Accidental>) -> Self {
        Self { letter, accidental }
    }

    /// Get the semitone value for this spelling
    pub fn semitone(&self) -> u8 {
        let base = match self.letter {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => 0,
        };

        let adjustment: i8 = match self.accidental {
            None | Some(Accidental::Natural) => 0,
            Some(Accidental::Sharp) => 1,
            Some(Accidental::Flat) => -1,
            Some(Accidental::DoubleSharp) => 2,
            Some(Accidental::DoubleFlat) => -2,
        };

        ((base as i8 + adjustment + 12) % 12) as u8
    }

    /// Convert to a MusicalNote
    pub fn to_note(&self) -> MusicalNote {
        MusicalNote::from_letter_and_accidental(self.letter, self.accidental)
    }
}

impl std::fmt::Display for NoteSpelling {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let acc_str = match self.accidental {
            None | Some(Accidental::Natural) => "",
            Some(Accidental::Sharp) => "#",
            Some(Accidental::Flat) => "b",
            Some(Accidental::DoubleSharp) => "##",
            Some(Accidental::DoubleFlat) => "bb",
        };
        write!(f, "{}{}", self.letter, acc_str)
    }
}

impl KeySpelling {
    /// Create a new key spelling context from a root note
    ///
    /// Automatically determines whether to use sharps or flats based on
    /// the circle of fifths.
    pub fn new(root: &MusicalNote, is_major: bool) -> Self {
        let (prefers_sharps, accidental_count) = Self::determine_key_type(root, is_major);

        let mut spelling = Self {
            root: root.clone(),
            is_major,
            prefers_sharps,
            accidental_count,
            strict_spellings: [NoteSpelling::new('C', None); 12],
            relaxed_spellings: [NoteSpelling::new('C', None); 12],
        };

        spelling.compute_spellings();
        spelling
    }

    /// Create a major key spelling
    pub fn major(root: &MusicalNote) -> Self {
        Self::new(root, true)
    }

    /// Create a minor key spelling
    pub fn minor(root: &MusicalNote) -> Self {
        Self::new(root, false)
    }

    /// Whether this key prefers flat spellings (e.g., Bb, Eb, Ab).
    pub fn prefers_flat(&self) -> bool {
        !self.prefers_sharps
    }

    /// Determine if a key uses sharps or flats and how many
    fn determine_key_type(root: &MusicalNote, is_major: bool) -> (bool, i8) {
        // Get the semitone and adjust for minor keys (relative major is 3 semitones up)
        let major_root_semitone = if is_major {
            root.semitone()
        } else {
            (root.semitone() + 3) % 12
        };

        // Circle of fifths position (C=0, G=1, D=2, etc. for sharps; F=-1, Bb=-2, etc. for flats)
        // Sharp keys: C(0), G(1#), D(2#), A(3#), E(4#), B(5#), F#(6#), C#(7#)
        // Flat keys:  C(0), F(1b), Bb(2b), Eb(3b), Ab(4b), Db(5b), Gb(6b), Cb(7b)

        // Determine based on the root's current spelling
        let root_has_sharp = root.name().contains('#');
        let root_has_flat = root.name().contains('b') && root.letter != 'B';

        // Standard major key signatures by semitone
        match major_root_semitone {
            0 => (true, 0), // C major: no sharps/flats
            1 => {
                // C# or Db major
                if root_has_flat || (!root_has_sharp && !is_major) {
                    (false, -5) // Db major: 5 flats
                } else {
                    (true, 7) // C# major: 7 sharps
                }
            }
            2 => (true, 2), // D major: 2 sharps
            3 => {
                // D# or Eb major
                if root_has_sharp {
                    (true, 6) // D# major (theoretical): 6 sharps + double sharps
                } else {
                    (false, -3) // Eb major: 3 flats
                }
            }
            4 => (true, 4),   // E major: 4 sharps
            5 => (false, -1), // F major: 1 flat
            6 => {
                // F# or Gb major
                if root_has_flat {
                    (false, -6) // Gb major: 6 flats
                } else {
                    (true, 6) // F# major: 6 sharps
                }
            }
            7 => (true, 1),    // G major: 1 sharp
            8 => (false, -4),  // Ab major: 4 flats
            9 => (true, 3),    // A major: 3 sharps
            10 => (false, -2), // Bb major: 2 flats
            11 => {
                // B or Cb major
                if root_has_flat {
                    (false, -7) // Cb major: 7 flats
                } else {
                    (true, 5) // B major: 5 sharps
                }
            }
            _ => (true, 0),
        }
    }

    /// Compute the spellings for all 12 semitones
    fn compute_spellings(&mut self) {
        // First, compute the scale degrees for strict spelling
        let scale_spellings = self.compute_scale_spellings();

        // Initialize with default sharp/flat preference
        for semitone in 0..12 {
            let default = if self.prefers_sharps {
                Self::default_sharp_spelling(semitone)
            } else {
                Self::default_flat_spelling(semitone)
            };
            self.strict_spellings[semitone as usize] = default;
            self.relaxed_spellings[semitone as usize] = default;
        }

        // Apply scale degree spellings (these take priority)
        for spelling in &scale_spellings {
            let semitone = spelling.semitone() as usize;
            self.strict_spellings[semitone] = *spelling;

            // For relaxed mode, convert double accidentals
            let relaxed = Self::relax_spelling(*spelling, self.prefers_sharps);
            self.relaxed_spellings[semitone] = relaxed;
        }

        // For chromatic notes (not in scale), determine spelling based on context
        self.compute_chromatic_spellings();
    }

    /// Compute the spellings for the 7 scale degrees
    fn compute_scale_spellings(&self) -> Vec<NoteSpelling> {
        let letters = ['C', 'D', 'E', 'F', 'G', 'A', 'B'];

        // Find the index of the root letter
        let root_letter = self.root.letter.to_ascii_uppercase();
        let root_letter_idx = letters.iter().position(|&c| c == root_letter).unwrap_or(0);

        // Scale intervals in semitones
        // Major: W W H W W W H (0, 2, 4, 5, 7, 9, 11)
        // Natural minor: W H W W H W W (0, 2, 3, 5, 7, 8, 10)
        let intervals: [u8; 7] = if self.is_major {
            [0, 2, 4, 5, 7, 9, 11]
        } else {
            [0, 2, 3, 5, 7, 8, 10]
        };

        let mut spellings = Vec::with_capacity(7);
        let root_semitone = self.root.semitone();

        for (degree, &interval) in intervals.iter().enumerate() {
            // The letter for this degree
            let letter_idx = (root_letter_idx + degree) % 7;
            let letter = letters[letter_idx];

            // The target semitone for this degree
            let target_semitone = (root_semitone + interval) % 12;

            // The natural semitone for this letter
            let letter_semitone = match letter {
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
            let diff = (target_semitone as i8 - letter_semitone as i8 + 12) % 12;
            let accidental = match diff {
                0 => None,
                1 => Some(Accidental::Sharp),
                2 => Some(Accidental::DoubleSharp),
                11 => Some(Accidental::Flat),
                10 => Some(Accidental::DoubleFlat),
                _ => {
                    // Edge case - shouldn't happen in standard keys
                    if diff < 6 {
                        Some(Accidental::Sharp)
                    } else {
                        Some(Accidental::Flat)
                    }
                }
            };

            spellings.push(NoteSpelling::new(letter, accidental));
        }

        spellings
    }

    /// Convert a spelling with awkward accidentals to a simpler enharmonic
    ///
    /// This handles:
    /// - Double sharps/flats → single accidentals
    /// - E# → F, B# → C (notes that don't exist in standard naming)
    /// - Cb → B, Fb → E (notes that don't exist in standard naming)
    fn relax_spelling(spelling: NoteSpelling, _prefer_sharp: bool) -> NoteSpelling {
        match spelling.accidental {
            Some(Accidental::DoubleSharp) => {
                // X## -> next letter (possibly with sharp)
                let new_letter = match spelling.letter {
                    'C' => 'D',
                    'D' => 'E',
                    'E' => 'F', // E## = F# (need sharp)
                    'F' => 'G',
                    'G' => 'A',
                    'A' => 'B',
                    'B' => 'C', // B## = C# (need sharp)
                    _ => spelling.letter,
                };
                // E## and B## need a sharp on the new letter
                let new_acc = match spelling.letter {
                    'E' | 'B' => Some(Accidental::Sharp),
                    _ => None,
                };
                NoteSpelling::new(new_letter, new_acc)
            }
            Some(Accidental::DoubleFlat) => {
                // Xbb -> previous letter (possibly with flat)
                let new_letter = match spelling.letter {
                    'C' => 'B', // Cbb = Bb (need flat)
                    'D' => 'C',
                    'E' => 'D',
                    'F' => 'E', // Fbb = Eb (need flat)
                    'G' => 'F',
                    'A' => 'G',
                    'B' => 'A',
                    _ => spelling.letter,
                };
                // Cbb and Fbb need a flat on the new letter
                let new_acc = match spelling.letter {
                    'C' | 'F' => Some(Accidental::Flat),
                    _ => None,
                };
                NoteSpelling::new(new_letter, new_acc)
            }
            Some(Accidental::Sharp) => {
                // E# → F, B# → C (these notes don't exist in standard naming)
                match spelling.letter {
                    'E' => NoteSpelling::new('F', None),
                    'B' => NoteSpelling::new('C', None),
                    _ => spelling,
                }
            }
            Some(Accidental::Flat) => {
                // Cb → B, Fb → E (these notes don't exist in standard naming)
                match spelling.letter {
                    'C' => NoteSpelling::new('B', None),
                    'F' => NoteSpelling::new('E', None),
                    _ => spelling,
                }
            }
            _ => spelling,
        }
    }

    /// Compute spellings for chromatic (non-scale) notes
    fn compute_chromatic_spellings(&mut self) {
        // For chromatic notes, we need to choose the appropriate spelling
        // based on the key context. Generally:
        // - In sharp keys, chromatic notes are spelled with sharps
        // - In flat keys, chromatic notes are spelled with flats
        // - Exception: leading tones and chromatic alterations of scale degrees

        for semitone in 0..12u8 {
            // Check if this semitone is already covered by a scale degree
            // by seeing if the strict spelling matches a standard scale note
            let _current = self.strict_spellings[semitone as usize];

            // If this is a chromatic note (not matching a scale degree),
            // apply appropriate spelling
            // We can detect this if the current spelling doesn't match standard patterns
            let is_scale_tone = self.is_scale_tone(semitone);

            if !is_scale_tone {
                let spelling = if self.prefers_sharps {
                    Self::default_sharp_spelling(semitone)
                } else {
                    Self::default_flat_spelling(semitone)
                };
                self.strict_spellings[semitone as usize] = spelling;
                self.relaxed_spellings[semitone as usize] = spelling;
            }
        }
    }

    /// Check if a semitone is a scale tone
    fn is_scale_tone(&self, semitone: u8) -> bool {
        let root_semitone = self.root.semitone();
        let intervals: [u8; 7] = if self.is_major {
            [0, 2, 4, 5, 7, 9, 11]
        } else {
            [0, 2, 3, 5, 7, 8, 10]
        };

        intervals
            .iter()
            .any(|&interval| (root_semitone + interval) % 12 == semitone)
    }

    /// Default sharp spelling for a semitone
    fn default_sharp_spelling(semitone: u8) -> NoteSpelling {
        match semitone {
            0 => NoteSpelling::new('C', None),
            1 => NoteSpelling::new('C', Some(Accidental::Sharp)),
            2 => NoteSpelling::new('D', None),
            3 => NoteSpelling::new('D', Some(Accidental::Sharp)),
            4 => NoteSpelling::new('E', None),
            5 => NoteSpelling::new('F', None),
            6 => NoteSpelling::new('F', Some(Accidental::Sharp)),
            7 => NoteSpelling::new('G', None),
            8 => NoteSpelling::new('G', Some(Accidental::Sharp)),
            9 => NoteSpelling::new('A', None),
            10 => NoteSpelling::new('A', Some(Accidental::Sharp)),
            11 => NoteSpelling::new('B', None),
            _ => NoteSpelling::new('C', None),
        }
    }

    /// Default flat spelling for a semitone
    fn default_flat_spelling(semitone: u8) -> NoteSpelling {
        match semitone {
            0 => NoteSpelling::new('C', None),
            1 => NoteSpelling::new('D', Some(Accidental::Flat)),
            2 => NoteSpelling::new('D', None),
            3 => NoteSpelling::new('E', Some(Accidental::Flat)),
            4 => NoteSpelling::new('E', None),
            5 => NoteSpelling::new('F', None),
            6 => NoteSpelling::new('G', Some(Accidental::Flat)),
            7 => NoteSpelling::new('G', None),
            8 => NoteSpelling::new('A', Some(Accidental::Flat)),
            9 => NoteSpelling::new('A', None),
            10 => NoteSpelling::new('B', Some(Accidental::Flat)),
            11 => NoteSpelling::new('B', None),
            _ => NoteSpelling::new('C', None),
        }
    }

    /// Spell a semitone using strict mode
    pub fn spell_strict(&self, semitone: u8) -> NoteSpelling {
        self.strict_spellings[(semitone % 12) as usize]
    }

    /// Spell a semitone using relaxed mode
    pub fn spell_relaxed(&self, semitone: u8) -> NoteSpelling {
        self.relaxed_spellings[(semitone % 12) as usize]
    }

    /// Spell a semitone using the specified mode
    pub fn spell(&self, semitone: u8, mode: SpellingMode) -> NoteSpelling {
        match mode {
            SpellingMode::Strict => self.spell_strict(semitone),
            SpellingMode::Relaxed => self.spell_relaxed(semitone),
        }
    }

    /// Convert a MusicalNote to its proper spelling in this key
    pub fn respell(&self, note: &MusicalNote, mode: SpellingMode) -> MusicalNote {
        self.spell(note.semitone(), mode).to_note()
    }

    /// Get all scale notes with proper spelling
    pub fn scale_notes(&self, mode: SpellingMode) -> Vec<MusicalNote> {
        let intervals: [u8; 7] = if self.is_major {
            [0, 2, 4, 5, 7, 9, 11]
        } else {
            [0, 2, 3, 5, 7, 8, 10]
        };
        let root_semitone = self.root.semitone();

        intervals
            .iter()
            .map(|&interval| {
                let semitone = (root_semitone + interval) % 12;
                self.spell(semitone, mode).to_note()
            })
            .collect()
    }

    /// Get whether this key prefers sharps
    pub fn prefers_sharps(&self) -> bool {
        self.prefers_sharps
    }

    /// Get the number of accidentals in the key signature
    /// Positive = sharps, negative = flats
    pub fn accidental_count(&self) -> i8 {
        self.accidental_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(s: &str) -> MusicalNote {
        MusicalNote::from_string(s).unwrap()
    }

    #[test]
    fn test_c_major_scale() {
        let spelling = KeySpelling::major(&note("C"));
        let scale: Vec<String> = spelling
            .scale_notes(SpellingMode::Strict)
            .iter()
            .map(|n| n.name())
            .collect();

        assert_eq!(scale, vec!["C", "D", "E", "F", "G", "A", "B"]);
    }

    #[test]
    fn test_g_major_scale() {
        let spelling = KeySpelling::major(&note("G"));
        let scale: Vec<String> = spelling
            .scale_notes(SpellingMode::Strict)
            .iter()
            .map(|n| n.name())
            .collect();

        // G major has F#, not Gb
        assert_eq!(scale, vec!["G", "A", "B", "C", "D", "E", "F#"]);
    }

    #[test]
    fn test_e_major_scale() {
        let spelling = KeySpelling::major(&note("E"));
        let scale: Vec<String> = spelling
            .scale_notes(SpellingMode::Strict)
            .iter()
            .map(|n| n.name())
            .collect();

        // E major: E F# G# A B C# D#
        assert_eq!(scale, vec!["E", "F#", "G#", "A", "B", "C#", "D#"]);
    }

    #[test]
    fn test_f_major_scale() {
        let spelling = KeySpelling::major(&note("F"));
        let scale: Vec<String> = spelling
            .scale_notes(SpellingMode::Strict)
            .iter()
            .map(|n| n.name())
            .collect();

        // F major has Bb, not A#
        assert_eq!(scale, vec!["F", "G", "A", "Bb", "C", "D", "E"]);
    }

    #[test]
    fn test_bb_major_scale() {
        let spelling = KeySpelling::major(&note("Bb"));
        let scale: Vec<String> = spelling
            .scale_notes(SpellingMode::Strict)
            .iter()
            .map(|n| n.name())
            .collect();

        // Bb major: Bb C D Eb F G A
        assert_eq!(scale, vec!["Bb", "C", "D", "Eb", "F", "G", "A"]);
    }

    #[test]
    fn test_gb_major_scale() {
        let spelling = KeySpelling::major(&note("Gb"));
        let scale: Vec<String> = spelling
            .scale_notes(SpellingMode::Strict)
            .iter()
            .map(|n| n.name())
            .collect();

        // Gb major: Gb Ab Bb Cb Db Eb F
        assert_eq!(scale, vec!["Gb", "Ab", "Bb", "Cb", "Db", "Eb", "F"]);
    }

    #[test]
    fn test_f_sharp_major_scale() {
        let spelling = KeySpelling::major(&note("F#"));
        let scale: Vec<String> = spelling
            .scale_notes(SpellingMode::Strict)
            .iter()
            .map(|n| n.name())
            .collect();

        // F# major: F# G# A# B C# D# E#
        assert_eq!(scale, vec!["F#", "G#", "A#", "B", "C#", "D#", "E#"]);
    }

    #[test]
    fn test_c_sharp_major_scale_strict() {
        let spelling = KeySpelling::major(&note("C#"));
        let scale: Vec<String> = spelling
            .scale_notes(SpellingMode::Strict)
            .iter()
            .map(|n| n.name())
            .collect();

        // C# major strict: C# D# E# F# G# A# B#
        assert_eq!(scale, vec!["C#", "D#", "E#", "F#", "G#", "A#", "B#"]);
    }

    #[test]
    fn test_c_sharp_major_scale_relaxed() {
        let spelling = KeySpelling::major(&note("C#"));
        let scale: Vec<String> = spelling
            .scale_notes(SpellingMode::Relaxed)
            .iter()
            .map(|n| n.name())
            .collect();

        // C# major relaxed: C# D# F F# G# A# C (avoids E# and B#)
        assert_eq!(scale, vec!["C#", "D#", "F", "F#", "G#", "A#", "C"]);
    }

    #[test]
    fn test_cb_major_scale_strict() {
        let spelling = KeySpelling::major(&note("Cb"));
        let scale: Vec<String> = spelling
            .scale_notes(SpellingMode::Strict)
            .iter()
            .map(|n| n.name())
            .collect();

        // Cb major strict: Cb Db Eb Fb Gb Ab Bb
        assert_eq!(scale, vec!["Cb", "Db", "Eb", "Fb", "Gb", "Ab", "Bb"]);
    }

    #[test]
    fn test_cb_major_scale_relaxed() {
        let spelling = KeySpelling::major(&note("Cb"));
        let scale: Vec<String> = spelling
            .scale_notes(SpellingMode::Relaxed)
            .iter()
            .map(|n| n.name())
            .collect();

        // Cb major relaxed: B Db Eb E Gb Ab Bb (avoids Cb and Fb)
        assert_eq!(scale, vec!["B", "Db", "Eb", "E", "Gb", "Ab", "Bb"]);
    }

    #[test]
    fn test_key_type_detection() {
        // Sharp keys
        assert!(KeySpelling::major(&note("G")).prefers_sharps());
        assert!(KeySpelling::major(&note("D")).prefers_sharps());
        assert!(KeySpelling::major(&note("A")).prefers_sharps());
        assert!(KeySpelling::major(&note("E")).prefers_sharps());
        assert!(KeySpelling::major(&note("B")).prefers_sharps());
        assert!(KeySpelling::major(&note("F#")).prefers_sharps());
        assert!(KeySpelling::major(&note("C#")).prefers_sharps());

        // Flat keys
        assert!(!KeySpelling::major(&note("F")).prefers_sharps());
        assert!(!KeySpelling::major(&note("Bb")).prefers_sharps());
        assert!(!KeySpelling::major(&note("Eb")).prefers_sharps());
        assert!(!KeySpelling::major(&note("Ab")).prefers_sharps());
        assert!(!KeySpelling::major(&note("Db")).prefers_sharps());
        assert!(!KeySpelling::major(&note("Gb")).prefers_sharps());
        assert!(!KeySpelling::major(&note("Cb")).prefers_sharps());
    }

    #[test]
    fn test_accidental_count() {
        assert_eq!(KeySpelling::major(&note("C")).accidental_count(), 0);
        assert_eq!(KeySpelling::major(&note("G")).accidental_count(), 1);
        assert_eq!(KeySpelling::major(&note("D")).accidental_count(), 2);
        assert_eq!(KeySpelling::major(&note("A")).accidental_count(), 3);
        assert_eq!(KeySpelling::major(&note("E")).accidental_count(), 4);
        assert_eq!(KeySpelling::major(&note("B")).accidental_count(), 5);
        assert_eq!(KeySpelling::major(&note("F#")).accidental_count(), 6);
        assert_eq!(KeySpelling::major(&note("C#")).accidental_count(), 7);

        assert_eq!(KeySpelling::major(&note("F")).accidental_count(), -1);
        assert_eq!(KeySpelling::major(&note("Bb")).accidental_count(), -2);
        assert_eq!(KeySpelling::major(&note("Eb")).accidental_count(), -3);
        assert_eq!(KeySpelling::major(&note("Ab")).accidental_count(), -4);
        assert_eq!(KeySpelling::major(&note("Db")).accidental_count(), -5);
        assert_eq!(KeySpelling::major(&note("Gb")).accidental_count(), -6);
        assert_eq!(KeySpelling::major(&note("Cb")).accidental_count(), -7);
    }

    #[test]
    fn test_respell_note() {
        let e_major = KeySpelling::major(&note("E"));

        // F# in E major stays F#
        let f_sharp = note("F#");
        let respelled = e_major.respell(&f_sharp, SpellingMode::Strict);
        assert_eq!(respelled.name(), "F#");

        // Gb in E major becomes F# (since E major uses sharps)
        let g_flat = note("Gb");
        let respelled = e_major.respell(&g_flat, SpellingMode::Strict);
        assert_eq!(respelled.name(), "F#");
    }

    #[test]
    fn test_chromatic_spellings() {
        // In E major, chromatic notes should use sharps
        let e_major = KeySpelling::major(&note("E"));

        // C natural (not in E major scale) should be C
        assert_eq!(e_major.spell_strict(0).to_string(), "C");

        // D natural (not in E major scale) should be D
        assert_eq!(e_major.spell_strict(2).to_string(), "D");

        // In F major, chromatic notes should use flats
        let f_major = KeySpelling::major(&note("F"));

        // The note between A and B in F major should be Bb (in the scale)
        assert_eq!(f_major.spell_strict(10).to_string(), "Bb");
    }

    #[test]
    fn test_minor_key() {
        let a_minor = KeySpelling::minor(&note("A"));
        let scale: Vec<String> = a_minor
            .scale_notes(SpellingMode::Strict)
            .iter()
            .map(|n| n.name())
            .collect();

        // A minor (natural): A B C D E F G
        assert_eq!(scale, vec!["A", "B", "C", "D", "E", "F", "G"]);
    }

    #[test]
    fn test_e_minor_key() {
        let e_minor = KeySpelling::minor(&note("E"));
        let scale: Vec<String> = e_minor
            .scale_notes(SpellingMode::Strict)
            .iter()
            .map(|n| n.name())
            .collect();

        // E minor has F#
        assert_eq!(scale, vec!["E", "F#", "G", "A", "B", "C", "D"]);
    }

    #[test]
    fn test_d_minor_key() {
        let d_minor = KeySpelling::minor(&note("D"));
        let scale: Vec<String> = d_minor
            .scale_notes(SpellingMode::Strict)
            .iter()
            .map(|n| n.name())
            .collect();

        // D minor has Bb
        assert_eq!(scale, vec!["D", "E", "F", "G", "A", "Bb", "C"]);
    }

    #[test]
    fn test_survey_all_major_keys() {
        println!("\n=== Major Key Spellings Survey ===");

        let keys = [
            "C", "G", "D", "A", "E", "B", "F#", "C#", "F", "Bb", "Eb", "Ab", "Db", "Gb", "Cb",
        ];

        for key_name in keys {
            let root = note(key_name);
            let spelling = KeySpelling::major(&root);
            let strict: Vec<String> = spelling
                .scale_notes(SpellingMode::Strict)
                .iter()
                .map(|n| n.name())
                .collect();
            let relaxed: Vec<String> = spelling
                .scale_notes(SpellingMode::Relaxed)
                .iter()
                .map(|n| n.name())
                .collect();

            println!(
                "{} major ({}{}): strict={:?}",
                key_name,
                if spelling.prefers_sharps() { "#" } else { "b" },
                spelling.accidental_count().abs(),
                strict
            );
            if strict != relaxed {
                println!("         relaxed={:?}", relaxed);
            }
        }
    }
}
