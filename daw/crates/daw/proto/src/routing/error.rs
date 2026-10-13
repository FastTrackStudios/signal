//! Routing service errors

use facet::Facet;

/// Errors that can occur during routing operations
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum RoutingError {
    /// Route not found
    NotFound(String),
    /// Source track not found
    SourceTrackNotFound(String),
    /// Destination track not found
    DestTrackNotFound(String),
    /// Hardware output not found or unavailable
    HardwareOutputNotFound(u32),
    /// Cannot create route (e.g., would create feedback loop)
    CannotCreateRoute(String),
    /// Failed to create route
    CreateFailed(String),
    /// Failed to remove route
    RemoveFailed(String),
    /// Invalid volume value
    InvalidVolume(String),
    /// Invalid pan value
    InvalidPan(String),
    /// Invalid channel mapping
    InvalidChannelMapping(String),
    /// Operation not supported
    NotSupported(String),
    /// Internal error
    Internal(String),
}

impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Route not found: {}", msg),
            Self::SourceTrackNotFound(msg) => write!(f, "Source track not found: {}", msg),
            Self::DestTrackNotFound(msg) => write!(f, "Destination track not found: {}", msg),
            Self::HardwareOutputNotFound(idx) => {
                write!(f, "Hardware output {} not found or unavailable", idx)
            }
            Self::CannotCreateRoute(msg) => write!(f, "Cannot create route: {}", msg),
            Self::CreateFailed(msg) => write!(f, "Failed to create route: {}", msg),
            Self::RemoveFailed(msg) => write!(f, "Failed to remove route: {}", msg),
            Self::InvalidVolume(msg) => write!(f, "Invalid volume: {}", msg),
            Self::InvalidPan(msg) => write!(f, "Invalid pan: {}", msg),
            Self::InvalidChannelMapping(msg) => write!(f, "Invalid channel mapping: {}", msg),
            Self::NotSupported(msg) => write!(f, "Operation not supported: {}", msg),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}
