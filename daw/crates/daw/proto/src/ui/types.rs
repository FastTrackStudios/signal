//! UI service data types.

use facet::Facet;

/// Result from a user input dialog.
#[derive(Clone, Debug, Facet)]
#[repr(C)]
pub struct UserInputResult {
    /// True if the user clicked OK, false if cancelled.
    pub ok: bool,
    /// The values entered by the user, in the same order as the prompts.
    pub values: Vec<String>,
}
