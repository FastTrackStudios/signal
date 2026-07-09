//! Node-based signal flow model and interactive canvas.
//!
//! Represents audio processing nodes positioned on an infinite 2D canvas
//! with wire connections between them. Includes full interactive components
//! for pan/zoom, drag, wire creation, and module management.

pub mod builder;
pub mod drag_handler;
pub mod models;
pub mod module_container;
pub mod node_block;
pub mod view;
pub mod wire;
pub mod wire_layer;

pub use builder::{EngineData, LayerData, ModuleChainInput};
pub use models::{
    GraphModule, Node, NodeGraph, NodeParameter, NodePort, NodePosition, NodeSize, NodeWidget,
    ParameterType, Wire,
};
pub use module_container::ModuleContainer;
pub use node_block::NodeBlock;
pub use view::NodeGraphView;

use uuid::Uuid;

// ---------------------------------------------------------------------------
// Core NodeGraph impl (new, add/remove/find, connect, disconnect, layout)
// ---------------------------------------------------------------------------

impl NodeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_module(&mut self, module: GraphModule) -> Uuid {
        let id = module.id;
        self.modules.push(module);
        id
    }

    pub fn find_module(&self, id: Uuid) -> Option<&GraphModule> {
        self.modules.iter().find(|m| m.id == id)
    }

    pub fn find_module_mut(&mut self, id: Uuid) -> Option<&mut GraphModule> {
        self.modules.iter_mut().find(|m| m.id == id)
    }

    pub fn module_at(&self, x: f64, y: f64) -> Option<&GraphModule> {
        self.modules.iter().rev().find(|m| m.contains_point(x, y))
    }

    pub fn add_node(&mut self, node: Node) -> Uuid {
        let id = node.id;
        self.nodes.push(node);
        id
    }

    pub fn remove_module(&mut self, id: Uuid) {
        self.modules.retain(|m| m.id != id);
        self.wires.retain(|w| w.from_node != id && w.to_node != id);
    }

    pub fn remove_node(&mut self, id: Uuid) {
        self.nodes.retain(|n| n.id != id);
        self.wires.retain(|w| w.from_node != id && w.to_node != id);
    }

    pub fn has_wire(&self, from_node: Uuid, from_port: &str, to_node: Uuid, to_port: &str) -> bool {
        self.wires.iter().any(|w| {
            w.from_node == from_node
                && w.from_port == from_port
                && w.to_node == to_node
                && w.to_port == to_port
        })
    }

    /// Validate and add a wire. Returns None if the wire is invalid.
    pub fn try_connect(
        &mut self,
        from_node: Uuid,
        from_port: impl Into<String>,
        to_node: Uuid,
        to_port: impl Into<String>,
    ) -> Option<Uuid> {
        if from_node == to_node {
            return None;
        }
        let from_port = from_port.into();
        let to_port = to_port.into();
        if self.has_wire(from_node, &from_port, to_node, &to_port) {
            return None;
        }
        let wire = Wire::new(from_node, from_port, to_node, to_port);
        let id = wire.id;
        self.wires.push(wire);
        Some(id)
    }

    pub fn find_node(&self, id: Uuid) -> Option<&Node> {
        if let Some(node) = self.nodes.iter().find(|n| n.id == id) {
            return Some(node);
        }
        for module in &self.modules {
            if let Some(node) = module.find_node(id) {
                return Some(node);
            }
        }
        None
    }

    pub fn find_node_mut(&mut self, id: Uuid) -> Option<&mut Node> {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
            return Some(node);
        }
        for module in &mut self.modules {
            if let Some(node) = module.find_node_mut(id) {
                return Some(node);
            }
        }
        None
    }

    pub fn node_at(&self, x: f64, y: f64) -> Option<&Node> {
        self.nodes.iter().rev().find(|n| n.contains_point(x, y))
    }

    pub fn connect(
        &mut self,
        from_node: Uuid,
        from_port: impl Into<String>,
        to_node: Uuid,
        to_port: impl Into<String>,
    ) -> Uuid {
        let wire = Wire::new(from_node, from_port, to_node, to_port);
        let id = wire.id;
        self.wires.push(wire);
        id
    }

    pub fn disconnect(&mut self, id: Uuid) {
        self.wires.retain(|w| w.id != id);
    }

    /// Automatically arrange modules vertically with proper spacing.
    pub fn compact_layout(&mut self, gap: f64) {
        if self.modules.is_empty() {
            return;
        }

        let mut indices: Vec<usize> = (0..self.modules.len()).collect();
        indices.sort_by(|a, b| {
            self.modules[*a]
                .position
                .y
                .partial_cmp(&self.modules[*b].position.y)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let row_threshold = 50.0;
        let mut rows: Vec<Vec<usize>> = Vec::new();

        for idx in indices {
            let this_y = self.modules[idx].position.y;
            let same_row = rows.last().map_or(false, |row| {
                let row_y = self.modules[row[0]].position.y;
                (this_y - row_y).abs() < row_threshold
            });

            if same_row {
                rows.last_mut().unwrap().push(idx);
            } else {
                rows.push(vec![idx]);
            }
        }

        let mut y = 50.0;
        for row in &rows {
            for &idx in row {
                self.modules[idx].position.y = y;
            }
            let max_height = row
                .iter()
                .map(|&idx| self.modules[idx].size.height)
                .fold(0.0f64, f64::max);
            y += max_height + gap;
        }
    }
}
