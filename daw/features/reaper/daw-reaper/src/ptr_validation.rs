//! Pointer Validation — WithReaperPtr trait
//!
//! Inspired by rea-rs's `WithReaperPtr` trait and `ReaperPointer` enum.
//! Provides a unified interface for validating raw REAPER pointers before use.
//!
//! REAPER uses raw C pointers for tracks, items, takes, and envelopes. These
//! pointers can become dangling if the user deletes an object between when we
//! resolve it and when we use it. REAPER's `ValidatePtr2` API checks if a
//! pointer is still recognized by a given project.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::ptr_validation::WithReaperPtr;
//!
//! let track = project.track_by_index(0)?;
//! track.require_valid(&project)?; // Returns DawError::InvalidObject on stale ptr
//! ```

use daw_proto::DawError;
use reaper_high::Reaper;

/// A validated REAPER pointer.
///
/// Wraps the different raw pointer types that REAPER uses, providing a uniform
/// validation interface. Modeled after rea-rs's `ReaperPointer` enum.
#[derive(Debug, Clone, Copy)]
pub enum ReaperPointer {
    Track(reaper_medium::MediaTrack),
    Item(reaper_medium::MediaItem),
    Take(reaper_medium::MediaItemTake),
}

/// Trait for REAPER objects that hold a raw pointer which can be validated.
///
/// Modeled after rea-rs's `WithReaperPtr` trait. Types implementing this can
/// be checked against REAPER's internal pointer registry via `ValidatePtr2`.
pub trait WithReaperPtr {
    /// Get the pointer wrapped in `ReaperPointer` for validation.
    fn as_reaper_pointer(&self) -> Option<ReaperPointer>;

    /// Validate that this pointer is still alive within the given project.
    ///
    /// Returns `Ok(())` if the pointer is valid, or `DawError::InvalidObject`
    /// if the object has been deleted.
    fn require_valid(&self, project: &reaper_high::Project) -> Result<(), DawError> {
        let Some(ptr) = self.as_reaper_pointer() else {
            return Err(DawError::InvalidObject(
                "Could not obtain raw pointer".to_string(),
            ));
        };
        if validate_ptr(project, ptr) {
            Ok(())
        } else {
            Err(DawError::InvalidObject(format!(
                "{} pointer is no longer valid",
                ptr.type_name()
            )))
        }
    }

    /// Validate and return the raw pointer if valid.
    fn require_valid_ptr(&self, project: &reaper_high::Project) -> Result<ReaperPointer, DawError> {
        self.require_valid(project)?;
        self.as_reaper_pointer()
            .ok_or_else(|| DawError::InvalidObject("Could not obtain raw pointer".to_string()))
    }
}

impl ReaperPointer {
    /// Human-readable type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Track(_) => "Track",
            Self::Item(_) => "Item",
            Self::Take(_) => "Take",
        }
    }
}

// =============================================================================
// WithReaperPtr implementations
// =============================================================================

impl WithReaperPtr for reaper_high::Track {
    fn as_reaper_pointer(&self) -> Option<ReaperPointer> {
        self.raw().ok().map(ReaperPointer::Track)
    }
}

// reaper_medium types don't have a fallible accessor — they ARE the raw pointer.
// We wrap them in newtype wrappers for the trait impl.

/// Wrapper for validating a `MediaItem` pointer.
pub struct ValidatedItem(pub reaper_medium::MediaItem);

impl WithReaperPtr for ValidatedItem {
    fn as_reaper_pointer(&self) -> Option<ReaperPointer> {
        Some(ReaperPointer::Item(self.0))
    }
}

/// Wrapper for validating a `MediaItemTake` pointer.
pub struct ValidatedTake(pub reaper_medium::MediaItemTake);

impl WithReaperPtr for ValidatedTake {
    fn as_reaper_pointer(&self) -> Option<ReaperPointer> {
        Some(ReaperPointer::Take(self.0))
    }
}

// =============================================================================
// Core validation function
// =============================================================================

/// Validate a REAPER pointer against a project's internal registry.
///
/// Calls REAPER's `ValidatePtr2` which checks whether the pointer is still
/// recognized as a live object within the given project context.
pub fn validate_ptr(project: &reaper_high::Project, ptr: ReaperPointer) -> bool {
    let medium = Reaper::get().medium_reaper();
    let ctx = reaper_medium::ProjectContext::Proj(project.raw());

    match ptr {
        ReaperPointer::Track(raw) => medium.validate_ptr_2(ctx, raw),
        ReaperPointer::Item(raw) => medium.validate_ptr_2(ctx, raw),
        ReaperPointer::Take(raw) => medium.validate_ptr_2(ctx, raw),
    }
}

/// Validate a REAPER pointer using a medium-level project context.
///
/// Use this when you already have a `reaper_medium::ProjectContext` (e.g., in
/// item.rs where we don't always have a `reaper_high::Project`).
pub fn validate_ptr_with_context(ctx: reaper_medium::ProjectContext, ptr: ReaperPointer) -> bool {
    let medium = Reaper::get().medium_reaper();
    match ptr {
        ReaperPointer::Track(raw) => medium.validate_ptr_2(ctx, raw),
        ReaperPointer::Item(raw) => medium.validate_ptr_2(ctx, raw),
        ReaperPointer::Take(raw) => medium.validate_ptr_2(ctx, raw),
    }
}
