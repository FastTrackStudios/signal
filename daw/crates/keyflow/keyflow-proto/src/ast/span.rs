//! Span utilities for AST nodes.
//!
//! Provides the [`Spanned`] wrapper and [`AstNode`] trait for source tracking.

use crate::parsing::TextSpan;
use facet::Facet;

/// A value with an associated source span.
///
/// This wrapper preserves the source location for any AST node, enabling
/// precise error messages and bidirectional linking between rendered
/// elements and source text.
///
/// # Example
///
/// ```
/// use keyflow_proto::ast::Spanned;
/// use keyflow_proto::parsing::TextSpan;
///
/// let value = Spanned::new("Cmaj7", TextSpan::new(0, 5));
/// assert_eq!(value.value, "Cmaj7");
/// assert_eq!(value.span.len, 5);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
pub struct Spanned<T> {
    /// The wrapped value
    pub value: T,
    /// Source span in the original input
    pub span: TextSpan,
}

impl<T> Spanned<T> {
    /// Create a new spanned value.
    #[must_use]
    pub const fn new(value: T, span: TextSpan) -> Self {
        Self { value, span }
    }

    /// Create a spanned value with an empty span at position 0.
    ///
    /// Useful for programmatically constructed AST nodes.
    #[must_use]
    pub fn synthetic(value: T) -> Self {
        Self {
            value,
            span: TextSpan::empty(0),
        }
    }

    /// Map the inner value while preserving the span.
    #[must_use]
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Spanned<U> {
        Spanned {
            value: f(self.value),
            span: self.span,
        }
    }

    /// Get a reference to the inner value.
    #[must_use]
    pub const fn as_ref(&self) -> Spanned<&T> {
        Spanned {
            value: &self.value,
            span: self.span,
        }
    }

    /// Extend this span to include another span.
    #[must_use]
    pub fn extend_span(&self, other: TextSpan) -> Self
    where
        T: Clone,
    {
        Self {
            value: self.value.clone(),
            span: self.span.merge(&other),
        }
    }
}

impl<T: Default> Default for Spanned<T> {
    fn default() -> Self {
        Self::synthetic(T::default())
    }
}

impl<T> std::ops::Deref for Spanned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> std::ops::DerefMut for Spanned<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Trait for AST nodes that have source spans.
///
/// All AST types should implement this trait to enable uniform
/// span access and manipulation.
pub trait AstNode {
    /// Get the source span of this node.
    fn span(&self) -> TextSpan;

    /// Check if this node's span contains a byte offset.
    fn contains_offset(&self, offset: usize) -> bool {
        self.span().contains(offset)
    }

    /// Check if this node's span overlaps with another span.
    fn overlaps_span(&self, other: TextSpan) -> bool {
        self.span().overlaps(&other)
    }

    /// Extract the source text for this node from the original input.
    fn extract_source<'a>(&self, source: &'a str) -> Option<&'a str> {
        self.span().extract(source)
    }
}

// Implement AstNode for Spanned<T>
impl<T> AstNode for Spanned<T> {
    fn span(&self) -> TextSpan {
        self.span
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spanned_new() {
        let span = TextSpan::new(10, 5);
        let spanned = Spanned::new("test", span);

        assert_eq!(spanned.value, "test");
        assert_eq!(spanned.span.start, 10);
        assert_eq!(spanned.span.len, 5);
    }

    #[test]
    fn test_spanned_synthetic() {
        let spanned = Spanned::synthetic(42);

        assert_eq!(spanned.value, 42);
        assert_eq!(spanned.span.start, 0);
        assert_eq!(spanned.span.len, 0);
    }

    #[test]
    fn test_spanned_map() {
        let span = TextSpan::new(5, 3);
        let spanned = Spanned::new(10, span);
        let mapped = spanned.map(|x| x * 2);

        assert_eq!(mapped.value, 20);
        assert_eq!(mapped.span.start, 5);
    }

    #[test]
    fn test_spanned_deref() {
        let spanned = Spanned::new(String::from("hello"), TextSpan::new(0, 5));

        // Can access String methods through Deref
        assert_eq!(spanned.len(), 5);
        assert!(spanned.starts_with("hel"));
    }

    #[test]
    fn test_ast_node_trait() {
        let span = TextSpan::new(10, 5);
        let spanned = Spanned::new("test", span);

        assert_eq!(spanned.span().start, 10);
        assert!(spanned.contains_offset(12));
        assert!(!spanned.contains_offset(20));
    }

    #[test]
    fn test_extract_source() {
        let source = "Hello, World!";
        let span = TextSpan::new(7, 5);
        let spanned = Spanned::new((), span);

        assert_eq!(spanned.extract_source(source), Some("World"));
    }
}
