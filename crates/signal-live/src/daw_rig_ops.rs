//! Full rig provisioning onto REAPER tracks.
//!
//! Provides [`SignalLive::load_rig_to_daw`] which creates the complete
//! `[R]`/`[E]`/`[L]` track hierarchy via [`crate::daw_rig_builder`] and
//! loads all modules for a given rig + scene in one call.
//!
//! # Phase separation
//!
//! 1. **Resolve** — load all engine/layer/module data from storage repos.
//!    Builds a [`RigTemplate`] for track creation and collects module refs
//!    for the FX loading phase.
//! 2. **Execute** — call [`instantiate_rig`] to create the REAPER track
//!    hierarchy, then call [`load_module_to_track`] for each module ref.

use daw::Project;
use signal_proto::{
    rig::{Rig, RigSceneId},
    rig_template::{EngineTemplate, FxSendTemplate, LayerTemplate, RigTemplate},
    BlockType, ModulePresetId, ModuleSnapshotId, PresetId, SnapshotId, ALL_BLOCK_TYPES,
};
use signal_storage::{
    BlockRepo, EngineRepo, LayerRepo, ModuleRepo, ProfileRepo, RackRepo, RigRepo,
    SceneTemplateRepo, SetlistRepo, SongRepo,
};

use crate::daw_block_ops::{LoadBlockResult, LoadModuleResult};
use crate::daw_rig_builder::{instantiate_rig, RigInstance};
use crate::SignalLive;

// ─── Result types ────────────────────────────────────────────────

/// Module load results for a single layer track.
pub struct LayerLoadResult {
    /// REAPER track GUID for this layer.
    pub track_guid: String,
    /// Loaded modules on this layer track, in module-ref order.
    pub modules: Vec<LoadModuleResult>,
    /// Standalone block presets loaded directly (not wrapped in a module).
    pub standalone_blocks: Vec<LoadBlockResult>,
}

/// Result of loading an entire rig onto REAPER tracks.
pub struct RigLoadResult {
    /// The materialized REAPER track hierarchy (folder tracks, layer tracks, sends).
    pub rig_instance: RigInstance,
    /// Layer module results, flattened in engine-first, layer-second order.
    ///
    /// Index `i` corresponds to the `i`-th layer across all engines,
    /// enumerated in engine_selections order → layer_selections order.
    pub layer_results: Vec<LayerLoadResult>,
}

// ─── Internal resolution types ───────────────────────────────────

/// Resolved module reference ready for DAW execution.
struct ResolvedModuleRef {
    preset_id: ModulePresetId,
    snapshot_idx: usize,
    module_type: signal_proto::ModuleType,
}

/// Resolved standalone block reference ready for DAW execution.
struct ResolvedBlockRef {
    block_type: BlockType,
    preset_id: PresetId,
    snapshot_id: Option<SnapshotId>,
}

/// Resolved layer data (name + modules + standalone blocks to load).
struct ResolvedLayerInfo {
    name: String,
    module_refs: Vec<ResolvedModuleRef>,
    block_refs: Vec<ResolvedBlockRef>,
}

/// Resolved engine data (name + layers + FX send names).
struct ResolvedEngineInfo {
    name: String,
    layers: Vec<ResolvedLayerInfo>,
    fx_send_names: Vec<String>,
}

// ─── SignalLive impl ─────────────────────────────────────────────

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
    /// Load an entire rig onto REAPER tracks.
    ///
    /// Creates the `[R]`/`[E]`/`[L]` track hierarchy via `instantiate_rig`,
    /// then loads all modules from the selected scene onto their respective
    /// layer tracks via `load_module_to_track`.
    ///
    /// If `scene_id` is `None`, the rig's default scene is used.
    ///
    /// Returns a [`RigLoadResult`] containing the track handles and per-layer
    /// module FX GUIDs needed for scene switching.
    pub async fn load_rig_to_daw(
        &self,
        rig: &Rig,
        scene_id: Option<&RigSceneId>,
        project: &Project,
    ) -> Result<RigLoadResult, String> {
        // ── Phase 1: Resolve domain data from repos ───────────────────

        let scene = match scene_id {
            Some(id) => rig
                .variant(id)
                .ok_or_else(|| format!("Rig scene not found: {id}"))?,
            None => rig.default_variant().ok_or("Rig has no default scene")?,
        };

        // Load all module presets once — used to look up module_type and
        // snapshot index for each module_ref in every layer snapshot.
        let all_module_presets = self
            .module_repo
            .list_module_collections()
            .await
            .map_err(|e| format!("Failed to list module presets: {e}"))?;

        // Load all block presets once — used to look up block_type from
        // collection_id for standalone block_refs on layer snapshots.
        let mut all_block_presets = Vec::new();
        for &bt in ALL_BLOCK_TYPES {
            let presets = self
                .block_repo
                .list_block_collections(bt)
                .await
                .map_err(|e| format!("Failed to list {bt:?} block presets: {e}"))?;
            all_block_presets.extend(presets);
        }

        let mut engine_infos: Vec<ResolvedEngineInfo> = Vec::new();

        for engine_sel in &scene.engine_selections {
            let engine = self
                .engine_repo
                .load_engine(&engine_sel.engine_id)
                .await
                .map_err(|e| format!("Failed to load engine {}: {e}", engine_sel.engine_id))?
                .ok_or_else(|| format!("Engine not found: {}", engine_sel.engine_id))?;

            let engine_scene = engine.variant(&engine_sel.variant_id).ok_or_else(|| {
                format!(
                    "Engine scene '{}' not found in engine '{}'",
                    engine_sel.variant_id, engine.name
                )
            })?;

            let mut layer_infos: Vec<ResolvedLayerInfo> = Vec::new();

            for layer_sel in &engine_scene.layer_selections {
                let layer = self
                    .layer_repo
                    .load_layer(&layer_sel.layer_id)
                    .await
                    .map_err(|e| format!("Failed to load layer {}: {e}", layer_sel.layer_id))?
                    .ok_or_else(|| format!("Layer not found: {}", layer_sel.layer_id))?;

                // Find the selected snapshot (or fall back to default).
                let snapshot = layer
                    .variant(&layer_sel.variant_id)
                    .or_else(|| layer.default_variant())
                    .ok_or_else(|| format!("No snapshot found for layer '{}'", layer.name))?;

                // Resolve each module_ref to (preset_id, snapshot_idx, module_type).
                let mut resolved_module_refs: Vec<ResolvedModuleRef> = Vec::new();
                for module_ref in &snapshot.module_refs {
                    let module_preset = all_module_presets
                        .iter()
                        .find(|p| p.id() == &module_ref.collection_id)
                        .ok_or_else(|| {
                            format!("Module preset not found: {}", module_ref.collection_id)
                        })?;

                    let snapshot_idx = resolve_snapshot_idx(
                        module_preset.snapshots(),
                        module_ref.variant_id.as_ref(),
                    );

                    resolved_module_refs.push(ResolvedModuleRef {
                        preset_id: module_ref.collection_id.clone(),
                        snapshot_idx,
                        module_type: module_preset.module_type(),
                    });
                }

                // Resolve each block_ref to (block_type, preset_id, snapshot_id).
                let mut resolved_block_refs: Vec<ResolvedBlockRef> = Vec::new();
                for block_ref in &snapshot.block_refs {
                    let block_preset = all_block_presets
                        .iter()
                        .find(|p| p.id() == &block_ref.collection_id)
                        .ok_or_else(|| {
                            format!("Block preset not found: {}", block_ref.collection_id)
                        })?;

                    resolved_block_refs.push(ResolvedBlockRef {
                        block_type: block_preset.block_type(),
                        preset_id: block_ref.collection_id.clone(),
                        snapshot_id: block_ref.variant_id.clone(),
                    });
                }

                layer_infos.push(ResolvedLayerInfo {
                    name: layer.name.clone(),
                    module_refs: resolved_module_refs,
                    block_refs: resolved_block_refs,
                });
            }

            let fx_send_names: Vec<String> =
                engine.fx_sends.iter().map(|s| s.name.clone()).collect();

            engine_infos.push(ResolvedEngineInfo {
                name: engine.name.clone(),
                layers: layer_infos,
                fx_send_names,
            });
        }

        // ── Phase 2: Build RigTemplate and instantiate REAPER tracks ──

        let rig_template = RigTemplate {
            name: rig.name.clone(),
            engines: engine_infos
                .iter()
                .map(|e| EngineTemplate {
                    name: e.name.clone(),
                    layers: e
                        .layers
                        .iter()
                        .map(|l| LayerTemplate {
                            name: l.name.clone(),
                        })
                        .collect(),
                    fx_sends: e
                        .fx_send_names
                        .iter()
                        .map(|s| FxSendTemplate { name: s.clone() })
                        .collect(),
                })
                .collect(),
            fx_sends: rig
                .fx_sends
                .iter()
                .map(|s| FxSendTemplate {
                    name: s.name.clone(),
                })
                .collect(),
        };

        let rig_instance = instantiate_rig(&rig_template, project)
            .await
            .map_err(|e| format!("Failed to instantiate rig tracks: {e}"))?;

        // ── Phase 3: Load modules + standalone blocks onto each layer track ──

        let mut layer_results: Vec<LayerLoadResult> = Vec::new();

        for (engine_idx, engine_info) in engine_infos.iter().enumerate() {
            let engine_instance = &rig_instance.engine_instances[engine_idx];

            for (layer_idx, layer_info) in engine_info.layers.iter().enumerate() {
                let layer_track = &engine_instance.layer_tracks[layer_idx];
                let track_guid = layer_track.guid().to_string();

                let mut loaded_modules: Vec<LoadModuleResult> = Vec::new();

                for module_ref in &layer_info.module_refs {
                    let result = self
                        .load_module_to_track(
                            module_ref.module_type,
                            &module_ref.preset_id,
                            module_ref.snapshot_idx,
                            layer_track,
                        )
                        .await?;

                    loaded_modules.push(result);
                }

                // Load standalone block presets directly onto the layer track.
                let mut loaded_blocks: Vec<LoadBlockResult> = Vec::new();

                for block_ref in &layer_info.block_refs {
                    let result = self
                        .load_block_to_track(
                            block_ref.block_type,
                            &block_ref.preset_id,
                            block_ref.snapshot_id.as_ref(),
                            layer_track,
                        )
                        .await?;

                    loaded_blocks.push(result);
                }

                layer_results.push(LayerLoadResult {
                    track_guid,
                    modules: loaded_modules,
                    standalone_blocks: loaded_blocks,
                });
            }
        }

        Ok(RigLoadResult {
            rig_instance,
            layer_results,
        })
    }
}

// ─── Helpers ─────────────────────────────────────────────────────

/// Resolve a module snapshot variant ID to its index in the snapshots slice.
///
/// Returns `0` (the default snapshot) if `variant_id` is `None` or not found.
fn resolve_snapshot_idx(
    snapshots: &[signal_proto::ModuleSnapshot],
    variant_id: Option<&ModuleSnapshotId>,
) -> usize {
    match variant_id {
        None => 0,
        Some(vid) => snapshots.iter().position(|s| s.id() == vid).unwrap_or(0),
    }
}
