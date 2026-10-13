//! Musical key implementation
//!
//! Represents a musical key as a root note + scale mode

use super::scale::{ScaleMode, ScaleType};
use crate::primitives::{Interval, MusicalNote, Note};
use facet::Facet;
use std::fmt;

/// A musical key - just a root note and a scale mode
#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
pub struct Key {
    pub root: MusicalNote,
    pub mode: ScaleMode,
}

impl Key {
    /// Create a new key from a root note and scale mode
    pub fn new(root: MusicalNote, mode: ScaleMode) -> Self {
        Self { root, mode }
    }

    /// Convenience constructor for major keys (Ionian mode)
    pub fn major(root: MusicalNote) -> Self {
        Self::new(root, ScaleMode::ionian())
    }

    /// Convenience constructor for minor keys (Aeolian mode)
    pub fn minor(root: MusicalNote) -> Self {
        Self::new(root, ScaleMode::aeolian())
    }

    /// The diatonic triad quality of a scale degree (1-7) in this key — e.g.
    /// in C major degree 2 is minor and degree 7 is diminished. Returns `None`
    /// for out-of-range degrees. Used to give bare number-system chords their
    /// implied quality (`2` = ii minor) without writing it out.
    pub fn diatonic_quality(&self, degree: u8) -> Option<crate::chord::ChordQuality> {
        use super::scale::harmonization::{HarmonizationDepth, harmonize_scale};
        if !(1..=7).contains(&degree) {
            return None;
        }
        let chords = harmonize_scale(&self.mode, &self.root, HarmonizationDepth::Triads);
        chords.get((degree - 1) as usize).map(|c| c.quality)
    }

    /// Get the root note of the key
    pub fn root(&self) -> &MusicalNote {
        &self.root
    }

    /// Get the scale type (derived from the mode)
    pub fn scale_type(&self) -> ScaleType {
        self.mode.scale_type()
    }

    /// Get all notes in the scale
    pub fn notes(&self) -> Vec<MusicalNote> {
        let pattern = self.mode.interval_pattern();
        pattern
            .iter()
            .map(|&semitones| {
                let note_semitone = (self.root.semitone() + semitones) % 12;
                let prefer_sharp = self.root.name().contains('#');
                MusicalNote::from_semitone(note_semitone, prefer_sharp)
            })
            .collect()
    }

    /// Get the intervals that define this scale
    pub fn intervals(&self) -> Vec<Interval> {
        let pattern = self.mode.interval_pattern();
        pattern
            .iter()
            .filter_map(|&s| Interval::from_semitones(s))
            .collect()
    }

    /// Get the note for a specific scale degree (1-7)
    pub fn get_scale_degree(&self, degree: u8) -> Option<MusicalNote> {
        if !(1..=7).contains(&degree) {
            return None;
        }

        let pattern = self.mode.interval_pattern();
        let semitones = pattern.get((degree - 1) as usize)?;
        let note_semitone = (self.root.semitone() + semitones) % 12;
        let prefer_sharp = self.root.name().contains('#');
        Some(MusicalNote::from_semitone(note_semitone, prefer_sharp))
    }

    /// Get the scale degree of a given note (returns 1-7, or None if not in scale)
    pub fn degree_of_note(&self, note: &MusicalNote) -> Option<u8> {
        let target_semitone = note.semitone();
        let root_semitone = self.root.semitone();
        let relative_semitone = (target_semitone + 12 - root_semitone) % 12;

        let pattern = self.mode.interval_pattern();
        for (i, &scale_semitone) in pattern.iter().enumerate() {
            if scale_semitone == relative_semitone {
                return Some((i + 1) as u8);
            }
        }

        None
    }

    /// Get the full name of the key (e.g., "C Ionian", "A Aeolian")
    pub fn name(&self) -> String {
        format!("{} {}", self.root.name(), self.mode.name())
    }

    /// Convert this key to LilyPond notation
    ///
    /// # Returns
    /// LilyPond key notation (e.g., "\\key cis \\major", "\\key des \\minor")
    pub fn to_lilypond(&self) -> String {
        use crate::key::scale::ScaleMode;
        let root = self.root().to_lilypond();
        let mode = if self.mode == ScaleMode::ionian() {
            "\\major"
        } else {
            "\\minor"
        };
        format!("\\key {} {}", root, mode)
    }

    /// Get a short name (e.g., "C", "Am", "D Dor")
    pub fn short_name(&self) -> String {
        if self.mode == ScaleMode::ionian() {
            self.root.name()
        } else if self.mode == ScaleMode::aeolian() {
            format!("{}m", self.root.name())
        } else {
            format!("{} {}", self.root.name(), self.mode.short_name())
        }
    }

    /// Parse a key from a string.
    ///
    /// Supported formats:
    /// - `#G` / `bBb` / `C`         — major key (sharp/flat prefix or bare note)
    /// - `#Dm` / `Am` / `bBbm`      — minor key (`m` suffix)
    /// - `#Dmin` / `Amin`            — minor key (`min` suffix)
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();

        if s.is_empty() {
            return Err("Empty key string".to_string());
        }

        // Strip optional # or b prefix (key signature notation)
        let after_prefix = if s.starts_with('#') || s.starts_with('b') {
            &s[1..]
        } else {
            s
        };

        // Detect minor key suffix: "Dm", "Bbm", "Amin", "F#min"
        let (root_str, is_minor) = if after_prefix.ends_with("min") {
            (&after_prefix[..after_prefix.len() - 3], true)
        } else if after_prefix.ends_with('m')
            && !after_prefix.eq_ignore_ascii_case("abm")
            && after_prefix.len() > 1
        {
            // "m" suffix indicates minor, but avoid false positive on note "Abm"
            // which could be confused with Ab-minor vs the note Ab + stray m.
            // We check: is the part before "m" a valid note?
            let candidate = &after_prefix[..after_prefix.len() - 1];
            if MusicalNote::from_string(candidate).is_some() {
                (candidate, true)
            } else {
                (after_prefix, false)
            }
        } else {
            (after_prefix, false)
        };

        // Parse the root note
        let root = MusicalNote::from_string(root_str)
            .ok_or_else(|| format!("Invalid note name: {}", root_str))?;

        if is_minor {
            Ok(Self::minor(root))
        } else {
            Ok(Self::major(root))
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_major_key() {
        let c_major = Key::major(MusicalNote::c());
        assert_eq!(c_major.root().semitone(), 0);
        assert_eq!(c_major.mode, ScaleMode::ionian());
        assert_eq!(c_major.name(), "C Ionian");
    }

    #[test]
    fn test_minor_key() {
        let a_minor = Key::minor(MusicalNote::a());
        assert_eq!(a_minor.root().semitone(), 9);
        assert_eq!(a_minor.mode, ScaleMode::aeolian());
        assert_eq!(a_minor.name(), "A Aeolian");
    }

    #[test]
    fn test_scale_degrees() {
        let c_major = Key::major(MusicalNote::c());

        // Test getting notes by degree
        let tonic = c_major.get_scale_degree(1).unwrap();
        assert_eq!(tonic.semitone(), 0); // C

        let third = c_major.get_scale_degree(3).unwrap();
        assert_eq!(third.semitone(), 4); // E

        let fifth = c_major.get_scale_degree(5).unwrap();
        assert_eq!(fifth.semitone(), 7); // G
    }

    #[test]
    fn test_degree_of_note() {
        let c_major = Key::major(MusicalNote::c());

        let c = MusicalNote::c();
        assert_eq!(c_major.degree_of_note(&c), Some(1));

        let e = MusicalNote::from_string("E").unwrap();
        assert_eq!(c_major.degree_of_note(&e), Some(3));

        let g = MusicalNote::g();
        assert_eq!(c_major.degree_of_note(&g), Some(5));

        // Note not in scale
        let c_sharp = MusicalNote::from_string("C#").unwrap();
        assert_eq!(c_major.degree_of_note(&c_sharp), None);
    }

    #[test]
    fn test_notes() {
        let c_major = Key::major(MusicalNote::c());
        let notes = c_major.notes();

        assert_eq!(notes.len(), 7);
        // C, D, E, F, G, A, B
        assert_eq!(notes[0].semitone(), 0);
        assert_eq!(notes[1].semitone(), 2);
        assert_eq!(notes[2].semitone(), 4);
        assert_eq!(notes[3].semitone(), 5);
        assert_eq!(notes[4].semitone(), 7);
        assert_eq!(notes[5].semitone(), 9);
        assert_eq!(notes[6].semitone(), 11);
    }
}
