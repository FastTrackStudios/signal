//! Automation service errors

use facet::Facet;

/// Errors that can occur during automation operations
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum AutomationError {
    /// Envelope not found
    EnvelopeNotFound(String),
    /// Track not found
    TrackNotFound(String),
    /// FX not found (for FX parameter envelopes)
    FxNotFound(String),
    /// Parameter not found
    ParameterNotFound(String),
    /// Point not found
    PointNotFound(u32),
    /// Invalid value (must be 0.0-1.0)
    InvalidValue(String),
    /// Invalid time position
    InvalidTime(String),
    /// Invalid tension (must be -1.0 to 1.0)
    InvalidTension(String),
    /// Operation not supported
    NotSupported(String),
    /// Internal error
    Internal(String),
}

impl std::fmt::Display for AutomationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EnvelopeNotFound(msg) => write!(f, "Envelope not found: {}", msg),
            Self::TrackNotFound(msg) => write!(f, "Track not found: {}", msg),
            Self::FxNotFound(msg) => write!(f, "FX not found: {}", msg),
            Self::ParameterNotFound(msg) => write!(f, "Parameter not found: {}", msg),
            Self::PointNotFound(idx) => write!(f, "Point not found at index {}", idx),
            Self::InvalidValue(msg) => write!(f, "Invalid value: {}", msg),
            Self::InvalidTime(msg) => write!(f, "Invalid time: {}", msg),
            Self::InvalidTension(msg) => write!(f, "Invalid tension: {}", msg),
            Self::NotSupported(msg) => write!(f, "Operation not supported: {}", msg),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}
