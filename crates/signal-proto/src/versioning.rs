//! Entity versioning — typestate pattern for Draft vs Committed entities.
//!
//! Also defines sync metadata for device/cloud synchronization.

use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

// ---------------------------------------------------------------------------
// Typestate markers
// ---------------------------------------------------------------------------

/// Marker: entity is an uncommitted draft (local edits not yet saved).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Draft;

/// Marker: entity has been saved/committed to storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Committed;

// ---------------------------------------------------------------------------
// Versioned wrapper
// ---------------------------------------------------------------------------

/// A versioned entity wrapper with typestate tracking.
///
/// `State` is either [`Draft`] or [`Committed`].
///
/// ```ignore
/// let draft = Versioned::draft(my_preset);
/// let committed = draft.commit(1);
/// assert_eq!(committed.version(), 1);
/// let next_draft = committed.edit();
/// ```
#[derive(Debug, Clone)]
pub struct Versioned<T, State = Draft> {
    inner: T,
    version: u32,
    _state: PhantomData<State>,
}

impl<T> Versioned<T, Draft> {
    /// Create a new draft (version 0 = never committed).
    pub fn draft(inner: T) -> Self {
        Self {
            inner,
            version: 0,
            _state: PhantomData,
        }
    }

    /// Commit the draft, bumping to the given version number.
    pub fn commit(self, version: u32) -> Versioned<T, Committed> {
        Versioned {
            inner: self.inner,
            version,
            _state: PhantomData,
        }
    }

    /// Access the inner entity.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Mutably access the inner entity (only available on drafts).
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Consume and return the inner entity.
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Whether this is a fresh draft (never committed).
    pub fn is_new(&self) -> bool {
        self.version == 0
    }
}

impl<T> Versioned<T, Committed> {
    /// Reconstruct a committed entity from storage.
    pub fn from_storage(inner: T, version: u32) -> Self {
        Self {
            inner,
            version,
            _state: PhantomData,
        }
    }

    /// The committed version number.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Access the inner entity (read-only on committed).
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Consume and return the inner entity.
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Start editing — creates a new draft from the committed state.
    pub fn edit(self) -> Versioned<T, Draft> {
        Versioned {
            inner: self.inner,
            version: self.version,
            _state: PhantomData,
        }
    }
}

impl<T: PartialEq, S> PartialEq for Versioned<T, S> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner && self.version == other.version
    }
}

// ---------------------------------------------------------------------------
// Sync metadata
// ---------------------------------------------------------------------------

/// Metadata for device/cloud synchronization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncMetadata {
    /// Device ID that last modified this entity.
    pub device_id: String,

    /// Timestamp of last local modification (Unix epoch seconds).
    pub local_modified_at: u64,

    /// Timestamp of last successful sync to remote (None = never synced).
    pub remote_synced_at: Option<u64>,

    /// Remote version hash for conflict detection.
    pub remote_hash: Option<String>,

    /// Whether this entity has unsynced local changes.
    pub dirty: bool,
}

impl SyncMetadata {
    /// Create fresh sync metadata for a new entity.
    pub fn new(device_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            local_modified_at: 0,
            remote_synced_at: None,
            remote_hash: None,
            dirty: true,
        }
    }

    /// Mark as locally modified.
    pub fn mark_dirty(&mut self, timestamp: u64) {
        self.local_modified_at = timestamp;
        self.dirty = true;
    }

    /// Mark as synced with remote.
    pub fn mark_synced(&mut self, timestamp: u64, remote_hash: String) {
        self.remote_synced_at = Some(timestamp);
        self.remote_hash = Some(remote_hash);
        self.dirty = false;
    }

    /// Whether this entity needs to be pushed to remote.
    pub fn needs_sync(&self) -> bool {
        self.dirty
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_commit_edit_cycle() {
        let draft = Versioned::draft("hello".to_string());
        assert!(draft.is_new());
        assert_eq!(draft.inner(), "hello");

        let committed = draft.commit(1);
        assert_eq!(committed.version(), 1);
        assert_eq!(committed.inner(), "hello");

        let mut draft2 = committed.edit();
        assert!(!draft2.is_new());
        *draft2.inner_mut() = "world".to_string();

        let committed2 = draft2.commit(2);
        assert_eq!(committed2.version(), 2);
        assert_eq!(committed2.inner(), "world");
    }

    #[test]
    fn sync_metadata_lifecycle() {
        let mut sync = SyncMetadata::new("device-1");
        assert!(sync.needs_sync());
        assert!(sync.remote_synced_at.is_none());

        sync.mark_dirty(1000);
        assert_eq!(sync.local_modified_at, 1000);
        assert!(sync.needs_sync());

        sync.mark_synced(1001, "abc123".to_string());
        assert!(!sync.needs_sync());
        assert_eq!(sync.remote_synced_at, Some(1001));
        assert_eq!(sync.remote_hash.as_deref(), Some("abc123"));
    }
}
