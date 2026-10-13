//! Error types for dynamic template operations

use facet::Facet;
use thiserror::Error;

/// Errors that can occur during dynamic template operations
#[repr(C)]
#[derive(Debug, Clone, Error, Facet)]
pub enum DynamicTemplateError {
    /// No items provided to organize
    #[error("No items provided to organize")]
    NoItems,

    /// Failed to parse item name
    #[error("Failed to parse item name: {0}")]
    ParseError(String),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for dynamic template operations
pub type Result<T> = std::result::Result<T, DynamicTemplateError>;
