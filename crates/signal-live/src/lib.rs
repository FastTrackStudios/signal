//! Live service implementation for signal.
//!
//! Maps service traits onto storage repos:
//! - `BlockService` → `BlockRepo` + `ModuleRepo`
//! - `LayerService` → `LayerRepo`
//! - `EngineService` → `EngineRepo`
//! - `RigService` → `RigRepo`
//! - `ProfileService` → `ProfileRepo`
//! - `SongService` → `SongRepo`
//!
//! # Collection / Variant Mapping
//!
//! This service operates on *collection* and *variant* concepts:
//! - **Block collections** (`Preset`) group related block-parameter variants (`Snapshot`).
//! - **Module collections** (`ModulePreset`) group multi-block composition variants (`ModuleSnapshot`).
//! - **Layer collections** (`Layer`) group processing-lane variants (`LayerSnapshot`).
//! - **Engine collections** (`Engine`) group scene variants (`EngineScene`).
//! - **Rig presets** (`Rig`) group rig scene variants (`RigScene`).
//! - **Profiles** (`Profile`) group patch variants (`Patch`).
//! - **Songs** (`Song`) group section variants (`Section`).
//!
//! When a block variant is loaded (via `load_block_preset` / `load_block_preset_snapshot`), the
//! service applies a **side-effect**: the resolved block state is persisted as
//! the current active block.  This deterministic "load = apply" contract ensures
//! the active block always reflects the last loaded variant.

pub mod engine;

mod block_service;
mod browser_service;
mod engine_service;
mod layer_service;
mod profile_service;
mod rack_service;
mod resolve_service;
mod rig_service;
mod scene_template_service;
mod setlist_service;
mod song_service;

#[cfg(test)]
mod tests;

use roam::Context;
use signal_proto::{
    engine::{Engine, EngineId, EngineScene, EngineSceneId},
    layer::{Layer, LayerId, LayerSnapshot, LayerSnapshotId},
    override_policy::{validate_overrides, FreePolicy, ScenePolicy, SnapshotPolicy},
    overrides::{NodeOverrideOp, NodePathSegment},
    profile::{Patch, PatchId, PatchTarget, Profile, ProfileId},
    rack::{Rack, RackId},
    resolve::{
        LayerSource, ResolveError, ResolveTarget, ResolvedBlock, ResolvedEngine, ResolvedGraph,
        ResolvedLayer, ResolvedModule,
    },
    rig::{Rig, RigId, RigScene, RigSceneId},
    scene_template::{SceneTemplate, SceneTemplateId},
    setlist::{Setlist, SetlistEntry, SetlistEntryId, SetlistId},
    song::{Section, SectionId, Song, SongId},
    tagging::{
        infer_tags_from_name, BrowserEntityKind, BrowserEntry, BrowserHit, BrowserIndex,
        BrowserNodeId, BrowserQuery, StructuredTag, TagCategory, TagSet, TagWeights,
    },
    Block, BlockParameterOverride, BlockService, BlockType, BrowserService, EngineService,
    LayerService, ModuleBlockSource, ModulePreset, ModulePresetId, ModuleSnapshot,
    ModuleSnapshotId, Preset, PresetId, ProfileService, RackService, ResolveService, RigService,
    SceneTemplateService, SetlistService, Snapshot, SnapshotId, SongService, ALL_BLOCK_TYPES,
};
use signal_storage::{
    BlockRepo, BlockRepoLive, DatabaseConnection, EngineRepo, EngineRepoLive, LayerRepo,
    LayerRepoLive, ModuleRepo, ModuleRepoLive, ProfileRepo, ProfileRepoLive, RackRepo,
    RackRepoLive, RigRepo, RigRepoLive, SceneTemplateRepo, SceneTemplateRepoLive, SetlistRepo,
    SetlistRepoLive, SongRepo, SongRepoLive,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

// region: --- ServiceCache

/// In-memory read cache for list queries. Populated on first access, invalidated
/// on writes. All fields are `Option` — `None` means "not yet cached".
pub(crate) struct ServiceCache {
    pub(crate) block_collections: HashMap<BlockType, Vec<Preset>>,
    pub(crate) module_collections: Option<Vec<ModulePreset>>,
    pub(crate) layers: Option<Vec<Layer>>,
    pub(crate) engines: Option<Vec<Engine>>,
    pub(crate) rigs: Option<Vec<Rig>>,
    pub(crate) racks: Option<Vec<Rack>>,
}

impl ServiceCache {
    fn new() -> Self {
        Self {
            block_collections: HashMap::new(),
            module_collections: None,
            layers: None,
            engines: None,
            rigs: None,
            racks: None,
        }
    }
}

// endregion: --- ServiceCache

// region: --- SignalLive

/// Live service bridging RPC traits to storage repos.
///
/// Generic over all seven repo traits so tests can inject in-memory repos.
/// Default type parameters enable the common case without specifying concrete types.
pub struct SignalLive<
    B = BlockRepoLive,
    M = ModuleRepoLive,
    L = LayerRepoLive,
    E = EngineRepoLive,
    R = RigRepoLive,
    P = ProfileRepoLive,
    So = SongRepoLive,
    Se = SetlistRepoLive,
    St = SceneTemplateRepoLive,
    Ra = RackRepoLive,
> where
    B: BlockRepo,
    M: ModuleRepo,
    L: LayerRepo,
    E: EngineRepo,
    R: RigRepo,
    P: ProfileRepo,
    So: SongRepo,
    Se: SetlistRepo,
    St: SceneTemplateRepo,
    Ra: RackRepo,
{
    pub(crate) block_repo: Arc<B>,
    pub(crate) module_repo: Arc<M>,
    pub(crate) layer_repo: Arc<L>,
    pub(crate) engine_repo: Arc<E>,
    pub(crate) rig_repo: Arc<R>,
    pub(crate) profile_repo: Arc<P>,
    pub(crate) song_repo: Arc<So>,
    pub(crate) setlist_repo: Arc<Se>,
    pub(crate) scene_template_repo: Arc<St>,
    pub(crate) rack_repo: Arc<Ra>,
    pub(crate) cache: Arc<RwLock<ServiceCache>>,
}

impl<B, M, L, E, R, P, So, Se, St, Ra> SignalLive<B, M, L, E, R, P, So, Se, St, Ra>
where
    B: BlockRepo,
    M: ModuleRepo,
    L: LayerRepo,
    E: EngineRepo,
    R: RigRepo,
    P: ProfileRepo,
    So: SongRepo,
    Se: SetlistRepo,
    St: SceneTemplateRepo,
    Ra: RackRepo,
{
    pub fn new(
        block_repo: Arc<B>,
        module_repo: Arc<M>,
        layer_repo: Arc<L>,
        engine_repo: Arc<E>,
        rig_repo: Arc<R>,
        profile_repo: Arc<P>,
        song_repo: Arc<So>,
        setlist_repo: Arc<Se>,
        scene_template_repo: Arc<St>,
        rack_repo: Arc<Ra>,
    ) -> Self {
        Self {
            block_repo,
            module_repo,
            layer_repo,
            engine_repo,
            rig_repo,
            profile_repo,
            song_repo,
            setlist_repo,
            scene_template_repo,
            rack_repo,
            cache: Arc::new(RwLock::new(ServiceCache::new())),
        }
    }
}

impl
    SignalLive<
        BlockRepoLive,
        ModuleRepoLive,
        LayerRepoLive,
        EngineRepoLive,
        RigRepoLive,
        ProfileRepoLive,
        SongRepoLive,
        SetlistRepoLive,
        SceneTemplateRepoLive,
        RackRepoLive,
    >
{
    pub fn from_db(db: DatabaseConnection) -> Self {
        Self::new(
            Arc::new(BlockRepoLive::new(db.clone())),
            Arc::new(ModuleRepoLive::new(db.clone())),
            Arc::new(LayerRepoLive::new(db.clone())),
            Arc::new(EngineRepoLive::new(db.clone())),
            Arc::new(RigRepoLive::new(db.clone())),
            Arc::new(ProfileRepoLive::new(db.clone())),
            Arc::new(SongRepoLive::new(db.clone())),
            Arc::new(SetlistRepoLive::new(db.clone())),
            Arc::new(SceneTemplateRepoLive::new(db.clone())),
            Arc::new(RackRepoLive::new(db)),
        )
    }
}

// endregion: --- SignalLive
