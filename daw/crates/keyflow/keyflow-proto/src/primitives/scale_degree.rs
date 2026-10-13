//! Scale degree parsing
//!
//! Parses tokens into scale degrees (1-7 with optional accidentals)

use super::accidental::{Accidental, WithAccidental};
use crate::parsing::token::{Token, TokenType};

/// Result of parsing a scale degree from tokens
#[derive(Debug, Clone, PartialEq)]
pub struct ScaleDegreeToken {
    pub degree: u8,                     // 1-7
    pub accidental: Option<Accidental>, // Optional accidental modifier
    pub tokens_consumed: usize,
}

impl ScaleDegreeToken {
    /// Try to parse a scale degree from the start of a token stream
    /// Handles optional leading accidentals: #4, b5, ##7, etc.
    pub fn parse(tokens: &[Token]) -> Option<Self> {
        if tokens.is_empty() {
            return None;
        }

        let mut consumed = 0;
        let mut accidental: Option<Accidental> = None;

        // Handle leading accidentals
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
                // ASCII `b` lexes as a letter (only the unicode `♭` becomes a
                // Flat token), so accept it as a flat here too — `b3`, `b7`.
                TokenType::Flat | TokenType::Letter('b') => {
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

        // Expect a number for the scale degree
        if consumed >= tokens.len() {
            return None;
        }

        let degree = match &tokens[consumed].token_type {
            TokenType::Number(n) => {
                let d: u8 = n.parse().ok()?;
                if (1..=7).contains(&d) {
                    d
                } else {
                    return None;
                }
            }
            _ => return None,
        };

        consumed += 1;

        Some(ScaleDegreeToken {
            degree,
            accidental,
            tokens_consumed: consumed,
        })
    }
}

// Implement WithAccidental for ScaleDegreeToken
impl WithAccidental for ScaleDegreeToken {
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
    fn test_parse_scale_degree() {
        let mut lexer = Lexer::new("4".to_string());
        let tokens = lexer.tokenize();
        let result = ScaleDegreeToken::parse(&tokens).unwrap();

        assert_eq!(result.degree, 4);
        assert_eq!(result.accidental, None);
    }

    #[test]
    fn test_apply_sharp() {
        let mut lexer = Lexer::new("4".to_string());
        let tokens = lexer.tokenize();
        let result = ScaleDegreeToken::parse(&tokens).unwrap();
        let sharp_result = result.sharp();

        assert_eq!(sharp_result.degree, 4);
        assert_eq!(sharp_result.accidental, Some(Accidental::Sharp));
    }
}
