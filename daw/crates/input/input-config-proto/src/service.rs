//! `InputConfigService` — RPC surface for reading and editing keybind /
//! workflow configuration.
//!
//! This is the seam that makes every remote surface possible: the web
//! keybind editor, the site's "your current configuration" tutorial
//! rendering, and hub import/export all speak this trait. Hosts:
//!
//! - the REAPER extension (edits the live rig's config, hot-reloaded)
//! - the standalone keybind editor / fasttrackstudio app (edits a
//!   config dir on disk)
//! - the hub server (edits a profile stored in the database)
//!
//! Identity convention matches the on-disk layout: profiles are
//! directories of section files; overlays and workflows are single
//! files keyed by filename stem.

use facet::Facet;
use vox::service;

use crate::types::{OverlayConfig, ProfileConfig, SectionConfig, WorkflowConfig};

/// Summary of a profile (directory of section files).
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ProfileInfo {
    /// Directory-name id, e.g. `fasttrackstudio`.
    pub id: String,
    /// Display name from profile.styx.
    pub name: String,
    /// Description from profile.styx.
    pub description: String,
    /// Semver version from profile.styx.
    pub version: String,
}

/// Summary of an overlay file.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct OverlayInfo {
    /// Filename-stem id.
    pub id: String,
    /// Internal name from the file.
    pub name: String,
    pub description: String,
    pub priority: i32,
}

/// Summary of a workflow file.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct WorkflowInfo {
    /// Filename-stem id, e.g. `tempo-mapping`.
    pub id: String,
    /// Display name (file override or kebab→Title of the id).
    pub name: String,
    pub description: String,
}

/// Result of a write: whether the change was applied and hot-reloaded.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct WriteResult {
    pub ok: bool,
    /// Human-readable failure reason when `ok` is false.
    pub message: Option<String>,
}

impl WriteResult {
    pub fn ok() -> Self {
        Self {
            ok: true,
            message: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: Some(message.into()),
        }
    }
}

/// Read/edit keybind + workflow configuration over RPC.
///
/// Section/overlay/workflow payloads are the same facet structs the styx
/// files parse into — a client edit is "read config, mutate, write back".
/// Hosts snapshot before every write (file-backed undo) and hot-reload
/// where a live input processor exists.
#[service]
pub trait InputConfigService {
    // ── Profiles ────────────────────────────────────────────────────

    /// All profiles known to this host.
    async fn list_profiles(&self) -> Vec<ProfileInfo>;

    /// A profile's metadata + ordered section list.
    async fn read_profile(&self, profile: String) -> Option<ProfileConfig>;

    /// Create an empty profile scaffold. Fails if the id exists.
    async fn create_profile(&self, id: String, name: String) -> WriteResult;

    /// Read one section file of a profile (e.g. `transport`).
    async fn read_section(&self, profile: String, section: String) -> Option<SectionConfig>;

    /// Replace one section file of a profile.
    async fn write_section(
        &self,
        profile: String,
        section: String,
        config: SectionConfig,
    ) -> WriteResult;

    // ── Overlays ────────────────────────────────────────────────────

    async fn list_overlays(&self) -> Vec<OverlayInfo>;

    async fn read_overlay(&self, id: String) -> Option<OverlayConfig>;

    async fn write_overlay(&self, id: String, config: OverlayConfig) -> WriteResult;

    // ── Workflows ───────────────────────────────────────────────────

    async fn list_workflows(&self) -> Vec<WorkflowInfo>;

    async fn read_workflow(&self, id: String) -> Option<WorkflowConfig>;

    async fn write_workflow(&self, id: String, config: WorkflowConfig) -> WriteResult;
}
