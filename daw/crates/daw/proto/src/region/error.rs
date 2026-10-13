//! Region error types

use facet::Facet;
use std::fmt;

/// Errors that can occur during region operations
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Facet)]
pub enum RegionError {
    /// Region with the given ID was not found
    NotFound(u32),
    /// Invalid region range (e.g., end before start)
    InvalidRange { start: f64, end: f64 },
    /// Invalid region name (e.g., empty string)
    InvalidName(String),
    /// Operation failed with a message
    OperationFailed(String),
}

impl fmt::Display for RegionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "Region not found: {}", id),
            Self::InvalidRange { start, end } => {
                write!(f, "Invalid region range: {} - {}", start, end)
            }
            Self::InvalidName(name) => write!(f, "Invalid region name: {}", name),
            Self::OperationFailed(msg) => write!(f, "Operation failed: {}", msg),
        }
    }
}

impl std::error::Error for RegionError {}
