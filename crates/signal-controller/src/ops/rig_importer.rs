//! Import a DAW signal chain as a full rig preset hierarchy.
//!
//! Creates block presets → module presets → layer → engine → rig in
//! dependency order.  When state data is provided (captured from the live
//! DAW), it is stored on each block preset snapshot so that `rigs open`
//! can fully restore the plugin state.

use super::error::OpsError;
use crate::{SignalApi, SignalController};
use signal_proto::engine::{Engine, EngineId, EngineScene, EngineSceneId, LayerSelection};
use signal_proto::layer::{Layer, LayerId, LayerSnapshot, LayerSnapshotId, ModuleRef};
use signal_proto::rig::{EngineSelection, Rig, RigId, RigScene, RigSceneId};
use signal_proto::traits::Collection;
use signal_proto::{
    Block, BlockType, EngineType, Module, ModuleBlock, ModuleBlockSource, ModulePreset,
    ModulePresetId, ModuleSnapshot, ModuleSnapshotId, ModuleType, Preset, PresetId, Snapshot,
    SnapshotId,
};

// ── Input types ──────────────────────────────────────────────────
//
// These decouple the importer from `signal-daw-bridge` to avoid a
// cyclic dependency (controller → bridge → import → controller).
// The CLI converts `InferredChain` → `ImportChain` before calling us.

/// A module to import, with its blocks already flattened.
pub struct ImportModule {
    pub name: String,
    pub module_type: ModuleType,
    pub blocks: Vec<ImportBlock>,
    pub has_parallel_routing: bool,
}

/// A single block to import.
pub struct ImportBlock {
    pub label: String,
    pub block_type: BlockType,
    /// Raw DAW plugin identifier (e.g. "CLAP: Pro-Q 4 (FabFilter)").
    /// Stored as a `source:` metadata tag so `rigs open` can load the plugin.
    pub plugin_name: Option<String>,
    /// Binary plugin state captured from the live DAW.
    /// Stored on the block preset snapshot for full restore via `rigs open`.
    pub state_data: Option<Vec<u8>>,
}

/// Top-level input for the rig importer.
pub struct ImportChain {
    pub modules: Vec<ImportModule>,
    pub standalone_blocks: Vec<ImportBlock>,
}

// ── Output ───────────────────────────────────────────────────────

/// Summary returned after a successful rig import.
pub struct ImportedRig {
    pub rig: Rig,
    pub rig_id: RigId,
    pub new_block_preset_count: usize,
    pub reused_block_preset_count: usize,
    pub module_preset_ids: Vec<(String, ModulePresetId)>,
}

// ── Core logic ───────────────────────────────────────────────────

/// Import an [`ImportChain`] as a complete rig preset hierarchy.
///
/// For each module in the chain, block presets are found-or-created by
/// `(block_type, label)`, then assembled into a module preset.  All modules
/// are wired into a single layer → engine → rig with one "Default" scene.
pub async fn import_rig_from_chain<S: SignalApi>(
    signal: &SignalController<S>,
    chain: &ImportChain,
    rig_name: &str,
) -> Result<ImportedRig, OpsError> {
    let mut module_preset_ids: Vec<(String, ModulePresetId)> = Vec::new();
    let mut new_block_count: usize = 0;
    let mut reused_block_count: usize = 0;

    // ── Per-module: find-or-create block presets, build module preset ──

    for module in &chain.modules {
        if module.has_parallel_routing {
            eprintln!(
                "[import] warning: module \"{}\" has parallel routing; \
                 importing as flat serial block list",
                module.name
            );
        }

        let mut module_blocks = Vec::new();

        for (i, block) in module.blocks.iter().enumerate() {
            let (preset_id, is_new) = find_or_create_block_preset(
                signal,
                block.block_type,
                &block.label,
                block.plugin_name.as_deref(),
                block.state_data.as_deref(),
            )
            .await?;

            if is_new {
                new_block_count += 1;
            } else {
                reused_block_count += 1;
            }

            let block_id = format!("{}_{}", block.block_type.as_str(), i);
            module_blocks.push(ModuleBlock::new(
                block_id,
                &block.label,
                block.block_type,
                ModuleBlockSource::PresetDefault {
                    preset_id,
                    saved_at_version: None,
                },
            ));
        }

        let mp_id = create_module_preset(
            signal,
            &module.name,
            module.module_type,
            module_blocks,
        )
        .await?;

        module_preset_ids.push((module.name.clone(), mp_id));
    }

    // ── Standalone blocks → one extra module preset (if any) ──

    if !chain.standalone_blocks.is_empty() {
        let mut module_blocks = Vec::new();
        for (i, sb) in chain.standalone_blocks.iter().enumerate() {
            let (preset_id, is_new) = find_or_create_block_preset(
                signal,
                sb.block_type,
                &sb.label,
                sb.plugin_name.as_deref(),
                sb.state_data.as_deref(),
            )
            .await?;
            if is_new {
                new_block_count += 1;
            } else {
                reused_block_count += 1;
            }
            let block_id = format!("{}_{}", sb.block_type.as_str(), i);
            module_blocks.push(ModuleBlock::new(
                block_id,
                &sb.label,
                sb.block_type,
                ModuleBlockSource::PresetDefault {
                    preset_id,
                    saved_at_version: None,
                },
            ));
        }
        let mp_id = create_module_preset(
            signal,
            &format!("{rig_name} Standalone"),
            ModuleType::Custom,
            module_blocks,
        )
        .await?;
        module_preset_ids.push(("Standalone".to_string(), mp_id));
    }

    // ── Layer ──

    let layer_snap_id = LayerSnapshotId::new();
    let mut layer_snap = LayerSnapshot::new(layer_snap_id.clone(), "Default");
    for (_, mp_id) in &module_preset_ids {
        layer_snap = layer_snap.with_module(ModuleRef::new(mp_id.clone()));
    }

    let layer_id = LayerId::new();
    let layer = Layer::new(
        layer_id.clone(),
        format!("{rig_name} Layer"),
        EngineType::Guitar,
        layer_snap,
    );
    signal.layers().save(layer).await?;

    // ── Engine ──

    let engine_scene_id = EngineSceneId::new();
    let engine_scene = EngineScene::new(engine_scene_id.clone(), "Default")
        .with_layer(LayerSelection::new(layer_id.clone(), layer_snap_id));

    let engine_id = EngineId::new();
    let engine = Engine::new(
        engine_id.clone(),
        format!("{rig_name} Engine"),
        EngineType::Guitar,
        vec![layer_id],
        engine_scene,
    );
    signal.engines().save(engine).await?;

    // ── Rig ──

    let rig_scene = RigScene::new(RigSceneId::new(), "Default")
        .with_engine(EngineSelection::new(engine_id.clone(), engine_scene_id));

    let rig_id = RigId::new();
    let rig = Rig::new(rig_id.clone(), rig_name, vec![engine_id], rig_scene);
    let rig = signal.rigs().save(rig).await?;

    Ok(ImportedRig {
        rig,
        rig_id,
        new_block_preset_count: new_block_count,
        reused_block_preset_count: reused_block_count,
        module_preset_ids,
    })
}

// ── Helpers ──────────────────────────────────────────────────────

/// Find an existing block preset by `(block_type, name)` or create a new
/// block preset with captured state data.
///
/// When `plugin_name` is provided, stores it as a `source:` metadata tag
/// so that `rigs open` can determine which DAW plugin to load.
///
/// When `state_data` is provided, stores it on the snapshot so that
/// `rigs open` can fully restore the plugin state (bypassing param-by-name).
///
/// Returns `(PresetId, is_new)`.
async fn find_or_create_block_preset<S: SignalApi>(
    signal: &SignalController<S>,
    block_type: BlockType,
    label: &str,
    plugin_name: Option<&str>,
    state_data: Option<&[u8]>,
) -> Result<(PresetId, bool), OpsError> {
    let existing = signal.block_presets().list(block_type).await?;
    if let Some(mut preset) = existing.into_iter().find(|p| p.name() == label) {
        let mut dirty = false;

        // Back-fill source tag if the preset was created without one.
        if let Some(pn) = plugin_name {
            if !pn.is_empty() && !has_source_tag(&preset) {
                let tag = format!("source:{pn}");
                preset = preset
                    .with_metadata(signal_proto::metadata::Metadata::new().with_tag(tag));
                dirty = true;
            }
        }

        // Always overwrite state data when provided — earlier imports may have
        // stored data in the wrong format.
        if let Some(data) = state_data {
            let default_id = preset.default_snapshot().id().clone();
            if let Some(snap) = preset.variants_mut().iter_mut().find(|s| *s.id() == default_id) {
                snap.set_state_data(data.to_vec());
            }
            // Sync the private `default_snapshot` field from the updated snapshots vec.
            preset.set_default_variant_id(default_id);
            dirty = true;
        }

        if dirty {
            signal.block_presets().save(preset.clone()).await?;
        }
        return Ok((preset.id().clone(), false));
    }

    // Build snapshot — attach state data if captured from the live DAW.
    let mut snapshot = Snapshot::new(SnapshotId::new(), "Default", Block::default());
    if let Some(data) = state_data {
        snapshot = snapshot.with_state_data(data.to_vec());
    }

    let mut preset =
        Preset::with_default_snapshot(PresetId::new(), label, block_type, snapshot);

    if let Some(pn) = plugin_name {
        if !pn.is_empty() {
            let tag = format!("source:{pn}");
            preset = preset
                .with_metadata(signal_proto::metadata::Metadata::new().with_tag(tag));
        }
    }

    let saved = signal.block_presets().save(preset).await?;
    Ok((saved.id().clone(), true))
}

/// Check whether a block preset already has a `source:` metadata tag.
fn has_source_tag(preset: &Preset) -> bool {
    preset
        .metadata()
        .tags
        .as_slice()
        .iter()
        .any(|t| t.starts_with("source:"))
}

/// Create a module preset from a list of already-wired `ModuleBlock`s.
async fn create_module_preset<S: SignalApi>(
    signal: &SignalController<S>,
    name: &str,
    module_type: ModuleType,
    blocks: Vec<ModuleBlock>,
) -> Result<ModulePresetId, OpsError> {
    let module = Module::from_blocks(blocks);
    let snapshot = ModuleSnapshot::new(ModuleSnapshotId::new(), name, module);
    let preset = ModulePreset::new(ModulePresetId::new(), name, module_type, snapshot, vec![]);
    let saved = signal.module_presets().save(preset).await?;
    Ok(saved.id().clone())
}
