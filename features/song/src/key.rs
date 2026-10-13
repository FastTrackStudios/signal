//! [`Key`] — a small typed musical key value (root + mode).
//!
//! Stored as a compact string in frontmatter (e.g. `Bb Minor`,
//! `F# Dorian`, `C Major`) via `#[serde(into/try_from = String)]`, so the
//! on-disk form stays human-readable while the in-memory value is typed.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Diatonic letter name of a key's tonic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Letter {
    C,
    D,
    E,
    F,
    G,
    A,
    B,
}

impl Letter {
    const fn as_str(self) -> &'static str {
        match self {
            Self::C => "C",
            Self::D => "D",
            Self::E => "E",
            Self::F => "F",
            Self::G => "G",
            Self::A => "A",
            Self::B => "B",
        }
    }

    const fn parse(c: char) -> Option<Self> {
        Some(match c.to_ascii_uppercase() {
            'C' => Self::C,
            'D' => Self::D,
            'E' => Self::E,
            'F' => Self::F,
            'G' => Self::G,
            'A' => Self::A,
            'B' => Self::B,
            _ => return None,
        })
    }
}

/// Accidental applied to the tonic letter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Accidental {
    #[default]
    Natural,
    Sharp,
    Flat,
}

impl Accidental {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Natural => "",
            Self::Sharp => "#",
            Self::Flat => "b",
        }
    }
}

/// Mode / quality of the key. Kept small; extend as needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Major,
    Minor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
}

impl Mode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Major => "Major",
            Self::Minor => "Minor",
            Self::Dorian => "Dorian",
            Self::Phrygian => "Phrygian",
            Self::Lydian => "Lydian",
            Self::Mixolydian => "Mixolydian",
            Self::Locrian => "Locrian",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "major" | "maj" | "ionian" => Self::Major,
            "minor" | "min" | "m" | "aeolian" => Self::Minor,
            "dorian" => Self::Dorian,
            "phrygian" => Self::Phrygian,
            "lydian" => Self::Lydian,
            "mixolydian" | "mixo" => Self::Mixolydian,
            "locrian" => Self::Locrian,
            _ => return None,
        })
    }
}

/// A musical key: a tonic (letter + accidental) and a mode.
///
/// Round-trips through a compact display string like `Bb Minor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Key {
    pub letter: Letter,
    pub accidental: Accidental,
    pub mode: Mode,
}

impl Key {
    #[must_use]
    pub const fn new(letter: Letter, accidental: Accidental, mode: Mode) -> Self {
        Self {
            letter,
            accidental,
            mode,
        }
    }

    /// `C Major` — a sensible neutral default.
    #[must_use]
    pub const fn c_major() -> Self {
        Self::new(Letter::C, Accidental::Natural, Mode::Major)
    }
}

impl Default for Key {
    fn default() -> Self {
        Self::c_major()
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{} {}",
            self.letter.as_str(),
            self.accidental.as_str(),
            self.mode.as_str()
        )
    }
}

/// Error parsing a [`Key`] from its string form.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid key `{0}` (expected e.g. `Bb Minor`, `F# Dorian`, `C`)")]
pub struct ParseKeyError(pub String);

impl FromStr for Key {
    type Err = ParseKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        let mut chars = trimmed.chars();
        let letter = chars
            .next()
            .and_then(Letter::parse)
            .ok_or_else(|| ParseKeyError(s.to_string()))?;

        // Optional accidental immediately after the letter.
        let rest = chars.as_str();
        let (accidental, after_acc) = match rest.chars().next() {
            Some(c @ ('#' | '♯')) => (
                Accidental::Sharp,
                rest.get(c.len_utf8()..).unwrap_or_default(),
            ),
            Some(c @ ('b' | '♭')) => (
                Accidental::Flat,
                rest.get(c.len_utf8()..).unwrap_or_default(),
            ),
            _ => (Accidental::Natural, rest),
        };

        let mode_str = after_acc.trim();
        let mode = if mode_str.is_empty() {
            Mode::Major
        } else {
            Mode::parse(mode_str).ok_or_else(|| ParseKeyError(s.to_string()))?
        };

        Ok(Self::new(letter, accidental, mode))
    }
}

impl From<Key> for String {
    fn from(k: Key) -> Self {
        k.to_string()
    }
}

impl TryFrom<String> for Key {
    type Error = ParseKeyError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_common_keys() {
        for s in [
            "C Major",
            "Bb Minor",
            "F# Dorian",
            "A Minor",
            "Eb Mixolydian",
        ] {
            let k: Key = s.parse().unwrap();
            assert_eq!(k.to_string(), s);
        }
    }

    #[test]
    fn bare_root_defaults_to_major() {
        let k: Key = "G".parse().unwrap();
        assert_eq!(k, Key::new(Letter::G, Accidental::Natural, Mode::Major));
    }

    #[test]
    fn tolerant_mode_aliases() {
        assert_eq!("Bb m".parse::<Key>().unwrap().mode, Mode::Minor);
        assert_eq!("C maj".parse::<Key>().unwrap().mode, Mode::Major);
    }

    #[test]
    fn rejects_garbage() {
        assert!("H minor".parse::<Key>().is_err());
        assert!("C blahmode".parse::<Key>().is_err());
    }
}
