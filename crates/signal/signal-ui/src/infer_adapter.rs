//! Adapter: `InferredChain` → `Vec<ModuleChainData>` / `EngineFlowData`.
//!
//! Bridges the signal-daw-bridge inference output into the data types
//! consumed by the existing grid rendering pipeline.

use signal_daw_bridge::InferredChain;

use crate::views::{EngineFlowData, LayerFlowData, ModuleChainData};
use signal::{Block, ModuleBlock, ModuleBlockSource, SignalChain, SignalNode};

/// Convert an inferred chain into a flat list of module chain data for grid rendering.
pub fn inferred_chain_to_module_chains(chain: &InferredChain) -> Vec<ModuleChainData> {
    let mut out = Vec::new();

    for module in &chain.modules {
        let color = module.module_type.color();
        out.push(ModuleChainData {
            name: module.name.clone(),
            color_bg: color.bg.to_string(),
            color_fg: color.fg.to_string(),
            color_border: color.border.to_string(),
            chain: module.chain.clone(),
            module_type: Some(module.module_type),
        });
    }

    for block in &chain.standalone_blocks {
        let color = block.block_type.color();
        let module_block = ModuleBlock::new(
            &block.plugin_name,
            &block.name,
            block.block_type,
            ModuleBlockSource::Inline {
                block: Block::default(),
            },
        );
        out.push(ModuleChainData {
            name: block.name.clone(),
            color_bg: color.bg.to_string(),
            color_fg: color.fg.to_string(),
            color_border: color.border.to_string(),
            chain: SignalChain::new(vec![SignalNode::Block(Box::new(module_block))]),
            module_type: None,
        });
    }

    out
}

/// Wrap an inferred chain as a single-layer engine flow for the grid renderer.
pub fn inferred_chain_to_engine_flow(chain: &InferredChain, track_name: &str) -> EngineFlowData {
    EngineFlowData {
        name: track_name.to_string(),
        layers: vec![LayerFlowData {
            name: "FX Chain".to_string(),
            module_chains: inferred_chain_to_module_chains(chain),
        }],
    }
}
