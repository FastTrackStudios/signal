use crate::id::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentDiff {
    pub ops: Vec<DocumentOp>,
}

impl DocumentDiff {
    pub fn empty() -> Self {
        Self { ops: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DocumentOp {
    InsertNode { node_id: NodeId, parent_id: NodeId },
    RemoveNode { node_id: NodeId },
    UpdateRawJson { node_id: NodeId },
}
