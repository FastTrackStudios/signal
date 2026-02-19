//! Core data model types for the node graph.
//!
//! Ported from legacy signal-ui with `signal::BlockType` replacing
//! `signal_control::block::BlockType` and plain `f64` replacing `NormalizedF64`.

use signal::BlockType;
use uuid::Uuid;

/// A parameter attached to a processing node.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeParameter {
    pub id: String,
    pub name: String,
    /// Normalized 0.0–1.0 value.
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub unit: String,
    pub param_type: ParameterType,
    pub formatted_display: Option<String>,
}

/// Parameter interaction type.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ParameterType {
    #[default]
    Continuous,
    Stepped,
    Toggle,
    Choice(Vec<String>),
}

impl NodeParameter {
    pub fn new(id: impl Into<String>, name: impl Into<String>, value: f64) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            value: value.clamp(0.0, 1.0),
            min: 0.0,
            max: 1.0,
            unit: String::new(),
            param_type: ParameterType::Continuous,
            formatted_display: None,
        }
    }

    pub fn with_range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = unit.into();
        self
    }

    pub fn with_param_type(mut self, param_type: ParameterType) -> Self {
        self.param_type = param_type;
        self
    }

    pub fn with_formatted_display(mut self, formatted: impl Into<String>) -> Self {
        self.formatted_display = Some(formatted.into());
        self
    }

    /// Convert normalized value to UI text.
    pub fn display_value(&self) -> String {
        if let Some(formatted) = &self.formatted_display {
            if !formatted.trim().is_empty() {
                return formatted.clone();
            }
        }
        match &self.param_type {
            ParameterType::Toggle => {
                if self.value >= 0.5 {
                    "On".to_string()
                } else {
                    "Off".to_string()
                }
            }
            ParameterType::Choice(options) => {
                if options.is_empty() {
                    return format!("{:.1}", self.value);
                }
                let max_index = (options.len() - 1) as f64;
                let index = (self.value.clamp(0.0, 1.0) * max_index).round() as usize;
                options
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| format!("{:.1}", self.value))
            }
            ParameterType::Continuous | ParameterType::Stepped => {
                let real = self.min + (self.max - self.min) * self.value;
                if self.unit.is_empty() {
                    format!("{real:.1}")
                } else {
                    format!("{real:.1} {}", self.unit)
                }
            }
        }
    }
}

/// 2D position on the canvas (in canvas coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

impl NodePosition {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// Size of a node (in canvas coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeSize {
    pub width: f64,
    pub height: f64,
}

impl NodeSize {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    pub const fn small() -> Self {
        Self::new(160.0, 80.0)
    }

    pub const fn medium() -> Self {
        Self::new(220.0, 120.0)
    }

    pub const fn large() -> Self {
        Self::new(320.0, 180.0)
    }

    pub const fn xlarge() -> Self {
        Self::new(400.0, 220.0)
    }
}

impl Default for NodeSize {
    fn default() -> Self {
        Self::medium()
    }
}

/// Widget type to render inside a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NodeWidget {
    #[default]
    Label,
    EqGraph,
    CompressorGraph,
    GateGraph,
    DelayGraph,
    ReverbGraph,
    AmpCab,
    DriveGraph,
    ModulationGraph,
    Tuner,
    Looper,
}

/// Input/output port on a node.
#[derive(Debug, Clone, PartialEq)]
pub struct NodePort {
    pub id: String,
    pub label: String,
    pub is_input: bool,
    pub color: Option<String>,
}

impl NodePort {
    pub fn input(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            is_input: true,
            color: None,
        }
    }

    pub fn output(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            is_input: false,
            color: None,
        }
    }

    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }
}

/// A signal processing node on the canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: Uuid,
    pub name: String,
    pub short_label: Option<String>,
    pub block_type: BlockType,
    pub position: NodePosition,
    pub size: NodeSize,
    pub widget: NodeWidget,
    pub bypassed: bool,
    /// Whether this node is a placeholder (no plugin assigned yet).
    pub is_placeholder: bool,
    /// Optional description of the node's purpose.
    pub description: Option<String>,
    pub parameters: Vec<NodeParameter>,
    pub inputs: Vec<NodePort>,
    pub outputs: Vec<NodePort>,
}

impl Node {
    pub fn new(name: impl Into<String>, block_type: BlockType, position: NodePosition) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            short_label: None,
            block_type,
            position,
            size: NodeSize::default(),
            widget: NodeWidget::Label,
            bypassed: false,
            is_placeholder: false,
            description: None,
            parameters: Vec::new(),
            inputs: vec![
                NodePort::input("in_l", "In L"),
                NodePort::input("in_r", "In R"),
            ],
            outputs: vec![
                NodePort::output("out_l", "Out L"),
                NodePort::output("out_r", "Out R"),
            ],
        }
    }

    pub fn with_size(mut self, size: NodeSize) -> Self {
        self.size = size;
        self
    }
    pub fn with_widget(mut self, widget: NodeWidget) -> Self {
        self.widget = widget;
        self
    }
    pub fn with_bypassed(mut self, bypassed: bool) -> Self {
        self.bypassed = bypassed;
        self
    }

    pub fn with_short_label(mut self, label: impl Into<String>) -> Self {
        self.short_label = Some(label.into());
        self
    }

    pub fn with_parameters(mut self, parameters: Vec<NodeParameter>) -> Self {
        self.parameters = parameters;
        self
    }

    pub fn with_ports(mut self, inputs: Vec<NodePort>, outputs: Vec<NodePort>) -> Self {
        self.inputs = inputs;
        self.outputs = outputs;
        self
    }

    /// Check if a point (in canvas coordinates) is inside this node.
    pub fn contains_point(&self, x: f64, y: f64) -> bool {
        x >= self.position.x
            && x <= self.position.x + self.size.width
            && y >= self.position.y
            && y <= self.position.y + self.size.height
    }

    /// Get the center position of this node.
    pub fn center(&self) -> NodePosition {
        NodePosition::new(
            self.position.x + self.size.width / 2.0,
            self.position.y + self.size.height / 2.0,
        )
    }

    /// Get the position of a port (for wire connection).
    pub fn port_position(&self, port_id: &str, is_input: bool) -> Option<NodePosition> {
        let ports = if is_input {
            &self.inputs
        } else {
            &self.outputs
        };
        let port_index = ports.iter().position(|p| p.id == port_id)?;
        let port_count = ports.len();
        let port_spacing = self.size.height / (port_count + 1) as f64;
        let port_y = self.position.y + port_spacing * (port_index + 1) as f64;
        let port_x = if is_input {
            self.position.x
        } else {
            self.position.x + self.size.width
        };
        Some(NodePosition::new(port_x, port_y))
    }
}

/// Wire connection between two node ports.
#[derive(Debug, Clone, PartialEq)]
pub struct Wire {
    pub id: Uuid,
    pub from_node: Uuid,
    pub from_port: String,
    pub to_node: Uuid,
    pub to_port: String,
    pub color: Option<String>,
}

impl Wire {
    pub fn new(
        from_node: Uuid,
        from_port: impl Into<String>,
        to_node: Uuid,
        to_port: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            from_node,
            from_port: from_port.into(),
            to_node,
            to_port: to_port.into(),
            color: None,
        }
    }

    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }
}

/// A module container that groups multiple nodes on the node graph canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphModule {
    pub id: Uuid,
    pub name: String,
    pub block_type: BlockType,
    pub position: NodePosition,
    pub size: NodeSize,
    pub bypassed: bool,
    /// Whether this module is collapsed (hiding internal nodes/wires).
    pub collapsed: bool,
    pub nodes: Vec<Node>,
    pub internal_wires: Vec<Wire>,
    pub inputs: Vec<NodePort>,
    pub outputs: Vec<NodePort>,
}

impl GraphModule {
    pub fn new(name: impl Into<String>, block_type: BlockType, position: NodePosition) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            block_type,
            position,
            size: NodeSize::new(500.0, 300.0),
            bypassed: false,
            collapsed: false,
            nodes: Vec::new(),
            internal_wires: Vec::new(),
            inputs: vec![
                NodePort::input("in_l", "In L"),
                NodePort::input("in_r", "In R"),
            ],
            outputs: vec![
                NodePort::output("out_l", "Out L"),
                NodePort::output("out_r", "Out R"),
            ],
        }
    }

    pub fn with_size(mut self, size: NodeSize) -> Self {
        self.size = size;
        self
    }
    pub fn with_bypassed(mut self, bypassed: bool) -> Self {
        self.bypassed = bypassed;
        self
    }

    pub fn with_ports(mut self, inputs: Vec<NodePort>, outputs: Vec<NodePort>) -> Self {
        self.inputs = inputs;
        self.outputs = outputs;
        self
    }

    pub fn add_node(&mut self, node: Node) -> Uuid {
        let id = node.id;
        self.nodes.push(node);
        id
    }

    pub fn add_wire(&mut self, wire: Wire) {
        self.internal_wires.push(wire);
    }

    pub fn contains_point(&self, x: f64, y: f64) -> bool {
        x >= self.position.x
            && x <= self.position.x + self.size.width
            && y >= self.position.y
            && y <= self.position.y + self.size.height
    }

    /// Check if a point is in the title bar (for dragging).
    pub fn title_bar_contains(&self, x: f64, y: f64) -> bool {
        x >= self.position.x
            && x <= self.position.x + self.size.width
            && y >= self.position.y
            && y <= self.position.y + 40.0
    }

    pub fn port_position(&self, port_id: &str, is_input: bool) -> Option<NodePosition> {
        let ports = if is_input {
            &self.inputs
        } else {
            &self.outputs
        };
        let port_index = ports.iter().position(|p| p.id == port_id)?;
        let port_count = ports.len();
        let port_spacing = self.size.height / (port_count + 1) as f64;
        let port_y = self.position.y + port_spacing * (port_index + 1) as f64;
        let port_x = if is_input {
            self.position.x
        } else {
            self.position.x + self.size.width
        };
        Some(NodePosition::new(port_x, port_y))
    }

    pub fn find_node(&self, id: Uuid) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn find_node_mut(&mut self, id: Uuid) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// Calculate and set the module size to fit all internal nodes with padding.
    pub fn auto_size(&mut self, padding: f64) {
        if self.nodes.is_empty() {
            return;
        }
        let mut max_x = 0.0f64;
        let mut max_y = 0.0f64;
        for node in &self.nodes {
            max_x = max_x.max(node.position.x + node.size.width);
            max_y = max_y.max(node.position.y + node.size.height);
        }
        self.size = NodeSize::new(max_x + padding, 40.0 + max_y + padding);
    }
}

/// The complete node graph.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeGraph {
    /// Modules in the graph.
    pub modules: Vec<GraphModule>,
    /// Standalone nodes (not in any module).
    pub nodes: Vec<Node>,
    /// Wires connecting modules/nodes.
    pub wires: Vec<Wire>,
}
