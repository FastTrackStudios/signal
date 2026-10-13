//! Chord family - represents the seventh type of a chord
//!
//! Defines what kind of seventh (if any) the chord has

use crate::chord::degree::ChordDegree;
use crate::chord::quality::ChordQuality;
use crate::parsing::{ParseError, Token, TokenType};
use crate::primitives::Interval;
use facet::Facet;

/// Chord family - represents the seventh type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum ChordFamily {
    /// Major seventh (maj7) - adds major 7th to major triad
    Major7,
    /// Dominant seventh (7) - adds minor 7th to major triad
    Dominant7,
    /// Minor seventh (m7) - adds minor 7th to minor triad
    Minor7,
    /// Minor-major seventh (mM7) - adds major 7th to minor triad
    MinorMajor7,
    /// Half-diminished seventh (ø7) - adds minor 7th to diminished triad
    HalfDiminished,
    /// Fully diminished seventh (dim7) - adds diminished 7th to diminished triad
    FullyDiminished,
}

impl ChordFamily {
    /// Get the interval for the seventh in this family
    pub fn seventh_interval(&self) -> Interval {
        match self {
            ChordFamily::Major7 => Interval::MajorSeventh,
            ChordFamily::Dominant7 => Interval::MinorSeventh,
            ChordFamily::Minor7 => Interval::MinorSeventh,
            ChordFamily::MinorMajor7 => Interval::MajorSeventh,
            ChordFamily::HalfDiminished => Interval::MinorSeventh,
            ChordFamily::FullyDiminished => Interval::DiminishedSeventh,
        }
    }

    /// Get all degrees present in this family (just the seventh)
    pub fn degrees(&self) -> Vec<ChordDegree> {
        vec![ChordDegree::Seventh]
    }

    /// Get a symbol for this family's seventh
    pub fn symbol(&self) -> &'static str {
        match self {
            ChordFamily::Major7 => "maj7",
            ChordFamily::Dominant7 => "7",
            ChordFamily::Minor7 => "7", // Combined with quality "m" -> "m7"
            ChordFamily::MinorMajor7 => "Maj7", // Capital M to distinguish: "mMaj7" or "CmMaj7"
            ChordFamily::HalfDiminished => "m7b5", // Half-diminished: minor 7 flat 5
            ChordFamily::FullyDiminished => "dim7",
        }
    }

    /// Parse a chord family (seventh type) from tokens
    /// Returns (Option`<family>`, tokens_consumed)
    ///
    /// Handles: maj7, M7, Maj7, ma7, 7, m7, mM7, mMaj7, ø7, dim7, o7, etc.
    ///
    /// The family is determined by explicit notation in the tokens, not by inferring from quality.
    /// For example:
    /// - "maj7" or "M7" -> Major (or MinorMajor if preceded by minor quality)
    /// - "7" -> Dominant (unless quality is minor/dim, then Minor/HalfDim)
    /// - "dim7" or "o7" -> FullyDiminished
    /// - "ø" or "ø7" -> HalfDiminished
    pub fn parse(
        tokens: &[Token],
        quality: ChordQuality,
    ) -> Result<(Option<ChordFamily>, usize), ParseError> {
        if tokens.is_empty() {
            return Ok((None, 0));
        }

        let mut consumed = 0;

        // Check for "maj7", "M7", "Maj7", "Ma7", etc. (explicit major seventh)
        if consumed < tokens.len()
            && let TokenType::Letter('m') | TokenType::Letter('M') = tokens[consumed].token_type
        {
            let is_upper = matches!(tokens[consumed].token_type, TokenType::Letter('M'));

            // Look for "maj" followed by a number (maj6, maj7, maj9, maj11, maj13)
            if consumed + 2 < tokens.len()
                && let TokenType::Letter('a') = tokens[consumed + 1].token_type
                && let TokenType::Letter('j') = tokens[consumed + 2].token_type
            {
                consumed += 3; // "maj" or "Maj"

                // Look for a number (6, 7, 9, 11, 13)
                if consumed < tokens.len()
                    && let TokenType::Number(n) = &tokens[consumed].token_type
                {
                    // For maj6, maj9, maj11, maj13: consume "maj" but not the number
                    // The number will be handled by additions/extensions parsers
                    // For maj7: consume both "maj" and "7"
                    if n == "7" {
                        consumed += 1;
                        // Determine if it's major seventh or minor-major seventh
                        // based on the chord quality
                        let family = match quality {
                            ChordQuality::Minor => ChordFamily::MinorMajor7,
                            _ => ChordFamily::Major7,
                        };
                        return Ok((Some(family), consumed));
                    } else if n == "6" {
                        // "maj6" - this is a sixth chord, NOT a seventh chord
                        // Consume "maj" but return no family (triad)
                        // The "6" will be handled by the sixth chord parser
                        return Ok((None, consumed));
                    } else if n == "9" || n == "11" || n == "13" {
                        // "maj9", "maj11", "maj13" - these imply maj7 + extensions
                        // Consume only "maj", leave the number for extensions parser
                        let family = match quality {
                            ChordQuality::Minor => ChordFamily::MinorMajor7,
                            _ => ChordFamily::Major7,
                        };
                        return Ok((Some(family), consumed));
                    }
                }
            }

            // Just "M7" (shorthand for maj7)
            if is_upper
                && consumed + 1 < tokens.len()
                && let TokenType::Number(n) = &tokens[consumed + 1].token_type
                && n == "7"
            {
                let family = match quality {
                    ChordQuality::Minor => ChordFamily::MinorMajor7,
                    _ => ChordFamily::Major7,
                };
                return Ok((Some(family), 2));
            }

            // Reset if we didn't find maj7/M7
            consumed = 0;
        }

        // Check for "dim7" or "o7" (fully diminished seventh)
        if consumed < tokens.len() {
            if let TokenType::Letter('d') = tokens[consumed].token_type
                && consumed + 3 < tokens.len()
                && let TokenType::Letter('i') = tokens[consumed + 1].token_type
                && let TokenType::Letter('m') = tokens[consumed + 2].token_type
                && let TokenType::Number(n) = &tokens[consumed + 3].token_type
                && n == "7"
            {
                return Ok((Some(ChordFamily::FullyDiminished), 4));
            }

            if let TokenType::Letter('o') | TokenType::Circle = tokens[consumed].token_type
                && consumed + 1 < tokens.len()
                && let TokenType::Number(n) = &tokens[consumed + 1].token_type
                && n == "7"
            {
                return Ok((Some(ChordFamily::FullyDiminished), 2));
            }
        }

        // Check for "ø7" or "ø" (half-diminished seventh)
        if consumed < tokens.len()
            && let TokenType::HalfDiminished = tokens[consumed].token_type
        {
            if consumed + 1 < tokens.len()
                && let TokenType::Number(n) = &tokens[consumed + 1].token_type
                && n == "7"
            {
                return Ok((Some(ChordFamily::HalfDiminished), 2));
            }
            // ø without explicit 7 still means half-dim 7
            return Ok((Some(ChordFamily::HalfDiminished), 1));
        }

        // Check for plain "7" (context-dependent seventh)
        // The family depends on the base quality:
        // - Major quality -> Dominant seventh
        // - Minor quality -> Minor seventh
        // - Diminished quality -> Fully-diminished seventh. A `dim` triad is
        //   consumed by the quality parser, so `dim7` arrives here as
        //   Diminished + "7"; the diminished triad plus a diminished 7th is
        //   the fully-diminished chord (`dim7`/`o7`). Half-diminished is the
        //   distinct `ø7`/`m7b5` spelling, handled separately above.
        // - Augmented quality -> Augmented seventh (treated as Dominant with altered 5th)
        if consumed < tokens.len()
            && let TokenType::Number(n) = &tokens[consumed].token_type
            && n == "7"
        {
            let family = match quality {
                ChordQuality::Minor => ChordFamily::Minor7,
                ChordQuality::Diminished => ChordFamily::FullyDiminished,
                _ => ChordFamily::Dominant7, // Major, Augmented, Suspended, Power
            };
            return Ok((Some(family), 1));
        }

        // No seventh found
        Ok((None, 0))
    }
}

impl std::fmt::Display for ChordFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display outputs the chord notation symbol, not the full name
        write!(f, "{}", self.symbol())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seventh_intervals() {
        assert_eq!(
            ChordFamily::Major7.seventh_interval(),
            Interval::MajorSeventh
        );
        assert_eq!(
            ChordFamily::Dominant7.seventh_interval(),
            Interval::MinorSeventh
        );
        assert_eq!(
            ChordFamily::Minor7.seventh_interval(),
            Interval::MinorSeventh
        );
        assert_eq!(
            ChordFamily::MinorMajor7.seventh_interval(),
            Interval::MajorSeventh
        );
        assert_eq!(
            ChordFamily::HalfDiminished.seventh_interval(),
            Interval::MinorSeventh
        );
        assert_eq!(
            ChordFamily::FullyDiminished.seventh_interval(),
            Interval::DiminishedSeventh
        );
    }

    #[test]
    fn test_degrees() {
        assert_eq!(ChordFamily::Major7.degrees(), vec![ChordDegree::Seventh]);
        assert_eq!(ChordFamily::Dominant7.degrees(), vec![ChordDegree::Seventh]);
    }

    #[test]
    fn test_symbols() {
        assert_eq!(ChordFamily::Major7.symbol(), "maj7");
        assert_eq!(ChordFamily::Dominant7.symbol(), "7");
        assert_eq!(ChordFamily::Minor7.symbol(), "7");
        assert_eq!(ChordFamily::FullyDiminished.symbol(), "dim7");
    }
}
