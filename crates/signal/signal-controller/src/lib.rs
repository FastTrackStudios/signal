//! Service-driven controller for the signal domain.
//!
//! All domain operations are accessed through **namespace accessors**:
//!
//! ```rust,ignore
//! signal.blocks().get(BlockType::Amp).await;
//! signal.rigs().list().await;
//! signal.profiles().activate(profile_id, Some(patch_id)).await;
//! ```
//!
//! Cross-cutting operations that span multiple domains live directly on `SignalController`:
//! - [`resolve_target`](SignalController::resolve_target) — resolve any target into an executable graph
//! - [`browse`](SignalController::browse) / [`browser_index`](SignalController::browser_index) — semantic search across all domains
//! - [`save_built_rig`](SignalController::save_built_rig) — persist a complete rig graph in dependency order

pub mod events;

use events::EventBus;
use signal_live::engine::patch_applier::DawPatchApplier;
use signal_live::SignalLive;
use signal_proto::{
    resolve::{ResolveError, ResolveTarget, ResolvedGraph},
    tagging::{BrowserHit, BrowserIndex, BrowserQuery},
};
use std::sync::Arc;

pub trait SignalApi:
    signal_proto::BlockService
    + signal_proto::LayerService
    + signal_proto::EngineService
    + signal_proto::RigService
    + signal_proto::ProfileService
    + signal_proto::SongService
    + signal_proto::SetlistService
    + signal_proto::BrowserService
    + signal_proto::ResolveService
    + signal_proto::SceneTemplateService
    + signal_proto::RackService
{
}

impl<T> SignalApi for T where
    T: signal_proto::BlockService
        + signal_proto::LayerService
        + signal_proto::EngineService
        + signal_proto::RigService
        + signal_proto::ProfileService
        + signal_proto::SongService
        + signal_proto::SetlistService
        + signal_proto::BrowserService
        + signal_proto::ResolveService
        + signal_proto::SceneTemplateService
        + signal_proto::RackService
{
}

pub trait ContextFactory: Send + Sync {
    fn make_context(&self) -> roam::Context;
}

pub type SharedContextFactory = Arc<dyn ContextFactory>;

#[derive(Default)]
pub struct DefaultContextFactory;

impl ContextFactory for DefaultContextFactory {
    fn make_context(&self) -> roam::Context {
        roam::Context::new(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            vec![],
        )
    }
}

pub mod ops;

pub struct SignalController<S = SignalLive>
where
    S: SignalApi,
{
    pub(crate) service: Arc<S>,
    pub(crate) context_factory: SharedContextFactory,
    pub(crate) event_bus: Arc<EventBus>,
    pub(crate) daw_applier: Arc<std::sync::RwLock<Option<Arc<dyn DawPatchApplier>>>>,
}

impl<S> Clone for SignalController<S>
where
    S: SignalApi,
{
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            context_factory: self.context_factory.clone(),
            event_bus: self.event_bus.clone(),
            daw_applier: self.daw_applier.clone(),
        }
    }
}

impl<S> PartialEq for SignalController<S>
where
    S: SignalApi,
{
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.service, &other.service)
    }
}

impl<S> Eq for SignalController<S> where S: SignalApi {}

impl<S> SignalController<S>
where
    S: SignalApi,
{
    pub fn new(service: Arc<S>) -> Self {
        Self::new_with_context(service, Arc::new(DefaultContextFactory))
    }

    pub fn new_with_context(service: Arc<S>, context_factory: SharedContextFactory) -> Self {
        Self {
            service,
            context_factory,
            event_bus: Arc::new(EventBus::default()),
            daw_applier: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    /// Attach a DAW patch applier for live FX chain loading.
    /// Can be called at construction time or later — all clones share the same slot.
    pub fn with_daw_applier(self, applier: Arc<dyn DawPatchApplier>) -> Self {
        *self.daw_applier.write().expect("lock poisoned") = Some(applier);
        self
    }

    /// Set (or replace) the DAW patch applier after construction.
    /// All clones share the same slot, so replacing it affects all users.
    pub fn set_daw_applier(&self, applier: Arc<dyn DawPatchApplier>) {
        *self.daw_applier.write().expect("lock poisoned") = Some(applier);
    }

    /// Check if a DAW patch applier is attached.
    pub fn has_daw_applier(&self) -> bool {
        self.daw_applier.read().expect("lock poisoned").is_some()
    }

    /// Access the underlying service implementation.
    pub fn service(&self) -> &Arc<S> {
        &self.service
    }

    // region: --- Namespace accessors

    /// Block parameter operations.
    pub fn blocks(&self) -> ops::BlockOps<S> {
        ops::BlockOps(self.clone())
    }

    /// Block preset (collection) operations.
    pub fn block_presets(&self) -> ops::BlockPresetOps<S> {
        ops::BlockPresetOps(self.clone())
    }

    /// Module preset (collection) operations.
    pub fn module_presets(&self) -> ops::ModulePresetOps<S> {
        ops::ModulePresetOps(self.clone())
    }

    /// Layer operations.
    pub fn layers(&self) -> ops::LayerOps<S> {
        ops::LayerOps(self.clone())
    }

    /// Engine operations.
    pub fn engines(&self) -> ops::EngineOps<S> {
        ops::EngineOps(self.clone())
    }

    /// Rig operations.
    pub fn rigs(&self) -> ops::RigOps<S> {
        ops::RigOps(self.clone())
    }

    /// Profile operations.
    pub fn profiles(&self) -> ops::ProfileOps<S> {
        ops::ProfileOps(self.clone())
    }

    /// Song operations.
    pub fn songs(&self) -> ops::SongOps<S> {
        ops::SongOps(self.clone())
    }

    /// Setlist operations.
    pub fn setlists(&self) -> ops::SetlistOps<S> {
        ops::SetlistOps(self.clone())
    }

    /// Scene template operations.
    pub fn scene_templates(&self) -> ops::SceneTemplateOps<S> {
        ops::SceneTemplateOps(self.clone())
    }

    /// Rack operations.
    pub fn racks(&self) -> ops::RackOps<S> {
        ops::RackOps(self.clone())
    }

    // endregion: --- Namespace accessors

    // region: --- Event streaming

    /// Subscribe to signal events for reactive UI updates.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<events::SignalEvent> {
        self.event_bus.subscribe()
    }

    /// Get the event bus (for internal use by methods that emit events).
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    // endregion: --- Event streaming

    // region: --- Cross-cutting operations

    /// Build the current browser index across all signal domain levels.
    pub async fn browser_index(&self) -> Result<BrowserIndex, ops::OpsError> {
        let cx = self.context_factory.make_context();
        self.service
            .browser_index(&cx)
            .await
            .map_err(ops::OpsError::Storage)
    }

    /// Query the semantic browser using structured tags and fallback scoring.
    pub async fn browse(&self, query: BrowserQuery) -> Result<Vec<BrowserHit>, ops::OpsError> {
        let cx = self.context_factory.make_context();
        self.service
            .browse(&cx, query)
            .await
            .map_err(ops::OpsError::Storage)
    }

    /// Resolve any target (rig scene, profile patch, song section) into an executable graph.
    pub async fn resolve_target(
        &self,
        target: ResolveTarget,
    ) -> Result<ResolvedGraph, ResolveError> {
        let cx = self.context_factory.make_context();
        self.service.resolve_target(&cx, target).await
    }

    // endregion: --- Cross-cutting operations

    // region: --- Builder integration

    /// Save all entities from a [`BuiltRig`] in dependency order.
    ///
    /// Saves: block presets → module → layer → engine → rig → profile.
    pub async fn save_built_rig(
        &self,
        built: &signal_proto::builder::BuiltRig,
    ) -> Result<(), ops::OpsError> {
        for bp in &built.block_presets {
            self.block_presets().save(bp.preset.clone()).await?;
        }
        self.module_presets()
            .save(built.module_preset.clone())
            .await?;
        self.layers().save(built.layer.clone()).await?;
        self.engines().save(built.engine.clone()).await?;
        self.rigs().save(built.rig.clone()).await?;
        if let Some(profile) = &built.profile {
            self.profiles().save(profile.clone()).await?;
        }
        Ok(())
    }

    // endregion: --- Builder integration

    // region: --- Import

    /// Import a signal chain as a complete rig preset hierarchy.
    ///
    /// Creates block presets → module presets → layer → engine → rig.
    /// Blocks are structural skeletons (default params); use `presets capture`
    /// to fill in live values later.
    pub async fn import_rig_from_chain(
        &self,
        chain: &ops::rig_importer::ImportChain,
        rig_name: &str,
    ) -> Result<ops::rig_importer::ImportedRig, ops::OpsError> {
        ops::rig_importer::import_rig_from_chain(self, chain, rig_name).await
    }

    // endregion: --- Import
}
