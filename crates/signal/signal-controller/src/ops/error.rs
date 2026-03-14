//! Error types for controller operations.
//!
//! Defines [`OpsError`] which unifies not-found, variant-not-found, and
//! storage errors into a single error type for all ops handles.

use signal_proto::SignalServiceError;

/// Error returned by ops methods on controller handles.
#[derive(Debug, Clone, thiserror::Error)]
pub enum OpsError {
    /// The top-level entity was not found.
    #[error("{entity_type} not found: {id}")]
    NotFound {
        entity_type: &'static str,
        id: String,
    },
    /// The top-level entity exists but the requested nested variant was not found.
    #[error("{entity_type} variant {variant_id} not found in {parent_id}")]
    VariantNotFound {
        entity_type: &'static str,
        parent_id: String,
        variant_id: String,
    },
    /// A service/storage operation failed.
    #[error(transparent)]
    Storage(#[from] SignalServiceError),
}
