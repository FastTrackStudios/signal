//! Root notation parsing
//!
//! Handles parsing of chord roots from tokens (note names, scale degrees, roman numerals)

use crate::parsing::{ParseError, Token, TokenType};
use crate::primitives::{MusicalNoteToken, RomanNumeralToken, RootNotation, ScaleDegreeToken};

/// Which notation system the caller believes the surrounding context is in.
///
/// Only affects genuinely ambiguous roots: an ASCII `b` followed by a digit
/// (`b5`/`b6`/`b7`) is either the note B with that number as a descriptor
/// (`B7`) or a flat scale degree (`♭7`). `Letter`/`Auto` read it as the note;
/// `Degree` reads it as a flat degree. Everything else is unambiguous and
/// ignores this hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotationSystem {
    /// No context — treat ambiguous `b<digit>` as the note B (`b7` = B7).
    #[default]
    Auto,
    /// Letter-name context — same as `Auto` for disambiguation.
    Letter,
    /// Degree-based context (Nashville numbers or Roman numerals) — treat
    /// ambiguous `b<digit>` as a flat scale degree (`b7` = ♭7).
    Degree,
}

/// Result of parsing a root notation with token consumption info
#[derive(Debug, Clone, PartialEq)]
pub struct RootParseResult {
    /// The parsed root notation
    pub root: RootNotation,
    /// Number of tokens consumed from input
    pub tokens_consumed: usize,
}

impl RootParseResult {
    pub fn new(root: RootNotation, tokens_consumed: usize) -> Self {
        Self {
            root,
            tokens_consumed,
        }
    }
}

/// Parse root notation from tokens
///
/// Tries to parse as note name, scale degree, or roman numeral based on token patterns.
/// Returns the parsed root and number of tokens consumed.
pub fn parse_root(tokens: &[Token]) -> Result<RootParseResult, ParseError> {
    parse_root_with_system(tokens, NotationSystem::Auto)
}

/// Parse root notation, using a notation-system hint to resolve the ambiguous
/// `b<digit>` case (note B vs flat degree). See [`NotationSystem`].
pub fn parse_root_with_system(
    tokens: &[Token],
    system: NotationSystem,
) -> Result<RootParseResult, ParseError> {
    if tokens.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    // Skip whitespace
    let tokens = skip_whitespace(tokens);
    if tokens.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    // Try to determine which parser to use based on token pattern
    let parse_order = detect_parser_order(tokens, system);

    for parser_type in parse_order {
        match parser_type {
            ParserType::ScaleDegree => {
                if let Some(result) = ScaleDegreeToken::parse(tokens) {
                    let root = RootNotation::from_scale_degree(result.degree, result.accidental);
                    return Ok(RootParseResult::new(root, result.tokens_consumed));
                }
            }
            ParserType::Roman => {
                if let Some(result) = RomanNumeralToken::parse(tokens) {
                    let consumed = result.tokens_consumed;
                    let root = RootNotation::from_roman_token(result);
                    return Ok(RootParseResult::new(root, consumed));
                }
            }
            ParserType::NoteName => {
                if let Some(result) = MusicalNoteToken::parse(tokens) {
                    let consumed = result.tokens_consumed;
                    let root = RootNotation::from_note_token(result);
                    return Ok(RootParseResult::new(root, consumed));
                }
            }
        }
    }

    Err(ParseError::NoValidParser {
        context: format!(
            "Unable to parse root from tokens starting with: {:?}",
            tokens.first()
        ),
    })
}

/// Detect the order in which to try parsers based on token patterns
fn detect_parser_order(tokens: &[Token], system: NotationSystem) -> Vec<ParserType> {
    if tokens.is_empty() {
        return vec![
            ParserType::NoteName,
            ParserType::Roman,
            ParserType::ScaleDegree,
        ];
    }

    let first_token = &tokens[0].token_type;

    match first_token {
        // Accidental followed by...
        TokenType::Sharp | TokenType::Flat => {
            if let Some(second) = tokens.get(1) {
                match &second.token_type {
                    // Accidental + Number -> Scale Degree first
                    TokenType::Number(s) if is_scale_degree_number(s) => {
                        vec![
                            ParserType::ScaleDegree,
                            ParserType::NoteName,
                            ParserType::Roman,
                        ]
                    }
                    // Accidental + Roman letter (e.g. `#IV`, `♭VII`) -> Roman first.
                    TokenType::Letter(c) if is_roman_numeral_letter(*c) => {
                        vec![
                            ParserType::Roman,
                            ParserType::NoteName,
                            ParserType::ScaleDegree,
                        ]
                    }
                    // Accidental + other letter -> Note Name first (could be Roman too)
                    TokenType::Letter(_) => {
                        vec![
                            ParserType::NoteName,
                            ParserType::Roman,
                            ParserType::ScaleDegree,
                        ]
                    }
                    _ => vec![
                        ParserType::NoteName,
                        ParserType::Roman,
                        ParserType::ScaleDegree,
                    ],
                }
            } else {
                vec![
                    ParserType::NoteName,
                    ParserType::Roman,
                    ParserType::ScaleDegree,
                ]
            }
        }

        // Number -> Scale Degree first
        TokenType::Number(s) if is_scale_degree_number(s) => {
            vec![
                ParserType::ScaleDegree,
                ParserType::NoteName,
                ParserType::Roman,
            ]
        }

        // Letter -> Could be Note Name or Roman Numeral
        TokenType::Letter(c) => {
            // Leading ascii `b` is a flat (it lexes as a letter, not a Flat
            // token). When it sits in front of a Roman numeral — `bIII`,
            // `bvii` — treat it as a flat Roman, not the note B. A `b` in front
            // of a digit (`b7`) stays note-first here; that case is ambiguous
            // (B7 chord vs ♭7 degree) and is resolved with notation-system
            // context by the chart parser, not by token shape alone.
            let next_is_roman = matches!(
                tokens.get(1).map(|t| &t.token_type),
                Some(TokenType::Letter(n)) if is_roman_numeral_letter(*n)
            );
            // Ambiguous `b<digit>` (b5/b6/b7). In a degree-based context the
            // leading `b` is a flat degree (`b7` = ♭7); otherwise it's the
            // note B (`b7` = B7), which the note-name parser handles.
            let next_is_degree_digit = matches!(
                tokens.get(1).map(|t| &t.token_type),
                Some(TokenType::Number(n)) if is_scale_degree_number(n)
            );
            if *c == 'b' && next_is_degree_digit && system == NotationSystem::Degree {
                return vec![
                    ParserType::ScaleDegree,
                    ParserType::NoteName,
                    ParserType::Roman,
                ];
            }
            if (*c == 'b' && next_is_roman) || is_roman_numeral_letter(*c) {
                // Try Roman first, then Note Name
                vec![
                    ParserType::Roman,
                    ParserType::NoteName,
                    ParserType::ScaleDegree,
                ]
            } else {
                // Try Note Name first
                vec![
                    ParserType::NoteName,
                    ParserType::Roman,
                    ParserType::ScaleDegree,
                ]
            }
        }

        // Default order
        _ => vec![
            ParserType::NoteName,
            ParserType::Roman,
            ParserType::ScaleDegree,
        ],
    }
}

/// Check if a number string is a valid scale degree (1-7)
fn is_scale_degree_number(s: &str) -> bool {
    if let Ok(num) = s.parse::<u8>() {
        (1..=7).contains(&num)
    } else {
        false
    }
}

/// Check if a letter could be part of a Roman numeral
fn is_roman_numeral_letter(c: char) -> bool {
    matches!(c.to_ascii_uppercase(), 'I' | 'V' | 'X')
}

/// Skip leading whitespace tokens
fn skip_whitespace(tokens: &[Token]) -> &[Token] {
    let mut i = 0;
    while i < tokens.len() && tokens[i].token_type == TokenType::Space {
        i += 1;
    }
    &tokens[i..]
}

/// Parser type enum for determining which mini-parser to use
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParserType {
    ScaleDegree,
    Roman,
    NoteName,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::Lexer;

    #[test]
    fn test_parse_note_name() {
        let mut lexer = Lexer::new("C".to_string());
        let tokens = lexer.tokenize();
        let result = parse_root(&tokens).unwrap();

        assert_eq!(result.tokens_consumed, 1);
    }

    #[test]
    fn test_parse_sharp_note() {
        let mut lexer = Lexer::new("F#".to_string());
        let tokens = lexer.tokenize();
        let result = parse_root(&tokens).unwrap();

        assert_eq!(result.tokens_consumed, 2);
    }

    #[test]
    fn test_parse_scale_degree() {
        let mut lexer = Lexer::new("4".to_string());
        let tokens = lexer.tokenize();
        let result = parse_root(&tokens).unwrap();

        assert_eq!(result.tokens_consumed, 1);
    }

    #[test]
    fn test_parse_sharp_scale_degree() {
        let mut lexer = Lexer::new("#4".to_string());
        let tokens = lexer.tokenize();
        let result = parse_root(&tokens).unwrap();

        assert_eq!(result.tokens_consumed, 2);
    }

    #[test]
    fn test_parse_roman_numeral() {
        let mut lexer = Lexer::new("IV".to_string());
        let tokens = lexer.tokenize();
        let result = parse_root(&tokens).unwrap();

        assert_eq!(result.tokens_consumed, 2);
    }

    #[test]
    fn test_parse_lowercase_roman() {
        let mut lexer = Lexer::new("vi".to_string());
        let tokens = lexer.tokenize();
        let result = parse_root(&tokens).unwrap();

        assert_eq!(result.tokens_consumed, 2);
    }

    #[test]
    fn test_skip_whitespace() {
        let mut lexer = Lexer::new("  C".to_string());
        let tokens = lexer.tokenize();
        let result = parse_root(&tokens).unwrap();

        assert_eq!(result.tokens_consumed, 1);
    }
}
