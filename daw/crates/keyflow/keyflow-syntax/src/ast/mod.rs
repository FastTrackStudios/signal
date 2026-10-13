//! Abstract Syntax Tree for music notation.
//!
//! This module provides intermediate representations that preserve source spans
//! and defer semantic analysis (like interval computation) to a later phase.
//!
//! # Architecture
//!
//! The AST sits between raw parsing and semantic analysis:
//!
//! ```text
//! Input Text → Tokens → AST → Semantic Analysis → Final Types
//! ```
//!
//! ## Benefits
//!
//! - **Source Preservation**: All AST nodes track their source spans for error reporting
//! - **Deferred Validation**: Parse first, validate later (enables better error messages)
//! - **Testability**: AST can be inspected and tested independently of semantic rules
//! - **Flexibility**: Different semantic passes can interpret the same AST differently
//!
//! # Types
//!
//! - [`ChordAst`] - Intermediate chord representation with root, modifiers, and duration
//! - [`RhythmAst`] - Intermediate rhythm representation preserving notation style
//! - [`Spanned<T>`] - Wrapper that attaches source span to any value

mod chord;
mod rhythm;
mod span;

pub use chord::{
    AccidentalAst, AlterationAst, BassToneAst, ChordAst, ChordModifierAst, ExtensionAst,
    ExtensionQualityAst, QualityAst, RootAst,
};
pub use rhythm::{DurationAst, RhythmAst, RhythmKind, SlashCountAst};
pub use span::{AstNode, Spanned};
