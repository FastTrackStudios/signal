//! Deterministic domain resolver/compiler output types.
//!
//! Resolver turns selected variants (rig scene / profile patch / song section)
//! into an executable graph with effective override stack.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::engine::{EngineId, EngineSceneId};
use crate::layer::{LayerId, LayerSnapshotId};
use crate::overrides::Override;
use crate::profile::{PatchId, ProfileId};
use crate::rig::{RigId, RigSceneId};
use crate::song::{SectionId, SongId};
use crate::{Block, BlockType, ModulePresetId, ModuleSnapshotId, PresetId, SnapshotId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet, thiserror::Error)]
#[repr(C)]
pub enum ResolveError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid reference: {0}")]
    InvalidReference(String),
    #[error("cycle detected: {0}")]
    CycleDetected(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum ResolveTarget {
    RigScene {
        rig_id: RigId,
        scene_id: RigSceneId,
    },
    ProfilePatch {
        profile_id: ProfileId,
        patch_id: PatchId,
    },
    SongSection {
        song_id: SongId,
        section_id: SectionId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum LayerSource {
    LayerPreset {
        layer_id: LayerId,
        variant_id: LayerSnapshotId,
    },
    InlinedInParent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ResolvedBlock {
    pub node_id: String,
    pub label: String,
    pub block_type: BlockType,
    pub source_preset_id: Option<PresetId>,
    pub source_variant_id: Option<SnapshotId>,
    pub block: Block,
    /// Binary plugin state data for direct chunk loading (bypasses param name matching).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_data: Option<Vec<u8>>,
    /// `true` when the block's `saved_at_version` is older than the current snapshot version.
    /// Indicates the module chain references an outdated snapshot.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ResolvedModule {
    pub source_preset_id: ModulePresetId,
    pub source_variant_id: ModuleSnapshotId,
    pub blocks: Vec<ResolvedBlock>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ResolvedLayer {
    pub layer_id: LayerId,
    pub layer_variant_id: LayerSnapshotId,
    pub source: LayerSource,
    pub modules: Vec<ResolvedModule>,
    pub standalone_blocks: Vec<ResolvedBlock>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ResolvedEngine {
    pub engine_id: EngineId,
    pub engine_scene_id: EngineSceneId,
    pub layers: Vec<ResolvedLayer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ResolvedGraph {
    pub target: ResolveTarget,
    pub rig_id: RigId,
    pub rig_scene_id: RigSceneId,
    pub engines: Vec<ResolvedEngine>,
    pub effective_overrides: Vec<Override>,
}

/// A block reference whose `saved_at_version` is behind the current snapshot version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct StaleReference {
    /// The block's `node_id` in the resolved graph.
    pub block_node_id: String,
    /// Human-readable block label.
    pub block_label: String,
    /// The preset that was referenced.
    pub preset_id: PresetId,
    /// The version the module chain was saved against.
    pub saved_version: u32,
    /// The current version of the snapshot.
    pub current_version: u32,
}

impl ResolvedGraph {
    /// Collect all stale block references in this graph.
    ///
    /// A block is stale when its `saved_at_version` is behind the current
    /// snapshot version (indicated by the `stale` flag set during resolution).
    pub fn stale_references(&self) -> Vec<StaleReference> {
        let mut refs = Vec::new();
        for engine in &self.engines {
            for layer in &engine.layers {
                for module in &layer.modules {
                    for block in &module.blocks {
                        if block.stale {
                            refs.push(StaleReference {
                                block_node_id: block.node_id.clone(),
                                block_label: block.label.clone(),
                                preset_id: block
                                    .source_preset_id
                                    .clone()
                                    .unwrap_or_else(PresetId::new),
                                saved_version: 0, // not available from resolved form
                                current_version: 0,
                            });
                        }
                    }
                }
                for block in &layer.standalone_blocks {
                    if block.stale {
                        refs.push(StaleReference {
                            block_node_id: block.node_id.clone(),
                            block_label: block.label.clone(),
                            preset_id: block.source_preset_id.clone().unwrap_or_else(PresetId::new),
                            saved_version: 0,
                            current_version: 0,
                        });
                    }
                }
            }
        }
        refs
    }

    /// Find a block parameter value by matching `block_id_fragment` against
    /// each block's `node_id` or `label` (case-insensitive), then returning
    /// the value of the parameter whose id matches `param_id`.
    ///
    /// Walks engines → layers → modules → blocks and standalone blocks.
    pub fn find_param(&self, block_id_fragment: &str, param_id: &str) -> Option<f32> {
        for engine in &self.engines {
            for layer in &engine.layers {
                for module in &layer.modules {
                    if let Some(v) =
                        find_param_in_block(&module.blocks, block_id_fragment, param_id)
                    {
                        return Some(v);
                    }
                }
                if let Some(v) =
                    find_param_in_block(&layer.standalone_blocks, block_id_fragment, param_id)
                {
                    return Some(v);
                }
            }
        }
        None
    }
}

fn find_param_in_block(
    blocks: &[ResolvedBlock],
    block_id_fragment: &str,
    param_id: &str,
) -> Option<f32> {
    for block in blocks {
        if block.node_id.contains(block_id_fragment)
            || block.label.to_lowercase().contains(block_id_fragment)
        {
            for param in block.block.parameters() {
                if param.id() == param_id {
                    return Some(param.value().get());
                }
            }
        }
    }
    None
}
