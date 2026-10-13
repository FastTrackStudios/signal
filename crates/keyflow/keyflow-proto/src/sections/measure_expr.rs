//! Measure Count Expressions
//!
//! Supports expressions for section measure counts:
//! - Absolute: `8` → 8 measures
//! - Addition: `8+1` → 9 measures, `+1` → memory + 1
//! - Subtraction: `8-1` → 7 measures, `-1` → memory - 1
//! - Multiplication: `4x4` or `4*4` → 16 measures

use facet::Facet;

/// A measure count expression that can be absolute or relative to memory
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
#[derive(Default)]
pub enum MeasureExpression {
    /// No expression - use section memory
    #[default]
    UseMemory,
    /// Absolute number of measures
    Absolute(usize),
    /// Relative to memory: add measures
    Add(usize),
    /// Relative to memory: subtract measures
    Subtract(usize),
}

impl MeasureExpression {
    /// Parse a measure expression from a string
    ///
    /// Supports:
    /// - Simple numbers: "8" → Absolute(8)
    /// - Addition: "8+1" → Absolute(9), "+1" → Add(1)
    /// - Subtraction: "8-1" → Absolute(7), "-1" → Subtract(1)
    /// - Multiplication: "4x4" or "4*4" → Absolute(16)
    /// - Incomplete expressions are gracefully handled: "16+" → Absolute(16)
    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();

        if input.is_empty() {
            return Some(Self::UseMemory);
        }

        // Check for relative expressions first: +N or -N at the start
        if let Some(rest) = input.strip_prefix('+') {
            if let Ok(n) = rest.parse::<usize>() {
                return Some(Self::Add(n));
            }
            // Incomplete relative expression like "+" - treat as no expression
            return Some(Self::UseMemory);
        }

        if let Some(rest) = input.strip_prefix('-') {
            if let Ok(n) = rest.parse::<usize>() {
                return Some(Self::Subtract(n));
            }
            // Incomplete relative expression like "-" - treat as no expression
            return Some(Self::UseMemory);
        }

        // Try to evaluate as an expression
        if let Some(result) = Self::evaluate_expression(input) {
            return Some(Self::Absolute(result));
        }

        // Try simple number
        if let Ok(n) = input.parse::<usize>() {
            return Some(Self::Absolute(n));
        }

        // Try to extract leading number from incomplete expression (e.g., "16+" → 16)
        if let Some(n) = Self::extract_leading_number(input) {
            return Some(Self::Absolute(n));
        }

        None
    }

    /// Extract a leading number from a string that might have trailing operators
    /// e.g., "16+" → Some(16), "4x" → Some(4), "abc" → None
    fn extract_leading_number(input: &str) -> Option<usize> {
        let num_end = input
            .char_indices()
            .find(|(_, c)| !c.is_ascii_digit())
            .map(|(i, _)| i)
            .unwrap_or(input.len());

        if num_end > 0 {
            input[..num_end].parse::<usize>().ok()
        } else {
            None
        }
    }

    /// Evaluate an arithmetic expression
    ///
    /// Supports: +, -, x, *
    /// No spaces allowed within the expression (spaces separate tokens)
    fn evaluate_expression(input: &str) -> Option<usize> {
        // Try multiplication first (4x4 or 4*4)
        if let Some(pos) = input.find('x').or_else(|| input.find('*')) {
            let left = input[..pos].parse::<usize>().ok()?;
            let right = input[pos + 1..].parse::<usize>().ok()?;
            return Some(left * right);
        }

        // Try addition (8+1)
        // Find the last + or - that isn't at the start (to handle expressions, not signs)
        if let Some(pos) = input.rfind('+')
            && pos > 0
        {
            let left = input[..pos].parse::<usize>().ok()?;
            let right = input[pos + 1..].parse::<usize>().ok()?;
            return Some(left + right);
        }

        // Try subtraction (8-1)
        if let Some(pos) = input.rfind('-')
            && pos > 0
        {
            let left = input[..pos].parse::<usize>().ok()?;
            let right = input[pos + 1..].parse::<usize>().ok()?;
            return left.checked_sub(right);
        }

        None
    }

    /// Resolve the expression to an absolute measure count
    ///
    /// Uses the provided memory value for relative expressions
    pub fn resolve(&self, memory: Option<usize>) -> Option<usize> {
        match self {
            Self::UseMemory => memory,
            Self::Absolute(n) => Some(*n),
            Self::Add(n) => memory.map(|m| m + n),
            Self::Subtract(n) => memory.and_then(|m| m.checked_sub(*n)),
        }
    }

    /// Check if this expression modifies memory or just uses it
    pub fn updates_memory(&self) -> bool {
        matches!(self, Self::Absolute(_))
    }

    /// Get the absolute value if this is an absolute expression
    pub fn absolute_value(&self) -> Option<usize> {
        match self {
            Self::Absolute(n) => Some(*n),
            _ => None,
        }
    }
}

impl std::fmt::Display for MeasureExpression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UseMemory => Ok(()),
            Self::Absolute(n) => write!(f, "{n}"),
            Self::Add(n) => write!(f, "+{n}"),
            Self::Subtract(n) => write!(f, "-{n}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_absolute() {
        assert_eq!(
            MeasureExpression::parse("8"),
            Some(MeasureExpression::Absolute(8))
        );
        assert_eq!(
            MeasureExpression::parse("16"),
            Some(MeasureExpression::Absolute(16))
        );
        assert_eq!(
            MeasureExpression::parse("4"),
            Some(MeasureExpression::Absolute(4))
        );
    }

    #[test]
    fn test_parse_empty() {
        assert_eq!(
            MeasureExpression::parse(""),
            Some(MeasureExpression::UseMemory)
        );
        assert_eq!(
            MeasureExpression::parse("  "),
            Some(MeasureExpression::UseMemory)
        );
    }

    #[test]
    fn test_parse_relative_add() {
        assert_eq!(
            MeasureExpression::parse("+1"),
            Some(MeasureExpression::Add(1))
        );
        assert_eq!(
            MeasureExpression::parse("+4"),
            Some(MeasureExpression::Add(4))
        );
    }

    #[test]
    fn test_parse_relative_subtract() {
        assert_eq!(
            MeasureExpression::parse("-1"),
            Some(MeasureExpression::Subtract(1))
        );
        assert_eq!(
            MeasureExpression::parse("-2"),
            Some(MeasureExpression::Subtract(2))
        );
    }

    #[test]
    fn test_parse_addition_expression() {
        assert_eq!(
            MeasureExpression::parse("8+1"),
            Some(MeasureExpression::Absolute(9))
        );
        assert_eq!(
            MeasureExpression::parse("4+4"),
            Some(MeasureExpression::Absolute(8))
        );
    }

    #[test]
    fn test_parse_subtraction_expression() {
        assert_eq!(
            MeasureExpression::parse("8-1"),
            Some(MeasureExpression::Absolute(7))
        );
        assert_eq!(
            MeasureExpression::parse("10-2"),
            Some(MeasureExpression::Absolute(8))
        );
    }

    #[test]
    fn test_parse_multiplication() {
        assert_eq!(
            MeasureExpression::parse("4x4"),
            Some(MeasureExpression::Absolute(16))
        );
        assert_eq!(
            MeasureExpression::parse("4*4"),
            Some(MeasureExpression::Absolute(16))
        );
        assert_eq!(
            MeasureExpression::parse("2x8"),
            Some(MeasureExpression::Absolute(16))
        );
    }

    #[test]
    fn test_resolve_absolute() {
        let expr = MeasureExpression::Absolute(8);
        assert_eq!(expr.resolve(None), Some(8));
        assert_eq!(expr.resolve(Some(4)), Some(8));
    }

    #[test]
    fn test_resolve_use_memory() {
        let expr = MeasureExpression::UseMemory;
        assert_eq!(expr.resolve(None), None);
        assert_eq!(expr.resolve(Some(8)), Some(8));
    }

    #[test]
    fn test_resolve_add() {
        let expr = MeasureExpression::Add(1);
        assert_eq!(expr.resolve(None), None);
        assert_eq!(expr.resolve(Some(8)), Some(9));
    }

    #[test]
    fn test_resolve_subtract() {
        let expr = MeasureExpression::Subtract(1);
        assert_eq!(expr.resolve(None), None);
        assert_eq!(expr.resolve(Some(8)), Some(7));
        assert_eq!(expr.resolve(Some(0)), None); // Underflow protection
    }

    #[test]
    fn test_display() {
        assert_eq!(MeasureExpression::Absolute(8).to_string(), "8");
        assert_eq!(MeasureExpression::Add(1).to_string(), "+1");
        assert_eq!(MeasureExpression::Subtract(1).to_string(), "-1");
        assert_eq!(MeasureExpression::UseMemory.to_string(), "");
    }

    #[test]
    fn test_parse_incomplete_expressions() {
        // Incomplete expressions should extract the leading number
        assert_eq!(
            MeasureExpression::parse("16+"),
            Some(MeasureExpression::Absolute(16))
        );
        assert_eq!(
            MeasureExpression::parse("8-"),
            Some(MeasureExpression::Absolute(8))
        );
        assert_eq!(
            MeasureExpression::parse("4x"),
            Some(MeasureExpression::Absolute(4))
        );
        assert_eq!(
            MeasureExpression::parse("4*"),
            Some(MeasureExpression::Absolute(4))
        );
    }

    #[test]
    fn test_parse_incomplete_relative() {
        // Incomplete relative expressions should use memory
        assert_eq!(
            MeasureExpression::parse("+"),
            Some(MeasureExpression::UseMemory)
        );
        assert_eq!(
            MeasureExpression::parse("-"),
            Some(MeasureExpression::UseMemory)
        );
    }

    #[test]
    fn test_extract_leading_number() {
        assert_eq!(MeasureExpression::extract_leading_number("16+"), Some(16));
        assert_eq!(MeasureExpression::extract_leading_number("8-"), Some(8));
        assert_eq!(MeasureExpression::extract_leading_number("4x"), Some(4));
        assert_eq!(MeasureExpression::extract_leading_number("abc"), None);
        assert_eq!(MeasureExpression::extract_leading_number("+1"), None);
    }
}
