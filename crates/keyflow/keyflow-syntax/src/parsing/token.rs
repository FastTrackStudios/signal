//! Token types for the lexer
//!
//! Basic, context-free tokens that mini-parsers interpret based on context

use facet::Facet;
use std::fmt::{self, Display};

/// Basic token types emitted by the lexer
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum TokenType {
    // Basic character tokens
    Letter(char),   // Single alphabetic character (a-z, A-Z)
    Number(String), // Consecutive digits

    // Symbol tokens
    Sharp,      // # or ♯
    Flat,       // b or ♭
    Slash,      // /
    Plus,       // +
    Minus,      // -
    Underscore, // _
    Apostrophe, // '
    Dot,        // .
    Tilde,      // ~
    Asterisk,   // *

    // Special symbols for chord notation
    Triangle,       // △ or ^
    Circle,         // ° (diminished)
    HalfDiminished, // ø (half-diminished)

    // Delimiters
    LParen, // (
    RParen, // )
    Comma,  // ,

    // Text cue marker
    At, // @ (for instrument cues)

    // Comment marker
    Semicolon, // ; (for comments)

    // Optional readability separator between a chord's root and its quality:
    // `1:7`, `4:maj9`.
    Colon, // :

    // Repeat marker
    GreaterThan, // > (for accent shorthand ->)

    // Whitespace
    Space, // space or tab

    // Special markers
    Illegal,
    Eof,
}

impl Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenType::Letter(c) => write!(f, "{}", c),
            TokenType::Number(num) => write!(f, "{}", num),
            TokenType::Sharp => write!(f, "#"),
            TokenType::Flat => write!(f, "b"),
            TokenType::Slash => write!(f, "/"),
            TokenType::Plus => write!(f, "+"),
            TokenType::Minus => write!(f, "-"),
            TokenType::Underscore => write!(f, "_"),
            TokenType::Apostrophe => write!(f, "'"),
            TokenType::Dot => write!(f, "."),
            TokenType::Tilde => write!(f, "~"),
            TokenType::Asterisk => write!(f, "*"),
            TokenType::Triangle => write!(f, "△"),
            TokenType::Circle => write!(f, "°"),
            TokenType::HalfDiminished => write!(f, "ø"),
            TokenType::LParen => write!(f, "("),
            TokenType::RParen => write!(f, ")"),
            TokenType::Comma => write!(f, ","),
            TokenType::At => write!(f, "@"),
            TokenType::Semicolon => write!(f, ";"),
            TokenType::Colon => write!(f, ":"),
            TokenType::GreaterThan => write!(f, ">"),
            TokenType::Space => write!(f, " "),
            TokenType::Illegal => write!(f, "ILLEGAL"),
            TokenType::Eof => write!(f, "EOF"),
        }
    }
}

/// Token with position information
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Token {
    pub token_type: TokenType,
    /// Byte offset from start of input
    pub pos: usize,
    /// Byte length of token
    pub len: usize,
    /// Line number (1-indexed)
    pub line: u32,
    /// Column number (1-indexed)
    pub column: u32,
}

impl Token {
    /// Create a new token with position information
    pub fn new(token_type: TokenType, pos: usize, len: usize) -> Self {
        Token {
            token_type,
            pos,
            len,
            line: 1,
            column: 1,
        }
    }

    /// Create a new token with full position information including line and column
    pub fn with_location(
        token_type: TokenType,
        pos: usize,
        len: usize,
        line: u32,
        column: u32,
    ) -> Self {
        Token {
            token_type,
            pos,
            len,
            line,
            column,
        }
    }

    /// Convert this token's position to a TextSpan
    pub fn to_span(&self) -> super::span::TextSpan {
        super::span::TextSpan::with_location(self.pos, self.len, self.line, self.column)
    }
}

impl Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.token_type)
    }
}
