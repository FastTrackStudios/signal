use std::fmt;

/// Error returned by ops methods on controller handles.
#[derive(Debug, Clone)]
pub enum OpsError {
    /// The top-level entity was not found.
    NotFound {
        entity_type: &'static str,
        id: String,
    },
    /// The top-level entity exists but the requested nested variant was not found.
    VariantNotFound {
        entity_type: &'static str,
        parent_id: String,
        variant_id: String,
    },
    /// A storage/persistence operation failed.
    Storage(String),
}

impl fmt::Display for OpsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpsError::NotFound { entity_type, id } => {
                write!(f, "{entity_type} not found: {id}")
            }
            OpsError::VariantNotFound {
                entity_type,
                parent_id,
                variant_id,
            } => {
                write!(
                    f,
                    "{entity_type} variant {variant_id} not found in {parent_id}"
                )
            }
            OpsError::Storage(msg) => {
                write!(f, "storage error: {msg}")
            }
        }
    }
}

impl std::error::Error for OpsError {}
