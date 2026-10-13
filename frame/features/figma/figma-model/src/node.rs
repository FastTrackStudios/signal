use crate::figma::FigmaNodeData;
use crate::id::NodeId;
use figma_api::models;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Document,
    Canvas,
    Subcanvas,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodePayload {
    Document(Box<models::DocumentNode>),
    Canvas(Box<models::CanvasNode>),
    Subcanvas(Box<models::SubcanvasNode>),
    Synthetic(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameNode {
    pub id: NodeId,
    pub figma_id: String,
    pub name: String,
    pub figma_type: String,
    pub kind: NodeKind,
    pub payload: NodePayload,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub visible: bool,
    pub locked: bool,
    pub plugin_data: std::collections::HashMap<String, serde_json::Value>,
    pub figma_data: FigmaNodeData,
    pub raw: serde_json::Value,
}

impl FrameNode {
    pub fn new_document(node: models::DocumentNode) -> Self {
        let raw = serde_json::to_value(&node).unwrap_or(serde_json::Value::Null);
        let (plugin_data, figma_data) = extract_figma_extras(&raw);
        Self {
            id: NodeId::new(),
            figma_id: node.id.clone(),
            name: node.name.clone(),
            figma_type: "DOCUMENT".to_string(),
            kind: NodeKind::Document,
            payload: NodePayload::Document(Box::new(node)),
            parent: None,
            children: Vec::new(),
            visible: raw.get("visible").and_then(|v| v.as_bool()).unwrap_or(true),
            locked: raw.get("locked").and_then(|v| v.as_bool()).unwrap_or(false),
            plugin_data,
            figma_data,
            raw,
        }
    }

    pub fn new_canvas(node: models::CanvasNode) -> Self {
        let raw = serde_json::to_value(&node).unwrap_or(serde_json::Value::Null);
        let (plugin_data, figma_data) = extract_figma_extras(&raw);
        Self {
            id: NodeId::new(),
            figma_id: node.id.clone(),
            name: node.name.clone(),
            figma_type: "CANVAS".to_string(),
            kind: NodeKind::Canvas,
            payload: NodePayload::Canvas(Box::new(node)),
            parent: None,
            children: Vec::new(),
            visible: raw.get("visible").and_then(|v| v.as_bool()).unwrap_or(true),
            locked: raw.get("locked").and_then(|v| v.as_bool()).unwrap_or(false),
            plugin_data,
            figma_data,
            raw,
        }
    }

    pub fn new_subcanvas(node: models::SubcanvasNode) -> Self {
        let raw = serde_json::to_value(&node).unwrap_or(serde_json::Value::Null);
        let figma_type = raw
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN")
            .to_string();
        let figma_id = raw
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let name = raw
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unnamed")
            .to_string();
        let (plugin_data, figma_data) = extract_figma_extras(&raw);

        Self {
            id: NodeId::new(),
            figma_id,
            name,
            figma_type,
            kind: NodeKind::Subcanvas,
            payload: NodePayload::Subcanvas(Box::new(node)),
            parent: None,
            children: Vec::new(),
            visible: raw.get("visible").and_then(|v| v.as_bool()).unwrap_or(true),
            locked: raw.get("locked").and_then(|v| v.as_bool()).unwrap_or(false),
            plugin_data,
            figma_data,
            raw,
        }
    }

    pub fn node_type(&self) -> &str {
        &self.figma_type
    }

    pub fn new_synthetic(raw: serde_json::Value) -> Self {
        let figma_type = raw
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("SYNTHETIC")
            .to_string();
        let figma_id = raw
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let name = raw
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unnamed")
            .to_string();
        let (plugin_data, figma_data) = extract_figma_extras(&raw);

        let kind = match figma_type.as_str() {
            "DOCUMENT" => NodeKind::Document,
            "CANVAS" => NodeKind::Canvas,
            _ => NodeKind::Subcanvas,
        };

        Self {
            id: NodeId::new(),
            figma_id,
            name,
            figma_type,
            kind,
            payload: NodePayload::Synthetic(raw.clone()),
            parent: None,
            children: Vec::new(),
            visible: raw.get("visible").and_then(|v| v.as_bool()).unwrap_or(true),
            locked: raw.get("locked").and_then(|v| v.as_bool()).unwrap_or(false),
            plugin_data,
            figma_data,
            raw,
        }
    }
}

fn extract_figma_extras(
    raw: &serde_json::Value,
) -> (
    std::collections::HashMap<String, serde_json::Value>,
    FigmaNodeData,
) {
    let plugin_data = raw
        .get("pluginData")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let explicit_modes = raw
        .get("explicitVariableModes")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect::<std::collections::HashMap<_, _>>()
        });

    let component_property_references = raw
        .get("componentPropertyReferences")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect::<std::collections::HashMap<_, _>>()
        });

    let figma_data = FigmaNodeData {
        shared_plugin_data: raw.get("sharedPluginData").cloned(),
        bound_variables: raw.get("boundVariables").cloned(),
        explicit_variable_modes: explicit_modes,
        component_property_references,
        unresolved: std::collections::HashMap::new(),
    };

    (plugin_data, figma_data)
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ArcData {
    pub starting_angle: f64,
    pub ending_angle: f64,
    pub inner_radius: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ExportFormat {
    #[default]
    Png,
    Jpg,
    Svg,
    Pdf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ExportConstraintType {
    #[default]
    Scale,
    Width,
    Height,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportSetting {
    pub suffix: String,
    pub format: ExportFormat,
    pub constraint_type: ExportConstraintType,
    pub constraint_value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyOverride {
    pub path: Vec<String>,
    pub property: OverrideProperty,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OverrideProperty {
    Raw(serde_json::Value),
}
