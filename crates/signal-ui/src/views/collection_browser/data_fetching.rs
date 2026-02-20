//! Async data fetching and detail resolution for the collection browser.
//!
//! Each `fetch_col*` function queries the `Signal` for the
//! appropriate domain entities and maps them into `ColumnItem` rows.

use signal::layer::Layer;
use signal::rig::RigType;
use signal::tagging::{StructuredTag, TagCategory, TagSet};
use signal::traits::HasMetadata;
use signal::Signal;
use signal::{
    BlockType, ModuleBlock, ModuleBlockSource, ModulePreset, Preset, SignalChain, SignalNode,
    ALL_BLOCK_TYPES,
};
use std::collections::HashMap;

use super::grid_conversion::ParamLookup;
use super::types::{
    ColumnItem, DetailData, EngineFlowData, LayerFlowData, ModuleChainData, NavCategory,
};

/// Sentinel tag value to distinguish layer presets from rig presets in the Presets nav.
const LAYER_PRESET_TAG: usize = usize::MAX;

// region: --- Column fetching

pub(super) async fn fetch_col2(
    signal: &Signal,
    nav: NavCategory,
    rig_type: RigType,
) -> Vec<ColumnItem> {
    match nav {
        NavCategory::Presets => {
            let mut items: Vec<ColumnItem> = Vec::new();

            // Rig presets (tag: None)
            let rigs = signal.rigs().list().await.unwrap_or_default();
            items.extend(
                rigs.into_iter()
                    .filter(|r| r.rig_type.map_or(false, |rt| rt == rig_type))
                    .map(|r| {
                        let meta = r.metadata().clone();
                        let tags = TagSet::from_tags(&meta.tags);
                        ColumnItem {
                            id: r.id.to_string(),
                            name: r.name.clone(),
                            subtitle: Some("Rig".to_string()),
                            badge: Some(format!("{}", r.variants.len())),
                            metadata: Some(meta),
                            structured_tags: tags,
                            detail: DetailData::default(),
                            tag: None,
                        }
                    }),
            );

            // Layer presets (tag: Some(LAYER_PRESET_TAG))
            let et = rig_type_to_engine_type(rig_type);
            let layers = signal.layers().list().await.unwrap_or_default();
            let all_module_presets = signal.module_presets().list().await.unwrap_or_default();
            let block_preset_lookup = build_block_preset_lookup(signal).await;
            for layer in layers.into_iter().filter(|l| l.engine_type == et) {
                let module_chains = resolve_layer_module_chains(
                    signal,
                    &layer,
                    &all_module_presets,
                    &block_preset_lookup,
                )
                .await;
                let meta = layer.metadata().clone();
                let tags = TagSet::from_tags(&meta.tags);
                items.push(ColumnItem {
                    id: layer.id.to_string(),
                    name: layer.name.clone(),
                    subtitle: Some("Layer".to_string()),
                    badge: Some(format!("{}", layer.variants.len())),
                    metadata: Some(meta),
                    structured_tags: tags,
                    detail: DetailData {
                        module_chains,
                        ..Default::default()
                    },
                    tag: Some(LAYER_PRESET_TAG),
                });
            }

            items
        }
        NavCategory::Engines => {
            let et = rig_type_to_engine_type(rig_type);
            let engines = signal.engines().list().await.unwrap_or_default();
            engines
                .into_iter()
                .filter(|e| e.engine_type == et)
                .map(|e| {
                    let meta = e.metadata().clone();
                    let tags = TagSet::from_tags(&meta.tags);
                    ColumnItem {
                        id: e.id.to_string(),
                        name: e.name.clone(),
                        subtitle: Some(format!("{} layer(s)", e.layer_ids.len())),
                        badge: Some(format!("{}", e.variants.len())),
                        metadata: Some(meta),
                        structured_tags: tags,
                        detail: DetailData::default(),
                        tag: None,
                    }
                })
                .collect()
        }
        NavCategory::Layers => {
            let et = rig_type_to_engine_type(rig_type);
            let layers = signal.layers().list().await.unwrap_or_default();
            let all_module_presets = signal.module_presets().list().await.unwrap_or_default();
            let block_preset_lookup = build_block_preset_lookup(signal).await;
            let mut items = Vec::new();
            for layer in layers.into_iter().filter(|l| l.engine_type == et) {
                let module_chains = resolve_layer_module_chains(
                    signal,
                    &layer,
                    &all_module_presets,
                    &block_preset_lookup,
                )
                .await;
                let meta = layer.metadata().clone();
                let tags = TagSet::from_tags(&meta.tags);
                items.push(ColumnItem {
                    id: layer.id.to_string(),
                    name: layer.name.clone(),
                    subtitle: Some(format!("{} variant(s)", layer.variants.len())),
                    badge: Some(format!("{}", layer.variants.len())),
                    metadata: Some(meta),
                    structured_tags: tags,
                    detail: DetailData {
                        module_chains,
                        ..Default::default()
                    },
                    tag: None,
                });
            }
            items
        }
        NavCategory::Modules => {
            // Show module types as col2 items (like Blocks shows block types).
            // Count how many presets exist per module type for the badge.
            let all_presets = signal.module_presets().list().await.unwrap_or_default();
            signal::ALL_MODULE_TYPES
                .iter()
                .enumerate()
                .map(|(idx, &mt)| {
                    let count = all_presets.iter().filter(|p| p.module_type() == mt).count();
                    let mut tags = TagSet::default();
                    tags.insert(StructuredTag::new(TagCategory::Module, mt.as_str()));
                    ColumnItem {
                        id: mt.as_str().to_string(),
                        name: mt.display_name().to_string(),
                        subtitle: Some(mt.category().display_name().to_string()),
                        badge: if count > 0 {
                            Some(format!("{count}"))
                        } else {
                            None
                        },
                        metadata: None,
                        structured_tags: tags,
                        detail: DetailData::default(),
                        tag: Some(idx),
                    }
                })
                .collect()
        }
        NavCategory::Blocks => ALL_BLOCK_TYPES
            .iter()
            .enumerate()
            .map(|(idx, bt)| {
                let mut tags = TagSet::default();
                tags.insert(StructuredTag::new(TagCategory::Block, bt.as_str()));
                ColumnItem {
                    id: bt.as_str().to_string(),
                    name: bt.display_name().to_string(),
                    subtitle: Some(bt.category().display_name().to_string()),
                    badge: None,
                    metadata: None,
                    structured_tags: tags,
                    detail: DetailData::default(),
                    tag: Some(idx),
                }
            })
            .collect(),
    }
}

/// Returns (column items, block presets cache).
/// The cache is non-empty only for `NavCategory::Blocks` — it holds the raw
/// `Preset` objects so col4 can extract snapshots without re-querying.
pub(super) async fn fetch_col3(
    signal: &Signal,
    nav: NavCategory,
    col2_id: &str,
    col2_tag: Option<usize>,
) -> (Vec<ColumnItem>, Vec<Preset>) {
    match nav {
        NavCategory::Presets => {
            let is_layer = col2_tag == Some(LAYER_PRESET_TAG);
            let items = if is_layer {
                // Layer preset — show variants with module chains
                if let Some(layer) = signal.layers().load(col2_id).await.ok().flatten() {
                    let all_module_presets =
                        signal.module_presets().list().await.unwrap_or_default();
                    let block_preset_lookup = build_block_preset_lookup(signal).await;
                    let mut out = Vec::new();
                    for v in &layer.variants {
                        let module_chains = resolve_variant_module_chains(
                            signal,
                            v,
                            &all_module_presets,
                            &block_preset_lookup,
                        )
                        .await;
                        let ref_count =
                            v.module_refs.len() + v.block_refs.len() + v.plugin_refs.len();
                        let meta = v.metadata().clone();
                        let tags = TagSet::from_tags(&meta.tags);
                        out.push(ColumnItem {
                            id: v.id.to_string(),
                            name: v.name.clone(),
                            subtitle: Some(format!("{} module(s)", ref_count)),
                            badge: None,
                            metadata: Some(meta),
                            structured_tags: tags,
                            detail: DetailData {
                                module_chains,
                                ..Default::default()
                            },
                            tag: None,
                        });
                    }
                    out
                } else {
                    Vec::new()
                }
            } else {
                // Rig preset — show scenes with engines
                if let Some(rig) = signal.rigs().load(col2_id).await.ok().flatten() {
                    let all_module_presets =
                        signal.module_presets().list().await.unwrap_or_default();
                    let block_preset_lookup = build_block_preset_lookup(signal).await;
                    let mut out = Vec::new();
                    for (idx, v) in rig.variants.iter().enumerate() {
                        // Lazy scene resolution: only resolve the first scene eagerly.
                        // Remaining scenes are resolved on click via resolve_scene_detail.
                        let engines = if idx == 0 {
                            resolve_rig_scene_engines(
                                signal,
                                v,
                                &all_module_presets,
                                &block_preset_lookup,
                            )
                            .await
                        } else {
                            Vec::new()
                        };
                        let meta = v.metadata().clone();
                        let tags = TagSet::from_tags(&meta.tags);
                        out.push(ColumnItem {
                            id: v.id.to_string(),
                            name: v.name.clone(),
                            subtitle: Some(format!("{} engine(s)", v.engine_selections.len())),
                            badge: None,
                            metadata: Some(meta),
                            structured_tags: tags,
                            detail: DetailData {
                                engines,
                                ..Default::default()
                            },
                            tag: None,
                        });
                    }
                    out
                } else {
                    Vec::new()
                }
            };
            (items, Vec::new())
        }
        NavCategory::Engines => {
            let items = if let Some(engine) = signal.engines().load(col2_id).await.ok().flatten() {
                let all_module_presets = signal.module_presets().list().await.unwrap_or_default();
                let block_preset_lookup = build_block_preset_lookup(signal).await;
                let mut items = Vec::new();
                for layer_id in &engine.layer_ids {
                    if let Some(layer) =
                        signal.layers().load(layer_id.as_str()).await.ok().flatten()
                    {
                        let module_chains = resolve_layer_module_chains(
                            signal,
                            &layer,
                            &all_module_presets,
                            &block_preset_lookup,
                        )
                        .await;
                        let meta = layer.metadata().clone();
                        let tags = TagSet::from_tags(&meta.tags);
                        items.push(ColumnItem {
                            id: layer.id.to_string(),
                            name: layer.name.clone(),
                            subtitle: Some(format!("{} variant(s)", layer.variants.len())),
                            badge: None,
                            metadata: Some(meta),
                            structured_tags: tags,
                            detail: DetailData {
                                module_chains,
                                ..Default::default()
                            },
                            tag: None,
                        });
                    }
                }
                items
            } else {
                Vec::new()
            };
            (items, Vec::new())
        }
        NavCategory::Layers => {
            let items = if let Some(layer) = signal.layers().load(col2_id).await.ok().flatten() {
                let all_module_presets = signal.module_presets().list().await.unwrap_or_default();
                let block_preset_lookup = build_block_preset_lookup(signal).await;
                let mut out = Vec::new();
                for v in &layer.variants {
                    let module_chains = resolve_variant_module_chains(
                        signal,
                        v,
                        &all_module_presets,
                        &block_preset_lookup,
                    )
                    .await;
                    let ref_count = v.module_refs.len() + v.block_refs.len() + v.plugin_refs.len();
                    let meta = v.metadata().clone();
                    let tags = TagSet::from_tags(&meta.tags);
                    out.push(ColumnItem {
                        id: v.id.to_string(),
                        name: v.name.clone(),
                        subtitle: Some(format!("{} module(s)", ref_count)),
                        badge: None,
                        metadata: Some(meta),
                        structured_tags: tags,
                        detail: DetailData {
                            module_chains,
                            ..Default::default()
                        },
                        tag: None,
                    });
                }
                out
            } else {
                Vec::new()
            };
            (items, Vec::new())
        }
        NavCategory::Modules => {
            // col2 is a module type index — show presets for that type.
            if let Some(idx) = col2_tag {
                if let Some(&mt) = signal::ALL_MODULE_TYPES.get(idx) {
                    let all_presets = signal.module_presets().list().await.unwrap_or_default();
                    let items: Vec<ColumnItem> = all_presets
                        .iter()
                        .filter(|p| p.module_type() == mt)
                        .map(|p| {
                            // Load default snapshot chain for detail preview
                            let chain = p.snapshots().first().map(|s| s.module().chain().clone());
                            ColumnItem {
                                id: p.id().to_string(),
                                name: p.name().to_string(),
                                subtitle: Some(format!("{} snapshot(s)", p.snapshots().len())),
                                badge: Some(format!("{}", p.snapshots().len())),
                                metadata: None,
                                structured_tags: TagSet::default(),
                                detail: DetailData {
                                    chain,
                                    ..Default::default()
                                },
                                tag: col2_tag,
                            }
                        })
                        .collect();
                    return (items, Vec::new());
                }
            }
            (Vec::new(), Vec::new())
        }
        NavCategory::Blocks => {
            if let Some(idx) = col2_tag {
                if let Some(&bt) = ALL_BLOCK_TYPES.get(idx) {
                    let presets = signal.block_presets().list(bt).await.unwrap_or_default();
                    let items = presets
                        .iter()
                        .map(|p| {
                            let tags = signal::tagging::infer_tags_from_name(p.name());
                            ColumnItem {
                                id: p.id().to_string(),
                                name: p.name().to_string(),
                                subtitle: None,
                                badge: Some(format!("{}", p.snapshots().len())),
                                metadata: None,
                                structured_tags: tags,
                                detail: DetailData::default(),
                                tag: col2_tag,
                            }
                        })
                        .collect();
                    return (items, presets);
                }
            }
            (Vec::new(), Vec::new())
        }
    }
}

// endregion: --- Column fetching

// region: --- Detail resolution helpers

/// Resolve a layer's default variant refs into `ModuleChainData` for grid rendering.
///
/// Delegates to [`resolve_variant_module_chains`] with the layer's default variant.
async fn resolve_layer_module_chains(
    signal: &Signal,
    layer: &Layer,
    all_module_presets: &[ModulePreset],
    block_preset_lookup: &HashMap<String, (BlockType, String)>,
) -> Vec<ModuleChainData> {
    let variant = match layer.default_variant() {
        Some(v) => v,
        None => return Vec::new(),
    };
    resolve_variant_module_chains(signal, variant, all_module_presets, block_preset_lookup).await
}

/// Resolve a specific layer snapshot's refs into `ModuleChainData` for grid rendering.
///
/// Handles all four ref types:
/// - `layer_refs` → recursively resolved nested layers
/// - `module_refs` → full module chains from module presets
/// - `block_refs` → single-block synthetic chains (one per block)
/// - `plugin_refs` → virtual module chains from plugin block defs
async fn resolve_variant_module_chains(
    signal: &Signal,
    variant: &signal::layer::LayerSnapshot,
    all_module_presets: &[ModulePreset],
    block_preset_lookup: &HashMap<String, (BlockType, String)>,
) -> Vec<ModuleChainData> {
    let mut out = Vec::new();

    // 1) Resolve layer_refs (recursive — nested layers)
    for lr in &variant.layer_refs {
        let layer_id_str = lr.collection_id.to_string();
        if let Some(nested_layer) = signal
            .layers()
            .load(layer_id_str.as_str())
            .await
            .ok()
            .flatten()
        {
            let nested = Box::pin(resolve_layer_module_chains(
                signal,
                &nested_layer,
                all_module_presets,
                block_preset_lookup,
            ))
            .await;
            out.extend(nested);
        }
    }

    // 2) Resolve module_refs (module presets with full signal chains)
    for mr in &variant.module_refs {
        let collection_id_str = mr.collection_id.to_string();
        let module_preset = all_module_presets
            .iter()
            .find(|p| p.id().to_string() == collection_id_str);
        let mt = module_preset.map(|p| p.module_type());
        let mc = mt
            .map(|m| m.color())
            .unwrap_or(signal::ModuleType::Drive.color());
        let module_name = module_preset
            .map(|p| p.name().to_string())
            .unwrap_or_else(|| format!("Module {}", mr.collection_id));
        let chain;
        if let Some(snapshot) = signal
            .module_presets()
            .load_default(collection_id_str)
            .await
            .ok()
            .flatten()
        {
            chain = snapshot.module().chain().clone();
        } else {
            chain = SignalChain::new(vec![]);
        }
        out.push(ModuleChainData {
            name: module_name,
            color_bg: mc.bg.to_string(),
            color_fg: mc.fg.to_string(),
            color_border: mc.border.to_string(),
            chain,
            module_type: mt,
        });
    }

    // 3) Resolve block_refs (standalone blocks → single-node chains)
    for br in &variant.block_refs {
        let preset_id_str = br.collection_id.to_string();
        let (bt, preset_name) = block_preset_lookup
            .get(&preset_id_str)
            .cloned()
            .unwrap_or((BlockType::Custom, format!("Block {}", br.collection_id)));

        let source = match &br.variant_id {
            Some(snap_id) => ModuleBlockSource::PresetSnapshot {
                preset_id: br.collection_id.clone(),
                snapshot_id: snap_id.clone(),
                saved_at_version: None,
            },
            None => ModuleBlockSource::PresetDefault {
                preset_id: br.collection_id.clone(),
                saved_at_version: None,
            },
        };
        let node = SignalNode::Block(ModuleBlock::new(
            preset_id_str.clone(),
            &preset_name,
            bt,
            source,
        ));
        let chain = SignalChain::new(vec![node]);
        let color = bt.color();
        out.push(ModuleChainData {
            name: preset_name,
            color_bg: color.bg.to_string(),
            color_fg: color.fg.to_string(),
            color_border: color.border.to_string(),
            chain,
            module_type: None,
        });
    }

    // 4) Resolve plugin_refs (plugin block defs → virtual module chains)
    for pr in &variant.plugin_refs {
        for (label, mt, chain) in pr.def.to_module_chains() {
            let mc = mt.color();
            out.push(ModuleChainData {
                name: format!("{} / {}", pr.def.plugin_name, label),
                color_bg: mc.bg.to_string(),
                color_fg: mc.fg.to_string(),
                color_border: mc.border.to_string(),
                chain,
                module_type: Some(mt),
            });
        }
    }

    out
}

/// Build a lookup table of block preset ID → (BlockType, name).
///
/// Loads all block collections across every block type. This is cached
/// per-call since `resolve_layer_module_chains` may be called multiple
/// times for nested layers.
async fn build_block_preset_lookup(
    signal: &Signal,
) -> std::collections::HashMap<String, (BlockType, String)> {
    let mut lookup = std::collections::HashMap::new();
    for &bt in ALL_BLOCK_TYPES {
        for preset in signal.block_presets().list(bt).await.unwrap_or_default() {
            lookup.insert(preset.id().to_string(), (bt, preset.name().to_string()));
        }
    }
    lookup
}

/// Resolve a rig scene's full hierarchy into `EngineFlowData` for grid rendering.
///
/// Walks: `RigScene.engine_selections → Engine → EngineScene.layer_selections → Layer → modules`
async fn resolve_rig_scene_engines(
    signal: &Signal,
    scene: &signal::rig::RigScene,
    all_module_presets: &[ModulePreset],
    block_preset_lookup: &HashMap<String, (BlockType, String)>,
) -> Vec<EngineFlowData> {
    let mut engines = Vec::new();
    for es in &scene.engine_selections {
        let engine_id_str = es.engine_id.as_str();
        let engine = match signal.engines().load(engine_id_str).await.ok().flatten() {
            Some(e) => e,
            None => continue,
        };
        // Find the selected engine variant, fall back to default
        let engine_variant = engine
            .variant(&es.variant_id)
            .or_else(|| engine.default_variant());
        let engine_variant = match engine_variant {
            Some(v) => v,
            None => continue,
        };
        let mut layers = Vec::new();
        for ls in &engine_variant.layer_selections {
            let layer_id_str = ls.layer_id.as_str();
            let layer = match signal.layers().load(layer_id_str).await.ok().flatten() {
                Some(l) => l,
                None => continue,
            };
            let module_chains = resolve_layer_module_chains(
                signal,
                &layer,
                all_module_presets,
                block_preset_lookup,
            )
            .await;
            layers.push(LayerFlowData {
                name: layer.name.clone(),
                module_chains,
            });
        }
        engines.push(EngineFlowData {
            name: engine.name.clone(),
            layers,
        });
    }
    engines
}

/// On-demand resolution for a lazily-loaded rig scene.
///
/// Called when the user clicks a scene that was not eagerly resolved
/// (i.e. any scene other than the first). Loads the rig, finds the
/// matching scene, resolves engines, and builds a parameter lookup.
pub(super) async fn resolve_scene_detail(
    signal: &Signal,
    rig_id: &str,
    scene_id: &str,
) -> Option<(Vec<EngineFlowData>, ParamLookup)> {
    let rig = signal.rigs().load(rig_id).await.ok().flatten()?;
    let scene = rig.variants.iter().find(|v| v.id.to_string() == scene_id)?;

    let all_module_presets = signal.module_presets().list().await.unwrap_or_default();
    let block_preset_lookup = build_block_preset_lookup(signal).await;

    let engines =
        resolve_rig_scene_engines(signal, scene, &all_module_presets, &block_preset_lookup).await;

    // Build param lookup from the resolved engines
    let detail = DetailData {
        engines: engines.clone(),
        ..Default::default()
    };
    let temp_item = ColumnItem {
        id: scene_id.to_string(),
        name: String::new(),
        subtitle: None,
        badge: None,
        metadata: None,
        structured_tags: TagSet::default(),
        detail,
        tag: None,
    };
    let params = build_param_lookup(signal, &[temp_item]).await;

    Some((engines, params))
}

/// On-demand resolution for a layer variant.
///
/// Loads the layer, finds the matching variant (or default), resolves its
/// module chains, wraps them in a synthetic `EngineFlowData`, and builds
/// a parameter lookup — making the result compatible with `engines_to_grid_slots`.
pub(super) async fn resolve_layer_detail(
    signal: &Signal,
    layer_id: &str,
    variant_id: Option<&str>,
) -> Option<(Vec<EngineFlowData>, ParamLookup)> {
    let layer = signal.layers().load(layer_id).await.ok().flatten()?;
    let all_module_presets = signal.module_presets().list().await.unwrap_or_default();
    let block_preset_lookup = build_block_preset_lookup(signal).await;

    // Pick the requested variant, falling back to default
    let variant = variant_id
        .and_then(|vid| layer.variants.iter().find(|v| v.id.to_string() == vid))
        .or_else(|| layer.default_variant())?;

    let module_chains =
        resolve_variant_module_chains(signal, variant, &all_module_presets, &block_preset_lookup)
            .await;

    // Wrap in a synthetic EngineFlowData so it's grid-compatible
    let engine = EngineFlowData {
        name: layer.name.clone(),
        layers: vec![LayerFlowData {
            name: variant.name.clone(),
            module_chains,
        }],
    };
    let engines = vec![engine];

    // Build param lookup
    let detail = DetailData {
        engines: engines.clone(),
        ..Default::default()
    };
    let temp_item = ColumnItem {
        id: layer_id.to_string(),
        name: String::new(),
        subtitle: None,
        badge: None,
        metadata: None,
        structured_tags: TagSet::default(),
        detail,
        tag: None,
    };
    let params = build_param_lookup(signal, &[temp_item]).await;

    Some((engines, params))
}

// endregion: --- Detail resolution helpers

// region: --- Parameter resolution

/// Walk all column items' detail data, collect block source references,
/// and resolve them into a parameter lookup table.
pub(super) async fn build_param_lookup(signal: &Signal, items: &[ColumnItem]) -> ParamLookup {
    let mut lookup = ParamLookup::new();
    for item in items {
        collect_chain_sources(&item.detail, &mut lookup, signal).await;
    }
    lookup
}

/// Collect block parameters from all chains in a DetailData tree.
async fn collect_chain_sources(data: &DetailData, lookup: &mut ParamLookup, signal: &Signal) {
    // Walk engines → layers → module chains → chain nodes
    for engine in &data.engines {
        for layer in &engine.layers {
            for mc in &layer.module_chains {
                resolve_chain_params(&mc.chain, lookup, signal).await;
            }
        }
    }
    // Walk module_chains directly
    for mc in &data.module_chains {
        resolve_chain_params(&mc.chain, lookup, signal).await;
    }
    // Walk standalone chain
    if let Some(ref chain) = data.chain {
        resolve_chain_params(chain, lookup, signal).await;
    }
}

/// Walk a signal chain and resolve parameters for each block source.
async fn resolve_chain_params(chain: &SignalChain, lookup: &mut ParamLookup, signal: &Signal) {
    for node in chain.nodes() {
        resolve_node_params(node, lookup, signal).await;
    }
}

async fn resolve_node_params(node: &signal::SignalNode, lookup: &mut ParamLookup, signal: &Signal) {
    match node {
        signal::SignalNode::Block(mb) => {
            match mb.source() {
                signal::ModuleBlockSource::PresetSnapshot {
                    preset_id,
                    snapshot_id,
                    ..
                } => {
                    let key = (preset_id.to_string(), snapshot_id.to_string());
                    if !lookup.contains_key(&key) {
                        if let Some(block) = signal
                            .block_presets()
                            .load_variant(mb.block_type(), preset_id.clone(), snapshot_id.clone())
                            .await
                            .ok()
                            .flatten()
                        {
                            let params: Vec<(String, f32)> = block
                                .parameters()
                                .iter()
                                .map(|p| (p.name().to_string(), p.value().get()))
                                .collect();
                            lookup.insert(key, params);
                        }
                    }
                }
                signal::ModuleBlockSource::PresetDefault { preset_id, .. } => {
                    let key = (preset_id.to_string(), "default".to_string());
                    if !lookup.contains_key(&key) {
                        if let Some(block) = signal
                            .block_presets()
                            .load_default(mb.block_type(), preset_id.clone())
                            .await
                            .ok()
                            .flatten()
                        {
                            let params: Vec<(String, f32)> = block
                                .parameters()
                                .iter()
                                .map(|p| (p.name().to_string(), p.value().get()))
                                .collect();
                            lookup.insert(key, params);
                        }
                    }
                }
                signal::ModuleBlockSource::Inline { .. } => {
                    // Inline blocks carry their params directly — handled in extract_block_params
                }
            }
        }
        signal::SignalNode::Split { lanes } => {
            for lane in lanes {
                for n in lane.nodes() {
                    Box::pin(resolve_node_params(n, lookup, signal)).await;
                }
            }
        }
    }
}

// endregion: --- Parameter resolution

// region: --- Utility

pub fn rig_type_to_engine_type(rig_type: RigType) -> signal::EngineType {
    match rig_type {
        RigType::Guitar => signal::EngineType::Guitar,
        RigType::Bass => signal::EngineType::Bass,
        RigType::Keys => signal::EngineType::Keys,
        RigType::Drums | RigType::DrumReplacement => signal::EngineType::Guitar,
        RigType::Vocals => signal::EngineType::Vocal,
    }
}

// endregion: --- Utility

// region: --- Tests

#[cfg(test)]
mod tests {
    use super::*;
    use signal::rig::RigType;

    /// Reproduce the exact data pipeline the Manage tab uses:
    ///   1. bootstrap signal
    ///   2. list rigs, filter by Guitar
    ///   3. pick first preset, pick first scene
    ///   4. call resolve_scene_detail (same as manage's resolve_scene_engines)
    ///   5. call engines_to_grid_slots
    ///
    /// This test will tell us whether the data pipeline produces grid slots.
    #[tokio::test]
    async fn manage_tab_guitar_pipeline_produces_grid_slots() {
        let signal = signal::bootstrap_in_memory_controller_async()
            .await
            .expect("bootstrap failed");

        // Step 1: list rigs filtered by Guitar (same as manage tab effect)
        let rigs = signal.rigs().list().await.unwrap();
        let guitar_rigs: Vec<_> = rigs
            .into_iter()
            .filter(|r| r.rig_type.map_or(false, |t| t == RigType::Guitar))
            .collect();

        assert!(!guitar_rigs.is_empty(), "expected at least one Guitar rig");
        eprintln!(
            "Guitar rigs: {:?}",
            guitar_rigs
                .iter()
                .map(|r| (&r.name, r.variants.len()))
                .collect::<Vec<_>>()
        );

        let first_rig = &guitar_rigs[0];
        assert_eq!(first_rig.name, "MegaRig");
        assert_eq!(
            first_rig.variants.len(),
            2,
            "Guitar MegaRig should have 2 scenes"
        );

        let rig_id = first_rig.id.to_string();
        let first_scene = &first_rig.variants[0];
        let scene_id = first_scene.id.to_string();
        eprintln!(
            "rig_id={} scene_id={} scene_name={}",
            rig_id, scene_id, first_scene.name
        );

        // Step 2: resolve scene engines (same path as manage tab)
        let result = resolve_scene_detail(&signal, &rig_id, &scene_id).await;
        assert!(result.is_some(), "resolve_scene_detail returned None");

        let (engines, params) = result.unwrap();
        eprintln!("engines count: {}", engines.len());
        for (i, engine) in engines.iter().enumerate() {
            eprintln!(
                "  engine[{}] name={} layers={}",
                i,
                engine.name,
                engine.layers.len()
            );
            for (j, layer) in engine.layers.iter().enumerate() {
                eprintln!(
                    "    layer[{}] name={} module_chains={}",
                    j,
                    layer.name,
                    layer.module_chains.len()
                );
                for (k, mc) in layer.module_chains.iter().enumerate() {
                    eprintln!(
                        "      chain[{}] name={} nodes={}",
                        k,
                        mc.name,
                        mc.chain.nodes().len()
                    );
                }
            }
        }
        eprintln!("params count: {}", params.len());

        // Assertions
        assert!(!engines.is_empty(), "expected at least one engine");
        assert!(
            engines.iter().all(|e| !e.layers.is_empty()),
            "every engine should have at least one layer"
        );
        assert!(
            engines
                .iter()
                .flat_map(|e| &e.layers)
                .any(|l| !l.module_chains.is_empty()),
            "at least one layer should have module chains"
        );

        // Step 3: convert to grid slots (same as manage tab RSX)
        let grid_slots = super::super::grid_conversion::engines_to_grid_slots(&engines, &params);
        eprintln!("grid_slots count: {}", grid_slots.len());
        assert!(
            !grid_slots.is_empty(),
            "engines_to_grid_slots should produce non-empty grid slots"
        );
    }
}
