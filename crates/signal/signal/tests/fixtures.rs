//! Shared test fixtures for signal integration tests.
//!
//! Provides:
//! - [`controller()`] — bootstraps an in-memory controller
//! - Seed data lookup helpers for pre-existing megarigs
//! - [`save_built_rig()`] — persists a [`BuiltRig`] via the controller
//!
//! **Convention**: Tests should NOT use `seed_id()` directly. For new test
//! data, use [`RigBuilder`] and access IDs via `built.scene_id("Clean")` etc.
//! The seed ID helpers below are ONLY for referencing pre-existing seeded
//! data (the megarigs that ship with the app).
//!
//! Usage: `mod fixtures;` in each test file, then `use fixtures::*;`

#![allow(dead_code)]

use signal::builder::BuiltRig;
use signal::rig::{RigId, RigSceneId};
use signal::seed_id;
use signal::Signal;

// ─── Controller bootstrap ───────────────────────────────────────

pub async fn controller() -> Signal {
    signal::bootstrap_in_memory_controller_async()
        .await
        .expect("failed to bootstrap in-memory controller")
}

// ─── Guitar MegaRig seed IDs ────────────────────────────────────

pub fn guitar_megarig_id() -> RigId {
    seed_id("guitar-megarig").into()
}

pub fn guitar_megarig_default_scene() -> RigSceneId {
    seed_id("guitar-megarig-default").into()
}

pub fn guitar_megarig_lead_scene() -> RigSceneId {
    seed_id("guitar-megarig-lead").into()
}

// ─── Keys MegaRig seed IDs ─────────────────────────────────────

pub fn keys_megarig_id() -> RigId {
    seed_id("keys-megarig").into()
}

pub fn keys_megarig_default_scene() -> RigSceneId {
    seed_id("keys-megarig-default").into()
}

pub fn keys_megarig_wide_scene() -> RigSceneId {
    seed_id("keys-megarig-wide").into()
}

pub fn keys_megarig_focus_scene() -> RigSceneId {
    seed_id("keys-megarig-focus").into()
}

pub fn keys_megarig_air_scene() -> RigSceneId {
    seed_id("keys-megarig-air").into()
}

// ─── BuiltRig save helper ──────────────────────────────────────

/// Save all entities from a [`BuiltRig`] to the controller's storage.
///
/// **Prefer `signal.save_built_rig(&built)` instead** — same logic, lives on the controller.
#[deprecated(note = "use signal.save_built_rig(&built) instead")]
pub async fn save_built_rig(signal: &Signal, built: &BuiltRig) {
    signal.save_built_rig(built).await;
}
