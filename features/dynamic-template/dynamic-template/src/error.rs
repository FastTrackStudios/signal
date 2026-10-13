// region: --- Error

use std::fmt;

/// Custom error type for the dynamic-template module.
#[derive(Debug)]
pub enum Error {
    /// Custom error for flexible error creation
    Custom(String),

    // -- Externals
    /// Error from monarchy crate
    Monarchy(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Custom(msg) => write!(f, "{msg}"),
            Self::Monarchy(msg) => write!(f, "Monarchy error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    /// Create a custom error from any error type
    pub fn custom_from_err(err: impl std::error::Error) -> Self {
        Self::Custom(err.to_string())
    }

    /// Create a custom error from a string-like value
    pub fn custom(msg: impl Into<String>) -> Self {
        Self::Custom(msg.into())
    }
}

/// Result type alias for dynamic-template operations
pub type Result<T> = core::result::Result<T, Error>;

// endregion: --- Error
