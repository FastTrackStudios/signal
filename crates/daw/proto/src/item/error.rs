//! Item service errors

use facet::Facet;

/// Errors that can occur during item operations
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum ItemError {
    /// Item not found
    NotFound(String),
    /// Track not found
    TrackNotFound(String),
    /// Item is locked and cannot be modified
    ItemLocked(String),
    /// Invalid position (e.g., negative)
    InvalidPosition(String),
    /// Invalid length (e.g., zero or negative)
    InvalidLength(String),
    /// Failed to create item
    CreateFailed(String),
    /// Failed to delete item
    DeleteFailed(String),
    /// Failed to move item
    MoveFailed(String),
    /// Invalid fade settings
    InvalidFade(String),
    /// Source file not found
    SourceNotFound(String),
    /// Failed to load source
    SourceLoadFailed(String),
    /// Operation not supported
    NotSupported(String),
    /// Internal error
    Internal(String),
}

impl std::fmt::Display for ItemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Item not found: {}", msg),
            Self::TrackNotFound(msg) => write!(f, "Track not found: {}", msg),
            Self::ItemLocked(msg) => write!(f, "Item is locked: {}", msg),
            Self::InvalidPosition(msg) => write!(f, "Invalid position: {}", msg),
            Self::InvalidLength(msg) => write!(f, "Invalid length: {}", msg),
            Self::CreateFailed(msg) => write!(f, "Failed to create item: {}", msg),
            Self::DeleteFailed(msg) => write!(f, "Failed to delete item: {}", msg),
            Self::MoveFailed(msg) => write!(f, "Failed to move item: {}", msg),
            Self::InvalidFade(msg) => write!(f, "Invalid fade settings: {}", msg),
            Self::SourceNotFound(msg) => write!(f, "Source file not found: {}", msg),
            Self::SourceLoadFailed(msg) => write!(f, "Failed to load source: {}", msg),
            Self::NotSupported(msg) => write!(f, "Operation not supported: {}", msg),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

/// Errors that can occur during take operations
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum TakeError {
    /// Take not found
    NotFound(String),
    /// Item not found
    ItemNotFound(String),
    /// Failed to create take
    CreateFailed(String),
    /// Failed to delete take
    DeleteFailed(String),
    /// Cannot delete the only take
    CannotDeleteOnlyTake,
    /// Invalid playback rate
    InvalidPlayRate(String),
    /// Invalid pitch value
    InvalidPitch(String),
    /// Source file not found
    SourceNotFound(String),
    /// Failed to load source
    SourceLoadFailed(String),
    /// Operation not supported for this take type
    NotSupported(String),
    /// Internal error
    Internal(String),
}

impl std::fmt::Display for TakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Take not found: {}", msg),
            Self::ItemNotFound(msg) => write!(f, "Item not found: {}", msg),
            Self::CreateFailed(msg) => write!(f, "Failed to create take: {}", msg),
            Self::DeleteFailed(msg) => write!(f, "Failed to delete take: {}", msg),
            Self::CannotDeleteOnlyTake => write!(f, "Cannot delete the only take in an item"),
            Self::InvalidPlayRate(msg) => write!(f, "Invalid playback rate: {}", msg),
            Self::InvalidPitch(msg) => write!(f, "Invalid pitch value: {}", msg),
            Self::SourceNotFound(msg) => write!(f, "Source file not found: {}", msg),
            Self::SourceLoadFailed(msg) => write!(f, "Failed to load source: {}", msg),
            Self::NotSupported(msg) => write!(f, "Operation not supported: {}", msg),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}
