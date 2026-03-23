//! Service trait definitions — async interfaces for the signal domain.
//!
//! Each trait defines the operations available for a specific domain entity.
//! Implementations live in [`signal_live`] (runtime) and are wrapped by
//! [`signal_controller`] for the user-facing API.
//!
//! All methods return `Result<T, SignalServiceError>` using typed error variants
//! for structured diagnostics.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::engine;
use crate::ids::{ModulePresetId, ModuleSnapshotId, PresetId, SnapshotId};
use crate::layer;
use crate::model::{Block, ModulePreset, ModuleSnapshot, Preset, Snapshot};
use crate::profile;
use crate::rack;
use crate::resolve;
use crate::scene_template;
use crate::setlist;
use crate::song;
use crate::tagging;

// ─── SignalServiceError ────────────────────────────────────────

/// Typed error for service trait boundaries.
///
/// Replaces opaque `String` errors with structured variants that can be
/// pattern-matched for error handling, logging, and user-facing messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet, thiserror::Error)]
#[repr(C)]
pub enum SignalServiceError {
    /// Entity not found by ID.
    #[error("{entity} not found: {id}")]
    NotFound { entity: String, id: String },

    /// Underlying storage/persistence failure.
    #[error("storage error: {0}")]
    StorageError(String),

    /// Domain validation constraint violated.
    #[error("validation error: {0}")]
    ValidationError(String),

    /// Scene/graph resolution failure.
    #[error(transparent)]
    ResolveError(#[from] resolve::ResolveError),

    /// Catch-all for unexpected internal errors.
    #[error("internal error: {0}")]
    Internal(String),
}

impl SignalServiceError {
    /// Convenience for creating a NotFound error.
    pub fn not_found(entity: impl Into<String>, id: impl ToString) -> Self {
        Self::NotFound {
            entity: entity.into(),
            id: id.to_string(),
        }
    }
}

impl From<String> for SignalServiceError {
    fn from(s: String) -> Self {
        Self::Internal(s)
    }
}

// ─── Service traits ────────────────────────────────────────────

#[vox::service]
pub trait BlockService {
    async fn get_block(&self, block_type: crate::BlockType) -> Result<Block, SignalServiceError>;
    async fn set_block(
        &self,
        block_type: crate::BlockType,
        block: Block,
    ) -> Result<Block, SignalServiceError>;
    async fn list_block_presets(
        &self,
        block_type: crate::BlockType,
    ) -> Result<Vec<Preset>, SignalServiceError>;
    async fn load_block_preset(
        &self,
        block_type: crate::BlockType,
        preset_id: PresetId,
    ) -> Result<Option<Snapshot>, SignalServiceError>;
    async fn load_block_preset_snapshot(
        &self,
        block_type: crate::BlockType,
        preset_id: PresetId,
        snapshot_id: SnapshotId,
    ) -> Result<Option<Snapshot>, SignalServiceError>;
    async fn save_block_preset(&self, preset: Preset) -> Result<(), SignalServiceError>;
    async fn delete_block_preset(
        &self,
        block_type: crate::BlockType,
        preset_id: PresetId,
    ) -> Result<(), SignalServiceError>;
    async fn list_module_presets(&self) -> Result<Vec<ModulePreset>, SignalServiceError>;
    async fn load_module_preset(
        &self,
        preset_id: ModulePresetId,
    ) -> Result<Option<ModuleSnapshot>, SignalServiceError>;
    async fn load_module_preset_snapshot(
        &self,
        preset_id: ModulePresetId,
        snapshot_id: ModuleSnapshotId,
    ) -> Result<Option<ModuleSnapshot>, SignalServiceError>;
    async fn save_module_collection(&self, preset: ModulePreset) -> Result<(), SignalServiceError>;
    async fn delete_module_collection(&self, id: ModulePresetId) -> Result<(), SignalServiceError>;
}

#[vox::service]
pub trait LayerService {
    async fn list_layers(&self) -> Result<Vec<layer::Layer>, SignalServiceError>;
    async fn load_layer(
        &self,
        id: layer::LayerId,
    ) -> Result<Option<layer::Layer>, SignalServiceError>;
    async fn save_layer(&self, layer: layer::Layer) -> Result<(), SignalServiceError>;
    async fn delete_layer(&self, id: layer::LayerId) -> Result<(), SignalServiceError>;
    async fn load_layer_variant(
        &self,
        layer_id: layer::LayerId,
        variant_id: layer::LayerSnapshotId,
    ) -> Result<Option<layer::LayerSnapshot>, SignalServiceError>;
}

#[vox::service]
pub trait EngineService {
    async fn list_engines(&self) -> Result<Vec<engine::Engine>, SignalServiceError>;
    async fn load_engine(
        &self,
        id: engine::EngineId,
    ) -> Result<Option<engine::Engine>, SignalServiceError>;
    async fn save_engine(&self, engine: engine::Engine) -> Result<(), SignalServiceError>;
    async fn delete_engine(&self, id: engine::EngineId) -> Result<(), SignalServiceError>;
    async fn load_engine_variant(
        &self,
        engine_id: engine::EngineId,
        variant_id: engine::EngineSceneId,
    ) -> Result<Option<engine::EngineScene>, SignalServiceError>;
}

#[vox::service]
pub trait RigService {
    async fn list_rigs(&self) -> Result<Vec<crate::rig::Rig>, SignalServiceError>;
    async fn load_rig(
        &self,
        id: crate::rig::RigId,
    ) -> Result<Option<crate::rig::Rig>, SignalServiceError>;
    async fn save_rig(&self, rig: crate::rig::Rig) -> Result<(), SignalServiceError>;
    async fn delete_rig(&self, id: crate::rig::RigId) -> Result<(), SignalServiceError>;
    async fn load_rig_variant(
        &self,
        rig_id: crate::rig::RigId,
        variant_id: crate::rig::RigSceneId,
    ) -> Result<Option<crate::rig::RigScene>, SignalServiceError>;
}

#[vox::service]
pub trait ProfileService {
    async fn list_profiles(&self) -> Result<Vec<profile::Profile>, SignalServiceError>;
    async fn load_profile(
        &self,
        id: profile::ProfileId,
    ) -> Result<Option<profile::Profile>, SignalServiceError>;
    async fn save_profile(&self, profile: profile::Profile) -> Result<(), SignalServiceError>;
    async fn delete_profile(&self, id: profile::ProfileId) -> Result<(), SignalServiceError>;
    async fn load_profile_variant(
        &self,
        profile_id: profile::ProfileId,
        variant_id: profile::PatchId,
    ) -> Result<Option<profile::Patch>, SignalServiceError>;
}

#[vox::service]
pub trait SongService {
    async fn list_songs(&self) -> Result<Vec<song::Song>, SignalServiceError>;
    async fn load_song(&self, id: song::SongId) -> Result<Option<song::Song>, SignalServiceError>;
    async fn save_song(&self, song: song::Song) -> Result<(), SignalServiceError>;
    async fn delete_song(&self, id: song::SongId) -> Result<(), SignalServiceError>;
    async fn load_song_variant(
        &self,
        song_id: song::SongId,
        variant_id: song::SectionId,
    ) -> Result<Option<song::Section>, SignalServiceError>;
}

#[vox::service]
pub trait SetlistService {
    async fn list_setlists(&self) -> Result<Vec<setlist::Setlist>, SignalServiceError>;
    async fn load_setlist(
        &self,
        id: setlist::SetlistId,
    ) -> Result<Option<setlist::Setlist>, SignalServiceError>;
    async fn save_setlist(&self, setlist: setlist::Setlist) -> Result<(), SignalServiceError>;
    async fn delete_setlist(&self, id: setlist::SetlistId) -> Result<(), SignalServiceError>;
    async fn load_setlist_entry(
        &self,
        setlist_id: setlist::SetlistId,
        entry_id: setlist::SetlistEntryId,
    ) -> Result<Option<setlist::SetlistEntry>, SignalServiceError>;
}

#[vox::service]
pub trait BrowserService {
    async fn browser_index(&self) -> Result<tagging::BrowserIndex, SignalServiceError>;
    async fn browse(
        &self,
        query: tagging::BrowserQuery,
    ) -> Result<Vec<tagging::BrowserHit>, SignalServiceError>;
}

#[vox::service]
pub trait ResolveService {
    async fn resolve_target(
        &self,
        target: resolve::ResolveTarget,
    ) -> Result<resolve::ResolvedGraph, resolve::ResolveError>;
}

#[vox::service]
pub trait SceneTemplateService {
    async fn list_scene_templates(
        &self,
    ) -> Result<Vec<scene_template::SceneTemplate>, SignalServiceError>;
    async fn load_scene_template(
        &self,
        id: scene_template::SceneTemplateId,
    ) -> Result<Option<scene_template::SceneTemplate>, SignalServiceError>;
    async fn save_scene_template(
        &self,
        template: scene_template::SceneTemplate,
    ) -> Result<(), SignalServiceError>;
    async fn delete_scene_template(
        &self,
        id: scene_template::SceneTemplateId,
    ) -> Result<(), SignalServiceError>;
    async fn reorder_scene_templates(
        &self,
        ordered_ids: Vec<scene_template::SceneTemplateId>,
    ) -> Result<(), SignalServiceError>;
}

#[vox::service]
pub trait RackService {
    async fn list_racks(&self) -> Result<Vec<rack::Rack>, SignalServiceError>;
    async fn load_rack(&self, id: rack::RackId) -> Result<Option<rack::Rack>, SignalServiceError>;
    async fn save_rack(&self, rack: rack::Rack) -> Result<(), SignalServiceError>;
    async fn delete_rack(&self, id: rack::RackId) -> Result<(), SignalServiceError>;
}

// ─── Extension ↔ Controller real-time parameter protocol ─────────

/// Batch parameter write request for cross-track modulation.
///
/// The controller (or fts-signal-controller CLAP plugin) sends these to
/// a track's signal-extension via SHM. The extension applies them from
/// its own audio thread using `TrackFX_SetParamNormalized` (same-track safe).
#[derive(Debug, Clone, Serialize, Deserialize, Facet)]
#[repr(C)]
pub struct ParamWriteRequest {
    pub fx_index: u32,
    pub param_index: u32,
    pub value: f64,
}

/// Snapshot of an extension's current FX chain state.
#[derive(Debug, Clone, Serialize, Deserialize, Facet)]
#[repr(C)]
pub struct ExtensionFxState {
    /// Track GUID.
    pub track_guid: String,
    /// Number of FX on the track.
    pub fx_count: u32,
    /// Whether the extension is ready to accept parameter writes.
    pub ready: bool,
}

/// Service implemented by each signal-extension instance (one per track).
///
/// The fts-signal-controller CLAP plugin and signal-desktop app use this
/// to control FX parameters on remote tracks via SHM. Each extension only
/// touches its own track's parameters — cross-track modulation is routed
/// through this service rather than calling REAPER APIs across tracks.
///
/// # Real-time path
///
/// `set_param` and `set_params_batch` are designed for the real-time path.
/// The extension picks these up in its `process()` callback and applies
/// them via `TrackFX_SetParamNormalized` (same-track, audio-thread safe).
///
/// # Setup path
///
/// `apply_graph` and `setup_rig` handle rig construction: adding/removing
/// FX, loading state chunks, setting initial parameters. These run on the
/// main thread (not sample-accurate, but fine for setup operations).
#[vox::service]
pub trait ExtensionParamService {
    /// Set a single FX parameter on this extension's track.
    async fn set_param(
        &self,
        fx_index: u32,
        param_index: u32,
        value: f64,
    ) -> Result<(), SignalServiceError>;

    /// Set multiple FX parameters atomically (within one audio block).
    async fn set_params_batch(
        &self,
        writes: Vec<ParamWriteRequest>,
    ) -> Result<(), SignalServiceError>;

    /// Apply a resolved graph to this extension's track (rig setup / scene switch).
    async fn apply_graph(
        &self,
        graph: crate::resolve::ResolvedGraph,
    ) -> Result<(), SignalServiceError>;

    /// Report this extension's current FX chain state.
    async fn report_fx_state(&self) -> Result<ExtensionFxState, SignalServiceError>;
}

