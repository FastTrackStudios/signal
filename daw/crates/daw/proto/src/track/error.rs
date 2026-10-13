//! Track error types

use facet::Facet;

/// Errors that can occur during track operations
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum TrackError {
    /// Track not found
    NotFound(String),
    /// Invalid track reference
    InvalidReference(String),
    /// Operation not supported
    NotSupported(String),
    /// Internal DAW error
    Internal(String),
}

impl std::fmt::Display for TrackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Track not found: {msg}"),
            Self::InvalidReference(msg) => write!(f, "Invalid track reference: {msg}"),
            Self::NotSupported(msg) => write!(f, "Operation not supported: {msg}"),
            Self::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl std::error::Error for TrackError {}
