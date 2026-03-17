//! ID infrastructure — branded newtype macros, seed helpers, and core ID types.
//!
//! This module provides:
//! - [`typed_uuid_id!`] — macro for creating globally-unique UUID-backed IDs
//! - [`typed_string_id!`] — macro for creating human-readable string IDs
//! - [`IdFactory`] / [`RuntimeIdFactory`] — abstraction for UUID generation
//! - [`seed_id`] — deterministic UUID generation for seed data and tests
//! - Core ID types used by the preset/module model: [`PresetId`], [`SnapshotId`],
//!   [`ModulePresetId`], [`ModuleSnapshotId`]

use uuid::Uuid;

// ─── IdFactory ─────────────────────────────────────────────────

/// Shared contract for generating globally unique IDs at runtime.
pub trait IdFactory: Send + Sync {
    fn new_uuid(&self) -> Uuid;
}

/// Default runtime ID factory. Uses UUIDv7 for sortable, globally unique IDs.
#[derive(Debug, Default, Clone, Copy)]
pub struct RuntimeIdFactory;

impl IdFactory for RuntimeIdFactory {
    fn new_uuid(&self) -> Uuid {
        Uuid::now_v7()
    }
}

// ─── Seed helpers ──────────────────────────────────────────────

/// Namespace UUID for deterministic seed data IDs (v5).
pub const SEED_UUID_NS: Uuid = Uuid::from_bytes([
    0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x51, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
]);

/// Generate a deterministic UUID from a human-readable name.
/// Same name always produces same UUID — used for seed data and tests.
pub fn seed_id(name: &str) -> Uuid {
    Uuid::new_v5(&SEED_UUID_NS, name.as_bytes())
}

// ─── Core ID types ─────────────────────────────────────────────

crate::typed_uuid_id!(
    /// Branded type for preset identifiers.
    PresetId
);
crate::typed_uuid_id!(
    /// Branded type for snapshot identifiers.
    SnapshotId
);
crate::typed_uuid_id!(
    /// Branded type for module preset identifiers.
    ModulePresetId
);
crate::typed_uuid_id!(
    /// Branded type for module snapshot identifiers.
    ModuleSnapshotId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_id_is_deterministic() {
        let a = seed_id("test-name");
        let b = seed_id("test-name");
        assert_eq!(a, b);

        let c = seed_id("different-name");
        assert_ne!(a, c);
    }

    #[test]
    fn uuid_id_from_string_round_trip() {
        let id = PresetId::new();
        let s = id.to_string();
        let parsed = PresetId::from(s);
        assert_eq!(id, parsed);
    }
}
