//! Unified DAW error type
//!
//! Inspired by rea-rs's `ReaperError`, this provides a single error enum that
//! covers cross-cutting failure modes across all DAW services. Domain-specific
//! error types (`TrackError`, `FxError`, etc.) still exist for consumers who
//! need finer-grained matching, but `DawError` is the primary error type used
//! throughout the service layer.

use facet::Facet;

/// Unified error type for DAW operations.
///
/// Modeled after rea-rs's `ReaperError`, this covers the common failure modes
/// that arise when interacting with a DAW:
///
/// - **InvalidObject**: A pointer/handle to a DAW object (track, item, FX, etc.)
///   is no longer valid. This happens when the user deletes something between
///   the time we resolve it and the time we use it.
///
/// - **NotFound**: A lookup by GUID, name, or index found nothing.
///
/// - **OutOfRange**: An index was beyond the valid range.
///
/// - **OperationFailed**: A DAW API call returned an error or failure code.
///
/// - **NotSupported**: The operation isn't available in this DAW backend.
///
/// - **MainThreadUnavailable**: The main thread bridge (`TaskSupport`) is not
///   initialized — typically means the extension hasn't finished starting up.
///
/// - **Internal**: Catch-all for unexpected failures.
///
/// - **AlreadyExists**: A create-with-identity call named an id (a GUID)
///   the project already holds. Creation never silently duplicates one.
///
/// # Usage
///
/// ```rust
/// use daw_proto::DawError;
///
/// fn example() -> Result<(), DawError> {
///     Err(DawError::not_found("Track", "abc-123"))
/// }
/// ```
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum DawError {
    /// A DAW object pointer/handle is no longer valid (deleted, moved, etc.)
    InvalidObject(String),

    /// A lookup by GUID, name, or index found nothing
    NotFound(String),

    /// An index was out of the valid range
    OutOfRange {
        index: u32,
        max: u32,
        context: String,
    },

    /// A DAW API call returned an error or failure code
    OperationFailed(String),

    /// The operation isn't available in this backend
    NotSupported(String),

    /// The main thread bridge is not initialized
    MainThreadUnavailable,

    /// Catch-all for unexpected failures
    Internal(String),

    /// A create call named an id (a GUID) that the project already holds.
    ///
    /// Last in the enum so the wire discriminants of the variants before
    /// it do not move.
    AlreadyExists(String),
}

impl DawError {
    // =========================================================================
    // Constructors — named constructors for common patterns
    // =========================================================================

    /// Object not found by some identifier
    pub fn not_found(object_type: &str, id: &str) -> Self {
        Self::NotFound(format!("{} not found: {}", object_type, id))
    }

    /// Object pointer is no longer valid
    pub fn invalid_object(object_type: &str, id: &str) -> Self {
        Self::InvalidObject(format!("{} pointer no longer valid: {}", object_type, id))
    }

    /// Index out of range
    pub fn out_of_range(index: u32, max: u32, context: impl Into<String>) -> Self {
        Self::OutOfRange {
            index,
            max,
            context: context.into(),
        }
    }

    /// DAW API operation failed
    pub fn operation_failed(msg: impl Into<String>) -> Self {
        Self::OperationFailed(msg.into())
    }

    /// Backend doesn't support this operation
    pub fn not_supported(msg: impl Into<String>) -> Self {
        Self::NotSupported(msg.into())
    }

    /// Internal/unexpected error
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    /// An object with this id already exists
    pub fn already_exists(object_type: &str, id: &str) -> Self {
        Self::AlreadyExists(format!("{} already exists: {}", object_type, id))
    }
}

impl std::fmt::Display for DawError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidObject(msg) => write!(f, "Invalid object: {}", msg),
            Self::NotFound(msg) => write!(f, "Not found: {}", msg),
            Self::OutOfRange {
                index,
                max,
                context,
            } => write!(
                f,
                "Index {} out of range (max {}) in {}",
                index, max, context
            ),
            Self::OperationFailed(msg) => write!(f, "Operation failed: {}", msg),
            Self::NotSupported(msg) => write!(f, "Not supported: {}", msg),
            Self::MainThreadUnavailable => write!(f, "Main thread bridge not available"),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
            Self::AlreadyExists(msg) => write!(f, "Already exists: {}", msg),
        }
    }
}

impl std::error::Error for DawError {}

// =========================================================================
// Conversions from domain-specific errors
// =========================================================================

impl From<String> for DawError {
    fn from(s: String) -> Self {
        Self::Internal(s)
    }
}

impl From<&str> for DawError {
    fn from(s: &str) -> Self {
        Self::Internal(s.to_string())
    }
}

impl From<crate::TrackError> for DawError {
    fn from(e: crate::TrackError) -> Self {
        match e {
            crate::TrackError::NotFound(msg) => Self::NotFound(msg),
            crate::TrackError::InvalidReference(msg) => Self::InvalidObject(msg),
            crate::TrackError::NotSupported(msg) => Self::NotSupported(msg),
            crate::TrackError::Internal(msg) => Self::Internal(msg),
        }
    }
}

impl From<crate::FxError> for DawError {
    fn from(e: crate::FxError) -> Self {
        match e {
            crate::FxError::NotFound(msg) => Self::NotFound(msg),
            crate::FxError::ParameterNotFound(msg) => Self::NotFound(msg),
            crate::FxError::InvalidReference(msg) => Self::InvalidObject(msg),
            crate::FxError::InvalidContext(msg) => Self::InvalidObject(msg),
            crate::FxError::PluginNotAvailable(msg) => Self::NotFound(msg),
            crate::FxError::NotSupported(msg) => Self::NotSupported(msg),
            crate::FxError::Internal(msg) => Self::Internal(msg),
        }
    }
}

impl From<crate::RoutingError> for DawError {
    fn from(e: crate::RoutingError) -> Self {
        match e {
            crate::RoutingError::NotFound(msg) => Self::NotFound(msg),
            crate::RoutingError::SourceTrackNotFound(msg) => Self::NotFound(msg),
            crate::RoutingError::DestTrackNotFound(msg) => Self::NotFound(msg),
            crate::RoutingError::HardwareOutputNotFound(idx) => {
                Self::NotFound(format!("Hardware output {} not found", idx))
            }
            crate::RoutingError::CannotCreateRoute(msg) => Self::OperationFailed(msg),
            crate::RoutingError::CreateFailed(msg) => Self::OperationFailed(msg),
            crate::RoutingError::RemoveFailed(msg) => Self::OperationFailed(msg),
            crate::RoutingError::InvalidVolume(msg) => Self::OperationFailed(msg),
            crate::RoutingError::InvalidPan(msg) => Self::OperationFailed(msg),
            crate::RoutingError::InvalidChannelMapping(msg) => Self::OperationFailed(msg),
            crate::RoutingError::NotSupported(msg) => Self::NotSupported(msg),
            crate::RoutingError::Internal(msg) => Self::Internal(msg),
        }
    }
}

impl From<crate::TransportError> for DawError {
    fn from(e: crate::TransportError) -> Self {
        Self::OperationFailed(e.to_string())
    }
}

/// Type alias for Results using DawError
pub type DawResult<T> = Result<T, DawError>;
