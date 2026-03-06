//! DAW block loading operations.
//!
//! Provides `load_block_to_track` on `SignalLive`, which handles the full flow
//! of adding an FX plugin to a REAPER track, applying preset state, and renaming
//! the FX slot using the Signal naming convention.
//!
//! The loading pipeline is split into two phases:
//!
//! 1. **Resolve** — look up the preset, extract the plugin name from metadata tags,
//!    select the snapshot, and gather block state. Testable with in-memory SQLite.
//! 2. **Execute** — add the FX to a DAW track, apply state, and rename. Requires a
//!    running DAW instance.

use daw::{FxNodeId, FxRoutingMode, TrackHandle};
use signal_proto::plugin_block::FxRole;
use signal_proto::traits::HasMetadata;
use signal_proto::{
    Block, BlockParameterOverride, BlockType, ModuleBlock, ModulePresetId, ModuleType, PresetId,
    SignalChain, SignalNode, SnapshotId,
};

use crate::macro_setup::MacroSetupResult;
use crate::SignalLive;
use signal_storage::{
    BlockRepo, EngineRepo, LayerRepo, ModuleRepo, ProfileRepo, RackRepo, RigRepo,
    SceneTemplateRepo, SetlistRepo, SongRepo,
};

// ─── Result types ───────────────────────────────────────────────

/// Result of a successful module-to-track load.
pub struct LoadModuleResult {
    /// One result per block in the module.
    pub loaded_fx: Vec<LoadBlockResult>,
    /// Module-level display name (e.g. "EQ Module: Pro-Q 4 35-Band").
    pub display_name: String,
}

/// Result of a successful block-to-track load.
pub struct LoadBlockResult {
    /// The REAPER FX GUID assigned to the new FX instance.
    pub fx_guid: String,
    /// The display name applied to the FX slot (e.g. "EQ Block: My Preset").
    pub display_name: String,
    /// Macro setup result, present when the block has a `MacroBank`.
    pub macro_setup: Option<MacroSetupResult>,
}

// ─── Resolved types (DAW-independent) ───────────────────────────

/// Pre-resolved data for loading a single FX instance onto a DAW track.
///
/// Contains everything needed to add, configure, and rename an FX — without
/// requiring access to the DAW or storage repos.
#[derive(Debug)]
pub struct ResolvedFxLoad {
    /// REAPER plugin identifier (e.g. `"CLAP: FabFilter Pro-Q 4"`).
    pub plugin_name: String,
    /// The block state (parameters) to apply.
    pub block: Block,
    /// Optional binary state chunk (preferred over param-by-param when present).
    pub state_data: Option<Vec<u8>>,
    /// Module-level parameter overrides applied on top of the block state.
    pub overrides: Vec<BlockParameterOverride>,
    /// The display name for the FX slot (e.g. `"EQ Block: Pro-Q 4"`).
    pub display_name: String,
}

/// A resolved node in the signal chain — either a single FX or a parallel split.
#[derive(Debug)]
pub enum ResolvedSignalNode {
    /// A single FX plugin to load.
    Fx(ResolvedFxLoad),
    /// A parallel split: each inner chain is one lane, mixed back at the output.
    Split(Vec<ResolvedSignalChain>),
}

/// A resolved sequence of signal-chain nodes, preserving parallel routing structure.
#[derive(Debug)]
pub struct ResolvedSignalChain {
    pub nodes: Vec<ResolvedSignalNode>,
}

impl ResolvedSignalChain {
    /// Collect all leaf `ResolvedFxLoad`s in depth-first order.
    ///
    /// Used by callers that only need a flat list regardless of topology —
    /// e.g., existing tests or simple serial modules.
    pub fn all_fx_loads(&self) -> Vec<&ResolvedFxLoad> {
        let mut out = Vec::new();
        self.collect_fx_loads(&mut out);
        out
    }

    fn collect_fx_loads<'a>(&'a self, out: &mut Vec<&'a ResolvedFxLoad>) {
        for node in &self.nodes {
            match node {
                ResolvedSignalNode::Fx(fx) => out.push(fx),
                ResolvedSignalNode::Split(lanes) => {
                    for lane in lanes {
                        lane.collect_fx_loads(out);
                    }
                }
            }
        }
    }
}

/// Pre-resolved data for loading a full module (multiple FX) onto a DAW track.
#[derive(Debug)]
pub struct ResolvedModuleLoad {
    /// Resolved signal chain preserving parallel routing structure.
    pub chain: ResolvedSignalChain,
    /// Module-level display name (e.g. `"EQ Module: Pro-Q 4 35-Band"`).
    pub display_name: String,
}

impl ResolvedModuleLoad {
    /// Flat list of all resolved FX loads in depth-first order.
    ///
    /// Backward-compatible accessor for callers that don't need topology.
    pub fn fx_loads(&self) -> Vec<&ResolvedFxLoad> {
        self.chain.all_fx_loads()
    }
}

// ─── Helpers ────────────────────────────────────────────────────

/// Extract the raw REAPER plugin name from any item's `source:` metadata tag.
fn raw_plugin_name(item: &impl HasMetadata) -> Option<String> {
    item.metadata()
        .tags
        .as_slice()
        .iter()
        .find_map(|t| t.strip_prefix("source:").map(|s| s.to_string()))
}

// ─── Plugin parameter mapping ──────────────────────────────────
//
// Seed presets use abstract parameter names (e.g. "low", "mid", "output").
// Real plugins expose different parameter names via CLAP/VST. This mapping
// expands abstract blocks into concrete DAW-ready parameters at load time.
//
// Imported presets (via signal-import) already carry `daw_name` from the
// import pipeline, so this mapping only fires when `daw_name` is absent.

/// Expand abstract block parameters into real DAW plugin parameters.
///
/// If the block already has `daw_name` set on its parameters (e.g. from an
/// import), it is returned unchanged. Otherwise, the plugin name is matched
/// to apply the correct mapping.
fn apply_plugin_param_mapping(plugin_name: &str, block: &Block) -> Block {
    // Skip if params already have daw_names (imported presets)
    if block.parameters().iter().any(|p| p.daw_name().is_some()) {
        return block.clone();
    }

    match plugin_name {
        s if s.contains("Pro-Q") && s.contains("FabFilter") => map_proq4_params(block),
        _ => block.clone(),
    }
}

/// Map abstract 5-band EQ params to FabFilter Pro-Q 4 CLAP parameters.
///
/// Abstract params: low, low_mid, mid, high_mid, high, output
/// Each gain value (0.0–1.0, 0.5 = flat) is mapped to a Pro-Q 4 band with:
///   - Band N Used = 1.0 (enable the band)
///   - Band N Enabled = 1.0
///   - Band N Frequency (fixed per band: 100, 400, 1500, 4000, 10000 Hz)
///   - Band N Gain (from the abstract param value)
///   - Band N Q = default (0.10 normalized = Q of 1.0)
///   - Band N Shape (Low/High = shelf, others = bell)
///
/// Normalization matches signal-import's FabFilter parser:
///   freq_norm = (log2(Hz) - 3.32) / (14.29 - 3.32)
///   gain is passed through (already 0.0–1.0 where 0.5 = 0 dB)
fn map_proq4_params(block: &Block) -> Block {
    use signal_proto::BlockParameter as BP;

    // Band layout: (abstract_id, band_num, freq_hz, shape_norm)
    // Shape: Bell=0.0, LowShelf=0.125, HighShelf=0.375
    let bands: &[(&str, u8, f32, f32)] = &[
        ("low",      1, 100.0,   0.125), // Low shelf
        ("low_mid",  2, 400.0,   0.0),   // Bell
        ("mid",      3, 1500.0,  0.0),   // Bell
        ("high_mid", 4, 4000.0,  0.0),   // Bell
        ("high",     5, 10000.0, 0.375), // High shelf
    ];

    let params = block.parameters();
    let mut daw_params = Vec::new();

    for &(abstract_id, band, freq_hz, shape) in bands {
        let gain = params
            .iter()
            .find(|p| p.id() == abstract_id)
            .map(|p| p.value().get())
            .unwrap_or(0.5);

        let freq_norm = ((freq_hz.log2() - 3.32) / (14.29 - 3.32)).clamp(0.0, 1.0);
        let q_norm: f32 = 0.10; // Q=1.0 default bell width

        daw_params.push(
            BP::new(format!("b{band}_used"), format!("B{band} Used"), 1.0)
                .with_daw_name(format!("Band {band} Used")),
        );
        daw_params.push(
            BP::new(format!("b{band}_enabled"), format!("B{band} On"), 1.0)
                .with_daw_name(format!("Band {band} Enabled")),
        );
        daw_params.push(
            BP::new(format!("b{band}_freq"), format!("B{band} Freq"), freq_norm)
                .with_daw_name(format!("Band {band} Frequency")),
        );
        daw_params.push(
            BP::new(format!("b{band}_gain"), format!("B{band} Gain"), gain)
                .with_daw_name(format!("Band {band} Gain")),
        );
        daw_params.push(
            BP::new(format!("b{band}_q"), format!("B{band} Q"), q_norm)
                .with_daw_name(format!("Band {band} Q")),
        );
        daw_params.push(
            BP::new(format!("b{band}_shape"), format!("B{band} Shape"), shape)
                .with_daw_name(format!("Band {band} Shape")),
        );
    }

    // Output level
    let output = params
        .iter()
        .find(|p| p.id() == "output")
        .map(|p| p.value().get())
        .unwrap_or(0.5);
    daw_params.push(
        BP::new("output", "Output", output)
            .with_daw_name("Output Level"),
    );

    Block::from_parameters(daw_params)
}

// ─── SignalLive impl ────────────────────────────────────────────

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
    // ── Resolution (DAW-independent) ────────────────────────────

    /// Resolve a block preset into everything needed for a DAW load.
    ///
    /// Looks up the preset in storage, extracts the plugin name from the
    /// `source:` metadata tag, selects the requested snapshot, and gathers
    /// the block state and optional binary chunk.
    pub async fn resolve_block_load(
        &self,
        block_type: BlockType,
        preset_id: &PresetId,
        snapshot_id: Option<&SnapshotId>,
    ) -> Result<ResolvedFxLoad, String> {
        // 1. Load preset from block repo.
        let presets = self
            .block_repo
            .list_block_collections(block_type)
            .await
            .map_err(|e| format!("Failed to list presets: {e}"))?;

        let preset = presets
            .into_iter()
            .find(|p| p.id() == preset_id)
            .ok_or_else(|| format!("Preset not found: {preset_id}"))?;

        // 2. Extract plugin name from source: tag.
        let plugin_name = raw_plugin_name(&preset)
            .ok_or("Preset has no source: tag — cannot determine plugin name")?;

        // 3. Get the requested snapshot by ID, or default if None.
        let (snapshot, is_default) = match snapshot_id {
            None => (preset.default_snapshot(), true),
            Some(id) => {
                let default = preset.default_snapshot();
                if default.id() == id {
                    (default, true)
                } else {
                    let snap = preset.snapshot(id).ok_or_else(|| {
                        format!("Snapshot not found: {id}")
                    })?;
                    (snap, false)
                }
            }
        };

        let block = snapshot.block();
        let state_data = snapshot.state_data().map(|d| d.to_vec());

        // 4. Build display name — include snapshot name for non-default.
        let name = if is_default {
            preset.name().to_string()
        } else {
            format!("{} - {}", preset.name(), snapshot.name())
        };
        let role = FxRole::Block {
            block_type,
            name,
        };
        let display_name = role.display_name();

        Ok(ResolvedFxLoad {
            plugin_name,
            block,
            state_data,
            overrides: Vec::new(),
            display_name,
        })
    }

    /// Resolve a module preset into resolved loads for each block in the module.
    pub async fn resolve_module_load(
        &self,
        module_type: ModuleType,
        preset_id: &ModulePresetId,
        snapshot_idx: usize,
    ) -> Result<ResolvedModuleLoad, String> {
        // 1. Load module preset.
        let module_presets = self
            .module_repo
            .list_module_collections()
            .await
            .map_err(|e| format!("Failed to list module presets: {e}"))?;

        let module_preset = module_presets
            .into_iter()
            .find(|p| p.id() == preset_id)
            .ok_or_else(|| format!("Module preset not found: {preset_id}"))?;

        // 2. Get the requested snapshot.
        let snapshots = module_preset.snapshots();
        let snapshot = if snapshot_idx == 0 {
            module_preset.default_snapshot().clone()
        } else {
            snapshots
                .get(snapshot_idx)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Snapshot index {} out of range (module preset has {} snapshots)",
                        snapshot_idx,
                        snapshots.len()
                    )
                })?
        };

        // 3. Recursively resolve the signal chain, preserving parallel routing topology.
        let chain = self
            .resolve_signal_chain(snapshot.module().chain())
            .await?;

        // 4. Build module-level display name.
        let module_role = FxRole::Module {
            module_type,
            name: module_preset.name().to_string(),
        };
        let display_name = module_role.display_name();

        Ok(ResolvedModuleLoad { chain, display_name })
    }

    // ── Resolve helpers ─────────────────────────────────────────

    /// Recursively resolve a `SignalChain` into a `ResolvedSignalChain`.
    ///
    /// Preserves parallel split topology — each `SignalNode::Split` becomes a
    /// `ResolvedSignalNode::Split` with one resolved lane per non-empty child chain.
    fn resolve_signal_chain<'a>(
        &'a self,
        chain: &'a SignalChain,
    ) -> futures::future::BoxFuture<'a, Result<ResolvedSignalChain, String>> {
        Box::pin(async move {
            let mut nodes = Vec::new();
            for node in chain.nodes() {
                match node {
                    SignalNode::Block(module_block) => {
                        let resolved = self.resolve_module_block(module_block).await?;
                        nodes.push(ResolvedSignalNode::Fx(resolved));
                    }
                    SignalNode::Split { lanes } => {
                        let mut resolved_lanes = Vec::with_capacity(lanes.len());
                        for lane in lanes {
                            let resolved_lane = self.resolve_signal_chain(lane).await?;
                            resolved_lanes.push(resolved_lane);
                        }
                        nodes.push(ResolvedSignalNode::Split(resolved_lanes));
                    }
                }
            }
            Ok(ResolvedSignalChain { nodes })
        })
    }

    /// Resolve a single `ModuleBlock` reference into a `ResolvedFxLoad`.
    async fn resolve_module_block(
        &self,
        module_block: &ModuleBlock,
    ) -> Result<ResolvedFxLoad, String> {
        let (block_preset, snapshot_to_apply) = match module_block.source() {
            signal_proto::ModuleBlockSource::PresetDefault { preset_id, .. } => {
                let presets = self
                    .block_repo
                    .list_block_collections(module_block.block_type())
                    .await
                    .map_err(|e| format!("Failed to list block presets: {e}"))?;
                let preset = presets
                    .into_iter()
                    .find(|p| p.id() == preset_id)
                    .ok_or_else(|| format!("Block preset not found: {preset_id}"))?;
                let snap = preset.default_snapshot();
                (preset, snap)
            }
            signal_proto::ModuleBlockSource::PresetSnapshot {
                preset_id,
                snapshot_id,
                ..
            } => {
                let presets = self
                    .block_repo
                    .list_block_collections(module_block.block_type())
                    .await
                    .map_err(|e| format!("Failed to list block presets: {e}"))?;
                let preset = presets
                    .into_iter()
                    .find(|p| p.id() == preset_id)
                    .ok_or_else(|| format!("Block preset not found: {preset_id}"))?;
                let snap = preset
                    .snapshot(snapshot_id)
                    .ok_or_else(|| format!("Snapshot not found: {snapshot_id}"))?;
                (preset, snap)
            }
            signal_proto::ModuleBlockSource::Inline { block: _ } => {
                return Err(format!(
                    "Inline block source for '{}' cannot be loaded (no plugin name)",
                    module_block.label()
                ));
            }
        };

        let plugin_name = raw_plugin_name(&block_preset).ok_or_else(|| {
            format!(
                "Block preset '{}' has no source: tag — cannot determine plugin name",
                block_preset.name()
            )
        })?;

        let block = snapshot_to_apply.block();
        let state_data = snapshot_to_apply.state_data().map(|d| d.to_vec());

        let role = FxRole::Block {
            block_type: module_block.block_type(),
            name: format!("{} - {}", block_preset.name(), module_block.label()),
        };
        let display_name = role.display_name();

        Ok(ResolvedFxLoad {
            plugin_name,
            block,
            state_data,
            overrides: module_block.overrides().to_vec(),
            display_name,
        })
    }

    // ── Execution (DAW-dependent) ───────────────────────────────

    /// Load a block preset onto a DAW track: add FX, apply state, rename.
    ///
    /// Returns the FX GUID and display name on success.
    pub async fn load_block_to_track(
        &self,
        block_type: BlockType,
        preset_id: &PresetId,
        snapshot_id: Option<&SnapshotId>,
        track: &TrackHandle,
    ) -> Result<LoadBlockResult, String> {
        let resolved = self
            .resolve_block_load(block_type, preset_id, snapshot_id)
            .await?;
        Self::execute_fx_load(&resolved, track).await
    }

    /// Load a module preset onto a DAW track inside an FX container.
    ///
    /// Handles serial and parallel (`Split`) nodes:
    /// - Serial blocks are loaded directly in order.
    /// - `Split` nodes create a parallel REAPER container with one sub-container per lane.
    ///
    /// The outer module container wraps all top-level nodes.
    pub async fn load_module_to_track(
        &self,
        module_type: ModuleType,
        preset_id: &ModulePresetId,
        snapshot_idx: usize,
        track: &TrackHandle,
    ) -> Result<LoadModuleResult, String> {
        let resolved = self
            .resolve_module_load(module_type, preset_id, snapshot_idx)
            .await?;

        // Recursively load FX and build parallel containers for Split nodes.
        let (loaded_fx, top_node_ids) =
            execute_chain_nodes(&resolved.chain, track).await?;

        // Enclose all top-level nodes in the outer module container.
        track
            .fx_chain()
            .enclose_in_container(&top_node_ids, &resolved.display_name)
            .await
            .map_err(|e| format!("Failed to create module container: {e}"))?;

        Ok(LoadModuleResult {
            loaded_fx,
            display_name: resolved.display_name,
        })
    }

    /// Execute a single resolved FX load against a DAW track.
    async fn execute_fx_load(
        resolved: &ResolvedFxLoad,
        track: &TrackHandle,
    ) -> Result<LoadBlockResult, String> {
        execute_fx_load_free(resolved, track).await
    }
    /// Load multiple block presets onto a DAW track in parallel.
    ///
    /// Adds all FX sequentially (to get stable indices), then configures
    /// them (state injection, rename, macros) concurrently.
    pub async fn load_blocks_to_track(
        &self,
        loads: Vec<(BlockType, &PresetId, Option<&SnapshotId>)>,
        track: &TrackHandle,
    ) -> Result<Vec<LoadBlockResult>, String> {
        // Phase 1: Resolve all loads in parallel.
        let resolve_futures: Vec<_> = loads
            .iter()
            .map(|(bt, pid, sid)| self.resolve_block_load(*bt, pid, *sid))
            .collect();
        let resolved: Vec<ResolvedFxLoad> = futures::future::try_join_all(resolve_futures).await?;

        // Phase 2: Add all FX sequentially (index tracking requires ordering).
        let mut fx_pairs: Vec<(ResolvedFxLoad, daw::FxHandle, u32)> = Vec::with_capacity(resolved.len());
        for r in resolved {
            let fx_index = track
                .fx_chain()
                .count()
                .await
                .map_err(|e| format!("Failed to count FX: {e}"))?;
            let fx = track
                .fx_chain()
                .add(&r.plugin_name)
                .await
                .map_err(|e| format!("Failed to add FX '{}': {e}", r.plugin_name))?;
            fx_pairs.push((r, fx, fx_index));
        }

        // Phase 3: Configure all FX in parallel (state, rename, macros).
        let configure_futures: Vec<_> = fx_pairs
            .into_iter()
            .map(|(r, fx, idx)| {
                let track = track.clone();
                async move { configure_fx_free(&r, &track, fx, idx).await }
            })
            .collect();
        futures::future::try_join_all(configure_futures).await
    }

}

/// Load a single resolved FX onto a DAW track: add, configure, and rename.
async fn execute_fx_load_free(
    resolved: &ResolvedFxLoad,
    track: &TrackHandle,
) -> Result<LoadBlockResult, String> {
    let fx_index = track
        .fx_chain()
        .count()
        .await
        .map_err(|e| format!("Failed to count FX: {e}"))?;
    let fx = track
        .fx_chain()
        .add(&resolved.plugin_name)
        .await
        .map_err(|e| format!("Failed to add FX: {e}"))?;
    configure_fx_free(resolved, track, fx, fx_index).await
}

/// Configure an already-added FX: apply state, overrides, rename, macros.
async fn configure_fx_free(
    resolved: &ResolvedFxLoad,
    track: &TrackHandle,
    mut fx: daw::FxHandle,
    fx_index: u32,
) -> Result<LoadBlockResult, String> {
    if let Some(data) = &resolved.state_data {
        if resolved.plugin_name.contains("NeuralAmpModeler") {
            let model_path = std::str::from_utf8(data)
                .map_err(|e| format!("NAM state_data is not valid UTF-8: {e}"))?;
            inject_nam_model_state(&fx, model_path)
                .await
                .map_err(|e| format!("Failed to load NAM model: {e}"))?;
        } else if std::str::from_utf8(data).is_ok() {
            fx.set_state_chunk(data.clone())
                .await
                .map_err(|e| format!("Failed to apply state chunk: {e}"))?;
        } else {
            inject_binary_state(&fx, data)
                .await
                .map_err(|e| format!("Failed to inject binary state: {e}"))?;
            fx = track
                .fx_chain()
                .by_index(fx_index)
                .await
                .map_err(|e| format!("Failed to re-acquire FX after binary inject: {e}"))?
                .ok_or("FX disappeared after binary state injection")?;
        }
    } else {
        let mapped = apply_plugin_param_mapping(&resolved.plugin_name, &resolved.block);
        for param in mapped.parameters() {
            fx.param_by_name(param.effective_daw_name())
                .set(param.value().get() as f64)
                .await
                .map_err(|e| format!("Failed to set param '{}': {e}", param.name()))?;
        }
    }

    for ovr in &resolved.overrides {
        fx.param_by_name(ovr.parameter_id())
            .set(ovr.value().get() as f64)
            .await
            .map_err(|e| format!("Failed to apply override '{}': {e}", ovr.parameter_id()))?;
    }

    fx.rename(&resolved.display_name)
        .await
        .map_err(|e| format!("Failed to rename FX: {e}"))?;

    let macro_setup =
        crate::macro_setup::setup_macros_for_block(track, &fx, &resolved.block).await?;

    Ok(LoadBlockResult {
        fx_guid: fx.guid().to_string(),
        display_name: resolved.display_name.clone(),
        macro_setup,
    })
}

/// Recursively execute a `ResolvedSignalChain`, loading FX and building
/// parallel containers for `Split` nodes.
///
/// Returns `(all_block_results, top_level_node_ids)` where `top_level_node_ids`
/// are the FX node IDs at the current level — either direct FX GUIDs or
/// container IDs created for split nodes. The caller uses `top_level_node_ids`
/// to enclose everything in the outer module container.
fn execute_chain_nodes<'a>(
    chain: &'a ResolvedSignalChain,
    track: &'a TrackHandle,
) -> futures::future::BoxFuture<'a, Result<(Vec<LoadBlockResult>, Vec<FxNodeId>), String>> {
    Box::pin(async move {
        let mut all_results: Vec<LoadBlockResult> = Vec::new();
        let mut top_node_ids: Vec<FxNodeId> = Vec::new();

        for node in &chain.nodes {
            match node {
                ResolvedSignalNode::Fx(resolved) => {
                    let result = execute_fx_load_free(resolved, track).await?;
                    let node_id = FxNodeId::from_guid(&result.fx_guid);
                    top_node_ids.push(node_id);
                    all_results.push(result);
                }
                ResolvedSignalNode::Split(lanes) => {
                    let mut lane_node_ids: Vec<FxNodeId> = Vec::new();

                    for (lane_idx, lane) in lanes.iter().enumerate() {
                        if lane.nodes.is_empty() {
                            continue; // skip empty placeholder lanes
                        }

                        let (lane_results, lane_top_ids) =
                            execute_chain_nodes(lane, track).await?;
                        all_results.extend(lane_results);

                        // Lanes with a single node are used directly; multi-node
                        // lanes need a serial sub-container so they run in sequence.
                        let lane_node_id = if lane_top_ids.len() == 1 {
                            lane_top_ids.into_iter().next().unwrap()
                        } else {
                            let lane_name = format!("Lane {}", lane_idx + 1);
                            track
                                .fx_chain()
                                .enclose_in_container(&lane_top_ids, &lane_name)
                                .await
                                .map_err(|e| format!("Failed to create lane container: {e}"))?
                        };
                        lane_node_ids.push(lane_node_id);
                    }

                    if lane_node_ids.is_empty() {
                        continue; // all lanes were empty
                    }

                    // Wrap all lanes in a parallel container.
                    let split_id = track
                        .fx_chain()
                        .enclose_in_container(&lane_node_ids, "Split")
                        .await
                        .map_err(|e| format!("Failed to create split container: {e}"))?;

                    track
                        .fx_chain()
                        .set_routing_mode(&split_id, FxRoutingMode::Parallel)
                        .await
                        .map_err(|e| format!("Failed to set parallel routing on split: {e}"))?;

                    top_node_ids.push(split_id);
                }
            }
        }

        Ok((all_results, top_node_ids))
    })
}

/// Load a NAM model into an already-added NeuralAmpModeler FX instance.
///
/// Follows the proven approach from `reaper_nam_load.rs`:
/// 1. Get the FX's default REAPER chunk (has valid VST3 header)
/// 2. Extract the base64 plugin state
/// 3. Decode it as a `NamVstChunk`
/// 4. Rewrite the model path
/// 5. Re-encode and set back via the rebuilt REAPER chunk
async fn inject_nam_model_state(fx: &daw::FxHandle, model_path: &str) -> Result<(), String> {
    use nam_manager::{decode_chunk, encode_chunk, rewrite_paths};

    // 1. Get the default REAPER chunk from the just-added FX.
    let reaper_chunk = fx
        .state_chunk_encoded()
        .await
        .map_err(|e| format!("Failed to get default chunk: {e}"))?
        .ok_or("FX has no default chunk")?;

    // 2. Extract the base64 plugin state from the REAPER chunk.
    let segments = extract_state_base64(&reaper_chunk)
        .ok_or("Failed to extract base64 state from REAPER chunk")?;
    let unified_b64 = first_base64_segment(&segments);

    // 3. Decode the NAM binary chunk (gets the real REAPER VST3 header).
    let mut nam_chunk = decode_chunk(unified_b64.trim())
        .map_err(|e| format!("Failed to decode NAM chunk: {e}"))?;

    // 4. Rewrite the model path.
    rewrite_paths(&mut nam_chunk, Some(model_path), None);

    // 5. Re-encode and rebuild the REAPER chunk.
    let new_state_b64 = encode_chunk(&nam_chunk);
    let rebuilt = rebuild_chunk_with_state(&reaper_chunk, &new_state_b64);
    fx.set_state_chunk_encoded(rebuilt)
        .await
        .map_err(|e| format!("Failed to set rebuilt chunk: {e}"))
}

/// Inject raw binary plugin state into an FX's default REAPER chunk.
///
/// Replaces the base64 state segment in the FX's existing REAPER chunk
/// with the provided binary data (base64-encoded).
async fn inject_binary_state(fx: &daw::FxHandle, binary_state: &[u8]) -> Result<(), String> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

    let existing = fx
        .state_chunk_encoded()
        .await
        .map_err(|e| format!("Failed to get default chunk: {e}"))?
        .ok_or("FX has no default chunk to inject state into")?;

    let new_b64 = BASE64.encode(binary_state);
    let rebuilt = rebuild_chunk_with_state(&existing, &new_b64);
    fx.set_state_chunk_encoded(rebuilt)
        .await
        .map_err(|e| format!("Failed to set rebuilt chunk: {e}"))
}

// ─── REAPER chunk helpers ──────────────────────────────────────

/// Extract base64 data lines from a REAPER VST/VST3/CLAP chunk block.
///
/// For VST/VST3, extracts lines between header and footer.
/// For CLAP, extracts lines from within the `<STATE` block.
fn extract_state_base64(chunk: &str) -> Option<Vec<String>> {
    let lines: Vec<&str> = chunk.lines().collect();
    if lines.len() < 3 {
        return None;
    }

    let header = lines[0].trim();
    if header.starts_with("<CLAP") {
        // CLAP: extract only lines inside <STATE ... >
        let mut in_state = false;
        let mut data_lines = Vec::new();
        for &line in &lines[1..] {
            let trimmed = line.trim();
            if !in_state && trimmed.starts_with("<STATE") {
                in_state = true;
                continue;
            }
            if in_state {
                if trimmed == ">" {
                    break;
                }
                if !trimmed.is_empty() {
                    data_lines.push(trimmed.to_string());
                }
            }
        }
        if data_lines.is_empty() { None } else { Some(data_lines) }
    } else {
        // VST/VST3: flat structure
        let data_lines: Vec<String> = lines[1..lines.len() - 1]
            .iter()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if data_lines.is_empty() { None } else { Some(data_lines) }
    }
}

/// Extract the first base64 segment (up to and including the `=`-padded line).
fn first_base64_segment(segments: &[String]) -> String {
    let mut result = String::new();
    for line in segments {
        result.push_str(line);
        if line.ends_with('=') {
            break;
        }
    }
    result
}

/// Rebuild a REAPER text chunk with new base64 plugin state.
///
/// Handles two chunk formats:
/// - **VST/VST3**: flat structure — header, base64 data, optional trailing metadata, `>`
/// - **CLAP**: nested structure — header, CFG/IN_PINS/etc., `<STATE` block with base64, `>`
///
/// For CLAP chunks, only the `<STATE>` block content is replaced; everything else
/// (CFG, IN_PINS, etc.) is preserved.
fn rebuild_chunk_with_state(chunk: &str, new_b64: &str) -> String {
    let lines: Vec<&str> = chunk.lines().collect();
    let header = lines.first().copied().unwrap_or("");

    // Detect CLAP chunk format by header
    let trimmed_header = header.trim();
    if trimmed_header.starts_with("<CLAP") {
        return rebuild_clap_chunk_with_state(&lines, new_b64);
    }

    // VST/VST3: flat structure
    let data_lines: Vec<&str> = lines[1..lines.len().saturating_sub(1)]
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    let mut trailing: Vec<&str> = Vec::new();
    let mut found_end = false;
    for line in &data_lines {
        if found_end {
            trailing.push(line);
        } else if line.ends_with('=') {
            found_end = true;
        }
    }

    let mut result = String::from(header);
    result.push('\n');
    for chunk_line in new_b64.as_bytes().chunks(128) {
        result.push_str("  ");
        result.push_str(&String::from_utf8_lossy(chunk_line));
        result.push('\n');
    }
    for t in &trailing {
        result.push_str("  ");
        result.push_str(t);
        result.push('\n');
    }
    result.push('>');
    result
}

/// Rebuild a CLAP chunk, replacing only the `<STATE` block content.
///
/// CLAP chunk structure:
/// ```text
/// <CLAP "CLAP: Pro-R 2 (FabFilter)" com.fabfilter.pro-r.2 ""
///   CFG 4 760 335 ""
///   <IN_PINS
///   >
///   <STATE
///     <base64 lines>
///   >
/// >
/// ```
fn rebuild_clap_chunk_with_state(lines: &[&str], new_b64: &str) -> String {
    let mut result = String::new();

    // Track whether we're inside the <STATE block
    let mut in_state = false;
    let mut state_replaced = false;

    for &line in lines {
        let trimmed = line.trim();

        if !in_state && trimmed.starts_with("<STATE") {
            // Start of STATE block — write the opening tag
            result.push_str(line);
            result.push('\n');
            in_state = true;
            // Write new base64 content
            for b64_chunk in new_b64.as_bytes().chunks(128) {
                result.push_str("    ");
                result.push_str(&String::from_utf8_lossy(b64_chunk));
                result.push('\n');
            }
            state_replaced = true;
        } else if in_state {
            if trimmed == ">" {
                // End of STATE block — write the closing >
                result.push_str(line);
                result.push('\n');
                in_state = false;
            }
            // Skip original STATE content (replaced above)
        } else {
            // Preserve everything outside STATE block (header, CFG, IN_PINS, etc.)
            result.push_str(line);
            result.push('\n');
        }
    }

    // If no STATE block was found, fall back to appending state before the final >
    if !state_replaced {
        // Remove trailing > and newline, add STATE block, re-add >
        let trimmed = result.trim_end().trim_end_matches('>').to_string();
        result = trimmed;
        result.push_str("  <STATE\n");
        for b64_chunk in new_b64.as_bytes().chunks(128) {
            result.push_str("    ");
            result.push_str(&String::from_utf8_lossy(b64_chunk));
            result.push('\n');
        }
        result.push_str("  >\n");
        result.push('>');
    } else {
        // Remove trailing newline added by the loop
        if result.ends_with('\n') {
            result.pop();
        }
    }

    result
}
