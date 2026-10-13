//! Root notation - unified representation for note names, scale degrees, and roman numerals
//!
//! This module provides a single RootNotation type that can represent any of the three
//! ways to specify a chord root, while preserving the original format for display.

use super::accidental::Accidental;
use super::note::{MusicalNote, MusicalNoteToken, Note};
use super::roman_numeral::RomanNumeralToken;
use facet::Facet;
use std::fmt;

/// Case for Roman numeral notation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum RomanCase {
    Upper, // I, II, III, IV, V, VI, VII (typically major)
    Lower, // i, ii, iii, iv, v, vi, vii (typically minor)
}

/// The original format used to specify the root
#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum RootFormat {
    /// Scale degree (1-7) with optional accidental (e.g. `b3`, `#4`)
    ScaleDegree {
        degree: u8,
        accidental: Option<Accidental>,
    },
    /// Roman numeral (I-VII or i-vii) with optional leading accidental (e.g. `bIII`, `#IV`)
    RomanNumeral {
        degree: u8,
        case: RomanCase,
        accidental: Option<Accidental>,
    },
    /// Explicit note name (C, D#, Eb, etc.)
    NoteName(String),
    /// Empty/rest - no root note (used for rhythm-only entries like rests)
    Empty,
}

/// Unified root notation that works for all three input formats
///
/// This struct stores both the resolved note and the original format,
/// making all three formats 100% interchangeable while preserving
/// how the user originally wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
pub struct RootNotation {
    /// The resolved note (may be None if key-relative and not yet resolved)
    resolved_note: Option<MusicalNote>,
    /// The original format as written by the user
    original_format: RootFormat,
    /// Applied / secondary-chord target (`V/V` = "five of five"). When set, this
    /// root is resolved relative to `applied_target` treated as a temporary
    /// (major) tonic, rather than against the song key, and it displays as
    /// `numerator/target`. Only Roman-over-Roman roots set this; an ordinary
    /// slash bass stays on the chord's `bass` field. Stored as a `RootFormat`
    /// (not a boxed `RootNotation`) to keep the type non-recursive for `Facet`.
    #[facet(default)]
    applied_target: Option<RootFormat>,
}

impl RootNotation {
    /// Create an empty root notation (for rests and rhythm-only entries)
    pub fn empty() -> Self {
        Self {
            resolved_note: None,
            original_format: RootFormat::Empty,
            applied_target: None,
        }
    }

    /// Create from a scale degree with optional accidental
    pub fn from_scale_degree(degree: u8, accidental: Option<Accidental>) -> Self {
        assert!((1..=7).contains(&degree), "Scale degree must be 1-7");
        Self {
            resolved_note: None, // Needs key context to resolve
            original_format: RootFormat::ScaleDegree { degree, accidental },
            applied_target: None,
        }
    }

    /// Create from a roman numeral (no accidental)
    pub fn from_roman_numeral(degree: u8, case: RomanCase) -> Self {
        Self::from_roman_numeral_with_accidental(degree, case, None)
    }

    /// Create from a roman numeral with optional leading accidental
    pub fn from_roman_numeral_with_accidental(
        degree: u8,
        case: RomanCase,
        accidental: Option<Accidental>,
    ) -> Self {
        assert!(
            (1..=7).contains(&degree),
            "Roman numeral degree must be 1-7"
        );
        Self {
            resolved_note: None, // Needs key context to resolve
            original_format: RootFormat::RomanNumeral {
                degree,
                case,
                accidental,
            },
            applied_target: None,
        }
    }

    /// Create from an explicit note name
    pub fn from_note_name(note: MusicalNote) -> Self {
        Self {
            resolved_note: Some(note.clone()),
            original_format: RootFormat::NoteName(note.name()),
            applied_target: None,
        }
    }

    /// Create from a parsed note name token
    pub fn from_note_token(token: MusicalNoteToken) -> Self {
        let note = MusicalNote::from_letter_and_accidental(token.letter, token.accidental);
        Self::from_note_name(note)
    }

    /// Create from a parsed Roman numeral token
    pub fn from_roman_token(token: RomanNumeralToken) -> Self {
        Self::from_roman_numeral_with_accidental(token.degree, token.case, token.accidental)
    }

    /// Parse from a string (auto-detect format)
    ///
    /// Recognizes optional leading `b`, `bb`, `#`, `##` accidental for scale degrees
    /// and Roman numerals (e.g. `b3`, `#IV`, `bbVII`).
    pub fn from_string(s: &str) -> Option<Self> {
        // Key-relative forms own the leading lowercase-`b` / `#` grammar
        // (`b3`, `#IV`). Try them before note names so `b3` doesn't get parsed
        // as a B-something note. Note names start with uppercase A-G and so
        // never have a leading-accidental prefix in this scheme.
        let (accidental, rest) = split_leading_accidental(s);

        if let Ok(degree) = rest.parse::<u8>()
            && (1..=7).contains(&degree)
        {
            return Some(Self::from_scale_degree(degree, accidental));
        }

        if let Some((degree, case)) = Self::parse_roman_numeral(rest) {
            return Some(Self::from_roman_numeral_with_accidental(
                degree, case, accidental,
            ));
        }

        // Fall back to note name (e.g. "Eb", "F#", "C")
        if let Some(note) = MusicalNote::from_string(s) {
            return Some(Self::from_note_name(note));
        }

        None
    }

    /// Parse a roman numeral string (no accidental prefix)
    fn parse_roman_numeral(s: &str) -> Option<(u8, RomanCase)> {
        if s.is_empty() {
            return None;
        }
        let upper = s.to_uppercase();
        let case = if s.chars().all(|c| !c.is_lowercase() || !c.is_alphabetic()) {
            RomanCase::Upper
        } else {
            RomanCase::Lower
        };

        let degree = match upper.as_str() {
            "I" => 1,
            "II" => 2,
            "III" => 3,
            "IV" => 4,
            "V" => 5,
            "VI" => 6,
            "VII" => 7,
            _ => return None,
        };

        Some((degree, case))
    }

    /// Check if this notation is key-relative (needs key context to resolve to a note)
    pub fn is_key_relative(&self) -> bool {
        matches!(
            self.original_format,
            RootFormat::ScaleDegree { .. } | RootFormat::RomanNumeral { .. }
        )
    }

    /// Whether this root was written as a Roman numeral.
    pub fn is_roman(&self) -> bool {
        matches!(self.original_format, RootFormat::RomanNumeral { .. })
    }

    /// Mark this root as an applied / secondary chord whose `target` (another
    /// Roman numeral) is tonicised — `V/V`. The target is resolved against the
    /// song key, then this root is resolved against that target as a major
    /// tonic. See [`RootNotation::resolve`].
    #[must_use]
    pub fn with_applied_target(mut self, target: RootNotation) -> Self {
        self.applied_target = Some(target.original_format);
        self
    }

    /// The applied-chord target's `RootFormat`, if this is a secondary chord.
    pub fn applied_target(&self) -> Option<&RootFormat> {
        self.applied_target.as_ref()
    }

    /// The plain diatonic scale-degree number (1-7) if this root is a bare
    /// Nashville number with no accidental. Returns `None` for note names,
    /// Roman numerals (their case already implies quality), chromatic degrees
    /// like `b3`, or out-of-range numbers. Used to apply the key's implied
    /// quality to bare number chords.
    pub fn diatonic_scale_degree(&self) -> Option<u8> {
        match &self.original_format {
            RootFormat::ScaleDegree {
                degree,
                accidental: None,
            } if (1..=7).contains(degree) => Some(*degree),
            _ => None,
        }
    }

    /// Get the accidental modifier on a key-relative root, if any.
    /// Returns `None` for note names, `Empty`, or unmodified scale degrees / numerals.
    pub fn accidental(&self) -> Option<Accidental> {
        match &self.original_format {
            RootFormat::ScaleDegree { accidental, .. }
            | RootFormat::RomanNumeral { accidental, .. } => *accidental,
            _ => None,
        }
    }

    /// Get the roman case if this is a roman numeral root
    pub fn roman_case(&self) -> Option<RomanCase> {
        match &self.original_format {
            RootFormat::RomanNumeral { case, .. } => Some(*case),
            _ => None,
        }
    }

    /// Get the original format
    pub fn original_format(&self) -> &RootFormat {
        &self.original_format
    }

    /// Get the resolved note (if available)
    pub fn resolved_note(&self) -> Option<&MusicalNote> {
        self.resolved_note.as_ref()
    }

    /// Resolve to a MusicalNote (cloned), optionally using a key context
    ///
    /// For note names, returns the note directly.
    /// For scale degrees and roman numerals, requires a Key to resolve.
    ///
    /// # Arguments
    /// * `key` - Optional key context for resolving scale degrees/roman numerals
    ///
    /// # Returns
    /// * `Some(MusicalNote)` if resolved successfully
    /// * `None` if key context is required but not provided
    pub fn resolve(&self, key: Option<&crate::key::Key>) -> Option<MusicalNote> {
        // Applied / secondary chord (`V/V`): resolve the target against the song
        // key to get a temporary tonic, then resolve the numerator against that
        // tonic (treated as major). `V/V` in C → target V = G → numerator V in G
        // major = D.
        if let Some(target_format) = &self.applied_target {
            let target = RootNotation {
                resolved_note: None,
                original_format: target_format.clone(),
                applied_target: None,
            };
            let tonic = target.resolve(key)?;
            let applied_key = crate::key::Key::major(tonic);
            let numerator = RootNotation {
                resolved_note: None,
                original_format: self.original_format.clone(),
                applied_target: None,
            };
            return numerator.resolve(Some(&applied_key));
        }

        // If already resolved, return it
        if let Some(ref note) = self.resolved_note {
            return Some(note.clone());
        }

        // For note names without resolved_note, shouldn't happen but handle it
        if matches!(self.original_format, RootFormat::NoteName(_)) {
            return None; // This should have been set during construction
        }

        // For scale degrees and roman numerals, need a key
        let key = key?;

        let (degree, accidental) = match &self.original_format {
            RootFormat::ScaleDegree { degree, accidental } => (*degree, *accidental),
            RootFormat::RomanNumeral {
                degree, accidental, ..
            } => (*degree, *accidental),
            RootFormat::NoteName(_) => unreachable!(),
            RootFormat::Empty => return None, // Empty root has no note
        };

        let resolved = key.get_scale_degree(degree)?;
        Some(apply_accidental(
            MusicalNote::new(resolved.name(), resolved.semitone()),
            accidental,
        ))
    }

    /// Resolve to an actual note given a key context
    /// For key-relative notations, this computes the note from the scale degree
    pub fn resolve_with_key(&mut self, key: &crate::key::Key) -> Result<&MusicalNote, String> {
        if let Some(ref note) = self.resolved_note {
            return Ok(note);
        }

        let (degree, accidental) = match &self.original_format {
            RootFormat::ScaleDegree { degree, accidental } => (*degree, *accidental),
            RootFormat::RomanNumeral {
                degree, accidental, ..
            } => (*degree, *accidental),
            RootFormat::NoteName(_) => {
                return Err("Note name should already be resolved".to_string());
            }
            RootFormat::Empty => {
                return Err("Empty root notation has no note to resolve".to_string());
            }
        };

        let resolved = key
            .get_scale_degree(degree)
            .ok_or_else(|| format!("Could not resolve scale degree {} in key", degree))?;

        let note = apply_accidental(
            MusicalNote::new(resolved.name(), resolved.semitone()),
            accidental,
        );
        self.resolved_note = Some(note);

        Ok(self.resolved_note.as_ref().unwrap())
    }

    /// Get the scale degree if this is a key-relative notation (1-7)
    pub fn scale_degree(&self) -> Option<u8> {
        match &self.original_format {
            RootFormat::ScaleDegree { degree, .. } => Some(*degree),
            RootFormat::RomanNumeral { degree, .. } => Some(*degree),
            RootFormat::NoteName(_) | RootFormat::Empty => None,
        }
    }

    /// Clone the root notation
    pub fn clone_notation(&self) -> Self {
        self.clone()
    }

    /// Convert this root notation to LilyPond format
    ///
    /// # Arguments
    /// * `key` - Optional key context for resolving scale degrees and roman numerals
    ///
    /// # Returns
    /// LilyPond note name (e.g., "cis", "des", "f")
    pub fn to_lilypond(&self, key: Option<&crate::key::Key>) -> String {
        // Try to resolve the note
        if let Some(note) = self.resolve(key) {
            return note.to_lilypond();
        }

        // Fallback: if it's already resolved, use it
        if let Some(note) = self.resolved_note() {
            return note.to_lilypond();
        }

        // Last resort: use a placeholder
        "c".to_string()
    }
}

/// Strip a leading accidental marker (`bb`, `b`, `##`, `#`) from `s` and return
/// the parsed accidental (if any) plus the rest of the string.
fn split_leading_accidental(s: &str) -> (Option<Accidental>, &str) {
    if let Some(rest) = s.strip_prefix("bb") {
        (Some(Accidental::DoubleFlat), rest)
    } else if let Some(rest) = s.strip_prefix("##") {
        (Some(Accidental::DoubleSharp), rest)
    } else if let Some(rest) = s.strip_prefix('b') {
        (Some(Accidental::Flat), rest)
    } else if let Some(rest) = s.strip_prefix('#') {
        (Some(Accidental::Sharp), rest)
    } else {
        (None, s)
    }
}

fn accidental_prefix(acc: Option<Accidental>) -> &'static str {
    match acc {
        Some(a) => a.to_str(),
        None => "",
    }
}

/// Apply an accidental's semitone offset to `note`, returning a new `MusicalNote`.
/// Re-spells using sharps when raising and flats when lowering, to mirror common
/// roman-numeral conventions (`b3` → flatted, `#4` → sharped).
fn apply_accidental(note: MusicalNote, acc: Option<Accidental>) -> MusicalNote {
    let Some(acc) = acc else { return note };
    let offset = acc.semitone_offset();
    if offset == 0 {
        return note;
    }
    let new_semitone = ((note.semitone() as i16 + offset as i16).rem_euclid(12)) as u8;
    let prefer_sharp = offset > 0;
    MusicalNote::from_semitone(new_semitone, prefer_sharp)
}

impl fmt::Display for RootNotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let display_str = match &self.original_format {
            RootFormat::ScaleDegree { degree, accidental } => {
                format!("{}{}", accidental_prefix(*accidental), degree)
            }
            RootFormat::RomanNumeral {
                degree,
                case,
                accidental,
            } => {
                let base = match *degree {
                    1 => "I",
                    2 => "II",
                    3 => "III",
                    4 => "IV",
                    5 => "V",
                    6 => "VI",
                    7 => "VII",
                    _ => "?",
                };
                let numeral = match case {
                    RomanCase::Upper => base.to_string(),
                    RomanCase::Lower => base.to_lowercase(),
                };
                format!("{}{}", accidental_prefix(*accidental), numeral)
            }
            RootFormat::NoteName(name) => name.clone(),
            RootFormat::Empty => String::new(), // Empty display for rests
        };
        write!(f, "{}", display_str)?;
        // Applied / secondary chord renders `numerator/target` (`V/V`).
        if let Some(target_format) = &self.applied_target {
            let target = RootNotation {
                resolved_note: None,
                original_format: target_format.clone(),
                applied_target: None,
            };
            write!(f, "/{}", target)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_scale_degree() {
        let root = RootNotation::from_scale_degree(4, None);
        assert!(root.is_key_relative());
        assert_eq!(format!("{}", root), "4");
    }

    #[test]
    fn test_from_roman_numeral() {
        let root = RootNotation::from_roman_numeral(5, RomanCase::Upper);
        assert!(root.is_key_relative());
        assert_eq!(format!("{}", root), "V");

        let root = RootNotation::from_roman_numeral(2, RomanCase::Lower);
        assert_eq!(format!("{}", root), "ii");
    }

    #[test]
    fn test_from_note_name() {
        let note = MusicalNote::from_string("C#").unwrap();
        let root = RootNotation::from_note_name(note);
        assert!(!root.is_key_relative());
        assert_eq!(format!("{}", root), "C#");
        assert!(root.resolved_note().is_some());
    }

    #[test]
    fn test_from_string_scale_degree() {
        let root = RootNotation::from_string("4").unwrap();
        assert!(root.is_key_relative());
        assert_eq!(format!("{}", root), "4");
    }

    #[test]
    fn test_from_string_roman_numeral() {
        let root = RootNotation::from_string("V").unwrap();
        assert!(root.is_key_relative());
        assert_eq!(format!("{}", root), "V");

        let root = RootNotation::from_string("vi").unwrap();
        assert_eq!(format!("{}", root), "vi");
    }

    #[test]
    fn test_from_string_note_name() {
        let root = RootNotation::from_string("Eb").unwrap();
        assert!(!root.is_key_relative());
        assert_eq!(format!("{}", root), "Eb");
    }

    #[test]
    fn test_scale_degree_with_accidental_round_trips() {
        let root = RootNotation::from_scale_degree(3, Some(Accidental::Flat));
        assert_eq!(root.accidental(), Some(Accidental::Flat));
        assert_eq!(format!("{}", root), "b3");

        let parsed = RootNotation::from_string("b3").unwrap();
        assert_eq!(parsed.accidental(), Some(Accidental::Flat));
        assert_eq!(parsed.scale_degree(), Some(3));
        assert_eq!(format!("{}", parsed), "b3");

        let sharp = RootNotation::from_string("#4").unwrap();
        assert_eq!(sharp.accidental(), Some(Accidental::Sharp));
        assert_eq!(format!("{}", sharp), "#4");
    }

    #[test]
    fn test_roman_numeral_with_accidental_round_trips() {
        let parsed = RootNotation::from_string("bIII").unwrap();
        assert_eq!(parsed.accidental(), Some(Accidental::Flat));
        assert_eq!(parsed.scale_degree(), Some(3));
        assert_eq!(format!("{}", parsed), "bIII");

        let parsed = RootNotation::from_string("#iv").unwrap();
        assert_eq!(parsed.accidental(), Some(Accidental::Sharp));
        assert_eq!(format!("{}", parsed), "#iv");
    }

    #[test]
    fn test_accidental_applied_on_resolve() {
        use crate::key::{Key, scale::ScaleMode};
        // C major: scale degree 3 = E. b3 should resolve to Eb.
        let key = Key::new(MusicalNote::from_string("C").unwrap(), ScaleMode::ionian());

        let plain = RootNotation::from_scale_degree(3, None);
        let flat = RootNotation::from_scale_degree(3, Some(Accidental::Flat));

        let plain_note = plain.resolve(Some(&key)).unwrap();
        let flat_note = flat.resolve(Some(&key)).unwrap();

        assert_eq!(plain_note.semitone(), 4); // E
        assert_eq!(flat_note.semitone(), 3); // Eb
    }

    #[test]
    fn test_interchangeable() {
        // All three formats should work the same way from the application's perspective
        let scale_degree = RootNotation::from_string("4").unwrap();
        let roman = RootNotation::from_string("IV").unwrap();
        let note = RootNotation::from_string("F").unwrap();

        // All are RootNotation types - completely interchangeable
        assert!(scale_degree.is_key_relative());
        assert!(roman.is_key_relative());
        assert!(!note.is_key_relative());

        // But they remember their original format
        assert_eq!(format!("{}", scale_degree), "4");
        assert_eq!(format!("{}", roman), "IV");
        assert_eq!(format!("{}", note), "F");
    }
}
