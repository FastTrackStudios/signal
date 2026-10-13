//! Common lexer that emits basic tokens
//!
//! This lexer is context-free and doesn't interpret meaning.
//! Mini-parsers receive these tokens and interpret them contextually.

use super::token::{Token, TokenType};
use std::iter::Peekable;
use std::str::Chars;

/// Basic lexer that tokenizes input without context
pub struct Lexer {
    input: String,
}

/// Internal state for tracking position during lexing
struct LexerState {
    /// Byte offset from start of input
    pos: usize,
    /// Current line number (1-indexed)
    line: u32,
    /// Current column number (1-indexed)
    column: u32,
}

impl LexerState {
    fn new() -> Self {
        Self {
            pos: 0,
            line: 1,
            column: 1,
        }
    }

    /// Advance past a character, updating position tracking
    fn advance(&mut self, ch: char) {
        self.pos += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
    }
}

impl Lexer {
    pub fn new(input: String) -> Self {
        Lexer { input }
    }

    /// Tokenize the entire input
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut chars = self.input.chars().peekable();
        let mut state = LexerState::new();

        while chars.peek().is_some() {
            let token = self.scan_token(&mut chars, &mut state);
            tokens.push(token);
        }

        // Add EOF token at final position
        tokens.push(Token::with_location(
            TokenType::Eof,
            state.pos,
            0,
            state.line,
            state.column,
        ));
        tokens
    }

    fn scan_token(&self, chars: &mut Peekable<Chars>, state: &mut LexerState) -> Token {
        let start_pos = state.pos;
        let start_line = state.line;
        let start_column = state.column;

        let ch = chars.next().unwrap();
        state.advance(ch);

        match ch {
            // Symbols
            '#' | '♯' => Token::with_location(
                TokenType::Sharp,
                start_pos,
                ch.len_utf8(),
                start_line,
                start_column,
            ),
            '♭' => Token::with_location(
                TokenType::Flat,
                start_pos,
                ch.len_utf8(),
                start_line,
                start_column,
            ),
            '/' => Token::with_location(TokenType::Slash, start_pos, 1, start_line, start_column),
            '+' => Token::with_location(TokenType::Plus, start_pos, 1, start_line, start_column),
            '-' => Token::with_location(TokenType::Minus, start_pos, 1, start_line, start_column),
            '_' => Token::with_location(
                TokenType::Underscore,
                start_pos,
                1,
                start_line,
                start_column,
            ),
            '\'' => Token::with_location(
                TokenType::Apostrophe,
                start_pos,
                1,
                start_line,
                start_column,
            ),
            '.' => Token::with_location(TokenType::Dot, start_pos, 1, start_line, start_column),
            '~' => Token::with_location(TokenType::Tilde, start_pos, 1, start_line, start_column),
            '*' => {
                Token::with_location(TokenType::Asterisk, start_pos, 1, start_line, start_column)
            }
            '△' | '^' => Token::with_location(
                TokenType::Triangle,
                start_pos,
                ch.len_utf8(),
                start_line,
                start_column,
            ),
            '°' => Token::with_location(
                TokenType::Circle,
                start_pos,
                ch.len_utf8(),
                start_line,
                start_column,
            ),
            'ø' => Token::with_location(
                TokenType::HalfDiminished,
                start_pos,
                ch.len_utf8(),
                start_line,
                start_column,
            ),
            '(' => Token::with_location(TokenType::LParen, start_pos, 1, start_line, start_column),
            ')' => Token::with_location(TokenType::RParen, start_pos, 1, start_line, start_column),
            ',' => Token::with_location(TokenType::Comma, start_pos, 1, start_line, start_column),
            '@' => Token::with_location(TokenType::At, start_pos, 1, start_line, start_column),
            ';' => {
                Token::with_location(TokenType::Semicolon, start_pos, 1, start_line, start_column)
            }
            '>' => Token::with_location(
                TokenType::GreaterThan,
                start_pos,
                1,
                start_line,
                start_column,
            ),

            // Whitespace
            ' ' | '\t' => {
                Token::with_location(TokenType::Space, start_pos, 1, start_line, start_column)
            }

            // Numbers - collect consecutive digits
            c if c.is_ascii_digit() => {
                let mut num = String::from(c);
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_digit() {
                        let next_ch = chars.next().unwrap();
                        num.push(next_ch);
                        state.advance(next_ch);
                    } else {
                        break;
                    }
                }
                let len = num.len();
                Token::with_location(
                    TokenType::Number(num),
                    start_pos,
                    len,
                    start_line,
                    start_column,
                )
            }

            // Letters - single character
            // Note: 'b' could be note B or flat - context determines meaning
            c if c.is_ascii_alphabetic() => {
                Token::with_location(TokenType::Letter(c), start_pos, 1, start_line, start_column)
            }

            // Unknown character
            _ => Token::with_location(
                TokenType::Illegal,
                start_pos,
                ch.len_utf8(),
                start_line,
                start_column,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokenization() {
        let mut lexer = Lexer::new("C#maj7".to_string());
        let tokens = lexer.tokenize();

        assert_eq!(tokens.len(), 7); // C, #, m, a, j, 7, EOF
        assert!(matches!(tokens[0].token_type, TokenType::Letter('C')));
        assert!(matches!(tokens[1].token_type, TokenType::Sharp));
    }

    #[test]
    fn test_number_grouping() {
        let mut lexer = Lexer::new("Cmaj13".to_string());
        let tokens = lexer.tokenize();

        // Should group "13" as a single number token
        assert!(matches!(tokens[4].token_type, TokenType::Number(ref n) if n == "13"));
    }

    #[test]
    fn test_slash_notation() {
        let mut lexer = Lexer::new("C/E".to_string());
        let tokens = lexer.tokenize();

        assert!(matches!(tokens[0].token_type, TokenType::Letter('C')));
        assert!(matches!(tokens[1].token_type, TokenType::Slash));
        assert!(matches!(tokens[2].token_type, TokenType::Letter('E')));
    }

    #[test]
    fn test_line_column_tracking() {
        let mut lexer = Lexer::new("C#maj7".to_string());
        let tokens = lexer.tokenize();

        // All tokens on line 1
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[0].column, 1); // C at column 1
        assert_eq!(tokens[1].column, 2); // # at column 2
        assert_eq!(tokens[2].column, 3); // m at column 3
        assert_eq!(tokens[3].column, 4); // a at column 4
        assert_eq!(tokens[4].column, 5); // j at column 5
        assert_eq!(tokens[5].column, 6); // 7 at column 6
    }

    #[test]
    fn test_position_tracking() {
        let mut lexer = Lexer::new("Am7".to_string());
        let tokens = lexer.tokenize();

        // A at pos 0, m at pos 1, 7 at pos 2
        assert_eq!(tokens[0].pos, 0);
        assert_eq!(tokens[0].len, 1);
        assert_eq!(tokens[1].pos, 1);
        assert_eq!(tokens[2].pos, 2);
    }

    #[test]
    fn test_multi_digit_number_span() {
        let mut lexer = Lexer::new("C13".to_string());
        let tokens = lexer.tokenize();

        // C at pos 0, 13 at pos 1 with len 2
        assert_eq!(tokens[0].pos, 0);
        assert_eq!(tokens[1].pos, 1);
        assert_eq!(tokens[1].len, 2);
        assert!(matches!(tokens[1].token_type, TokenType::Number(ref n) if n == "13"));
    }

    #[test]
    fn test_token_to_span() {
        let mut lexer = Lexer::new("Gmaj7".to_string());
        let tokens = lexer.tokenize();

        let span = tokens[0].to_span();
        assert_eq!(span.start, 0);
        assert_eq!(span.len, 1);
        assert_eq!(span.line, 1);
        assert_eq!(span.column, 1);
    }
}
