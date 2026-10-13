//! FX error types

use facet::Facet;

/// Errors that can occur during FX operations
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum FxError {
    /// FX not found
    NotFound(String),
    /// Parameter not found
    ParameterNotFound(String),
    /// Invalid FX reference
    InvalidReference(String),
    /// FX chain context not valid
    InvalidContext(String),
    /// Plugin not available
    PluginNotAvailable(String),
    /// Operation not supported
    NotSupported(String),
    /// Internal DAW error
    Internal(String),
}

impl std::fmt::Display for FxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "FX not found: {msg}"),
            Self::ParameterNotFound(msg) => write!(f, "Parameter not found: {msg}"),
            Self::InvalidReference(msg) => write!(f, "Invalid FX reference: {msg}"),
            Self::InvalidContext(msg) => write!(f, "Invalid FX chain context: {msg}"),
            Self::PluginNotAvailable(msg) => write!(f, "Plugin not available: {msg}"),
            Self::NotSupported(msg) => write!(f, "Operation not supported: {msg}"),
            Self::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl std::error::Error for FxError {}
