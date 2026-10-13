//! FX chain, FX handle, and parameter handle

use std::sync::Arc;

use crate::Result;
use crate::{DawClients, Error};
use daw_proto::{
    AddFxAtRequest, CreateContainerRequest, EncloseInContainerRequest, Fx, FxChainContext,
    FxChannelConfig, FxContainerChannelConfig, FxLatency, FxNodeId, FxParamModulation, FxParameter,
    FxPinMappings, FxPresetIndex, FxRef, FxRoutingMode, FxStateChunk, FxTarget, FxTree,
    MoveFromContainerRequest, MoveToContainerRequest, ProjectContext,
    SetContainerChannelConfigRequest, SetNamedConfigRequest, SetParameterByNameRequest,
    SetParameterRequest,
};

/// Handle to an FX chain
///
/// Represents a chain of audio effects that can be on a track (output or input)
/// or in the monitoring path.
///
/// # Example
///
/// ```no_run
/// use daw_control::Daw;
///
/// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
/// let daw = Daw::new(handle);
/// let project = daw.current_project().await?;
///
/// // Get a track's FX chain
/// let track = project.tracks().by_name("Vocals").await?.unwrap();
/// let chain = track.fx_chain();
///
/// // Enumerate FX
/// for fx in chain.all().await? {
///     println!("FX: {} ({:?})", fx.name, fx.plugin_type);
/// }
///
/// // Add and configure FX
/// let compressor = chain.add("ReaComp").await?;
/// compressor.param_by_name("Threshold").set(0.3).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct FxChain {
    context: FxChainContext,
    project_id: String,
    clients: Arc<DawClients>,
}

impl FxChain {
    /// Create a new FX chain handle
    pub(crate) fn new(
        context: FxChainContext,
        project_id: String,
        clients: Arc<DawClients>,
    ) -> Self {
        Self {
            context,
            project_id,
            clients,
        }
    }

    /// Helper to create project context
    fn project_context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    // =========================================================================
    // Query Methods
    // =========================================================================

    /// Get all FX in the chain
    pub async fn all(&self) -> Result<Vec<Fx>> {
        let fx_list = self
            .clients
            .fx
            .list(self.project_context(), self.context.clone())
            .await?;
        Ok(fx_list)
    }

    /// Get FX by index
    pub async fn by_index(&self, index: u32) -> Result<Option<FxHandle>> {
        let target = FxTarget::new(self.context.clone(), FxRef::Index(index));
        let fx = self.clients.fx.get(self.project_context(), target).await?;

        Ok(fx.map(|f| {
            FxHandle::new(
                self.context.clone(),
                f.guid,
                self.project_id.clone(),
                self.clients.clone(),
            )
        }))
    }

    /// Get FX by GUID
    pub async fn by_guid(&self, guid: &str) -> Result<Option<FxHandle>> {
        let target = FxTarget::new(self.context.clone(), FxRef::Guid(guid.to_string()));
        let fx = self.clients.fx.get(self.project_context(), target).await?;

        Ok(fx.map(|f| {
            FxHandle::new(
                self.context.clone(),
                f.guid,
                self.project_id.clone(),
                self.clients.clone(),
            )
        }))
    }

    /// Get FX by name (first match)
    pub async fn by_name(&self, name: &str) -> Result<Option<FxHandle>> {
        let target = FxTarget::new(self.context.clone(), FxRef::Name(name.to_string()));
        let fx = self.clients.fx.get(self.project_context(), target).await?;

        Ok(fx.map(|f| {
            FxHandle::new(
                self.context.clone(),
                f.guid,
                self.project_id.clone(),
                self.clients.clone(),
            )
        }))
    }

    /// Get FX count
    pub async fn count(&self) -> Result<u32> {
        let count = self
            .clients
            .fx
            .count(self.project_context(), self.context.clone())
            .await?;
        Ok(count)
    }

    // =========================================================================
    // FX Management
    // =========================================================================

    /// Add an FX to the end of the chain
    ///
    /// Returns a handle to the newly added FX.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use daw_control::FxChain;
    /// # async fn example(chain: FxChain) -> daw_control::Result<()> {
    /// chain.add("ReaComp").await?;
    /// chain.add("VST3: FabFilter Pro-C 2").await?;
    /// chain.add("JS: 1175 Compressor").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn add(&self, fx_name: &str) -> Result<FxHandle> {
        let guid = self
            .clients
            .fx
            .add(
                self.project_context(),
                self.context.clone(),
                fx_name.to_string(),
            )
            .await?
            .ok_or_else(|| Error::Other(format!("Failed to add FX: {}", fx_name)))?;

        Ok(FxHandle::new(
            self.context.clone(),
            guid,
            self.project_id.clone(),
            self.clients.clone(),
        ))
    }

    // =========================================================================
    // State Chunk Operations
    // =========================================================================

    /// Capture state chunks for all FX in the chain.
    ///
    /// Returns a list of `FxStateChunk` entries in chain order, each containing
    /// the FX GUID, index, plugin name, and base64-encoded binary state.
    pub async fn state(&self) -> Result<Vec<FxStateChunk>> {
        let chunks = self
            .clients
            .fx
            .chain_state(self.project_context(), self.context.clone())
            .await?;
        Ok(chunks)
    }

    /// Restore state chunks for all FX in the chain.
    ///
    /// Matches FX by GUID and applies their saved state. FX not found in
    /// the current chain are skipped gracefully.
    pub async fn restore_state(&self, chunks: Vec<FxStateChunk>) -> Result<()> {
        self.clients
            .fx
            .set_chain_state(self.project_context(), self.context.clone(), chunks)
            .await??;
        Ok(())
    }

    // `subscribe_events` retired with the architect::rpc port —
    // FX event streaming lives on a sibling trait.

    // =========================================================================
    // FX Management (continued)
    // =========================================================================

    /// Add an FX at a specific position in the chain
    pub async fn add_at(&self, fx_name: &str, index: u32) -> Result<FxHandle> {
        let request = AddFxAtRequest {
            context: self.context.clone(),
            name: fx_name.to_string(),
            index,
        };

        let guid = self
            .clients
            .fx
            .add_at(self.project_context(), request)
            .await?
            .ok_or_else(|| {
                Error::Other(format!("Failed to add FX at index {}: {}", index, fx_name))
            })?;

        Ok(FxHandle::new(
            self.context.clone(),
            guid,
            self.project_id.clone(),
            self.clients.clone(),
        ))
    }

    // =========================================================================
    // Container / Tree Operations
    // =========================================================================

    /// Get the full FX tree (containers + plugins) for this chain.
    pub async fn tree(&self) -> Result<FxTree> {
        let tree = self
            .clients
            .fx
            .tree(self.project_context(), self.context.clone())
            .await?;
        Ok(tree)
    }

    /// Create a new container at the specified position.
    pub async fn create_container(&self, name: &str, index: u32) -> Result<FxNodeId> {
        let request = CreateContainerRequest {
            context: self.context.clone(),
            name: name.to_string(),
            index,
        };
        self.clients
            .fx
            .create_container(self.project_context(), request)
            .await?
            .ok_or_else(|| Error::Other(format!("Failed to create container: {}", name)))
    }

    /// Move an FX node into a container at the specified child position.
    pub async fn move_to_container(
        &self,
        node_id: &FxNodeId,
        container_id: &FxNodeId,
        child_index: u32,
    ) -> Result<()> {
        let request = MoveToContainerRequest {
            context: self.context.clone(),
            node_id: node_id.clone(),
            container_id: container_id.clone(),
            child_index,
        };
        self.clients
            .fx
            .move_to_container(self.project_context(), request)
            .await??;
        Ok(())
    }

    /// Move an FX node out of its container to a top-level position.
    pub async fn move_from_container(&self, node_id: &FxNodeId, target_index: u32) -> Result<()> {
        let request = MoveFromContainerRequest {
            context: self.context.clone(),
            node_id: node_id.clone(),
            target_index,
        };
        self.clients
            .fx
            .move_from_container(self.project_context(), request)
            .await??;
        Ok(())
    }

    /// Set the routing mode (serial/parallel) for a container.
    pub async fn set_routing_mode(
        &self,
        container_id: &FxNodeId,
        mode: FxRoutingMode,
    ) -> Result<()> {
        self.clients
            .fx
            .set_routing_mode(
                self.project_context(),
                self.context.clone(),
                container_id.clone(),
                mode,
            )
            .await??;
        Ok(())
    }

    /// Get channel configuration for a container.
    pub async fn container_channel_config(
        &self,
        container_id: &FxNodeId,
    ) -> Result<FxContainerChannelConfig> {
        let config = self
            .clients
            .fx
            .container_channel_config(
                self.project_context(),
                self.context.clone(),
                container_id.clone(),
            )
            .await?
            .ok_or_else(|| Error::Other("Container channel config not found".to_string()))?;
        Ok(config)
    }

    /// Set channel configuration for a container.
    pub async fn set_container_channel_config(
        &self,
        container_id: &FxNodeId,
        config: FxContainerChannelConfig,
    ) -> Result<()> {
        let request = SetContainerChannelConfigRequest {
            context: self.context.clone(),
            container_id: container_id.clone(),
            config,
        };
        self.clients
            .fx
            .set_container_channel_config(self.project_context(), request)
            .await??;
        Ok(())
    }

    /// Enclose existing FX nodes in a new container.
    pub async fn enclose_in_container(
        &self,
        node_ids: &[FxNodeId],
        name: &str,
    ) -> Result<FxNodeId> {
        let request = EncloseInContainerRequest {
            context: self.context.clone(),
            node_ids: node_ids.to_vec(),
            name: name.to_string(),
        };
        self.clients
            .fx
            .enclose_in_container(self.project_context(), request)
            .await?
            .ok_or_else(|| Error::Other("Failed to enclose FX in container".to_string()))
    }

    /// Explode a container: move all children to the parent level, then delete it.
    pub async fn explode_container(&self, container_id: &FxNodeId) -> Result<()> {
        self.clients
            .fx
            .explode_container(
                self.project_context(),
                self.context.clone(),
                container_id.clone(),
            )
            .await??;
        Ok(())
    }

    /// Rename a container.
    pub async fn rename_container(&self, container_id: &FxNodeId, name: &str) -> Result<()> {
        self.clients
            .fx
            .rename_container(
                self.project_context(),
                self.context.clone(),
                container_id.clone(),
                name.to_string(),
            )
            .await??;
        Ok(())
    }

    // =========================================================================
    // Raw FX Chain Chunk Operations
    // =========================================================================

    /// Get the raw RPP chunk text for this FX chain section.
    ///
    /// Returns the full `<FXCHAIN ...>...</FXCHAIN>` block as RPP text.
    /// Useful for extracting container blocks for module presets.
    pub async fn fx_chain_chunk_text(&self) -> Result<String> {
        self.clients
            .fx
            .chain_chunk_text(self.project_context(), self.context.clone())
            .await?
            .ok_or_else(|| Error::Other("No FX chain chunk found".to_string()))
    }

    /// Insert a raw RPP chunk block into this FX chain.
    ///
    /// The `chunk_text` should be a complete RPP block (e.g., a `<CONTAINER>`
    /// block). REAPER handles all plugin instantiation and state restoration
    /// atomically.
    pub async fn insert_chunk(&self, chunk_text: &str) -> Result<()> {
        self.clients
            .fx
            .insert_chain_chunk(
                self.project_context(),
                self.context.clone(),
                chunk_text.to_string(),
            )
            .await??;
        Ok(())
    }
}

impl std::fmt::Debug for FxChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FxChain")
            .field("context", &self.context)
            .field("project_id", &self.project_id)
            .finish()
    }
}

// =============================================================================
// FxHandle
// =============================================================================

/// Handle to a single FX instance
///
/// This handle represents a specific audio effect plugin. It's lightweight
/// and cheap to clone.
///
/// # Example
///
/// ```no_run
/// use daw_control::Daw;
///
/// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
/// let daw = Daw::new(handle);
/// let project = daw.current_project().await?;
/// let track = project.tracks().by_name("Vocals").await?.unwrap();
///
/// // Get an FX
/// let compressor = track.fx_chain().by_name("ReaComp").await?.unwrap();
///
/// // Enable/disable
/// compressor.enable().await?;
/// compressor.toggle().await?;
///
/// // Parameters
/// compressor.param(0).set(0.5).await?;
/// compressor.param_by_name("Threshold").set(0.3).await?;
///
/// // Presets
/// compressor.next_preset().await?;
///
/// // UI
/// compressor.open_ui().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct FxHandle {
    context: FxChainContext,
    fx_guid: String,
    project_id: String,
    clients: Arc<DawClients>,
}

impl FxHandle {
    /// Create a new FX handle
    pub(crate) fn new(
        context: FxChainContext,
        fx_guid: String,
        project_id: String,
        clients: Arc<DawClients>,
    ) -> Self {
        Self {
            context,
            fx_guid,
            project_id,
            clients,
        }
    }

    /// Get the FX GUID
    pub fn guid(&self) -> &str {
        &self.fx_guid
    }

    /// Helper to create project context
    fn project_context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    /// Helper to create FX target
    fn target(&self) -> FxTarget {
        FxTarget::new(self.context.clone(), FxRef::Guid(self.fx_guid.clone()))
    }

    // =========================================================================
    // Info
    // =========================================================================

    /// Get full FX state
    pub async fn info(&self) -> Result<Fx> {
        self.clients
            .fx
            .get(self.project_context(), self.target())
            .await?
            .ok_or_else(|| Error::Other(format!("FX not found: {}", self.fx_guid)))
    }

    // =========================================================================
    // Enable/Bypass
    // =========================================================================

    /// Enable the FX (un-bypass)
    pub async fn enable(&self) -> Result<()> {
        self.clients
            .fx
            .set_enabled(self.project_context(), self.target(), true)
            .await??;
        Ok(())
    }

    /// Take the FX offline (unloaded, its state kept) or back online.
    pub async fn set_offline(&self, offline: bool) -> Result<()> {
        self.clients
            .fx
            .set_offline(self.project_context(), self.target(), offline)
            .await??;
        Ok(())
    }

    /// Disable/bypass the FX
    pub async fn disable(&self) -> Result<()> {
        self.clients
            .fx
            .set_enabled(self.project_context(), self.target(), false)
            .await??;
        Ok(())
    }

    /// Toggle enabled state
    pub async fn toggle(&self) -> Result<()> {
        let info = self.info().await?;
        self.clients
            .fx
            .set_enabled(self.project_context(), self.target(), !info.enabled)
            .await??;
        Ok(())
    }

    /// Check if FX is enabled
    pub async fn is_enabled(&self) -> Result<bool> {
        Ok(self.info().await?.enabled)
    }

    // =========================================================================
    // Parameters
    // =========================================================================

    /// Get all parameters
    pub async fn parameters(&self) -> Result<Vec<FxParameter>> {
        let params = self
            .clients
            .fx
            .parameters(self.project_context(), self.target())
            .await?;
        Ok(params)
    }

    /// Get a parameter handle by index
    pub fn param(&self, index: u32) -> FxParamHandle {
        FxParamHandle::new_by_index(
            self.context.clone(),
            self.fx_guid.clone(),
            index,
            self.project_id.clone(),
            self.clients.clone(),
        )
    }

    /// Get a parameter handle by name
    pub fn param_by_name(&self, name: &str) -> FxParamHandle {
        FxParamHandle::new_by_name(
            self.context.clone(),
            self.fx_guid.clone(),
            name.to_string(),
            self.project_id.clone(),
            self.clients.clone(),
        )
    }

    // =========================================================================
    // Presets
    // =========================================================================

    /// Get the current preset index, total count, and name.
    ///
    /// Returns `None` if the FX doesn't support presets or isn't found.
    pub async fn preset_index(&self) -> Result<Option<FxPresetIndex>> {
        let result = self
            .clients
            .fx
            .preset_index(self.project_context(), self.target())
            .await?;
        Ok(result)
    }

    /// Navigate to the next preset
    pub async fn next_preset(&self) -> Result<()> {
        self.clients
            .fx
            .next_preset(self.project_context(), self.target())
            .await??;
        Ok(())
    }

    /// Navigate to the previous preset
    pub async fn prev_preset(&self) -> Result<()> {
        self.clients
            .fx
            .prev_preset(self.project_context(), self.target())
            .await??;
        Ok(())
    }

    /// Set preset by index
    pub async fn set_preset(&self, index: u32) -> Result<()> {
        self.clients
            .fx
            .set_preset(self.project_context(), self.target(), index)
            .await??;
        Ok(())
    }

    // =========================================================================
    // UI
    // =========================================================================

    /// Open the FX UI window
    pub async fn open_ui(&self) -> Result<()> {
        self.clients
            .fx
            .open_ui(self.project_context(), self.target())
            .await??;
        Ok(())
    }

    /// Close the FX UI window
    pub async fn close_ui(&self) -> Result<()> {
        self.clients
            .fx
            .close_ui(self.project_context(), self.target())
            .await??;
        Ok(())
    }

    /// Toggle the FX UI window
    pub async fn toggle_ui(&self) -> Result<()> {
        self.clients
            .fx
            .toggle_ui(self.project_context(), self.target())
            .await??;
        Ok(())
    }

    // =========================================================================
    // Chain Operations
    // =========================================================================

    /// Remove this FX from the chain
    pub async fn remove(&self) -> Result<()> {
        self.clients
            .fx
            .remove(self.project_context(), self.target())
            .await??;
        Ok(())
    }

    /// Move this FX to a new position in the chain
    pub async fn move_to(&self, index: u32) -> Result<()> {
        self.clients
            .fx
            .move_to(self.project_context(), self.target(), index)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Advanced
    // =========================================================================

    /// Get FX latency information
    pub async fn latency(&self) -> Result<FxLatency> {
        let latency = self
            .clients
            .fx
            .latency(self.project_context(), self.target())
            .await?
            .ok_or_else(|| Error::Other("FX latency not available".to_string()))?;
        Ok(latency)
    }

    // =========================================================================
    // State Chunk Operations
    // =========================================================================

    /// Get the binary state chunk for this FX (decoded bytes).
    ///
    /// Captures the complete plugin state including internal state
    /// not exposed as parameters.
    pub async fn state_chunk(&self) -> Result<Option<Vec<u8>>> {
        let chunk = self
            .clients
            .fx
            .state_chunk(self.project_context(), self.target())
            .await?;
        Ok(chunk)
    }

    /// Set the binary state chunk for this FX (decoded bytes).
    ///
    /// Restores complete plugin state. The FX must already exist in the chain.
    pub async fn set_state_chunk(&self, chunk: Vec<u8>) -> Result<()> {
        self.clients
            .fx
            .set_state_chunk(self.project_context(), self.target(), chunk)
            .await??;
        Ok(())
    }

    /// Get the base64-encoded state chunk for this FX.
    ///
    /// More efficient than `state_chunk()` when the data will be serialized
    /// (avoids decode + re-encode round-trip).
    pub async fn state_chunk_encoded(&self) -> Result<Option<String>> {
        let encoded = self
            .clients
            .fx
            .state_chunk_encoded(self.project_context(), self.target())
            .await?;
        Ok(encoded)
    }

    /// Set the base64-encoded state chunk for this FX.
    pub async fn set_state_chunk_encoded(&self, encoded: String) -> Result<()> {
        self.clients
            .fx
            .set_state_chunk_encoded(self.project_context(), self.target(), encoded)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Named Config
    // =========================================================================

    /// Get a named configuration parameter
    pub async fn get_config(&self, key: &str) -> Result<Option<String>> {
        let value = self
            .clients
            .fx
            .named_config(self.project_context(), self.target(), key.to_string())
            .await?;
        Ok(value)
    }

    /// Rename this FX instance (overrides the default plugin name in the FX chain).
    ///
    /// Sets REAPER's `renamed_name` config parameter. Pass an empty string to
    /// revert to the default plugin name.
    pub async fn rename(&self, name: &str) -> Result<()> {
        self.set_config("renamed_name", name).await
    }

    /// Set a named configuration parameter
    pub async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let request = SetNamedConfigRequest {
            target: self.target(),
            key: key.to_string(),
            value: value.to_string(),
        };
        self.clients
            .fx
            .set_named_config(self.project_context(), request)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Channel Configuration
    // =========================================================================

    /// Get the channel configuration for this FX.
    ///
    /// Returns channel count, mode, and supported flags.
    /// Not applicable to containers — use `FxChain::container_channel_config` instead.
    pub async fn channel_config(&self) -> Result<FxChannelConfig> {
        self.clients
            .fx
            .channel_config(self.project_context(), self.target())
            .await?
            .ok_or_else(|| Error::Other("FX channel config not available".to_string()))
    }

    /// Set the channel configuration for this FX.
    ///
    /// Use `FxChannelConfig::silent()` to zero output channels (for gapless loading).
    /// Use `FxChannelConfig::stereo()` to restore normal stereo output.
    pub async fn set_channel_config(&self, config: FxChannelConfig) -> Result<()> {
        self.clients
            .fx
            .set_channel_config(self.project_context(), self.target(), config)
            .await??;
        Ok(())
    }

    // =========================================================================
    // Output Silence/Restore (Pin Mappings)
    // =========================================================================

    /// Silence this FX by zeroing all output pin mappings.
    ///
    /// Returns the saved pin mappings for later restoration.
    /// This is the reliable mechanism for gapless loading — `channel_config`
    /// via `SetNamedConfigParm` is read-only in REAPER.
    pub async fn silence_output(&self) -> Result<FxPinMappings> {
        Ok(self
            .clients
            .fx
            .silence_output(self.project_context(), self.target())
            .await??)
    }

    /// Restore this FX's output pin mappings from a previously saved state.
    ///
    /// If `saved` has no entries, restores default stereo pass-through.
    pub async fn restore_output(&self, saved: FxPinMappings) -> Result<()> {
        self.clients
            .fx
            .restore_output(self.project_context(), self.target(), saved)
            .await??;
        Ok(())
    }
}

impl std::fmt::Debug for FxHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FxHandle")
            .field("fx_guid", &self.fx_guid)
            .field("context", &self.context)
            .field("project_id", &self.project_id)
            .finish()
    }
}

// =============================================================================
// FxParamHandle
// =============================================================================

/// Handle to an FX parameter
///
/// This handle represents a specific parameter on an FX. Parameters can be
/// referenced by index or by name.
///
/// # Example
///
/// ```no_run
/// use daw_control::Daw;
///
/// # async fn example(handle: vox::Caller) -> daw_control::Result<()> {
/// let daw = Daw::new(handle);
/// let project = daw.current_project().await?;
/// let track = project.tracks().by_name("Vocals").await?.unwrap();
/// let compressor = track.fx_chain().by_name("ReaComp").await?.unwrap();
///
/// // By index
/// let threshold = compressor.param(0);
/// threshold.set(0.5).await?;
/// let value = threshold.get().await?;
///
/// // By name
/// let ratio = compressor.param_by_name("Ratio");
/// ratio.set(0.7).await?;
/// let formatted = ratio.formatted().await?;
///
/// // Modulation info
/// let mod_state = ratio.modulation().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct FxParamHandle {
    context: FxChainContext,
    fx_guid: String,
    param_ref: ParamRef,
    project_id: String,
    clients: Arc<DawClients>,
}

/// Internal reference to a parameter (by index or name)
#[derive(Clone, Debug)]
enum ParamRef {
    Index(u32),
    Name(String),
}

impl FxParamHandle {
    /// Create a parameter handle by index
    pub(crate) fn new_by_index(
        context: FxChainContext,
        fx_guid: String,
        index: u32,
        project_id: String,
        clients: Arc<DawClients>,
    ) -> Self {
        Self {
            context,
            fx_guid,
            param_ref: ParamRef::Index(index),
            project_id,
            clients,
        }
    }

    /// Create a parameter handle by name
    pub(crate) fn new_by_name(
        context: FxChainContext,
        fx_guid: String,
        name: String,
        project_id: String,
        clients: Arc<DawClients>,
    ) -> Self {
        Self {
            context,
            fx_guid,
            param_ref: ParamRef::Name(name),
            project_id,
            clients,
        }
    }

    /// Helper to create project context
    fn project_context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_id.clone())
    }

    /// Helper to create FX target
    fn target(&self) -> FxTarget {
        FxTarget::new(self.context.clone(), FxRef::Guid(self.fx_guid.clone()))
    }

    // =========================================================================
    // Get/Set
    // =========================================================================

    /// Get the parameter state
    pub async fn info(&self) -> Result<FxParameter> {
        match &self.param_ref {
            ParamRef::Index(index) => self
                .clients
                .fx
                .parameter(self.project_context(), self.target(), *index)
                .await?
                .ok_or_else(|| Error::Other(format!("Parameter not found at index {}", index))),
            ParamRef::Name(name) => self
                .clients
                .fx
                .parameter_by_name(self.project_context(), self.target(), name.clone())
                .await?
                .ok_or_else(|| Error::Other(format!("Parameter not found: {}", name))),
        }
    }

    /// Get current value (normalized 0.0-1.0)
    pub async fn get(&self) -> Result<f64> {
        Ok(self.info().await?.value)
    }

    /// Set value (normalized 0.0-1.0)
    pub async fn set(&self, value: f64) -> Result<()> {
        match &self.param_ref {
            ParamRef::Index(index) => {
                let request = SetParameterRequest {
                    target: self.target(),
                    index: *index,
                    value,
                };
                self.clients
                    .fx
                    .set_parameter(self.project_context(), request)
                    .await??;
            }
            ParamRef::Name(name) => {
                let request = SetParameterByNameRequest {
                    target: self.target(),
                    name: name.clone(),
                    value,
                };
                self.clients
                    .fx
                    .set_parameter_by_name(self.project_context(), request)
                    .await??;
            }
        }
        Ok(())
    }

    /// Get formatted display value (e.g., "-12.5 dB")
    pub async fn formatted(&self) -> Result<String> {
        Ok(self.info().await?.formatted)
    }

    // =========================================================================
    // Advanced
    // =========================================================================

    /// Get modulation state (LFO, parameter linking)
    pub async fn modulation(&self) -> Result<FxParamModulation> {
        // Need to resolve index for modulation query
        let index = match &self.param_ref {
            ParamRef::Index(i) => *i,
            ParamRef::Name(_) => self.info().await?.index,
        };

        let mod_state = self
            .clients
            .fx
            .param_modulation(self.project_context(), self.target(), index)
            .await?
            .ok_or_else(|| Error::Other("Parameter modulation not available".to_string()))?;
        Ok(mod_state)
    }
}

impl std::fmt::Debug for FxParamHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FxParamHandle")
            .field("fx_guid", &self.fx_guid)
            .field("param_ref", &self.param_ref)
            .field("context", &self.context)
            .field("project_id", &self.project_id)
            .finish()
    }
}
