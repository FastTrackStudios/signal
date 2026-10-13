//! Chord alterations - modifications to expected intervals
//!
//! Represents alterations like b5, #5, b9, #9, #11, b13

use crate::chord::degree::ChordDegree;
use crate::parsing::{ParseError, Token, TokenType};
use crate::primitives::Interval;
use facet::Facet;

/// An alteration to a chord degree
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
pub struct Alteration {
    /// The degree being altered (e.g., 5th, 9th, 11th, 13th)
    pub degree: ChordDegree,
    /// The altered interval (e.g., DiminishedFifth, AugmentedFifth, FlatNinth)
    pub interval: Interval,
}

impl Alteration {
    /// Create a new alteration
    pub fn new(degree: ChordDegree, interval: Interval) -> Self {
        Self { degree, interval }
    }

    /// Convenience constructor for b5
    pub fn flat_five() -> Self {
        Self {
            degree: ChordDegree::Fifth,
            interval: Interval::DiminishedFifth,
        }
    }

    /// Convenience constructor for #5
    pub fn sharp_five() -> Self {
        Self {
            degree: ChordDegree::Fifth,
            interval: Interval::AugmentedFifth,
        }
    }

    /// Convenience constructor for b9
    pub fn flat_nine() -> Self {
        Self {
            degree: ChordDegree::Ninth,
            interval: Interval::FlatNinth,
        }
    }

    /// Convenience constructor for #9
    pub fn sharp_nine() -> Self {
        Self {
            degree: ChordDegree::Ninth,
            interval: Interval::SharpNinth,
        }
    }

    /// Convenience constructor for #11
    pub fn sharp_eleven() -> Self {
        Self {
            degree: ChordDegree::Eleventh,
            interval: Interval::SharpEleventh,
        }
    }

    /// Convenience constructor for b13
    pub fn flat_thirteen() -> Self {
        Self {
            degree: ChordDegree::Thirteenth,
            interval: Interval::FlatThirteenth,
        }
    }

    /// Get the symbol for this alteration
    pub fn symbol(&self) -> String {
        self.interval.to_chord_notation()
    }

    /// Check if this is a valid alteration for the given degree
    ///
    /// For example, you can't have a b9 without a 9th degree present,
    /// and you can't alter the root or third in most contexts.
    pub fn is_valid_for_degree(&self, degree: ChordDegree) -> bool {
        self.degree == degree
    }

    /// Parse alterations from tokens (inside parentheses or standalone)
    /// Returns (Vec`<Alteration>`, tokens_consumed)
    ///
    /// Handles: b5, #5, b9, #9, #11, b13
    /// Alterations modify the expected interval for a degree
    pub fn parse(tokens: &[Token]) -> Result<(Vec<Alteration>, usize), ParseError> {
        if tokens.is_empty() {
            return Ok((Vec::new(), 0));
        }

        let mut alterations = Vec::new();
        let mut consumed = 0;

        // Check for opening paren (alterations might be in parens or standalone)
        let in_parens = if consumed < tokens.len() {
            matches!(tokens[consumed].token_type, TokenType::LParen)
        } else {
            false
        };

        if in_parens {
            consumed += 1;
        }

        loop {
            if consumed >= tokens.len() {
                break;
            }

            // Check for closing paren
            if in_parens && matches!(tokens[consumed].token_type, TokenType::RParen) {
                consumed += 1;
                break;
            }

            // Check for comma separator
            if matches!(tokens[consumed].token_type, TokenType::Comma) {
                consumed += 1;
                continue;
            }

            // Parse alteration: accidental + number
            let accidental_token = if consumed < tokens.len() {
                match tokens[consumed].token_type {
                    TokenType::Flat | TokenType::Letter('b') => {
                        consumed += 1;
                        Some(TokenType::Flat) // Normalize 'b' to Flat
                    }
                    TokenType::Sharp => {
                        consumed += 1;
                        Some(TokenType::Sharp)
                    }
                    _ => None,
                }
            } else {
                None
            };

            if accidental_token.is_none() {
                // No alteration found
                if !in_parens {
                    break;
                }
                // In parens but no alteration - might be something else, stop
                if in_parens {
                    consumed -= 1; // Back up before the closing paren
                }
                break;
            }

            // Get the number
            if consumed < tokens.len() {
                if let TokenType::Number(n) = &tokens[consumed].token_type {
                    let degree = ChordDegree::from_number(n.parse().ok().unwrap_or(0));
                    if let Some(deg) = degree {
                        // Determine the altered interval
                        let interval = match (accidental_token.unwrap(), n.as_str()) {
                            (TokenType::Flat, "5") => Some(Interval::DiminishedFifth),
                            (TokenType::Sharp, "5") => Some(Interval::AugmentedFifth),
                            (TokenType::Flat, "9") => Some(Interval::FlatNinth),
                            (TokenType::Sharp, "9") => Some(Interval::SharpNinth),
                            (TokenType::Sharp, "11") => Some(Interval::SharpEleventh),
                            (TokenType::Flat, "13") => Some(Interval::FlatThirteenth),
                            _ => None,
                        };

                        if let Some(int) = interval {
                            alterations.push(Alteration::new(deg, int));
                        }
                    }
                    consumed += 1;
                } else {
                    // Number not found after accidental
                    consumed -= 1; // Back up
                    break;
                }
            } else {
                // No more tokens
                consumed -= 1; // Back up
                break;
            }

            // If not in parens, only parse one alteration
            if !in_parens {
                break;
            }
        }

        Ok((alterations, consumed))
    }
}

impl std::fmt::Display for Alteration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.symbol())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flat_five() {
        let alt = Alteration::flat_five();
        assert_eq!(alt.degree, ChordDegree::Fifth);
        assert_eq!(alt.interval, Interval::DiminishedFifth);
        assert_eq!(alt.symbol(), "b5");
    }

    #[test]
    fn test_sharp_five() {
        let alt = Alteration::sharp_five();
        assert_eq!(alt.degree, ChordDegree::Fifth);
        assert_eq!(alt.interval, Interval::AugmentedFifth);
        assert_eq!(alt.symbol(), "#5");
    }

    #[test]
    fn test_flat_nine() {
        let alt = Alteration::flat_nine();
        assert_eq!(alt.degree, ChordDegree::Ninth);
        assert_eq!(alt.interval, Interval::FlatNinth);
        assert_eq!(alt.symbol(), "b9");
    }

    #[test]
    fn test_sharp_nine() {
        let alt = Alteration::sharp_nine();
        assert_eq!(alt.degree, ChordDegree::Ninth);
        assert_eq!(alt.interval, Interval::SharpNinth);
        assert_eq!(alt.symbol(), "#9");
    }

    #[test]
    fn test_sharp_eleven() {
        let alt = Alteration::sharp_eleven();
        assert_eq!(alt.degree, ChordDegree::Eleventh);
        assert_eq!(alt.interval, Interval::SharpEleventh);
        assert_eq!(alt.symbol(), "#11");
    }

    #[test]
    fn test_flat_thirteen() {
        let alt = Alteration::flat_thirteen();
        assert_eq!(alt.degree, ChordDegree::Thirteenth);
        assert_eq!(alt.interval, Interval::FlatThirteenth);
        assert_eq!(alt.symbol(), "b13");
    }

    #[test]
    fn test_is_valid_for_degree() {
        let alt = Alteration::flat_five();
        assert!(alt.is_valid_for_degree(ChordDegree::Fifth));
        assert!(!alt.is_valid_for_degree(ChordDegree::Ninth));
    }
}
