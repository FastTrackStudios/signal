//! Marker error types

use facet::Facet;
use std::fmt;

/// Errors that can occur during marker operations
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Facet)]
pub enum MarkerError {
    /// Marker with the given ID was not found
    NotFound(u32),
    /// Invalid marker position (e.g., negative time)
    InvalidPosition(f64),
    /// Invalid marker name (e.g., empty string)
    InvalidName(String),
    /// Operation failed with a message
    OperationFailed(String),
}

impl fmt::Display for MarkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "Marker not found: {}", id),
            Self::InvalidPosition(pos) => write!(f, "Invalid marker position: {}", pos),
            Self::InvalidName(name) => write!(f, "Invalid marker name: {}", name),
            Self::OperationFailed(msg) => write!(f, "Operation failed: {}", msg),
        }
    }
}

impl std::error::Error for MarkerError {}
