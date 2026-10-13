//! Keyflow concrete syntax support.
//!
//! This crate owns source-oriented types: spans, tokens, syntax AST nodes, and
//! editor highlighting. `keyflow-proto` uses these types in domain contracts,
//! but service and domain models live in `keyflow-proto`.

#![deny(unsafe_code)]

pub mod ast;

#[cfg(feature = "highlighting")]
pub mod highlighting;

pub mod parsing;

pub use parsing::{Lexer, ParseError, TextSpan, Token, TokenType};
