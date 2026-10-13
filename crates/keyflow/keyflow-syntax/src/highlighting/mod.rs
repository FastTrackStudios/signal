//! Syntax Highlighting for Keyflow Notation
//!
//! Parser-integrated syntax highlighting that leverages the AST's source spans
//! for accurate, semantic highlighting of music chart notation.
//!
//! ## Features
//!
//! - Uses the actual parser's AST, ensuring highlighting matches parsing
//! - Supports all Keyflow notation elements (chords, sections, metadata, etc.)
//! - Multiple output formats: HTML, ANSI terminal, styled spans for UI frameworks
//! - Theme support with dark and light presets
//!
//! ## Example
//!
//! ```rust,ignore
//! use keyflow_syntax::highlighting::{Highlighter, Theme};
//!
//! let source = "Gmaj7_8 | Am | D_4 G";
//! let spans = Highlighter::highlight_line(source);
//! let html = Highlighter::to_html(source, &spans, &Theme::default_dark());
//! ```

mod highlighter;
mod kind;
mod render;
mod theme;

pub use highlighter::Highlighter;
pub use kind::{HighlightKind, HighlightSpan};
pub use render::{Renderer, StyledSpan};
pub use theme::{Color, Theme};
