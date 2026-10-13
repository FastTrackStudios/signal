//! Error handling for chord parsing
//!
//! Provides detailed, position-aware errors for chord parsing with verbose display

use std::{error::Error, fmt};

/// Errors that can occur when parsing a chord
/// Includes position tracking (1-based) and detailed error messages
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChordParseError {
    /// Empty input provided
    EmptyInput,

    /// Missing root note
    MissingRootNote,

    /// Illegal token at position (1-based)
    IllegalToken(usize),

    /// Unexpected note at position (1-based)
    UnexpectedNote(usize),

    /// Duplicate modifier (e.g., multiple sharps/flats on same degree)
    DuplicateModifier { modifier: String, position: usize },

    /// Duplicate extension (e.g., two 9ths)
    DuplicateExtension { position: usize, degree: String },

    /// Invalid extension number
    InvalidExtension { position: usize, value: String },

    /// Inconsistent extensions (e.g., b9 and #9)
    InconsistentExtension { first: String, second: String },

    /// Unexpected modifier at position
    UnexpectedModifier(usize),

    /// Invalid alteration for this chord
    InvalidAlteration {
        position: usize,
        degree: String,
        reason: String,
    },

    /// Missing add target
    MissingAddTarget { position: usize, length: usize },

    /// Illegal add target
    IllegalAddTarget {
        position: usize,
        length: usize,
        target: String,
    },

    /// Illegal or missing omit target
    IllegalOrMissingOmitTarget { position: usize, length: usize },

    /// Illegal slash chord notation
    IllegalSlashNotation { position: usize, reason: String },

    /// Unexpected closing parenthesis
    UnexpectedClosingParenthesis(usize),

    /// Missing closing parenthesis
    MissingClosingParenthesis(usize),

    /// Nested parenthesis (not allowed)
    NestedParenthesis(usize),

    /// Wrong expression target
    WrongExpressionTarget {
        position: usize,
        expected: String,
        found: String,
    },

    /// Three consecutive semitones (voice leading issue)
    ThreeConsecutiveSemitones(Vec<String>),

    /// Invalid power chord expression
    InvalidPowerExpression { reason: String },

    /// Conflicting chord components
    ConflictingComponents { first: String, second: String },

    /// Alteration on non-existent degree
    AlterationOnMissingDegree { degree: String, alteration: String },

    /// Invalid quality for family
    InvalidQualityForFamily { quality: String, family: String },
}

impl ChordParseError {
    /// Returns the position in the input string where the error occurred (1-based)
    /// Returns None if the error is not related to a specific position
    pub fn error_position(&self) -> Option<usize> {
        match self {
            ChordParseError::IllegalToken(pos)
            | ChordParseError::UnexpectedNote(pos)
            | ChordParseError::DuplicateModifier { position: pos, .. }
            | ChordParseError::DuplicateExtension { position: pos, .. }
            | ChordParseError::InvalidExtension { position: pos, .. }
            | ChordParseError::UnexpectedModifier(pos)
            | ChordParseError::InvalidAlteration { position: pos, .. }
            | ChordParseError::IllegalSlashNotation { position: pos, .. }
            | ChordParseError::UnexpectedClosingParenthesis(pos)
            | ChordParseError::MissingClosingParenthesis(pos)
            | ChordParseError::NestedParenthesis(pos)
            | ChordParseError::WrongExpressionTarget { position: pos, .. } => Some(*pos),

            ChordParseError::MissingAddTarget { position, length }
            | ChordParseError::IllegalAddTarget {
                position, length, ..
            }
            | ChordParseError::IllegalOrMissingOmitTarget { position, length } => {
                Some(*position + *length)
            }

            ChordParseError::MissingRootNote => Some(1),

            ChordParseError::EmptyInput
            | ChordParseError::InconsistentExtension { .. }
            | ChordParseError::ThreeConsecutiveSemitones(_)
            | ChordParseError::InvalidPowerExpression { .. }
            | ChordParseError::ConflictingComponents { .. }
            | ChordParseError::AlterationOnMissingDegree { .. }
            | ChordParseError::InvalidQualityForFamily { .. } => None,
        }
    }

    /// Surrounds the element at the given index with a marker (-> indicator)
    fn surround_element_at_index(&self, s: &str, index: usize) -> String {
        if index == 0 || index > s.len() {
            return format!("{}(_)", s);
        }

        let index = index - 1; // Convert to 0-based
        let before = &s[..index];
        let after = &s[index..];

        format!("{} ->{}", before, after)
    }

    /// Surrounds the element at the given index with a span marker
    fn surround_element_at_index_with_span(&self, s: &str, index: usize, len: usize) -> String {
        let index = index.saturating_sub(1) + len;
        if index >= s.len() {
            return format!("{}(_)", s);
        }

        let before = &s[..index];
        let after = &s[index..];

        format!("{} ->{}", before, after)
    }

    /// Returns a verbose display of the error, including the element at the position
    /// where the error occurred, with a visual indicator
    pub fn verbose_display(&self, origin: &str) -> String {
        match self {
            ChordParseError::IllegalToken(pos)
            | ChordParseError::UnexpectedNote(pos)
            | ChordParseError::UnexpectedModifier(pos)
            | ChordParseError::UnexpectedClosingParenthesis(pos)
            | ChordParseError::MissingClosingParenthesis(pos)
            | ChordParseError::NestedParenthesis(pos) => {
                format!(
                    "{}\n  {}",
                    self,
                    self.surround_element_at_index(origin, *pos)
                )
            }

            ChordParseError::DuplicateModifier { position: pos, .. }
            | ChordParseError::DuplicateExtension { position: pos, .. }
            | ChordParseError::InvalidExtension { position: pos, .. }
            | ChordParseError::InvalidAlteration { position: pos, .. }
            | ChordParseError::IllegalSlashNotation { position: pos, .. }
            | ChordParseError::WrongExpressionTarget { position: pos, .. } => {
                format!(
                    "{}\n  {}",
                    self,
                    self.surround_element_at_index(origin, *pos)
                )
            }

            ChordParseError::MissingAddTarget { position, length }
            | ChordParseError::IllegalAddTarget {
                position, length, ..
            }
            | ChordParseError::IllegalOrMissingOmitTarget { position, length } => {
                format!(
                    "{}\n  {}",
                    self,
                    self.surround_element_at_index_with_span(origin, *position, *length)
                )
            }

            _ => format!("{}", self),
        }
    }
}

impl fmt::Display for ChordParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChordParseError::EmptyInput => write!(f, "Empty input"),
            ChordParseError::MissingRootNote => write!(f, "Missing root note"),
            ChordParseError::IllegalToken(pos) => {
                write!(f, "Illegal token at position {}", pos)
            }
            ChordParseError::UnexpectedNote(pos) => {
                write!(f, "Unexpected note at position {}", pos)
            }
            ChordParseError::DuplicateModifier { modifier, position } => {
                write!(
                    f,
                    "Duplicate modifier '{}' at position {}",
                    modifier, position
                )
            }
            ChordParseError::DuplicateExtension { position, degree } => {
                write!(
                    f,
                    "Duplicate extension '{}' at position {}",
                    degree, position
                )
            }
            ChordParseError::InvalidExtension { position, value } => {
                write!(f, "Invalid extension '{}' at position {}", value, position)
            }
            ChordParseError::InconsistentExtension { first, second } => {
                write!(f, "Inconsistent extensions: '{}' and '{}'", first, second)
            }
            ChordParseError::UnexpectedModifier(pos) => {
                write!(f, "Unexpected modifier at position {}", pos)
            }
            ChordParseError::InvalidAlteration {
                position,
                degree,
                reason,
            } => {
                write!(
                    f,
                    "Invalid alteration for degree '{}' at position {}: {}",
                    degree, position, reason
                )
            }
            ChordParseError::MissingAddTarget { position, length } => {
                write!(f, "Missing add target at position {}", position + length)
            }
            ChordParseError::IllegalAddTarget {
                position,
                length,
                target,
            } => {
                write!(
                    f,
                    "Illegal add target '{}' at position {}",
                    target,
                    position + length
                )
            }
            ChordParseError::IllegalOrMissingOmitTarget { position, length } => {
                write!(
                    f,
                    "Illegal or missing omit target at position {}",
                    position + length
                )
            }
            ChordParseError::IllegalSlashNotation { position, reason } => {
                write!(
                    f,
                    "Illegal slash notation at position {}: {}",
                    position, reason
                )
            }
            ChordParseError::UnexpectedClosingParenthesis(pos) => {
                write!(f, "Unexpected closing parenthesis at position {}", pos)
            }
            ChordParseError::MissingClosingParenthesis(pos) => {
                write!(f, "Missing closing parenthesis at position {}", pos)
            }
            ChordParseError::NestedParenthesis(pos) => {
                write!(f, "Nested parenthesis at position {}", pos)
            }
            ChordParseError::WrongExpressionTarget {
                position,
                expected,
                found,
            } => {
                write!(
                    f,
                    "Wrong expression target at position {}: expected '{}', found '{}'",
                    position, expected, found
                )
            }
            ChordParseError::ThreeConsecutiveSemitones(notes) => {
                write!(f, "Three consecutive semitones: {:?}", notes)
            }
            ChordParseError::InvalidPowerExpression { reason } => {
                write!(f, "Invalid power chord expression: {}", reason)
            }
            ChordParseError::ConflictingComponents { first, second } => {
                write!(
                    f,
                    "Conflicting chord components: '{}' and '{}'",
                    first, second
                )
            }
            ChordParseError::AlterationOnMissingDegree { degree, alteration } => {
                write!(
                    f,
                    "Cannot apply alteration '{}' to degree '{}' which is not present in chord",
                    alteration, degree
                )
            }
            ChordParseError::InvalidQualityForFamily { quality, family } => {
                write!(f, "Invalid quality '{}' for family '{}'", quality, family)
            }
        }
    }
}

impl Error for ChordParseError {}

/// Multiple chord parsing errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChordParseErrors {
    pub errors: Vec<ChordParseError>,
}

impl ChordParseErrors {
    pub fn new(errors: Vec<ChordParseError>) -> Self {
        Self { errors }
    }

    pub fn add(&mut self, error: ChordParseError) {
        self.errors.push(error);
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// Returns verbose display of all errors
    pub fn verbose_display(&self, origin: &str) -> String {
        self.errors
            .iter()
            .map(|e| e.verbose_display(origin))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl fmt::Display for ChordParseErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Chord parse errors: ")?;
        for (i, error) in self.errors.iter().enumerate() {
            if i > 0 {
                write!(f, "; ")?;
            }
            write!(f, "{}", error)?;
        }
        Ok(())
    }
}

impl Error for ChordParseErrors {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input_error() {
        let error = ChordParseError::EmptyInput;
        assert_eq!(error.error_position(), None);
        assert_eq!(format!("{}", error), "Empty input");
    }

    #[test]
    fn test_missing_root_note_error() {
        let error = ChordParseError::MissingRootNote;
        assert_eq!(error.error_position(), Some(1));
        assert_eq!(format!("{}", error), "Missing root note");
    }

    #[test]
    fn test_illegal_token_error() {
        let error = ChordParseError::IllegalToken(5);
        assert_eq!(error.error_position(), Some(5));
        assert_eq!(format!("{}", error), "Illegal token at position 5");
    }

    #[test]
    fn test_duplicate_extension_error() {
        let error = ChordParseError::DuplicateExtension {
            position: 7,
            degree: "9".to_string(),
        };
        assert_eq!(error.error_position(), Some(7));
        assert_eq!(
            format!("{}", error),
            "Duplicate extension '9' at position 7"
        );
    }

    #[test]
    fn test_invalid_extension_error() {
        let error = ChordParseError::InvalidExtension {
            position: 4,
            value: "8".to_string(),
        };
        assert_eq!(error.error_position(), Some(4));
        assert_eq!(format!("{}", error), "Invalid extension '8' at position 4");
    }

    #[test]
    fn test_inconsistent_extension_error() {
        let error = ChordParseError::InconsistentExtension {
            first: "b9".to_string(),
            second: "#9".to_string(),
        };
        assert_eq!(error.error_position(), None);
        assert_eq!(
            format!("{}", error),
            "Inconsistent extensions: 'b9' and '#9'"
        );
    }

    #[test]
    fn test_invalid_alteration_error() {
        let error = ChordParseError::InvalidAlteration {
            position: 6,
            degree: "5".to_string(),
            reason: "Cannot alter fifth in power chord".to_string(),
        };
        assert_eq!(error.error_position(), Some(6));
        assert_eq!(
            format!("{}", error),
            "Invalid alteration for degree '5' at position 6: Cannot alter fifth in power chord"
        );
    }

    #[test]
    fn test_missing_add_target_error() {
        let error = ChordParseError::MissingAddTarget {
            position: 8,
            length: 3,
        };
        assert_eq!(error.error_position(), Some(11)); // position + length
        assert_eq!(format!("{}", error), "Missing add target at position 11");
    }

    #[test]
    fn test_illegal_add_target_error() {
        let error = ChordParseError::IllegalAddTarget {
            position: 5,
            length: 3,
            target: "8".to_string(),
        };
        assert_eq!(error.error_position(), Some(8)); // position + length
        assert_eq!(format!("{}", error), "Illegal add target '8' at position 8");
    }

    #[test]
    fn test_illegal_slash_notation_error() {
        let error = ChordParseError::IllegalSlashNotation {
            position: 10,
            reason: "Bass note cannot be a scale degree".to_string(),
        };
        assert_eq!(error.error_position(), Some(10));
        assert_eq!(
            format!("{}", error),
            "Illegal slash notation at position 10: Bass note cannot be a scale degree"
        );
    }

    #[test]
    fn test_unexpected_closing_parenthesis_error() {
        let error = ChordParseError::UnexpectedClosingParenthesis(12);
        assert_eq!(error.error_position(), Some(12));
        assert_eq!(
            format!("{}", error),
            "Unexpected closing parenthesis at position 12"
        );
    }

    #[test]
    fn test_missing_closing_parenthesis_error() {
        let error = ChordParseError::MissingClosingParenthesis(8);
        assert_eq!(error.error_position(), Some(8));
        assert_eq!(
            format!("{}", error),
            "Missing closing parenthesis at position 8"
        );
    }

    #[test]
    fn test_nested_parenthesis_error() {
        let error = ChordParseError::NestedParenthesis(9);
        assert_eq!(error.error_position(), Some(9));
        assert_eq!(format!("{}", error), "Nested parenthesis at position 9");
    }

    #[test]
    fn test_wrong_expression_target_error() {
        let error = ChordParseError::WrongExpressionTarget {
            position: 7,
            expected: "number".to_string(),
            found: "letter".to_string(),
        };
        assert_eq!(error.error_position(), Some(7));
        assert_eq!(
            format!("{}", error),
            "Wrong expression target at position 7: expected 'number', found 'letter'"
        );
    }

    #[test]
    fn test_three_consecutive_semitones_error() {
        let error = ChordParseError::ThreeConsecutiveSemitones(vec![
            "C".to_string(),
            "C#".to_string(),
            "D".to_string(),
        ]);
        assert_eq!(error.error_position(), None);
        assert!(format!("{}", error).contains("Three consecutive semitones"));
    }

    #[test]
    fn test_invalid_power_expression_error() {
        let error = ChordParseError::InvalidPowerExpression {
            reason: "Power chords should only contain root and fifth".to_string(),
        };
        assert_eq!(error.error_position(), None);
        assert_eq!(
            format!("{}", error),
            "Invalid power chord expression: Power chords should only contain root and fifth"
        );
    }

    #[test]
    fn test_conflicting_components_error() {
        let error = ChordParseError::ConflictingComponents {
            first: "major third".to_string(),
            second: "suspended fourth".to_string(),
        };
        assert_eq!(error.error_position(), None);
        assert_eq!(
            format!("{}", error),
            "Conflicting chord components: 'major third' and 'suspended fourth'"
        );
    }

    #[test]
    fn test_alteration_on_missing_degree_error() {
        let error = ChordParseError::AlterationOnMissingDegree {
            degree: "9".to_string(),
            alteration: "b9".to_string(),
        };
        assert_eq!(error.error_position(), None);
        assert_eq!(
            format!("{}", error),
            "Cannot apply alteration 'b9' to degree '9' which is not present in chord"
        );
    }

    #[test]
    fn test_invalid_quality_for_family_error() {
        let error = ChordParseError::InvalidQualityForFamily {
            quality: "augmented".to_string(),
            family: "minor seventh".to_string(),
        };
        assert_eq!(error.error_position(), None);
        assert_eq!(
            format!("{}", error),
            "Invalid quality 'augmented' for family 'minor seventh'"
        );
    }

    #[test]
    fn test_verbose_display_with_position() {
        let error = ChordParseError::IllegalToken(3);
        let origin = "Cmx7";
        let verbose = error.verbose_display(origin);

        assert!(verbose.contains("Illegal token"));
        assert!(verbose.contains("->"));
        assert!(verbose.contains("Cm"));
    }

    #[test]
    fn test_verbose_display_with_span() {
        let error = ChordParseError::MissingAddTarget {
            position: 4,
            length: 3,
        };
        let origin = "Cmajadd";
        let verbose = error.verbose_display(origin);

        assert!(verbose.contains("Missing add target"));
        assert!(verbose.contains("->"));
    }

    #[test]
    fn test_verbose_display_without_position() {
        let error = ChordParseError::ConflictingComponents {
            first: "major".to_string(),
            second: "minor".to_string(),
        };
        let origin = "Cmaj";
        let verbose = error.verbose_display(origin);

        // Should just show the error message without position indicator
        assert!(verbose.contains("Conflicting"));
        assert!(!verbose.contains("->"));
    }

    #[test]
    fn test_chord_parse_errors_collection() {
        let mut errors = ChordParseErrors::new(vec![]);
        assert!(errors.is_empty());
        assert_eq!(errors.len(), 0);

        errors.add(ChordParseError::EmptyInput);
        assert!(!errors.is_empty());
        assert_eq!(errors.len(), 1);

        errors.add(ChordParseError::MissingRootNote);
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn test_chord_parse_errors_display() {
        let errors = ChordParseErrors::new(vec![
            ChordParseError::EmptyInput,
            ChordParseError::IllegalToken(5),
        ]);

        let display = format!("{}", errors);
        assert!(display.contains("Empty input"));
        assert!(display.contains("Illegal token"));
    }

    #[test]
    fn test_chord_parse_errors_verbose_display() {
        let errors = ChordParseErrors::new(vec![
            ChordParseError::IllegalToken(3),
            ChordParseError::DuplicateExtension {
                position: 7,
                degree: "9".to_string(),
            },
        ]);

        let origin = "Cmx7b9b9";
        let verbose = errors.verbose_display(origin);

        assert!(verbose.contains("Illegal token"));
        assert!(verbose.contains("Duplicate extension"));
        assert!(verbose.contains("->"));
    }

    #[test]
    fn test_surround_element_at_index_start() {
        let error = ChordParseError::IllegalToken(1);
        let origin = "Xmaj7";
        let verbose = error.verbose_display(origin);

        // Should point at the first character
        assert!(verbose.contains(" ->Xmaj7") || verbose.contains("->Xmaj7"));
    }

    #[test]
    fn test_surround_element_at_index_middle() {
        let error = ChordParseError::IllegalToken(4);
        let origin = "Cmaj7";
        let verbose = error.verbose_display(origin);

        // Should point at the 4th character (a)
        assert!(verbose.contains("Cma ->j7"));
    }

    #[test]
    fn test_surround_element_at_index_end() {
        let error = ChordParseError::IllegalToken(6);
        let origin = "Cmaj7";
        let verbose = error.verbose_display(origin);

        // Should handle end-of-string gracefully
        assert!(verbose.contains("Cmaj7"));
    }

    #[test]
    fn test_surround_element_at_index_out_of_bounds() {
        let error = ChordParseError::IllegalToken(20);
        let origin = "Cmaj7";
        let verbose = error.verbose_display(origin);

        // Should handle out-of-bounds gracefully
        assert!(verbose.contains("Cmaj7(_)"));
    }

    #[test]
    fn test_duplicate_modifier_error() {
        let error = ChordParseError::DuplicateModifier {
            modifier: "sharp".to_string(),
            position: 5,
        };
        assert_eq!(error.error_position(), Some(5));
        assert_eq!(
            format!("{}", error),
            "Duplicate modifier 'sharp' at position 5"
        );
    }

    #[test]
    fn test_illegal_omit_target_error() {
        let error = ChordParseError::IllegalOrMissingOmitTarget {
            position: 6,
            length: 4,
        };
        assert_eq!(error.error_position(), Some(10)); // position + length
        assert_eq!(
            format!("{}", error),
            "Illegal or missing omit target at position 10"
        );
    }

    #[test]
    fn test_error_equality() {
        let error1 = ChordParseError::IllegalToken(5);
        let error2 = ChordParseError::IllegalToken(5);
        let error3 = ChordParseError::IllegalToken(6);

        assert_eq!(error1, error2);
        assert_ne!(error1, error3);
    }

    #[test]
    fn test_errors_collection_equality() {
        let errors1 = ChordParseErrors::new(vec![
            ChordParseError::EmptyInput,
            ChordParseError::IllegalToken(5),
        ]);

        let errors2 = ChordParseErrors::new(vec![
            ChordParseError::EmptyInput,
            ChordParseError::IllegalToken(5),
        ]);

        let errors3 = ChordParseErrors::new(vec![ChordParseError::EmptyInput]);

        assert_eq!(errors1, errors2);
        assert_ne!(errors1, errors3);
    }
}
