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

use daw::{FxNodeId, TrackHandle};
use signal_proto::plugin_block::FxRole;
use signal_proto::traits::HasMetadata;
use signal_proto::{
    Block, BlockParameterOverride, BlockType, ModulePresetId, ModuleType, PresetId,
};

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

/// Pre-resolved data for loading a full module (multiple FX) onto a DAW track.
#[derive(Debug)]
pub struct ResolvedModuleLoad {
    /// One resolved load per block in the module.
    pub fx_loads: Vec<ResolvedFxLoad>,
    /// Module-level display name (e.g. `"EQ Module: Pro-Q 4 35-Band"`).
    pub display_name: String,
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
        snapshot_idx: usize,
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

        // 3. Get the requested snapshot.
        let snapshots = preset.snapshots();
        let snapshot = if snapshot_idx == 0 {
            preset.default_snapshot()
        } else {
            snapshots.get(snapshot_idx).cloned().ok_or_else(|| {
                format!(
                    "Snapshot index {} out of range (preset has {} snapshots)",
                    snapshot_idx,
                    snapshots.len()
                )
            })?
        };

        let block = snapshot.block();
        let state_data = snapshot.state_data().map(|d| d.to_vec());

        // 4. Build display name — include snapshot name for non-default.
        let name = if snapshot_idx == 0 {
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

        // 3. For each block in the module, resolve source → gather state.
        let blocks = snapshot.module().blocks();
        let mut fx_loads = Vec::with_capacity(blocks.len());

        for module_block in &blocks {
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
                        .ok_or_else(|| {
                            format!("Block preset not found: {preset_id}")
                        })?;
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
                        .ok_or_else(|| {
                            format!("Block preset not found: {preset_id}")
                        })?;
                    let snap = preset.snapshot(snapshot_id).ok_or_else(|| {
                        format!("Snapshot not found: {snapshot_id}")
                    })?;
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

            fx_loads.push(ResolvedFxLoad {
                plugin_name,
                block,
                state_data,
                overrides: module_block.overrides().to_vec(),
                display_name,
            });
        }

        // 4. Build module-level display name.
        let module_role = FxRole::Module {
            module_type,
            name: module_preset.name().to_string(),
        };
        let display_name = module_role.display_name();

        Ok(ResolvedModuleLoad {
            fx_loads,
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
        snapshot_idx: usize,
        track: &TrackHandle,
    ) -> Result<LoadBlockResult, String> {
        let resolved = self
            .resolve_block_load(block_type, preset_id, snapshot_idx)
            .await?;
        Self::execute_fx_load(&resolved, track).await
    }

    /// Load a module preset onto a DAW track inside an FX container.
    ///
    /// Adds each block as an FX instance, applies state, renames, then wraps
    /// all the loaded FX in a container named after the module (e.g. "EQ: Pro-Q 4 3-Band").
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

        // Load each block as a flat FX on the track.
        let mut loaded_fx = Vec::with_capacity(resolved.fx_loads.len());
        for fx_load in &resolved.fx_loads {
            let result = Self::execute_fx_load(fx_load, track).await?;
            loaded_fx.push(result);
        }

        // Enclose all loaded FX in a container named after the module.
        let node_ids: Vec<FxNodeId> = loaded_fx
            .iter()
            .map(|r| FxNodeId::from_guid(&r.fx_guid))
            .collect();

        track
            .fx_chain()
            .enclose_in_container(&node_ids, &resolved.display_name)
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
        // 1. Add FX to the track.
        let fx = track
            .fx_chain()
            .add(&resolved.plugin_name)
            .await
            .map_err(|e| format!("Failed to add FX: {e}"))?;

        // 2. Apply state — prefer chunk, fall back to param-by-param.
        //    For param-by-param, apply plugin mapping to translate abstract
        //    parameter names to real DAW plugin names (e.g. "low" → "Band 1 Gain").
        if let Some(data) = &resolved.state_data {
            fx.set_state_chunk(data.clone())
                .await
                .map_err(|e| format!("Failed to apply state chunk: {e}"))?;
        } else {
            let mapped = apply_plugin_param_mapping(&resolved.plugin_name, &resolved.block);
            for param in mapped.parameters() {
                fx.param_by_name(param.effective_daw_name())
                    .set(param.value().get() as f64)
                    .await
                    .map_err(|e| format!("Failed to set param '{}': {e}", param.name()))?;
            }
        }

        // 3. Apply module-level parameter overrides on top.
        for ovr in &resolved.overrides {
            fx.param_by_name(ovr.parameter_id())
                .set(ovr.value().get() as f64)
                .await
                .map_err(|e| {
                    format!(
                        "Failed to apply override '{}': {e}",
                        ovr.parameter_id(),
                    )
                })?;
        }

        // 4. Rename the FX slot.
        fx.rename(&resolved.display_name)
            .await
            .map_err(|e| format!("Failed to rename FX: {e}"))?;

        Ok(LoadBlockResult {
            fx_guid: fx.guid().to_string(),
            display_name: resolved.display_name.clone(),
        })
    }
}
