//! Typed error for session service trait boundaries.

// The `Facet` derive below expands to offset arithmetic internally (the
// lint fires at the derive macro's own generated code, per clippy's "this
// error originates in the derive macro `Facet`" note) — there is no
// hand-written code here to rewrite without forking the `facet` crate.
#![allow(clippy::arithmetic_side_effects)]

use facet::Facet;
use serde::{Deserialize, Serialize};

// ─── SessionServiceError ────────────────────────────────────────

/// Typed error for session service trait boundaries.
///
/// All methods return `Result<T, SessionServiceError>` using typed error variants
/// for structured diagnostics.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet, thiserror::Error)]
pub enum SessionServiceError {
    /// Entity not found by ID.
    #[error("{entity} not found: {id}")]
    NotFound { entity: String, id: String },

    /// A DAW operation failed.
    #[error("daw error: {0}")]
    DawError(String),

    /// Hydration (data enrichment) failed.
    #[error("hydration error: {0}")]
    HydrationError(String),

    /// Catch-all for unexpected failures.
    #[error("internal error: {0}")]
    Internal(String),
}

impl SessionServiceError {
    /// Convenience for creating a `NotFound` error.
    pub fn not_found(entity: impl Into<String>, id: &(impl ToString + ?Sized)) -> Self {
        Self::NotFound {
            entity: entity.into(),
            id: id.to_string(),
        }
    }
}

impl From<String> for SessionServiceError {
    fn from(s: String) -> Self {
        Self::Internal(s)
    }
}

impl From<eyre::Report> for SessionServiceError {
    fn from(e: eyre::Report) -> Self {
        Self::Internal(format!("{e:#}"))
    }
}
