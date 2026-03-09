//! Factory and builder methods for [`NodeGraph`].
//!
//! Contains `widget_for_block_type`, `create_module_for_block_type`,
//! and `build_from_engines` (bridge from signal domain types).

use signal::{BlockType, ModuleBlock, SignalChain};
use uuid::Uuid;

use super::models::{GraphModule, Node, NodeGraph, NodePosition, NodeSize, NodeWidget, Wire};

impl NodeGraph {
    /// Get the appropriate widget and size for a block type.
    pub fn widget_for_block_type(block_type: BlockType) -> (NodeWidget, NodeSize) {
        match block_type {
            BlockType::Eq => (NodeWidget::EqGraph, NodeSize::xlarge()),
            BlockType::Compressor => (NodeWidget::CompressorGraph, NodeSize::large()),
            BlockType::Gate => (NodeWidget::GateGraph, NodeSize::medium()),
            BlockType::Delay => (NodeWidget::DelayGraph, NodeSize::large()),
            BlockType::Reverb => (NodeWidget::ReverbGraph, NodeSize::large()),
            BlockType::Drive | BlockType::Saturator => (NodeWidget::DriveGraph, NodeSize::medium()),
            BlockType::Modulation | BlockType::Trem | BlockType::Pitch => {
                (NodeWidget::ModulationGraph, NodeSize::medium())
            }
            BlockType::Amp | BlockType::Cabinet => (NodeWidget::AmpCab, NodeSize::medium()),
            BlockType::Tuner => (NodeWidget::Tuner, NodeSize::small()),
            BlockType::Freeze => (NodeWidget::Label, NodeSize::medium()),
            _ => (NodeWidget::Label, NodeSize::small()),
        }
    }

    /// Create a new module with a single node for a given block type.
    pub fn create_module_for_block_type(
        name: impl Into<String>,
        block_type: BlockType,
        position: NodePosition,
    ) -> GraphModule {
        let name = name.into();
        let (widget, size) = Self::widget_for_block_type(block_type);

        let node = Node::new(&name, block_type, NodePosition::new(10.0, 50.0))
            .with_size(size)
            .with_widget(widget);

        let mut module = GraphModule::new(&name, block_type, position);
        module.add_node(node);
        module.auto_size(20.0);
        module
    }

    /// Find an open position to place a new module.
    pub fn find_open_position(&self) -> NodePosition {
        if self.modules.is_empty() && self.nodes.is_empty() {
            return NodePosition::new(100.0, 100.0);
        }

        let mut max_bottom = 0.0f64;
        let mut leftmost_x = f64::MAX;

        for module in &self.modules {
            let bottom = module.position.y + module.size.height;
            max_bottom = max_bottom.max(bottom);
            leftmost_x = leftmost_x.min(module.position.x);
        }

        for node in &self.nodes {
            let bottom = node.position.y + node.size.height;
            max_bottom = max_bottom.max(bottom);
            leftmost_x = leftmost_x.min(node.position.x);
        }

        let x = if leftmost_x == f64::MAX {
            100.0
        } else {
            leftmost_x
        };
        NodePosition::new(x, max_bottom + 40.0)
    }

    /// Build a node graph from signal `EngineFlowData` slices.
    ///
    /// Creates a `GraphModule` per module chain within each engine/layer,
    /// with child `Node`s for each block in the signal chain. Modules are
    /// laid out vertically with auto-chained inter-module wires.
    pub fn build_from_engines(engines: &[EngineData]) -> Self {
        let mut graph = Self::new();
        let mut y_offset = 80.0;
        let module_x = 50.0;

        for engine in engines {
            for layer in &engine.layers {
                let mut prev_module_id: Option<Uuid> = None;

                for module_chain in &layer.module_chains {
                    let block_type = infer_block_type_from_chain(&module_chain.chain);
                    let gm = build_module_from_chain(
                        &module_chain.name,
                        block_type,
                        &module_chain.chain,
                        NodePosition::new(module_x, y_offset),
                    );

                    let module_height = gm.size.height;
                    let module_id = graph.add_module(gm);

                    if let Some(prev_mid) = prev_module_id {
                        graph.connect(prev_mid, "out_l", module_id, "in_l");
                    }
                    prev_module_id = Some(module_id);

                    y_offset += module_height + 30.0;
                }
            }
        }

        graph
    }
}

/// Minimal data structures for the builder (mirrors collection_browser types).
#[derive(Clone)]
pub struct EngineData {
    pub name: String,
    pub layers: Vec<LayerData>,
}

#[derive(Clone)]
pub struct LayerData {
    pub name: String,
    pub module_chains: Vec<ModuleChainInput>,
}

#[derive(Clone)]
pub struct ModuleChainInput {
    pub name: String,
    pub chain: SignalChain,
}

/// Infer the primary BlockType from a signal chain (first block's type).
fn infer_block_type_from_chain(chain: &SignalChain) -> BlockType {
    chain
        .blocks()
        .first()
        .map(|b| b.block_type())
        .unwrap_or(BlockType::Custom)
}

/// Build a `GraphModule` from a `SignalChain`, creating child nodes for each block.
fn build_module_from_chain(
    name: &str,
    block_type: BlockType,
    chain: &SignalChain,
    position: NodePosition,
) -> GraphModule {
    let module_width = 400.0;
    let node_height = 60.0;
    let node_gap = 10.0;
    let header_height = 40.0;

    let blocks: Vec<&ModuleBlock> = chain.blocks();
    let block_count = blocks.len();
    let content_height = header_height + (block_count as f64) * (node_height + node_gap) + 20.0;
    let module_height = content_height.max(120.0);

    let mut gm = GraphModule::new(name, block_type, position)
        .with_size(NodeSize::new(module_width, module_height));

    let mut prev_node_id: Option<Uuid> = None;
    for (i, mb) in blocks.iter().enumerate() {
        let (widget, _) = NodeGraph::widget_for_block_type(mb.block_type());
        let node = Node::new(
            mb.label(),
            mb.block_type(),
            NodePosition::new(20.0, header_height + (i as f64) * (node_height + node_gap)),
        )
        .with_size(NodeSize::new(module_width - 40.0, node_height))
        .with_widget(widget);
        let node_id = gm.add_node(node);

        if let Some(prev) = prev_node_id {
            gm.add_wire(Wire::new(prev, "out_l", node_id, "in_l"));
        }
        prev_node_id = Some(node_id);
    }

    gm
}
