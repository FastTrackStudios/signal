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
            Self::FxParametersFailed(reason) => {
                write!(f, "Failed to get FX parameters: {reason}")
            }
            Self::ParameterNotFound {
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
            Self::NoBindings => write!(f, "No bindings in macro bank"),
            Self::NoMacroBank => write!(f, "No macro bank on block"),
            Self::InvalidKnobRef(knob_id) => {
                write!(f, "Invalid knob reference: '{knob_id}'")
            }
            Self::ResolutionFailed(reason) => {
                write!(f, "Macro resolution failed: {reason}")
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
///
/// # Errors
///
/// Returns an error if the macro bank is invalid or incomplete.
pub fn validate_macro_bank(bank: &signal_macromod::MacroBank) -> MacroResult<()> {
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
                return Err(MacroError::InvalidKnobRef(format!(
                    "child knob of '{}' has empty ID",
                    knob.id
                )));
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
                    return Err(MacroError::InvalidKnobRef(format!(
                        "child knob of '{}' in group has empty ID",
                        knob.id
                    )));
                }
            }
        }
    }

    Ok(())
}
