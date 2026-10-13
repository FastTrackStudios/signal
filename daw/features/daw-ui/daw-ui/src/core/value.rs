//! Value types with validation, stepping, and formatting.
//!
//! Provides robust value handling with:
//! - Stepped/quantized values
//! - Snap-to behavior
//! - Value formatting
//! - Input validation

use crate::core::normal::Normal;

/// Value stepping configuration.
///
/// Controls how values are quantized and snapped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueStepping {
    /// Step size in normalized units (0.0-1.0).
    /// Set to 0.0 for continuous (unstepped) values.
    pub step: f32,
    /// Whether to snap to step boundaries.
    pub snap: bool,
    /// Snap threshold (how close to snap).
    /// Only used when `snap` is true.
    pub snap_threshold: f32,
}

impl ValueStepping {
    /// Continuous (unstepped) values.
    pub const CONTINUOUS: Self = Self {
        step: 0.0,
        snap: false,
        snap_threshold: 0.0,
    };

    /// Create stepping for a specific number of discrete values.
    ///
    /// # Example
    /// ```
    /// use audio_controls::core::value::ValueStepping;
    ///
    /// // 5 discrete values: 0.0, 0.25, 0.5, 0.75, 1.0
    /// let stepping = ValueStepping::discrete(5);
    /// ```
    #[must_use]
    pub fn discrete(count: usize) -> Self {
        let step = if count <= 1 {
            1.0
        } else {
            1.0 / (count - 1) as f32
        };
        Self {
            step,
            snap: true,
            snap_threshold: step * 0.5,
        }
    }

    /// Create stepping with a specific step size.
    #[must_use]
    pub fn with_step(step: f32) -> Self {
        Self {
            step: step.max(0.0),
            snap: false,
            snap_threshold: 0.0,
        }
    }

    /// Enable snapping to step boundaries.
    #[must_use]
    pub fn with_snap(mut self) -> Self {
        self.snap = true;
        self.snap_threshold = self.step * 0.5;
        self
    }

    /// Set custom snap threshold.
    #[must_use]
    pub fn with_snap_threshold(mut self, threshold: f32) -> Self {
        self.snap = true;
        self.snap_threshold = threshold;
        self
    }

    /// Apply stepping to a normalized value.
    #[must_use]
    pub fn apply(&self, value: Normal) -> Normal {
        if self.step <= 0.0 {
            return value;
        }

        let v = value.value();

        if self.snap {
            // Snap to nearest step
            let snapped = (v / self.step).round() * self.step;
            Normal::new(snapped)
        } else {
            // Quantize but don't snap during drag
            let quantized = (v / self.step).floor() * self.step;
            Normal::new(quantized)
        }
    }

    /// Check if value is near a step boundary.
    #[must_use]
    pub fn is_near_step(&self, value: Normal) -> bool {
        if self.step <= 0.0 {
            return false;
        }

        let v = value.value();
        let nearest_step = (v / self.step).round() * self.step;
        (v - nearest_step).abs() < self.snap_threshold
    }

    /// Get the nearest step value.
    #[must_use]
    pub fn nearest_step(&self, value: Normal) -> Normal {
        if self.step <= 0.0 {
            return value;
        }

        let v = value.value();
        let snapped = (v / self.step).round() * self.step;
        Normal::new(snapped)
    }

    /// Get all step values.
    pub fn all_steps(&self) -> Vec<f32> {
        if self.step <= 0.0 {
            return vec![0.0, 1.0];
        }

        let mut values = Vec::new();
        let mut v = 0.0;
        while v <= 1.0 + f32::EPSILON {
            values.push(v.min(1.0));
            v += self.step;
        }
        values
    }

    /// Get the number of steps.
    #[must_use]
    pub fn step_count(&self) -> usize {
        if self.step <= 0.0 {
            0
        } else {
            ((1.0 / self.step).round() as usize).saturating_add(1)
        }
    }
}

impl Default for ValueStepping {
    fn default() -> Self {
        Self::CONTINUOUS
    }
}

/// Value formatter for display.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueFormatter {
    /// Format as percentage (0-100%).
    Percent { decimals: usize },
    /// Format as decibels.
    Decibels { decimals: usize, min_db: f32 },
    /// Format as frequency (Hz/kHz).
    Frequency { decimals: usize },
    /// Format as plain number with optional unit.
    Number { decimals: usize, unit: String },
}

impl ValueFormatter {
    /// Format a value in the 0.0-1.0 range.
    #[must_use]
    pub fn format(&self, value: f32) -> String {
        match self {
            Self::Percent { decimals } => {
                format!("{:.decimals$}%", value * 100.0, decimals = decimals)
            }
            Self::Decibels { decimals, min_db } => {
                // Map 0-1 to min_db-0 dB (typical gain range)
                let db = value * (-min_db) + min_db;
                if db <= *min_db + 0.1 {
                    "-inf dB".to_string()
                } else {
                    format!("{db:+.decimals$} dB", decimals = decimals)
                }
            }
            Self::Frequency { decimals } => {
                // Logarithmic mapping: 20Hz-20kHz
                let hz = 20.0 * (1000.0_f32).powf(value);
                if hz >= 1000.0 {
                    format!("{:.decimals$} kHz", hz / 1000.0, decimals = decimals)
                } else {
                    format!("{hz:.decimals$} Hz", decimals = decimals)
                }
            }
            Self::Number { decimals, unit } => {
                if unit.is_empty() {
                    format!("{value:.decimals$}", decimals = decimals)
                } else {
                    format!("{value:.decimals$} {unit}", decimals = decimals)
                }
            }
        }
    }

    /// Create a percentage formatter.
    #[must_use]
    pub const fn percent() -> Self {
        Self::Percent { decimals: 0 }
    }

    /// Create a decibel formatter.
    #[must_use]
    pub const fn decibels() -> Self {
        Self::Decibels {
            decimals: 1,
            min_db: -60.0,
        }
    }

    /// Create a frequency formatter.
    #[must_use]
    pub const fn frequency() -> Self {
        Self::Frequency { decimals: 1 }
    }
}

impl Default for ValueFormatter {
    fn default() -> Self {
        Self::Percent { decimals: 0 }
    }
}

/// Result of validating a text input for a parameter value.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseResult {
    /// Successfully parsed a valid value.
    Valid(f32),
    /// Input is empty (may want to keep current value).
    Empty,
    /// Input is not a valid number.
    Invalid(String),
    /// Input is out of range.
    OutOfRange { value: f32, min: f32, max: f32 },
}

impl ParseResult {
    /// Check if the result is valid.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        matches!(self, Self::Valid(_))
    }

    /// Get the value if valid.
    #[must_use]
    pub fn value(&self) -> Option<f32> {
        match self {
            Self::Valid(v) => Some(*v),
            Self::OutOfRange { value, min, max } => Some(value.clamp(*min, *max)),
            _ => None,
        }
    }

    /// Get error message if invalid.
    #[must_use]
    pub fn error_message(&self) -> Option<String> {
        match self {
            Self::Invalid(msg) => Some(msg.clone()),
            Self::OutOfRange { value, min, max } => {
                Some(format!("{value} is out of range ({min} - {max})"))
            }
            _ => None,
        }
    }
}

/// Parse user text input into a parameter value.
pub struct ValueParser {
    /// Minimum value.
    pub min: f32,
    /// Maximum value.
    pub max: f32,
    /// Allow out-of-range values (will be clamped).
    pub allow_out_of_range: bool,
}

impl ValueParser {
    /// Create a parser for the 0-1 normalized range.
    #[must_use]
    pub const fn normalized() -> Self {
        Self {
            min: 0.0,
            max: 1.0,
            allow_out_of_range: true,
        }
    }

    /// Create a parser for a custom range.
    #[must_use]
    pub const fn range(min: f32, max: f32) -> Self {
        Self {
            min,
            max,
            allow_out_of_range: true,
        }
    }

    /// Parse input text.
    pub fn parse(&self, input: &str) -> ParseResult {
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return ParseResult::Empty;
        }

        // Remove common suffixes
        let cleaned = trimmed
            .trim_end_matches('%')
            .trim_end_matches("dB")
            .trim_end_matches("db")
            .trim_end_matches("Hz")
            .trim_end_matches("hz")
            .trim_end_matches("kHz")
            .trim_end_matches("khz")
            .trim();

        // Handle special cases
        if cleaned.eq_ignore_ascii_case("-inf") || cleaned.eq_ignore_ascii_case("-∞") {
            return ParseResult::Valid(self.min);
        }

        match cleaned.parse::<f32>() {
            Ok(value) => {
                // Handle kHz suffix
                let value = if trimmed.to_lowercase().contains("khz") {
                    value * 1000.0
                } else {
                    value
                };

                // Handle percentage
                let value = if trimmed.contains('%') && value > 1.0 {
                    value / 100.0
                } else {
                    value
                };

                if value >= self.min && value <= self.max {
                    ParseResult::Valid(value)
                } else if self.allow_out_of_range {
                    ParseResult::OutOfRange {
                        value,
                        min: self.min,
                        max: self.max,
                    }
                } else {
                    ParseResult::Invalid(format!(
                        "Value must be between {} and {}",
                        self.min, self.max
                    ))
                }
            }
            Err(_) => ParseResult::Invalid(format!("'{trimmed}' is not a valid number")),
        }
    }
}

impl Default for ValueParser {
    fn default() -> Self {
        Self::normalized()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_stepping_discrete() {
        let stepping = ValueStepping::discrete(5);
        assert_eq!(stepping.step_count(), 5);

        // Test all step values
        let steps = stepping.all_steps();
        assert_eq!(steps.len(), 5);
        assert!((steps[0] - 0.0).abs() < f32::EPSILON);
        assert!((steps[2] - 0.5).abs() < f32::EPSILON);
        assert!((steps[4] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn value_stepping_apply() {
        let stepping = ValueStepping::discrete(5).with_snap();

        // Should snap to nearest 0.25
        let snapped = stepping.apply(Normal::new(0.3));
        assert!((snapped.value() - 0.25).abs() < f32::EPSILON);

        let snapped = stepping.apply(Normal::new(0.4));
        assert!((snapped.value() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn value_formatter_percent() {
        let formatter = ValueFormatter::Percent { decimals: 1 };
        assert_eq!(formatter.format(0.5), "50.0%");
        assert_eq!(formatter.format(1.0), "100.0%");
    }

    #[test]
    fn value_parser_basic() {
        let parser = ValueParser::normalized();

        assert!(
            matches!(parser.parse("0.5"), ParseResult::Valid(v) if (v - 0.5).abs() < f32::EPSILON)
        );
        assert!(
            matches!(parser.parse("50%"), ParseResult::Valid(v) if (v - 0.5).abs() < f32::EPSILON)
        );
        assert!(matches!(parser.parse(""), ParseResult::Empty));
        assert!(matches!(parser.parse("abc"), ParseResult::Invalid(_)));
    }

    #[test]
    fn value_parser_out_of_range() {
        let parser = ValueParser::range(0.0, 1.0);

        match parser.parse("1.5") {
            ParseResult::OutOfRange { value, min, max } => {
                assert!((value - 1.5).abs() < f32::EPSILON);
                assert!((min - 0.0).abs() < f32::EPSILON);
                assert!((max - 1.0).abs() < f32::EPSILON);
            }
            _ => panic!("Expected OutOfRange"),
        }
    }

    #[test]
    fn value_parser_special_cases() {
        let parser = ValueParser::range(-60.0, 6.0);

        assert!(
            matches!(parser.parse("-inf"), ParseResult::Valid(v) if (v - (-60.0)).abs() < f32::EPSILON)
        );
        assert!(
            matches!(parser.parse("-6 dB"), ParseResult::Valid(v) if (v - (-6.0)).abs() < f32::EPSILON)
        );
    }
}
