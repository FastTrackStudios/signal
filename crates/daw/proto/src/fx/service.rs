//! Effects service — FX instance management (add/remove/configure
//! plugins in chains, parameter access, presets, state chunks,
//! containers/tree, channel config, pin mappings).
//!
//! "FX" = "Effects" — these are plugin instances mounted in chains.
//! See `PluginLoading` (`crate::plugin_loader`) for loading plugin
//! binaries off disk. Subscription stream retires; sibling trait
//! when revived.

use super::{
    AddFxAtRequest, CreateContainerRequest, EncloseInContainerRequest, Fx, FxChainContext,
    FxChannelConfig, FxContainerChannelConfig, FxLatency, FxNodeId, FxParamModulation, FxParameter,
    FxPinMappings, FxPresetIndex, FxRoutingMode, FxStateChunk, FxTarget, FxTree, InstalledFx,
    LastTouchedFx, MoveFromContainerRequest, MoveToContainerRequest,
    SetContainerChannelConfigRequest, SetNamedConfigRequest, SetParameterByNameRequest,
    SetParameterRequest,
};
use crate::batch::{FxChainArg, ProjectArg};
use crate::{DawResult, ProjectContext};

#[architect::rpc(ops(ProjectContext as ProjectArg, FxChainContext as FxChainArg))]
pub trait Effects {
    // ── Installed plugins ──────────────────────────────────────────

    /// All installed FX plugins (VST2/VST3/CLAP/AU/JS/…). Global to
    /// the DAW — no `ProjectContext` needed.
    fn list_installed(&self) -> Vec<InstalledFx>;

    /// Last touched FX parameter, if any.
    fn last_touched(&self) -> Option<LastTouchedFx>;

    // ── Chain queries ──────────────────────────────────────────────

    fn list(&self, project: ProjectContext, chain: FxChainContext) -> Vec<Fx>;
    fn get(&self, project: ProjectContext, target: FxTarget) -> Option<Fx>;
    fn count(&self, project: ProjectContext, chain: FxChainContext) -> u32;

    // ── State ──────────────────────────────────────────────────────

    fn set_enabled(
        &self,
        project: ProjectContext,
        target: FxTarget,
        enabled: bool,
    ) -> DawResult<()>;

    fn set_offline(
        &self,
        project: ProjectContext,
        target: FxTarget,
        offline: bool,
    ) -> DawResult<()>;

    // ── Management ─────────────────────────────────────────────────

    /// Add an FX to the end of a chain. Accepts short name
    /// (`"ReaComp"`), prefixed (`"VST3: FabFilter Pro-C 2"`), or JS
    /// (`"JS: 1175 Compressor"`). Returns the new FX's GUID.
    fn add(&self, project: ProjectContext, chain: FxChainContext, name: &str) -> Option<String>;

    /// Add an FX at a specific position.
    fn add_at(&self, project: ProjectContext, request: AddFxAtRequest) -> Option<String>;

    fn remove(&self, project: ProjectContext, target: FxTarget) -> DawResult<()>;

    fn move_to(&self, project: ProjectContext, target: FxTarget, new_index: u32) -> DawResult<()>;

    // ── Parameters ─────────────────────────────────────────────────

    fn parameters(&self, project: ProjectContext, target: FxTarget) -> Vec<FxParameter>;

    fn parameter(
        &self,
        project: ProjectContext,
        target: FxTarget,
        index: u32,
    ) -> Option<FxParameter>;

    fn set_parameter(&self, project: ProjectContext, request: SetParameterRequest)
    -> DawResult<()>;

    fn parameter_by_name(
        &self,
        project: ProjectContext,
        target: FxTarget,
        name: &str,
    ) -> Option<FxParameter>;

    fn set_parameter_by_name(
        &self,
        project: ProjectContext,
        request: SetParameterByNameRequest,
    ) -> DawResult<()>;

    // ── Presets ────────────────────────────────────────────────────

    /// Current preset index, total count, and name.
    fn preset_index(&self, project: ProjectContext, target: FxTarget) -> Option<FxPresetIndex>;

    fn next_preset(&self, project: ProjectContext, target: FxTarget) -> DawResult<()>;
    fn prev_preset(&self, project: ProjectContext, target: FxTarget) -> DawResult<()>;
    fn set_preset(&self, project: ProjectContext, target: FxTarget, index: u32) -> DawResult<()>;

    // ── UI ─────────────────────────────────────────────────────────

    fn open_ui(&self, project: ProjectContext, target: FxTarget) -> DawResult<()>;
    fn close_ui(&self, project: ProjectContext, target: FxTarget) -> DawResult<()>;
    fn toggle_ui(&self, project: ProjectContext, target: FxTarget) -> DawResult<()>;

    // ── Named config ───────────────────────────────────────────────

    /// `TrackFX_GetNamedConfigParm` — plugin-specific settings not
    /// exposed as regular parameters.
    fn named_config(&self, project: ProjectContext, target: FxTarget, key: &str) -> Option<String>;

    fn set_named_config(
        &self,
        project: ProjectContext,
        request: SetNamedConfigRequest,
    ) -> DawResult<()>;

    fn latency(&self, project: ProjectContext, target: FxTarget) -> Option<FxLatency>;

    fn param_modulation(
        &self,
        project: ProjectContext,
        target: FxTarget,
        param_index: u32,
    ) -> Option<FxParamModulation>;

    // ── Channel config (non-container) ─────────────────────────────

    fn channel_config(&self, project: ProjectContext, target: FxTarget) -> Option<FxChannelConfig>;

    fn set_channel_config(
        &self,
        project: ProjectContext,
        target: FxTarget,
        config: FxChannelConfig,
    ) -> DawResult<()>;

    // ── Output pin mappings (silence/restore) ──────────────────────

    /// Zero all output pin mappings. Returns the previous mappings so
    /// they can be restored later. The reliable mechanism for
    /// silencing non-container FX (channel_config via
    /// SetNamedConfigParm is read-only).
    fn silence_output(&self, project: ProjectContext, target: FxTarget)
    -> DawResult<FxPinMappings>;

    /// Restore output pin mappings. Empty `saved` restores stereo
    /// pass-through default.
    fn restore_output(
        &self,
        project: ProjectContext,
        target: FxTarget,
        saved: FxPinMappings,
    ) -> DawResult<()>;

    // ── State chunks ──────────────────────────────────────────────

    fn state_chunk(&self, project: ProjectContext, target: FxTarget) -> Option<Vec<u8>>;

    fn set_state_chunk(
        &self,
        project: ProjectContext,
        target: FxTarget,
        chunk: Vec<u8>,
    ) -> DawResult<()>;

    fn state_chunk_encoded(&self, project: ProjectContext, target: FxTarget) -> Option<String>;

    fn set_state_chunk_encoded(
        &self,
        project: ProjectContext,
        target: FxTarget,
        encoded: &str,
    ) -> DawResult<()>;

    fn chain_state(&self, project: ProjectContext, chain: FxChainContext) -> Vec<FxStateChunk>;

    fn set_chain_state(
        &self,
        project: ProjectContext,
        chain: FxChainContext,
        chunks: Vec<FxStateChunk>,
    ) -> DawResult<()>;

    // ── Containers / tree ──────────────────────────────────────────

    /// Full FX chain as a recursive tree (containers, children,
    /// routing modes, channel configs).
    fn tree(&self, project: ProjectContext, chain: FxChainContext) -> FxTree;

    /// Create a new empty container. Returns the new `FxNodeId`.
    fn create_container(
        &self,
        project: ProjectContext,
        request: CreateContainerRequest,
    ) -> Option<FxNodeId>;

    fn move_to_container(
        &self,
        project: ProjectContext,
        request: MoveToContainerRequest,
    ) -> DawResult<()>;

    fn move_from_container(
        &self,
        project: ProjectContext,
        request: MoveFromContainerRequest,
    ) -> DawResult<()>;

    fn set_routing_mode(
        &self,
        project: ProjectContext,
        chain: FxChainContext,
        node_id: FxNodeId,
        mode: FxRoutingMode,
    ) -> DawResult<()>;

    fn container_channel_config(
        &self,
        project: ProjectContext,
        chain: FxChainContext,
        container_id: FxNodeId,
    ) -> Option<FxContainerChannelConfig>;

    fn set_container_channel_config(
        &self,
        project: ProjectContext,
        request: SetContainerChannelConfigRequest,
    ) -> DawResult<()>;

    /// Create a new container at the position of the first specified
    /// node, then move all specified nodes into it. Returns the new
    /// container's id.
    fn enclose_in_container(
        &self,
        project: ProjectContext,
        request: EncloseInContainerRequest,
    ) -> Option<FxNodeId>;

    /// Move children out to the parent level, then remove the empty
    /// container.
    fn explode_container(
        &self,
        project: ProjectContext,
        chain: FxChainContext,
        container_id: FxNodeId,
    ) -> DawResult<()>;

    fn rename_container(
        &self,
        project: ProjectContext,
        chain: FxChainContext,
        container_id: FxNodeId,
        name: &str,
    ) -> DawResult<()>;

    // ── Raw chunk text ─────────────────────────────────────────────

    /// Raw RPP `<FXCHAIN ...>...</FXCHAIN>` block from the track's
    /// state chunk. `None` if no FX chain.
    fn chain_chunk_text(&self, project: ProjectContext, chain: FxChainContext) -> Option<String>;

    /// Insert a raw RPP block (e.g. a `<CONTAINER>` block with
    /// nested FX) at the end of the chain. REAPER handles plugin
    /// instantiation + state restoration atomically.
    fn insert_chain_chunk(
        &self,
        project: ProjectContext,
        chain: FxChainContext,
        chunk_text: &str,
    ) -> DawResult<()>;
}
