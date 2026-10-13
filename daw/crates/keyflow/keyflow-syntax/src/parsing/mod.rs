//! Parsing module - Token and span types

pub mod lexer;
pub mod span;
pub mod token;

pub use lexer::Lexer;
pub use span::TextSpan;
pub use token::{Token, TokenType};

use facet::Facet;
use thiserror::Error;

/// Parse error
#[repr(C)]
#[derive(Debug, Clone, Error, Facet)]
pub enum ParseError {
    #[error("Empty input")]
    EmptyInput,
    #[error("No valid parser: {context}")]
    NoValidParser { context: String },
    #[error("Unexpected token: {0:?}")]
    UnexpectedToken(Token),
    #[error("Invalid syntax: {0}")]
    InvalidSyntax(String),
    #[error("Expected {expected} but found {found}")]
    Expected { expected: String, found: String },
    #[error("EOF while parsing {0}")]
    UnexpectedEof(String),
    #[error("Invalid chord: {0}")]
    InvalidChord(String),
    #[error("Invalid measure: {0}")]
    InvalidMeasure(String),
    #[error("Invalid section: {0}")]
    InvalidSection(String),
}
