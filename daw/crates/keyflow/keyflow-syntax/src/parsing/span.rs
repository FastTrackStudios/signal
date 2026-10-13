//! Text span for source location tracking
//!
//! Represents a span of text in the original source input.
//! Used for bidirectional linking between rendered elements and source text.

use facet::Facet;
use std::ops::Range;

/// Represents a span of text in the original source input.
///
/// Used for source linking between rendered elements and original text,
/// enabling features like click-to-highlight and in-place editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Facet)]
pub struct TextSpan {
    /// Byte offset from the start of the source
    pub start: usize,
    /// Byte length of the span
    pub len: usize,
    /// Line number (1-indexed) for display purposes
    pub line: u32,
    /// Column number (1-indexed) for display purposes
    pub column: u32,
}

impl TextSpan {
    /// Create a new span with just byte position (line/column will be 0).
    #[must_use]
    pub const fn new(start: usize, len: usize) -> Self {
        Self {
            start,
            len,
            line: 0,
            column: 0,
        }
    }

    /// Create a span with full location information.
    #[must_use]
    pub const fn with_location(start: usize, len: usize, line: u32, column: u32) -> Self {
        Self {
            start,
            len,
            line,
            column,
        }
    }

    /// Create an empty span at the given position.
    #[must_use]
    pub const fn empty(start: usize) -> Self {
        Self::new(start, 0)
    }

    /// Get the end byte offset (exclusive).
    #[must_use]
    pub const fn end(&self) -> usize {
        self.start + self.len
    }

    /// Check if this span contains a byte offset.
    #[must_use]
    pub const fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.start + self.len
    }

    /// Check if this span is empty (zero length).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Convert to a byte range.
    #[must_use]
    pub const fn as_range(&self) -> Range<usize> {
        self.start..self.start + self.len
    }

    /// Check if two spans overlap.
    #[must_use]
    pub const fn overlaps(&self, other: &TextSpan) -> bool {
        self.start < other.end() && other.start < self.end()
    }

    /// Check if this span overlaps with a byte range.
    #[must_use]
    pub const fn overlaps_range(&self, range_start: usize, range_end: usize) -> bool {
        self.start < range_end && range_start < self.end()
    }

    /// Merge two spans into one that covers both.
    /// Line/column info is taken from the earlier span.
    #[must_use]
    pub fn merge(&self, other: &TextSpan) -> TextSpan {
        let start = self.start.min(other.start);
        let end = self.end().max(other.end());
        let (line, column) = if self.start <= other.start {
            (self.line, self.column)
        } else {
            (other.line, other.column)
        };
        TextSpan {
            start,
            len: end - start,
            line,
            column,
        }
    }

    /// Extend this span to include another position.
    #[must_use]
    pub const fn extend_to(&self, end: usize) -> TextSpan {
        TextSpan {
            start: self.start,
            len: end.saturating_sub(self.start),
            line: self.line,
            column: self.column,
        }
    }

    /// Extract the source text for this span from the original input.
    #[must_use]
    pub fn extract<'a>(&self, source: &'a str) -> Option<&'a str> {
        source.get(self.start..self.end())
    }
}

impl Default for TextSpan {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl From<Range<usize>> for TextSpan {
    fn from(range: Range<usize>) -> Self {
        Self::new(range.start, range.end - range.start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_span() {
        let span = TextSpan::new(10, 5);
        assert_eq!(span.start, 10);
        assert_eq!(span.len, 5);
        assert_eq!(span.end(), 15);
        assert_eq!(span.line, 0);
        assert_eq!(span.column, 0);
    }

    #[test]
    fn test_with_location() {
        let span = TextSpan::with_location(10, 5, 3, 7);
        assert_eq!(span.start, 10);
        assert_eq!(span.len, 5);
        assert_eq!(span.line, 3);
        assert_eq!(span.column, 7);
    }

    #[test]
    fn test_contains() {
        let span = TextSpan::new(10, 5);
        assert!(span.contains(10));
        assert!(span.contains(12));
        assert!(span.contains(14));
        assert!(!span.contains(9));
        assert!(!span.contains(15));
    }

    #[test]
    fn test_overlaps() {
        let span1 = TextSpan::new(10, 5); // 10-15
        let span2 = TextSpan::new(12, 5); // 12-17
        let span3 = TextSpan::new(15, 5); // 15-20
        let span4 = TextSpan::new(5, 5); // 5-10

        assert!(span1.overlaps(&span2));
        assert!(!span1.overlaps(&span3)); // Adjacent, not overlapping
        assert!(!span1.overlaps(&span4)); // Adjacent, not overlapping
    }

    #[test]
    fn test_merge() {
        let span1 = TextSpan::with_location(10, 5, 1, 10);
        let span2 = TextSpan::with_location(12, 8, 1, 12);

        let merged = span1.merge(&span2);
        assert_eq!(merged.start, 10);
        assert_eq!(merged.end(), 20);
        assert_eq!(merged.line, 1);
        assert_eq!(merged.column, 10);
    }

    #[test]
    fn test_extract() {
        let source = "Hello, World!";
        let span = TextSpan::new(7, 5);
        assert_eq!(span.extract(source), Some("World"));
    }

    #[test]
    fn test_from_range() {
        let span: TextSpan = (10..20).into();
        assert_eq!(span.start, 10);
        assert_eq!(span.len, 10);
    }
}
