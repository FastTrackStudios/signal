//! Error types for macro system operations.
//!
//! Provides clear, actionable error messages for debugging macro binding
//! and parameter resolution issues.

use std::fmt;

/// Result type for macro system operations.
pub type MacroResult<T> = Result<T, MacroError>;

/// Errors that can occur during macro setup and parameter resolution.
#[derive(Debug, Clone)]
pub enum MacroError {
    /// Failed to get FX parameters from the plugin.
    /// This typically indicates a DAW communication issue.
    FxParametersFailed(String),

    /// A macro binding references a parameter that doesn't exist on the target FX.
    ///
    /// # Example
    /// ```text
    /// ParameterNotFound {
    ///     fx_name: "ReaComp",
    ///     sought: "unknown_param",
    ///     available: ["ratio", "threshold", "attack", ...]
    /// }
    /// ```
    ParameterNotFound {
        /// Name of the target FX plugin
        fx_name: String,
        /// Parameter name that was sought
        sought: String,
        /// Available parameter names on the FX
        available: Vec<String>,
    },

    /// No bindings exist in the macro bank.
    NoBindings,

    /// No macro bank exists on the block.
    NoMacroBank,

    /// A parameter binding references an invalid knob or sub-macro ID.
    InvalidKnobRef(String),

    /// Generic binding resolution error.
    ResolutionFailed(String),
}

impl fmt::Display for MacroError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MacroError::FxParametersFailed(reason) => {
                write!(f, "Failed to get FX parameters: {}", reason)
            }
            MacroError::ParameterNotFound {
                fx_name,
                sought,
                available,
            } => {
                write!(
                    f,
                    "Parameter '{}' not found on FX '{}'. Available: {}",
                    sought,
                    fx_name,
                    available.join(", ")
                )
            }
            MacroError::NoBindings => write!(f, "No bindings in macro bank"),
            MacroError::NoMacroBank => write!(f, "No macro bank on block"),
            MacroError::InvalidKnobRef(knob_id) => {
                write!(f, "Invalid knob reference: '{}'", knob_id)
            }
            MacroError::ResolutionFailed(reason) => {
                write!(f, "Macro resolution failed: {}", reason)
            }
        }
    }
}

impl std::error::Error for MacroError {}

/// Validates macro setup inputs before processing.
///
/// # Returns
///
/// - `Ok(())` if inputs are valid
/// - `Err(MacroError)` if validation fails
pub fn validate_macro_bank(bank: &macromod::MacroBank) -> MacroResult<()> {
    if bank.knobs.is_empty() && bank.groups.is_empty() {
        return Err(MacroError::NoBindings);
    }

    // Validate that all knob IDs are non-empty
    for knob in &bank.knobs {
        if knob.id.is_empty() {
            return Err(MacroError::InvalidKnobRef(
                "knob with empty ID in main list".to_string(),
            ));
        }
        for child in &knob.children {
            if child.id.is_empty() {
                return Err(MacroError::InvalidKnobRef(
                    format!("child knob of '{}' has empty ID", knob.id),
                ));
            }
        }
    }

    // Validate group knobs
    for group in &bank.groups {
        for knob in &group.knobs {
            if knob.id.is_empty() {
                return Err(MacroError::InvalidKnobRef(
                    "knob with empty ID in group".to_string(),
                ));
            }
            for child in &knob.children {
                if child.id.is_empty() {
                    return Err(MacroError::InvalidKnobRef(
                        format!("child knob of '{}' in group has empty ID", knob.id),
                    ));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use macromod::MacroKnob;

    #[test]
    fn test_validate_empty_bank() {
        let bank = macromod::MacroBank::default();
        let result = validate_macro_bank(&bank);
        assert!(matches!(result, Err(MacroError::NoBindings)));
    }

    #[test]
    fn test_validate_valid_bank() {
        let mut bank = macromod::MacroBank::default();
        let mut knob = MacroKnob::new("drive", "Drive");
        knob.bindings
            .push(macromod::MacroBinding::from_ids("eq", "low", 0.0, 1.0));
        bank.add(knob);

        let result = validate_macro_bank(&bank);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_rejects_empty_knob_id() {
        let mut bank = macromod::MacroBank::default();
        let mut knob = MacroKnob::new("", "Empty ID");
        knob.bindings
            .push(macromod::MacroBinding::from_ids("eq", "low", 0.0, 1.0));
        bank.add(knob);

        let result = validate_macro_bank(&bank);
        assert!(matches!(
            result,
            Err(MacroError::InvalidKnobRef(_))
        ));
    }

    #[test]
    fn test_error_display() {
        let err = MacroError::NoMacroBank;
        assert_eq!(err.to_string(), "No macro bank on block");

        let err = MacroError::ParameterNotFound {
            fx_name: "ReaEQ".to_string(),
            sought: "bass".to_string(),
            available: vec!["low".to_string(), "mid".to_string(), "high".to_string()],
        };
        let msg = err.to_string();
        assert!(msg.contains("ReaEQ"));
        assert!(msg.contains("bass"));
        assert!(msg.contains("low, mid, high"));
    }
}
