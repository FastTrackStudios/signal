//! Musical tokens for parsing
//!
//! Domain-specific tokens that represent musical elements

use super::accidental::Accidental;
use super::root_notation::RomanCase;

/// Musical tokens for parsing chord and rhythm notation
#[derive(Debug, Clone, PartialEq)]
pub enum MusicalToken {
    /// Scale degree (1-7)
    ScaleDegree(u8),

    /// Roman numeral (I-VII or i-vii)
    RomanNumeral(u8, RomanCase),

    /// Note name (C, D, E, F, G, A, B)
    NoteName(String),

    /// Accidental modifier
    Accidental(Accidental),

    /// Chord quality (maj7, m7, sus4, add9, etc.)
    Quality(String),

    /// Slash for bass notes
    Slash,

    /// Duration notation (2, 4., /2, //)
    Duration(String),

    /// Space/rest marker
    Space,

    /// Rest marker
    Rest,

    /// Hash/pound for key signature
    Hash,

    /// Comma separator
    Comma,

    /// Number (for measures, etc.)
    Number(u32),
}

impl MusicalToken {
    /// Check if this token represents a root note
    pub fn is_root_note(&self) -> bool {
        matches!(
            self,
            MusicalToken::ScaleDegree(_)
                | MusicalToken::RomanNumeral(_, _)
                | MusicalToken::NoteName(_)
        )
    }

    /// Check if this token is an accidental
    pub fn is_accidental(&self) -> bool {
        matches!(self, MusicalToken::Accidental(_))
    }

    /// Check if this token is a quality modifier
    pub fn is_quality(&self) -> bool {
        matches!(self, MusicalToken::Quality(_))
    }
}

impl std::fmt::Display for MusicalToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MusicalToken::ScaleDegree(d) => write!(f, "{}", d),
            MusicalToken::RomanNumeral(d, case) => {
                let base = match d {
                    1 => "I",
                    2 => "II",
                    3 => "III",
                    4 => "IV",
                    5 => "V",
                    6 => "VI",
                    7 => "VII",
                    _ => "?",
                };
                match case {
                    RomanCase::Upper => write!(f, "{}", base),
                    RomanCase::Lower => write!(f, "{}", base.to_lowercase()),
                }
            }
            MusicalToken::NoteName(s) => write!(f, "{}", s),
            MusicalToken::Accidental(acc) => match acc {
                Accidental::Sharp => write!(f, "#"),
                Accidental::Flat => write!(f, "b"),
                Accidental::DoubleSharp => write!(f, "##"),
                Accidental::DoubleFlat => write!(f, "bb"),
                Accidental::Natural => write!(f, "♮"),
            },
            MusicalToken::Quality(s) => write!(f, "{}", s),
            MusicalToken::Slash => write!(f, "/"),
            MusicalToken::Duration(s) => write!(f, "{}", s),
            MusicalToken::Space => write!(f, "s"),
            MusicalToken::Rest => write!(f, "r"),
            MusicalToken::Hash => write!(f, "#"),
            MusicalToken::Comma => write!(f, ","),
            MusicalToken::Number(n) => write!(f, "{}", n),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_root_note() {
        assert!(MusicalToken::ScaleDegree(4).is_root_note());
        assert!(MusicalToken::RomanNumeral(5, RomanCase::Upper).is_root_note());
        assert!(MusicalToken::NoteName("C".to_string()).is_root_note());
        assert!(!MusicalToken::Quality("maj7".to_string()).is_root_note());
    }

    #[test]
    fn test_is_accidental() {
        assert!(MusicalToken::Accidental(Accidental::Sharp).is_accidental());
        assert!(!MusicalToken::NoteName("C".to_string()).is_accidental());
    }

    #[test]
    fn test_display() {
        assert_eq!(MusicalToken::ScaleDegree(4).to_string(), "4");
        assert_eq!(
            MusicalToken::RomanNumeral(5, RomanCase::Upper).to_string(),
            "V"
        );
        assert_eq!(
            MusicalToken::RomanNumeral(2, RomanCase::Lower).to_string(),
            "ii"
        );
        assert_eq!(MusicalToken::NoteName("C#".to_string()).to_string(), "C#");
        assert_eq!(MusicalToken::Accidental(Accidental::Sharp).to_string(), "#");
    }
}
