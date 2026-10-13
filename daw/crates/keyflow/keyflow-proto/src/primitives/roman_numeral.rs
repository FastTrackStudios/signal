//! Roman numeral parsing
//!
//! Parses tokens into Roman numerals (I, II, III, iv, v, etc.)

use super::accidental::{Accidental, WithAccidental};
use super::root_notation::RomanCase;
use crate::parsing::token::{Token, TokenType};

/// Result of parsing a Roman numeral from tokens
#[derive(Debug, Clone, PartialEq)]
pub struct RomanNumeralToken {
    pub degree: u8,                     // 1-7
    pub case: RomanCase,                // Upper or Lower
    pub accidental: Option<Accidental>, // Optional accidental modifier
    pub tokens_consumed: usize,
}

impl RomanNumeralToken {
    /// Try to parse a Roman numeral from the start of a token stream
    /// Returns Some(RomanNumeralToken) if successful, None if tokens don't represent a Roman numeral
    pub fn parse(tokens: &[Token]) -> Option<Self> {
        if tokens.is_empty() {
            return None;
        }

        let mut consumed = 0;

        // Leading accidental before the numeral: `bIII`, `#IV`, `♭VII`. ASCII
        // `b` lexes as `Letter('b')` (only `♭` becomes `Flat`), so accept it
        // here the same way the extension/alteration parsers do.
        let mut accidental: Option<Accidental> = None;
        while consumed < tokens.len() {
            match &tokens[consumed].token_type {
                TokenType::Sharp => {
                    accidental = Some(match accidental {
                        None | Some(Accidental::Natural) => Accidental::Sharp,
                        Some(Accidental::Sharp) => Accidental::DoubleSharp,
                        _ => return None,
                    });
                    consumed += 1;
                }
                TokenType::Flat | TokenType::Letter('b') => {
                    accidental = Some(match accidental {
                        None | Some(Accidental::Natural) => Accidental::Flat,
                        Some(Accidental::Flat) => Accidental::DoubleFlat,
                        _ => return None,
                    });
                    consumed += 1;
                }
                _ => break,
            }
        }

        // Collect consecutive letters that could form a Roman numeral
        let mut letters = String::new();
        let mut is_upper = None;

        for token in tokens[consumed..].iter() {
            match &token.token_type {
                TokenType::Letter(c) if Self::is_roman_letter(*c) => {
                    // Check case consistency
                    let current_upper = c.is_uppercase();
                    match is_upper {
                        None => is_upper = Some(current_upper),
                        Some(expected) if expected != current_upper => break, // Mixed case, stop
                        _ => {}
                    }

                    letters.push(c.to_ascii_uppercase());
                    consumed += 1;
                }
                _ => break,
            }
        }

        if letters.is_empty() {
            return None;
        }

        // Try to parse as Roman numeral
        let degree = Self::roman_to_degree(&letters)?;
        let case = match is_upper {
            Some(true) => RomanCase::Upper,
            Some(false) => RomanCase::Lower,
            None => return None,
        };

        Some(RomanNumeralToken {
            degree,
            case,
            accidental,
            tokens_consumed: consumed,
        })
    }

    fn is_roman_letter(c: char) -> bool {
        matches!(c.to_ascii_uppercase(), 'I' | 'V' | 'X')
    }

    fn roman_to_degree(roman: &str) -> Option<u8> {
        match roman {
            "I" => Some(1),
            "II" => Some(2),
            "III" => Some(3),
            "IV" => Some(4),
            "V" => Some(5),
            "VI" => Some(6),
            "VII" => Some(7),
            _ => None,
        }
    }
}

// Implement WithAccidental for RomanNumeralToken
impl WithAccidental for RomanNumeralToken {
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
    use crate::parsing::Lexer;

    #[test]
    fn test_parse_upper_roman() {
        let mut lexer = Lexer::new("IV".to_string());
        let tokens = lexer.tokenize();
        let result = RomanNumeralToken::parse(&tokens).unwrap();

        assert_eq!(result.degree, 4);
        assert_eq!(result.case, RomanCase::Upper);
        assert_eq!(result.tokens_consumed, 2);
    }

    #[test]
    fn test_parse_lower_roman() {
        let mut lexer = Lexer::new("vi".to_string());
        let tokens = lexer.tokenize();
        let result = RomanNumeralToken::parse(&tokens).unwrap();

        assert_eq!(result.degree, 6);
        assert_eq!(result.case, RomanCase::Lower);
        assert_eq!(result.tokens_consumed, 2);
    }

    #[test]
    fn test_parse_single_roman() {
        let mut lexer = Lexer::new("V".to_string());
        let tokens = lexer.tokenize();
        let result = RomanNumeralToken::parse(&tokens).unwrap();

        assert_eq!(result.degree, 5);
        assert_eq!(result.case, RomanCase::Upper);
    }

    #[test]
    fn test_parse_mixed_case_fails() {
        let mut lexer = Lexer::new("Iv".to_string());
        let tokens = lexer.tokenize();
        let result = RomanNumeralToken::parse(&tokens);

        // Should only parse "I" (uppercase) and stop at lowercase "v"
        assert!(result.is_some());
        assert_eq!(result.unwrap().tokens_consumed, 1);
    }
}
