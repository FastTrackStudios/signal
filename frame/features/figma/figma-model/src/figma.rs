use figma_api::models;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FigmaPayload {
    ApiGetFile(Box<models::GetFile>),
    FigBinary { bytes: Vec<u8>, note: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FigmaDocumentMeta {
    pub file_key: Option<String>,
    pub role: Option<String>,
    pub editor_type: Option<String>,
    pub version: Option<String>,
    pub schema_version: Option<f64>,
    pub last_modified: Option<String>,
    pub thumbnail_url: Option<String>,
}

impl FigmaDocumentMeta {
    pub fn from_get_file(file: &models::GetFile) -> Self {
        Self {
            file_key: None,
            role: Some(format!("{:?}", file.role)),
            editor_type: Some(format!("{:?}", file.editor_type)),
            version: Some(file.version.clone()),
            schema_version: Some(file.schema_version),
            last_modified: Some(file.last_modified.clone()),
            thumbnail_url: file.thumbnail_url.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FigmaNodeData {
    pub shared_plugin_data: Option<serde_json::Value>,
    pub bound_variables: Option<serde_json::Value>,
    pub explicit_variable_modes: Option<std::collections::HashMap<String, String>>,
    pub component_property_references: Option<std::collections::HashMap<String, String>>,
    pub unresolved: std::collections::HashMap<String, serde_json::Value>,
}
